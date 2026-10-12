//! 3D view of a molecule. [`scene::Scene`] decides what to draw and what lies under the
//! cursor; this adapter only handles input and hands the frame to the GPU.

#[cfg(feature = "wgpu")]
mod gpu;
pub mod scene;

use scene::{Camera, Pick, Scene};

/// Depth buffer the window must be created with; impostors write exact depth into it.
pub const DEPTH_BITS: u8 = 24;

pub struct Viewer {
    scene: Option<Scene>,
    camera: Camera,
    gpu: bool,
    /// Clicked item, kept until the next click.
    pinned: Option<Pick>,
    show_layers: bool,
    /// Animation time (s); a clock of our own so pausing freezes the fluctuations.
    clock: f64,
    playing: bool,
    pulse_amplitude: f64,
    /// Animation clock rate.
    pulse_speed: f64,
    /// Last hover query and its answer: picking tests every triangle, so redo it only
    /// when the pointer, camera or layers change.
    hover: Option<(HoverKey, Option<Pick>)>,
    /// What the GPU last received; an equal frame is not rebuilt or re-uploaded.
    drawn: Option<FrameKey>,
}

/// What a frame depends on. Time only counts while something animates, so a static view
/// is built once.
#[derive(PartialEq)]
struct FrameKey {
    clock: Option<f64>,
    pulse_amplitude: f64,
    layers: Vec<scene::Layer>,
}

#[derive(PartialEq)]
struct HoverKey {
    pointer: [f32; 2],
    camera: Camera,
    layers: Vec<scene::Layer>,
}

impl Viewer {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            scene: None,
            camera: Camera::new([1.0, 1.0]),
            #[cfg(feature = "wgpu")]
            gpu: gpu::register(cc),
            #[cfg(not(feature = "wgpu"))]
            gpu: {
                let _ = cc;
                false
            },
            pinned: None,
            show_layers: true,
            clock: 0.0,
            playing: true,
            pulse_amplitude: 1.0,
            pulse_speed: 1.0,
            hover: None,
            drawn: None,
        }
    }

    /// Show nothing, e.g. while a new structure is computing or after a failure, so
    /// the view never shows (and labels) a molecule other than the current one.
    pub fn clear(&mut self) {
        self.scene = None;
        self.pinned = None;
        self.hover = None;
        self.drawn = None;
    }

    /// Show a new result. A `recalculation` of the shown structure (same chains) keeps
    /// the camera, layer settings and structure-only geometry; anything else starts from
    /// the default view, which also keeps large assemblies from meshing unasked.
    pub fn set_scene(&mut self, scene: Scene, recalculation: bool) {
        let scene = match &self.scene {
            Some(old) if recalculation => scene.with_view_of(old),
            _ => {
                let (center, radius) = scene.bounds();
                self.camera.fit(center, radius);
                scene
            }
        };
        self.scene = Some(scene);
        self.pinned = None;
        self.hover = None;
        self.drawn = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(scene) = &mut self.scene {
            egui::Panel::right("viewer_layers")
                .resizable(false)
                .show_collapsible(ui, &mut self.show_layers, |ui| {
                    layers_ui(ui, scene);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Pulse ×");
                        ui.add(egui::Slider::new(&mut self.pulse_amplitude, 0.0..=5.0))
                            .on_hover_text(
                                "Scales the charge fluctuation σ in the animation: \
                                 1 = as sampled, larger exaggerates small fluctuations, 0 = mean only",
                            );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Speed ×");
                        ui.add(
                            egui::Slider::new(&mut self.pulse_speed, 0.1..=5.0)
                                .logarithmic(true),
                        )
                        .on_hover_text("How fast the fluctuation animation runs");
                    });
                });
        }
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let Some(scene) = &mut self.scene else {
            centered_text(
                ui,
                rect,
                "Load a structure: PDB ID, Open…, or drop a PDB/mmCIF file here",
            );
            return;
        };
        if !self.gpu {
            centered_text(ui, rect, "The 3D view needs WebGPU");
            return;
        }

        self.camera.set_viewport([rect.width(), rect.height()]);
        handle_input(ui, &response, &mut self.camera);

        // Not while dragging: the view moves every frame and picking tests every triangle.
        let hover = response
            .hover_pos()
            .filter(|_| !response.dragged())
            .and_then(|p| {
                let key = HoverKey {
                    pointer: [p.x - rect.min.x, p.y - rect.min.y],
                    camera: self.camera,
                    layers: scene.layers().to_vec(),
                };
                match &self.hover {
                    Some((last, pick)) if *last == key => pick.clone(),
                    _ => {
                        let pick = scene.pick(&self.camera, key.pointer);
                        self.hover = Some((key, pick.clone()));
                        pick
                    }
                }
            });
        if response.clicked() {
            self.pinned = hover.clone();
        }

        // Redraw continuously only while something pulses.
        let animating = self.playing && scene.animates(self.pulse_amplitude);
        if animating {
            self.clock += self.pulse_speed * f64::from(ui.input(|i| i.stable_dt));
            ui.ctx().request_repaint();
        }

        #[cfg(feature = "wgpu")]
        {
            let key = FrameKey {
                clock: scene.animates(self.pulse_amplitude).then_some(self.clock),
                pulse_amplitude: self.pulse_amplitude,
                layers: scene.layers().to_vec(),
            };
            let frame = (self.drawn.as_ref() != Some(&key))
                .then(|| scene.frame(self.clock, self.pulse_amplitude));
            self.drawn = Some(key);
            let draw = gpu::Draw {
                camera: self.camera,
                frame,
            };
            ui.painter()
                .add(eframe::egui_wgpu::Callback::new_paint_callback(rect, draw));
        }

        if let Some(pick) = &hover {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new("viewer_hover"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| ui.label(&pick.label));
        }
        let button = |i: f32| {
            egui::Rect::from_min_size(
                rect.right_top() + egui::vec2(-28.0 * (i + 1.0), 4.0),
                egui::vec2(24.0, 24.0),
            )
        };
        if ui
            .put(
                button(0.0),
                egui::Button::new("☰").selected(self.show_layers),
            )
            .on_hover_text("Show or hide the layers panel")
            .clicked()
        {
            self.show_layers = !self.show_layers;
        }
        let play = if self.playing { "⏸" } else { "▶" };
        if ui
            .put(button(1.0), egui::Button::new(play))
            .on_hover_text("Pause or resume the charge-fluctuation animation")
            .clicked()
        {
            self.playing = !self.playing;
        }

        if let Some(pick) = &self.pinned {
            ui.painter().text(
                rect.right_bottom() + egui::vec2(-8.0, -8.0),
                egui::Align2::RIGHT_BOTTOM,
                &pick.label,
                egui::FontId::proportional(14.0),
                ui.visuals().strong_text_color(),
            );
        }
    }
}

