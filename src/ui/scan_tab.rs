//! Scan one condition with the structure and options from the structure tab.

use web_time::Duration;

use super::{conditions_ui, progress_ui};
use crate::app::Session;
use crate::io;
use crate::model::{Axis, Range, Scan};

pub struct ScanTab {
    axis: Axis,
    /// Remembered per axis, indexed like [`Axis::ALL`].
    ranges: [Range; 3],
    scan: Option<Scan>,
    error: Option<String>,
}

impl Default for ScanTab {
    fn default() -> Self {
        Self {
            axis: Axis::default(),
            ranges: Axis::ALL.map(Axis::default_range),
            scan: None,
            error: None,
        }
    }
}

impl ScanTab {
    /// Drop results that belong to a previous structure.
    pub fn clear(&mut self) {
        self.scan = None;
        self.error = None;
    }

    /// True while work remains.
    pub fn advance(&mut self, budget: Duration) -> bool {
        self.scan.as_mut().is_some_and(|s| !s.advance(budget))
    }

    fn range_mut(&mut self) -> &mut Range {
        let i = Axis::ALL.iter().position(|&a| a == self.axis).unwrap_or(0);
        &mut self.ranges[i]
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, session: &mut Session) {
        egui::Panel::left("scan_controls")
            .resizable(true)
            .default_size(290.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui, session));
            });
        egui::CentralPanel::default().show(ui, |ui| match &self.scan {
            Some(scan) if !scan.rows().is_empty() => plots(ui, scan),
            _ => {
                ui.centered_and_justified(|ui| ui.weak("Choose a range and press Scan"));
            }
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui, session: &mut Session) {
        ui.heading("Scan");
        ui.horizontal(|ui| {
            for a in Axis::ALL {
                ui.selectable_value(&mut self.axis, a, a.label())
                    .on_hover_text("Condition to vary");
            }
        });
        let speed = match self.axis {
            Axis::Ph => 0.05,
            Axis::Salt => 0.005,
            Axis::Temperature => 0.5,
        };
        let r = self.range_mut();
        egui::Grid::new("range").num_columns(2).show(ui, |ui| {
            for (name, value, tip) in [
                ("From", &mut r.start, "First value"),
                ("To", &mut r.stop, "Last value (inclusive)"),
                ("Step", &mut r.step, "Spacing between points"),
            ] {
                ui.label(name);
                ui.add(egui::DragValue::new(value).speed(speed).max_decimals(4))
                    .on_hover_text(tip);
                ui.end_row();
            }
        });

        ui.separator();
        ui.label("Fixed conditions")
            .on_hover_text("Shared with the structure tab");
        conditions_ui(ui, &mut session.conditions, Some(self.axis));

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("MC sweeps");
            ui.add(
                egui::DragValue::new(&mut session.setup.mc_sweeps)
                    .range(0..=1_000_000)
                    .speed(100),
            )
            .on_hover_text(
                "Monte Carlo sweeps per titratable site and scan point; \
                 0 = Henderson-Hasselbalch only. Shared with the structure tab",
            );
        });
        if let Some(s) = &session.structure {
            ui.weak(s.name())
                .on_hover_text("Structure, chains and other options come from the structure tab");
        }
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }

        match &self.scan {
            Some(scan) if !scan.is_done() => {
                if progress_ui(ui, scan.progress()) {
                    self.scan = None;
                }
            }
            _ => {
                let can_run = session.structure.is_some() && !session.selected.is_empty();
                if ui
                    .add_enabled(can_run, egui::Button::new("▶ Scan"))
                    .on_hover_text("Titrate at every point of the range")
                    .clicked()
                {
                    self.start(session);
                }
            }
        }
        let done = self.scan.as_ref().filter(|s| s.is_done());
        if ui
            .add_enabled(done.is_some(), egui::Button::new("💾 CSV"))
            .on_hover_text("Scan results with settings and version in the header")
            .clicked()
            && let (Some(scan), Some(s)) = (done, &session.structure)
        {
            io::save(&format!("{}_scan.csv", s.stem()), &scan.csv());
        }
    }

    fn start(&mut self, session: &Session) {
        let Some(structure) = &session.structure else {
            return;
        };
        let range = *self.range_mut();
        match Scan::start(
            structure,
            &session.setup,
            &session.chains(),
            session.conditions,
            self.axis,
            range,
        ) {
            Ok(scan) => {
                self.scan = Some(scan);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

#[cfg(feature = "plot")]
fn plots(ui: &mut egui::Ui, scan: &Scan) {
    use cgkitten::Multipole;
    use egui_plot::{FilledArea, Legend, Line, Plot, Points, uniform_grid_spacer};

    type Extract = fn(&Multipole) -> (f64, f64);
    // (title, y label, value and ± band)
    let panels: [(&str, &str, Extract); 4] = [
        ("Net charge", "⟨Z⟩ (e)", |m| {
            (m.charge, m.capacitance().sqrt())
        }),
        ("Capacitance", "C = ⟨Z²⟩−⟨Z⟩² (e²)", |m| {
            (m.capacitance(), 0.0)
        }),
        ("Dipole moment", "⟨μ⟩ (e·Å)", |m| {
            (m.dipole, m.dipole_std())
        }),
        ("Quadrupole moment", "⟨‖Θ‖⟩ (e·Å²)", |m| {
            (m.quadrupole, m.quadrupole_std())
        }),
    ];
    let rows = scan.rows();
    let xs: Vec<f64> = rows.iter().map(|r| r.x).collect();
    let has_mc = rows.iter().any(|r| r.mc.is_some());
    let mc_color = egui::Color32::from_rgb(230, 120, 50);
    let hh_color = egui::Color32::from_rgb(90, 150, 230);

    let spacing = ui.spacing().item_spacing;
    let size = ui.available_size();
    let title_h = ui.text_style_height(&egui::TextStyle::Body) + spacing.y;
    let (w, h) = (
        (size.x - spacing.x) / 2.0,
        (size.y - spacing.y) / 2.0 - title_h,
    );
    egui::Grid::new("plots").spacing(spacing).show(ui, |ui| {
        for (i, (title, ylabel, extract)) in panels.into_iter().enumerate() {
            // The charge spread is √C, so say so: it is easily mistaken for C.
            let band_name = if i == 0 { "± √C" } else { "± σ" };
            let series = |get: &dyn Fn(&crate::model::Row) -> Option<(f64, f64)>| {
                rows.iter().filter_map(get).collect::<Vec<_>>()
            };
            let hh = series(&|r| Some(extract(&r.hh)));
            let mc = series(&|r| r.mc.as_ref().map(extract));
            ui.vertical(|ui| {
                ui.strong(title);
                Plot::new(title)
                    .width(w)
                    .height(h)
                    .x_axis_label(scan.axis().label())
                    .y_axis_label(ylabel)
                    .legend(Legend::default())
                    .x_grid_spacer(uniform_grid_spacer(nice_steps))
                    .y_grid_spacer(uniform_grid_spacer(nice_steps))
                    .label_formatter(|pos| hover_label(pos, scan))
                    .link_axis("scan", [true, false])
                    .link_cursor("scan", [true, false])
                    .show(ui, |plot| {
                        // Band on the best estimate only, to keep the panel readable.
                        let (band, color) = if has_mc {
                            (&mc, mc_color)
                        } else {
                            (&hh, hh_color)
                        };
                        if band.iter().any(|(_, s)| *s > 0.0) {
                            let lo: Vec<f64> = band.iter().map(|(v, s)| v - s).collect();
                            let hi: Vec<f64> = band.iter().map(|(v, s)| v + s).collect();
                            plot.add(
                                FilledArea::new(band_name, &xs[..band.len()], &lo, &hi)
                                    .fill_color(color.gamma_multiply(0.2)),
                            );
                        }
                        let line = |v: &[(f64, f64)]| -> Vec<[f64; 2]> {
                            xs.iter().zip(v).map(|(&x, (y, _))| [x, *y]).collect()
                        };
                        // Markers show where data actually is; lines only guide the eye.
                        // Shared names merge line and markers into one legend entry.
                        let mut series = vec![("HH", &hh, hh_color)];
                        if has_mc {
                            series.push(("MC", &mc, mc_color));
                        }
                        for (name, v, color) in series {
                            plot.line(Line::new(name, line(v)).color(color));
                            plot.add(Points::new(name, line(v)).color(color).radius(3.0));
                        }
                    });
            });
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
}

/// Every quantity at the hovered point, not just the one plotted.
#[cfg(feature = "plot")]
fn hover_label(pos: &egui_plot::HoverPosition<'_>, scan: &Scan) -> Option<String> {
    use egui_plot::HoverPosition::NearDataPoint;
    let NearDataPoint {
        plot_name, index, ..
    } = pos
    else {
        return None;
    };
    let row = scan.rows().get(*index)?;
    let m = match *plot_name {
        "HH" => &row.hh,
        "MC" => row.mc.as_ref()?,
        _ => return None, // the σ band
    };
    Some(format!(
        "{plot_name} at {} = {}\n⟨Z⟩ = {:+.2} e\nC = {:.2} e²\n⟨μ⟩ = {:.1} ± {:.1} e·Å\n⟨‖Θ‖⟩ = {:.0} ± {:.0} e·Å²",
        scan.axis().label(),
        row.x,
        m.charge,
        m.capacitance(),
        m.dipole,
        m.dipole_std(),
        m.quadrupole,
        m.quadrupole_std(),
    ))
}

/// 1-2-5 tick steps. The default steps in powers of ten, which on a pH axis in a
/// narrow panel labels only "10".
#[cfg(feature = "plot")]
fn nice_steps(input: egui_plot::GridInput) -> [f64; 3] {
    let base = input.base_step_size;
    let decade = 10f64.powf(base.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * decade)
        .find(|&s| s >= base)
        .unwrap_or(10.0 * decade);
    // Coarser levels stay multiples of the fine one so the marks line up.
    if (step / decade).round() == 5.0 {
        [step, 2.0 * step, 10.0 * step]
    } else {
        [step, 5.0 * step, 10.0 * step]
    }
}

#[cfg(not(feature = "plot"))]
fn plots(ui: &mut egui::Ui, scan: &Scan) {
    ui.label(format!(
        "{} points done; build with the `plot` feature for graphs, or save the CSV.",
        scan.rows().len()
    ));
}
