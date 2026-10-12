#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use eframe_wasm_template::{App, RENDERER, TITLE};

    eframe_wasm_template::init_logging();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 720.0]),
        renderer: RENDERER,
        ..Default::default()
    };
    eframe::run_native(TITLE, options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

// The browser entry point is `start` in lib.rs.
#[cfg(target_arch = "wasm32")]
fn main() {}
