//! Load a structure, choose options as on the cgkitten command line, see results.

use web_time::Duration;

use super::{conditions_ui, progress_ui, viewer};
use crate::app::Session;
use crate::io::{self, Inbox};
use crate::model::{
    Analysis, Conditions, ForceField, Policy, Properties, Scaling, Setup, default_pka, rcsb_url,
};

/// e·Å → Debye.
const DEBYE_PER_EANGSTROM: f64 = 4.803_204;

#[derive(Default)]
pub struct StructureTab {
    pdb_id: String,
    analysis: Option<Analysis>,
    /// Inputs behind `analysis`, to flag results as stale after edits.
    ran_with: Option<(Setup, Vec<String>, Conditions)>,
    error: Option<String>,
}

impl StructureTab {
    /// New structure: compute straight away so results appear without a click.
    pub fn loaded(&mut self, session: &Session) {
        self.error = None;
        self.run(session);
    }

    pub fn set_error(&mut self, e: String) {
        self.error = Some(e);
    }

    /// True while work remains.
    pub fn advance(&mut self, budget: Duration) -> bool {
        let Some(analysis) = &mut self.analysis else {
            return false;
        };
        match analysis.advance(budget) {
            Ok(done) => done.is_none(),
            Err(e) => {
                log::error!("{e}");
                self.error = Some(e);
                self.analysis = None;
                false
            }
        }
    }

    fn run(&mut self, session: &Session) {
        let Some(structure) = &session.structure else {
            return;
        };
        let chains = session.chains();
        match Analysis::start(structure, &session.setup, &chains, session.conditions) {
            Ok(a) => {
                self.analysis = Some(a);
                self.ran_with = Some((session.setup.clone(), chains, session.conditions));
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn is_stale(&self, session: &Session) -> bool {
        self.ran_with.as_ref().is_some_and(|(setup, chains, c)| {
            *setup != session.setup || *chains != session.chains() || *c != session.conditions
        })
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, session: &mut Session, inbox: &Inbox) {
        egui::Panel::left("structure_controls")
            .resizable(true)
            .default_size(290.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui, session, inbox));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let properties = self.analysis.as_ref().and_then(Analysis::properties);
            egui::Panel::top("properties").show(ui, |ui| {
                properties_ui(ui, properties, self.is_stale(session));
            });
            egui::CentralPanel::default().show(ui, |ui| viewer::ui(ui, properties));
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui, session: &mut Session, inbox: &Inbox) {
        ui.heading("Structure");
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.pdb_id)
                    .hint_text("PDB ID")
                    .desired_width(60.0)
                    .char_limit(4),
            );
            edit.on_hover_text("Four-character RCSB PDB ID, e.g. 2CDS");
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let valid = rcsb_url(&self.pdb_id);
            let fetch = ui
                .add_enabled(valid.is_ok(), egui::Button::new("⬇ Fetch"))
                .on_hover_text("Download the mmCIF file from RCSB")
                .clicked();
            if let (true, Ok((url, name))) = (fetch || enter, valid) {
                log::info!("fetching {url}");
                inbox.fetch(ui.ctx(), url, name);
            }
            if ui
                .button("📂 Open…")
                .on_hover_text("Open a local PDB or mmCIF file (or drop one on the window)")
                .clicked()
            {
                inbox.pick(ui.ctx());
            }
        });
        if let Some(s) = &session.structure {
            ui.label(egui::RichText::new(s.name()).strong());
            if s.chains().len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Chains:");
                    for chain in s.chains() {
                        let mut on = session.selected.contains(chain);
                        if ui
                            .checkbox(&mut on, chain.as_str())
                            .on_hover_text("Include this chain")
                            .changed()
                        {
                            if on {
                                session.selected.insert(chain.clone());
                            } else {
                                session.selected.remove(chain);
                            }
                        }
                    }
                });
            }
        }
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }

        ui.separator();
        ui.heading("Conditions");
        conditions_ui(ui, &mut session.conditions, None);

        ui.separator();
        setup_ui(ui, &mut session.setup);

        ui.separator();
        let running = self.analysis.as_ref().filter(|a| a.is_running());
        if let Some(a) = running {
            if progress_ui(ui, a.progress()) {
                self.analysis = None;
                self.ran_with = None;
            }
        } else {
            let can_run = session.structure.is_some() && !session.selected.is_empty();
            if ui
                .add_enabled(can_run, egui::Button::new("▶ Calculate"))
                .on_hover_text("Coarse-grain and titrate with the settings above")
                .on_disabled_hover_text("Load a structure and select at least one chain")
                .clicked()
            {
                self.run(session);
            }
        }

        let p = self.analysis.as_ref().and_then(Analysis::properties);
        ui.horizontal(|ui| {
            let save = |ui: &mut egui::Ui, label: &str, tip: &str| {
                ui.add_enabled(p.is_some(), egui::Button::new(label))
                    .on_hover_text(tip)
                    .clicked()
            };
            let xyz = save(
                ui,
                "💾 Structure",
                "Coarse-grained coordinates and charges (.xyz)",
            );
            let top = save(
                ui,
                "💾 Topology",
                "Faunus topology with force-field parameters (.yaml)",
            );
            if let Some(p) = p {
                if xyz {
                    io::save(&format!("{}.xyz", p.stem), &p.xyz);
                }
                if top {
                    io::save(&format!("{}_topology.yaml", p.stem), &p.topology);
                }
            }
        });
    }
}

