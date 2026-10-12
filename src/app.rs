use std::collections::VecDeque;

use crate::model::Oscillator;

/// Integration step; well below 1/ω for the slider range.
const DT: f64 = 1e-3;
/// Points kept for the plot; older ones scroll out.
const HISTORY: usize = 2000;

pub struct App {
    omega: f64,
    zeta: f64,
    running: bool,
    show_log: bool,
    oscillator: Oscillator,
    history: VecDeque<[f64; 2]>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // GPU compute hook: device and queue are available here when built with `wgpu`.
        #[cfg(feature = "wgpu")]
        if let Some(rs) = &cc.wgpu_render_state {
            log::info!("wgpu adapter: {:?}", rs.adapter.get_info().name);
        }
        #[cfg(not(feature = "wgpu"))]
        let _ = cc;

        log::info!("{} {}", crate::TITLE, crate::provenance());
        let (omega, zeta) = (2.0, 0.1);
        Self {
            omega,
            zeta,
            running: true,
            show_log: false,
            oscillator: Oscillator::new(omega, zeta).expect("defaults are valid"),
            history: VecDeque::with_capacity(HISTORY),
        }
    }

    fn restart(&mut self) {
        match Oscillator::new(self.omega, self.zeta) {
            Ok(osc) => {
                self.oscillator = osc;
                self.history.clear();
            }
            Err(err) => log::warn!("{err}"),
        }
    }

    /// Advances by the frame time so speed is independent of frame rate.
    fn advance(&mut self, frame_dt: f64) {
        let steps = (frame_dt / DT).round() as usize;
        for _ in 0..steps {
            self.oscillator.step(DT);
        }
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history
            .push_back([self.oscillator.time(), self.oscillator.position()]);
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Oscillator");
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut self.omega, 0.1..=10.0).text("ω"))
            .on_hover_text("Angular frequency of the undamped oscillator")
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.zeta, 0.0..=2.0).text("ζ"))
            .on_hover_text("Damping ratio: < 1 underdamped, 1 critical, > 1 overdamped")
            .changed();
        if changed {
            self.restart();
        }

        ui.horizontal(|ui| {
            let label = if self.running { "⏸ Pause" } else { "▶ Run" };
            if ui
                .button(label)
                .on_hover_text("Pause or resume time")
                .clicked()
            {
                self.running = !self.running;
            }
            if ui
                .button("⟲ Reset")
                .on_hover_text("Restart from x = 1")
                .clicked()
            {
                self.restart();
            }
        });

        ui.separator();
        ui.toggle_value(&mut self.show_log, "📜 Log")
            .on_hover_text("Show log messages");

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            #[cfg(target_arch = "wasm32")]
            ui.hyperlink_to("All versions", "../versions.html")
                .on_hover_text("Run an older release, e.g. to reproduce results");
            ui.label(crate::provenance())
                .on_hover_text("Version (git revision) of this build");
        });
    }

    #[cfg(feature = "plot")]
    fn plot(&self, ui: &mut egui::Ui) {
        use egui_plot::{Line, Plot, PlotPoints};
        let points: PlotPoints = self.history.iter().copied().collect();
        Plot::new("trajectory")
            .x_axis_label("t")
            .y_axis_label("x")
            .include_y(-1.0)
            .include_y(1.0)
            .show(ui, |plot_ui| plot_ui.line(Line::new("x(t)", points)));
    }

    #[cfg(not(feature = "plot"))]
    fn plot(&self, ui: &mut egui::Ui) {
        ui.label(format!(
            "t = {:.2}, x = {:+.4}  (build with `plot` feature for a graph)",
            self.oscillator.time(),
            self.oscillator.position()
        ));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.running {
            let dt = ui.input(|i| i.stable_dt) as f64;
            self.advance(dt);
            ui.ctx().request_repaint();
        }

        egui::Panel::left("controls").show(ui, |ui| self.controls(ui));
        egui::CentralPanel::default().show(ui, |ui| self.plot(ui));

        egui::Window::new("Log")
            .open(&mut self.show_log)
            .show(ui.ctx(), |ui| egui_logger::logger_ui().show(ui));
    }
}
