//! Platform file I/O. Browser reads are async, so every source (RCSB, file
//! dialog, drag-and-drop) delivers into one inbox that the UI polls each frame.

use std::sync::mpsc::{Receiver, Sender, channel};

/// File name and contents, or an error message.
pub type Loaded = Result<(String, Vec<u8>), String>;

pub struct Inbox {
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
}

impl Default for Inbox {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx }
    }
}

impl Inbox {
    pub fn poll(&self) -> Option<Loaded> {
        self.rx.try_recv().ok()
    }

    pub fn fetch(&self, ctx: &egui::Context, url: String, name: String) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let source = url.clone();
        let deliver = move |r: Result<Vec<u8>, String>| {
            let _ = tx.send(
                r.map(|data| (name, data))
                    .map_err(|e| format!("{source}: {e}")),
            );
            ctx.request_repaint();
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let r = fetch(&url).await;
            deliver(r);
        });
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let r = fetch(&url);
            deliver(r);
        });
    }

    pub fn pick(&self, ctx: &egui::Context) {
        const EXTENSIONS: [&str; 4] = ["cif", "mmcif", "pdb", "ent"];
        #[cfg(target_arch = "wasm32")]
        {
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let dialog = rfd::AsyncFileDialog::new().add_filter("Structure", &EXTENSIONS);
                if let Some(file) = dialog.pick_file().await {
                    let _ = tx.send(Ok((file.file_name(), file.read().await)));
                    ctx.request_repaint();
                }
            });
        }
        // Blocking: native dialogs must run on the main thread on macOS.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx;
            let dialog = rfd::FileDialog::new().add_filter("Structure", &EXTENSIONS);
            if let Some(path) = dialog.pick_file() {
                let name = path
                    .file_name()
                    .map_or_else(|| "structure".into(), |n| n.to_string_lossy().into_owned());
                let r = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()));
                let _ = self.tx.send(r.map(|data| (name, data)));
            }
        }
    }

    /// Take files dropped onto the window this frame.
    pub fn take_dropped(&self, ctx: &egui::Context) {
        let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for file in files {
            let name = file
                .path()
                .file_name()
                .map_or_else(|| "dropped".into(), |n| n.to_string_lossy().into_owned());
            #[cfg(not(target_arch = "wasm32"))]
            let _ = self.tx.send(file.bytes().map(|data| (name, data)));
            #[cfg(target_arch = "wasm32")]
            {
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = tx.send(file.bytes_async().await.map(|data| (name, data)));
                    ctx.request_repaint();
                });
            }
        }
    }
}

/// Offer `contents` for download (browser) or a save dialog (native).
pub fn save(name: &str, contents: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Err(e) = download(name, contents) {
        log::error!("download of {name} failed: {e:?}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
        match std::fs::write(&path, contents) {
            Ok(()) => log::info!("saved {}", path.display()),
            Err(e) => log::error!("{}: {e}", path.display()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    /// Large assemblies exceed ureq's 10 MB default.
    const MAX_BYTES: u64 = 500 << 20;
    ureq::get(url)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "not a Response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let buf = JsFuture::from(resp.array_buffer().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

#[cfg(target_arch = "wasm32")]
fn download(name: &str, contents: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let parts = js_sys::Array::of1(&contents.into());
    let blob = web_sys::Blob::new_with_str_sequence(&parts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url)
}