fn setup_ui(ui: &mut egui::Ui, setup: &mut Setup) {
    egui::Grid::new("setup").num_columns(2).show(ui, |ui| {
        ui.label("Coarse graining");
        egui::ComboBox::from_id_salt("policy")
            .selected_text(setup.policy.label())
            .show_ui(ui, |ui| {
                for p in Policy::ALL {
                    ui.selectable_value(&mut setup.policy, p, p.label());
                }
            })
            .response
            .on_hover_text("Multi-bead adds a side-chain bead at each titratable charge centre");
        ui.end_row();

        ui.label("MC sweeps");
        ui.add(
            egui::DragValue::new(&mut setup.mc_sweeps)
                .range(0..=1_000_000)
                .speed(100),
        )
        .on_hover_text(
            "Monte Carlo sweeps per titratable site; 0 = Henderson-Hasselbalch (no site coupling)",
        );
        ui.end_row();

        ui.label("Force field");
        egui::ComboBox::from_id_salt("force_field")
            .selected_text(setup.force_field.label())
            .show_ui(ui, |ui| {
                for f in ForceField::ALL {
                    ui.selectable_value(&mut setup.force_field, f, f.label());
                }
            })
            .response
            .on_hover_text("Pair potential in the exported topology; does not affect charges");
        ui.end_row();

        ui.label("Hydrophobic scaling");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("scaling")
                .width(50.0)
                .selected_text(setup.scaling.label())
                .show_ui(ui, |ui| {
                    for s in Scaling::ALL {
                        ui.selectable_value(&mut setup.scaling, s, s.label());
                    }
                })
                .response
                .on_hover_text("Scale λ (CALVADOS only) or ε of hydrophobic pair interactions");
            ui.add_enabled(
                setup.scaling != Scaling::None,
                egui::DragValue::new(&mut setup.scaling_factor)
                    .range(0.0..=5.0)
                    .speed(0.01),
            )
            .on_hover_text("Scaling factor");
        });
        ui.end_row();

        ui.label("Merge tolerance");
        ui.add(
            egui::DragValue::new(&mut setup.merge_tolerance)
                .range(0.0..=1.0)
                .speed(0.005),
        )
        .on_hover_text(
            "Relative charge difference within which titratable sites share a topology type",
        );
        ui.end_row();
    });

    egui::CollapsingHeader::new("pKa values")
        .id_salt("pka")
        .show(ui, |ui| {
            egui::Grid::new("pka_grid").num_columns(4).show(ui, |ui| {
                for (i, (name, pka)) in setup.pka.iter_mut().enumerate() {
                    ui.label(*name);
                    ui.add(egui::DragValue::new(pka).range(-5.0..=20.0).speed(0.01))
                        .on_hover_text(format!("Intrinsic pKa of {name}"));
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            if ui
                .button("⟲ Defaults")
                .on_hover_text("Thurlkill et al. 2006, as in Faunus")
                .clicked()
            {
                setup.pka = default_pka();
            }
        });
}

fn properties_ui(ui: &mut egui::Ui, p: Option<&Properties>, stale: bool) {
    let Some(p) = p else {
        ui.weak("No results yet");
        return;
    };
    let m = &p.multipole;
    ui.horizontal_wrapped(|ui| {
        let item = |ui: &mut egui::Ui, name: &str, value: String, tip: &str| {
            ui.vertical(|ui| {
                ui.weak(name);
                ui.label(egui::RichText::new(value).size(18.0).strong());
            })
            .response
            .on_hover_text(tip);
            ui.add_space(12.0);
        };
        item(
            ui,
            "Net charge ⟨Z⟩",
            format!("{:+.2} e", m.charge),
            "Mean net charge",
        );
        item(
            ui,
            "Capacitance C",
            format!("{:.2} e²", m.capacitance()),
            "⟨Z²⟩ − ⟨Z⟩²: how readily the net charge responds to a nearby charge",
        );
        item(
            ui,
            "Dipole ⟨μ⟩",
            format!("{:.0} ± {:.0} e·Å", m.dipole, m.dipole_std()),
            &format!(
                "About the centre of mass; ± is the fluctuation. {:.0} D",
                m.dipole * DEBYE_PER_EANGSTROM
            ),
        );
        item(
            ui,
            "Quadrupole ⟨‖Θ‖⟩",
            format!("{:.0} ± {:.0} e·Å²", m.quadrupole, m.quadrupole_std()),
            "Frobenius norm of the traceless quadrupole about the centre of mass",
        );
        item(
            ui,
            "Mass",
            format!("{:.1} kDa", p.mass / 1000.0),
            "Sum of atom masses in the selected chains",
        );
        item(
            ui,
            "Beads",
            format!("{} ({} titratable)", p.num_beads, p.num_sites),
            "Coarse-grained beads",
        );
    });
    ui.weak(format!("at {}", p.conditions));
    if stale {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Settings changed — press Calculate to update",
        );
    }
}
