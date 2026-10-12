//! Tabs and shared widgets. Thin: state lives in [`crate::model`].

mod scan_tab;
mod structure_tab;
mod viewer;

pub use scan_tab::ScanTab;
pub use structure_tab::StructureTab;

use crate::model::{Axis, Conditions};

/// Edits solution conditions, skipping the one a scan sweeps over.
fn conditions_ui(ui: &mut egui::Ui, c: &mut Conditions, swept: Option<Axis>) {
    egui::Grid::new("conditions").num_columns(2).show(ui, |ui| {
        if swept != Some(Axis::Ph) {
            ui.label("pH");
            ui.add(egui::DragValue::new(&mut c.ph).range(0.0..=14.0).speed(0.05))
                .on_hover_text("Solution pH, 0–14");
            ui.end_row();
        }
        if swept != Some(Axis::Salt) {
            ui.label("I (mol/L)");
            ui.add(
                egui::DragValue::new(&mut c.salt)
                    .range(0.0..=5.0)
                    .speed(0.005)
                    .max_decimals(4),
            )
            .on_hover_text("Ionic strength of a 1:1 salt; sets Debye screening. 0 = unscreened");
            ui.end_row();
        }
        if swept != Some(Axis::Temperature) {
            ui.label("T (K)");
            ui.add(
                egui::DragValue::new(&mut c.temperature)
                    .range(200.0..=400.0)
                    .speed(0.5),
            )
            .on_hover_text(
                "Temperature; enters via the Bjerrum and Debye lengths. pKa values are not temperature-corrected",
            );
            ui.end_row();
        }
    });
}

/// Progress bar with a cancel button; true if cancel was clicked.
fn progress_ui(ui: &mut egui::Ui, progress: f64) -> bool {
    ui.horizontal(|ui| {
        let cancel = ui
            .button("✖")
            .on_hover_text("Cancel the calculation")
            .clicked();
        ui.add(egui::ProgressBar::new(progress as f32).show_percentage());
        cancel
    })
    .inner
}
