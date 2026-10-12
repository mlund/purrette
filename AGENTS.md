# Agent instructions

Applies to all coding agents (Claude Code, Codex, ...). `.claude/CLAUDE.md` is a symlink to this file.
Read it as a skill: the rules below always apply, and the *Recipes* say how to add common features when asked.

<!-- template:start -->
## New project from this template

If `Cargo.toml` still says `name = "eframe_wasm_template"`, the repo was just created
from the template. Before anything else, ask the user for a crate name and title, then run:

```sh
scripts/rename.sh <snake_case_crate_name> "<App Title>"
```

It renames the crate, lib and bin, the repo slug in URLs, and the title in
`src/lib.rs` and `index.html`. It also removes this section and the template notes in
`README.md`, then deletes itself. Replace the demo `src/model/` with the real domain module,
and rewrite `README.md` for the new app.
<!-- template:end -->

Upon startup, load these two skills:

- Deep modules: https://deepwiki.com/mattpocock/skills/4.1.4-designing-deep-modules
- TDD: https://raw.githubusercontent.com/mattpocock/skills/refs/heads/main/skills/engineering/tdd/SKILL.md

## Layout

- `src/lib.rs`: wasm entry (`start`), logging, renderer choice, `provenance()`.
- `src/main.rs`: native entry.
- `src/app.rs`: the egui UI. Keep it thin: it reads and drives the domain module, holds no domain logic.
- `src/model/`: domain logic, testable without a UI. Replace the demo oscillator with the real model.
- `build.rs`: embeds `git describe` as `GIT_REVISION`.

## Design

- Prefer deep modules with a minimal public API (load the skill above).
- Make misuse hard: newtypes, enums, type-state over documented conventions.
- Internal state is the module's job, not the caller's; hide it behind the interface.
- Avoid panics under normal use; return errors. Surface them in the UI or via `log::warn!`.
- Code must build for both native and `wasm32-unknown-unknown`. Gate platform code with
  `#[cfg(target_arch = "wasm32")]`; no threads, filesystem or blocking I/O on wasm.
- I/O must work on macOS, Linux and Windows.

## Features

- `plot` (default): `egui_plot` charts. Code using it must compile without it (`#[cfg(feature = "plot")]`, with a fallback).
- `wgpu` (off): renders with wgpu/WebGPU instead of glow. It also gives GPU **compute**:
  `cc.wgpu_render_state` (`egui_wgpu::RenderState`) holds the `device` and `queue`.
  Use them for compute shaders (WGSL) and paint results with an egui paint callback or texture.
  Keep a CPU path for builds without the feature, and test numerics on the CPU path against the GPU path.
  WebGPU needs a secure context (`https` or `127.0.0.1`) and is still missing in some browsers.

## UI

- Every interactive element gets a tooltip (`.on_hover_text(...)`): say what it does and, where relevant, units or valid range.
- Use unicode for symbols in labels (ω, ζ, Å, ⟨x⟩).
- Show `crate::provenance()` wherever results leave the app (exported files, screenshots, the About area), for reproducibility.
- Log through the `log` macros; messages appear in the in-app Log window (`egui_logger`).

## Rust

- Idiomatic Rust, descriptive names.
- Use the LSP tool / rust-analyzer where possible.

## Docs and comments

- Terse. Explain *why*, not *what*; minimal jargon; unicode for math.
- Doc comments on public items say what a caller needs to know, not how it works inside.
- No comments restating the code.

## Testing

- Test-driven: one failing test, then the code that passes it (load the skill above).
- Test through the public interface. Expected values come from analytical results, literals or the
  spec, never from re-implementing the logic in the test.
- No weak tests. Before adding one, ask "which line of *our* code fails if this regresses?" If the answer
  is a std/egui/rng line or nothing, don't write it. Avoid asserting `is_finite()`, `> 0` or "doesn't panic"
  where a specific value is knowable.
- UI code (`app.rs`) is not unit tested; keep logic out of it so it needs none.

## Recipes

Apply these when the user asks for the feature, not by default. Each lists the pieces that must
exist so native and web builds behave alike.

### Save and load settings as JSON (reproducibility)

