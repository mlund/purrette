use std::collections::BTreeSet;

use web_time::Duration;

use crate::io::Inbox;
use crate::model::{Conditions, Setup, Structure};
use crate::ui::{ScanTab, StructureTab};
use crate::viewer::Viewer;

/// Compute budget per job and frame; two jobs still leave room for 60 Hz drawing.
const FRAME_BUDGET: Duration = Duration::from_millis(7);

/// Inputs shared by both tabs: the scan reuses what the structure tab set up.
#[derive(Default)]
pub struct Session {
    pub structure: Option<Structure>,
    /// Chains the user ticked.
    pub selected: BTreeSet<String>,
    pub setup: Setup,
    pub conditions: Conditions,
}

impl Session {
    /// Selected chains; empty means all, which keeps file headers short.
    pub fn chains(&self) -> Vec<String> {
        match &self.structure {
            Some(s) if self.selected.len() < s.chains().len() => {
                self.selected.iter().cloned().collect()
            }
            _ => Vec::new(),
        }
    }

    fn load(&mut self, name: String, data: Vec<u8>) -> Result<(), String> {
        let structure = Structure::parse(name, data)?;
        log::info!(
            "loaded {} (chains {})",
            structure.name(),
            structure.chains().join(", ")
        );
        self.selected = structure.chains().iter().cloned().collect();
        self.structure = Some(structure);
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Structure,
    Scan,
}

pub struct App {
    tab: Tab,
    session: Session,
    inbox: Inbox,
    structure_tab: StructureTab,
    scan_tab: ScanTab,
    show_log: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The 3D viewer will take device and queue from here.
        #[cfg(feature = "wgpu")]
        if let Some(rs) = &cc.wgpu_render_state {
            log::info!("wgpu adapter: {:?}", rs.adapter.get_info().name);
        }

        // egui's proportional font lacks ⟨ ⟩ ‖; the bundled monospace Hack has them.
        let mut fonts = egui::FontDefinitions::default();
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("Hack".to_owned());
        }
        cc.egui_ctx.set_fonts(fonts);

        log::info!(
            "{} {} (cgkitten {})",
            crate::TITLE,
            crate::provenance(),
            cgkitten::VERSION
        );
        Self {
            tab: Tab::Structure,
            session: Session::default(),
            inbox: Inbox::default(),
            structure_tab: StructureTab::new(Viewer::new(cc)),
            scan_tab: ScanTab::default(),
            show_log: false,
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.inbox.take_dropped(ctx);
        while let Some(loaded) = self.inbox.poll() {
            match loaded.and_then(|(name, data)| self.session.load(name, data)) {
                Ok(()) => {
                    self.scan_tab.clear();
                    self.structure_tab.loaded(&self.session);
                }
                Err(e) => {
                    log::error!("{e}");
                    self.structure_tab.set_error(e);
                }
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Structure, "🔬 Structure")
                .on_hover_text("Load a structure and compute its charge properties");
            ui.add_enabled_ui(self.session.structure.is_some(), |ui| {
                ui.selectable_value(&mut self.tab, Tab::Scan, "📈 Scan")
                    .on_hover_text(
                        "Charge, capacitance, dipole and quadrupole vs pH, salt or temperature",
                    )
                    .on_disabled_hover_text("Load a structure first");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.show_log, "📜 Log")
                    .on_hover_text("Show log messages");
                ui.label(egui::RichText::new(crate::provenance()).weak())
                    .on_hover_text(format!(
                        "Purrette version (git revision); cgkitten {}",
                        cgkitten::VERSION
                    ));
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);

        egui::Panel::top("tabs").show(ui, |ui| self.top_bar(ui));
        match self.tab {
            Tab::Structure => self.structure_tab.ui(ui, &mut self.session, &self.inbox),
            Tab::Scan => self.scan_tab.ui(ui, &mut self.session),
        }
        // Jobs keep running while the other tab is shown.
        let structure_busy = self.structure_tab.advance(FRAME_BUDGET);
        let scan_busy = self.scan_tab.advance(FRAME_BUDGET);
        if structure_busy || scan_busy {
            ctx.request_repaint();
        }

        egui::Window::new("Log")
            .open(&mut self.show_log)
            .show(&ctx, |ui| egui_logger::logger_ui().show(ui));
    }
}
