//! 3D view of the coarse-grained molecule. Placeholder that reserves the
//! central area so the layout does not shift when the viewer lands.
//!
//! Planned: an `egui_wgpu` paint callback drawing pseudo-solvent spheres
//! (voronota-ltr) as impostors, coloured by the screened potential
//! φ(r) = λ_B Σ qᵢ(t) e^{−κr}/r, with qᵢ(t) interpolated between titration
//! snapshots so the pulsing reflects sampled, correlated site fluctuations.

use crate::model::Properties;

pub fn ui(ui: &mut egui::Ui, properties: Option<&Properties>) {
    let rect = ui.available_rect_before_wrap();
    let visuals = ui.visuals();
    ui.painter()
        .rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let text = match properties {
        Some(_) => "3D electrostatic potential view — coming soon",
        None => "Load a structure: PDB ID, Open…, or drop a PDB/mmCIF file here",
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(15.0),
        visuals.weak_text_color(),
    );
    ui.allocate_rect(rect, egui::Sense::hover());
}