fn layers_ui(ui: &mut egui::Ui, scene: &mut Scene) {
    ui.heading("Layers");
    for (i, layer) in scene.layers_mut().iter_mut().enumerate() {
        ui.separator();
        ui.checkbox(&mut layer.visible, layer.kind.label())
            .on_hover_text("Show or hide this layer");
        ui.add_enabled_ui(layer.visible, |ui| {
            egui::Grid::new(("layer", i)).num_columns(2).show(ui, |ui| {
                ui.label("Style");
                egui::ComboBox::from_id_salt(("representation", i))
                    .selected_text(layer.representation.label())
                    .show_ui(ui, |ui| {
                        for &r in layer.kind.representations() {
                            ui.selectable_value(&mut layer.representation, r, r.label());
                        }
                    })
                    .response
                    .on_hover_text("How the items are drawn");
                ui.end_row();
                ui.label("Colour");
                egui::ComboBox::from_id_salt(("coloring", i))
                    .selected_text(layer.coloring.label())
                    .show_ui(ui, |ui| {
                        for &c in layer.kind.colorings() {
                            ui.selectable_value(&mut layer.coloring, c, c.label());
                        }
                    })
                    .response
                    .on_hover_text("What the colour shows");
                ui.end_row();
                if layer.representation.has_contours() {
                    ui.label("Contours");
                    ui.checkbox(&mut layer.contours, "")
                        .on_hover_text("Iso-lines of the colour field on this surface");
                    ui.end_row();
                }
                ui.label("Opacity");
                ui.add(egui::Slider::new(&mut layer.opacity, 0.1..=1.0).show_value(false))
                    .on_hover_text("Lower to see layers inside");
                ui.end_row();
            });
        });
    }
}

fn handle_input(ui: &egui::Ui, response: &egui::Response, camera: &mut Camera) {
    let drag = response.drag_delta();
    let pan = response.dragged_by(egui::PointerButton::Secondary)
        || (response.dragged() && ui.input(|i| i.modifiers.shift));
    if pan {
        camera.pan([drag.x, drag.y]);
    } else if response.dragged_by(egui::PointerButton::Primary) {
        camera.rotate([drag.x, drag.y]);
    }
    if response.hovered() {
        let zoom = ui.input(|i| i.zoom_delta() * (i.smooth_scroll_delta.y * 0.002).exp());
        if zoom != 1.0 {
            camera.zoom(zoom);
        }
    }
}

fn centered_text(ui: &egui::Ui, rect: egui::Rect, text: &str) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(15.0),
        ui.visuals().weak_text_color(),
    );
}
