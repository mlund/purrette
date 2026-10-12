//! egui app that runs natively and in the browser.
//!
//! Domain logic lives in [`model`]; `app` is a thin UI over it. Browser builds
//! enter through [`start`], which wasm-bindgen calls on module load.

mod app;
pub mod model;

pub use app::App;

/// Window/tab title, also used by the native runner.
pub const TITLE: &str = "eframe wasm template";

/// Crate version and git revision, e.g. `0.1.0 (v0.1.0-3-gabc123d-dirty)`.
/// Show it wherever results leave the app, so they can be reproduced.
pub fn provenance() -> String {
    format!("{} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_REVISION"))
}

/// glow is the lighter default; `wgpu` opts into WebGPU (and GPU compute).
#[cfg(feature = "wgpu")]
pub const RENDERER: eframe::Renderer = eframe::Renderer::Wgpu;
#[cfg(not(feature = "wgpu"))]
pub const RENDERER: eframe::Renderer = eframe::Renderer::Glow;

/// Routes `log` macros to the in-app log window. Call once, before the app starts.
pub fn init_logging() {
    if let Err(err) = egui_logger::builder()
        .max_level(log::LevelFilter::Info)
        .init()
    {
        eprintln!("logger already set: {err}");
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Hands the `<canvas id="the_canvas_id">` from index.html to eframe.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    use wasm_bindgen::JsCast as _;

    console_error_panic_hook::set_once();
    init_logging();

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = document
        .get_element_by_id("the_canvas_id")
        .ok_or_else(|| JsValue::from_str("canvas element not found"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("element is not a <canvas>"))?;
    let loading = document.get_element_by_id("loading");

    let options = eframe::WebOptions {
        renderer: RENDERER,
        ..Default::default()
    };
    wasm_bindgen_futures::spawn_local(async move {
        let result = eframe::WebRunner::new()
            .start(canvas, options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
            .await;
        match (result, loading) {
            (Ok(()), Some(loading)) => loading.remove(),
            (Err(err), Some(loading)) => {
                loading.set_text_content(Some(&format!("failed to start: {err:?}")))
            }
            (Err(err), None) => log::error!("eframe failed to start: {err:?}"),
            (Ok(()), None) => {}
        }
    });
    Ok(())
}