Use when results must be reproducible or shareable: a file that rebuilds the exact app state.
Reference implementation: [cppm-maker](https://github.com/mlund/cppm-maker) (`src/design.rs`, `src/download.rs`).

- Deps: `serde` (derive), `serde_json` and `rfd`. wasm-only: `js-sys`, plus the `web-sys` features `Blob`, `Url` and `HtmlAnchorElement`.
- One `Settings` type in the domain module (not `app.rs`), `#[derive(Serialize, Deserialize)]`:
  - a `format: u32` field, checked on load and bumped when an old file would be misread;
  - a `generator` field filled from `crate::provenance()`;
  - `#[serde(deny_unknown_fields)]`, so typos are errors rather than silently ignored;
  - `#[serde(default)]` only on fields added later, so older files still load;
  - units in doc comments (`/// Radius, Å.`); validate on load through the same constructors the UI uses.
- `to_json()` writes pretty JSON, because people read these files too. `from_json(&str) -> Result<_, SettingsError>` never panics.
- Save and load through `io.rs` (see *Load input files* below for loading): native `rfd::FileDialog`
  save. Web save: Blob → object URL → temporary `<a download>`, appended to `<body>`, clicked, removed,
  and the URL revoked after a timeout (Safari).
- UI: "Save settings" and "Load settings" buttons with tooltips, and dropping a file loads it too. Report the result in a status line or the log.
- Tests: a round-trip test (`from_json(to_json(x)) == x`) and an unknown-field rejection test.
  Also test a frozen JSON literal of the current format, so an accidental layout change fails.
- Tell the user they also get an exported-results variant almost for free: the same file plus computed outputs.

### Load input files: from disk, by URL, or by drag-and-drop

Use when the app reads user data (structures, tables, settings). Reference: [duello-web](https://github.com/mlund/duello-web)
(`src/app.rs`: `pick_file`, `fetch_pdb`, `fetch_url`). Offer all three; each yields `(name, Vec<u8>)` for one
parse function in the domain module.

- **File picker**: `rfd::AsyncFileDialog` works natively and in the browser. Run it on `spawn_local` (web) or a thread
  (native), then `file.read().await`.
- **URL**: prefer [`ehttp`](https://crates.io/crates/ehttp), a single API over browser `fetch` and native HTTP.
  duello-web's alternative is `web_sys` `fetch` on web plus `ureq` on a native thread. In the browser the
  server must send CORS headers; say so in the error. Keep IDs and URLs the user typed as part of the settings, for reproducibility.
- **Drag-and-drop**: read `ui.input(|i| i.raw.dropped_files.clone())`. Web gives `bytes`, native gives `path`; handle both.
  Show a hint while hovering (`i.raw.hovered_files`).
- Async results come back to `ui()` over an `mpsc` channel that is polled each frame. Call
  `ctx.request_repaint()` when sending, so the result shows without waiting for input.
- Errors (HTTP status, parse failure) go to the status line and `log::error!`, never a panic.
- Tests: the parse function only, on small inline fixtures. Don't test network or dialogs.

### Custom graphics (2D/3D views)

Use when the app draws its own scene rather than a plot. References: cppm-maker `src/view3d.rs` (3D sphere, picking)
and hot-gibbs `src/app.rs` (2D boxes).

- **Boundary**: a view module that borrows the model read-only and returns intents (`enum ViewEvent { Picked(Id), … }`).
  `app.rs` applies them. Projection, colours and hit testing stay inside the view, so it can later be replaced by a GPU renderer.
  The model exposes plain data (`[u8; 3]` colours, positions), not egui types.
- **Start simple**: `egui::Painter` shapes (`circle`, `line_segment`, …) are enough for a few hundred objects and need no
  backend code. For thousands, build one `egui::Mesh` per frame. Beyond that, use a `wgpu` paint callback (see GPU compute).
- **Canvas**: `let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag())`, then draw with `ui.painter_at(rect)` so nothing spills over panels.
- **Transforms**: use a uniform scale, `(w / bx).min(h / by)`, so shapes keep their aspect ratio, and centre the result.
  Physics has y up and the screen has y down, so use one `emath::RectTransform` (world → screen) rather than flipping
  coordinates by hand. Make radii at least 1 px.
- **3D**: the camera is `yaw`, `pitch`, `zoom`. Drag rotates, with pitch clamped just short of ±π/2 so it can't flip.
  Scroll zooms (`smooth_scroll_delta`, clamped). Perspective uses `focal / (eye − z).max(ε)` to avoid dividing by zero,
  with focal length from `rect.width().min(rect.height())`.
  Draw back to front (painter's algorithm, sorted with `f32::total_cmp`) and dim or thin far objects as a depth cue.
- **Picking**: on hover or click, take the front-most object whose screen radius contains the pointer. No GPU readback is needed.
  Show `response.on_hover_text_at_pointer(...)` for the hovered object, and use `response.context_menu` for actions.
  Tooltips apply to drawn objects too.
- **Theme**: take neutral colours (background, edges, outlines, selection) from `ui.visuals()` so dark and light mode
  both work. Hard-code only colours that carry meaning, kept as shared constants with the plot legend.
  Don't hard-code a dark background.
- **Repaint**: call `request_repaint()` only while something animates. Advance the simulation by `stable_dt`, not per frame, so speed doesn't depend on frame rate.
- **Tests**: unit-test the pure projection, transform and picking functions (e.g. a point at the centre maps to the rect centre, and the front object wins a pick). They are cheap, and errors there are otherwise found only by eye.

### GPU compute (`wgpu` feature)

Use for heavy, data-parallel numerics. See *Features* above for the hooks. Put the WGSL shader and its buffers in a
module behind `cfg(feature = "wgpu")` that has the same interface as the CPU implementation. Choose between
them at startup, depending on whether `cc.wgpu_render_state` is `Some`. Test the GPU path against the CPU
path, with a tolerance.

### Randomness

Add `rand` and a seedable generator (e.g. `rand_chacha`), and seed explicitly from a `seed` setting so runs
reproduce. With `rand` default features off (no `os_rng`), no `getrandom` wasm backend is needed. If OS entropy
is needed, add `getrandom` with its wasm/JS backend.

### Long computations

Keep the UI responsive. Run work in steps per frame (`request_repaint`) or on a worker: `std::thread` natively, chunked `spawn_local` on web.
Send results back over a channel. Never block in `ui()`.

## Workflow

- Ask before committing.
- Keep commit, PR and issue messages brief. Never append co-authorship lines, "Generated with" banners or similar.
- Finish complex tasks with `/code-review`, then `/simplify`.

## Before committing

- `cargo fmt`
- `cargo clippy --tests --no-deps --all-features -- -D warnings`
- `cargo test`
- `cargo check --target wasm32-unknown-unknown`, both with and without `--features wgpu`.
