# eframe wasm template

[![Run in browser](https://img.shields.io/badge/run-in%20browser-blue)](https://mlund.github.io/eframe-wasm-template/)
[![CI](https://github.com/mlund/eframe-wasm-template/actions/workflows/ci.yml/badge.svg)](https://github.com/mlund/eframe-wasm-template/actions/workflows/ci.yml)

<!-- template:start -->
Template for [egui](https://github.com/emilk/egui) apps that run natively and in the browser
(wasm). It's published on GitHub Pages, and every release stays online so older versions can be run again.

What you get:

- eframe app with a thin UI (`src/app.rs`) over a tested domain module (`src/model/`)
- Features: `plot` (egui_plot, on by default) and `wgpu` (WebGPU renderer and GPU compute, off by default)
- In-app log window ([egui_logger](https://crates.io/crates/egui_logger))
- Version and git revision shown in the app (`build.rs`)
- CI (fmt, clippy, tests, wasm build) and versioned GitHub Pages deploys
- [`AGENTS.md`](AGENTS.md): design, testing and style rules for coding agents, plus recipes for
  JSON settings I/O, loading files (disk, URL, drag-and-drop), custom 2D/3D graphics, GPU compute,
  seeded randomness and long computations

### Start a new app

```sh
gh repo create my-app --template mlund/eframe-wasm-template --private --clone
cd my-app
scripts/rename.sh my_app "My App"
```

Or click *Use this template* on GitHub. `rename.sh` renames the crate and title, removes these
template notes and deletes itself. Then replace `src/model/` and rewrite this README.
<!-- template:end -->

## Run and test locally

Needs stable Rust (see `rust-version` in `Cargo.toml`). For the browser build you also need the
wasm target and [trunk](https://trunkrs.dev):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Native:

```sh
cargo run --release                     # glow renderer, with plots
cargo run --release --features wgpu     # wgpu renderer
cargo run --release --no-default-features   # without plots
```

Browser:

```sh
trunk serve --release                   # http://127.0.0.1:8080
trunk serve --release --features wgpu
trunk build --release                   # static bundle in dist/
```

- Use `127.0.0.1`, not `localhost`. Safari only allows WebGPU in a secure context.
- Add `--no-autoreload` if a rebuild should not reload the page and discard what's on screen.

Tests and checks, as run by CI:

```sh
cargo test
cargo fmt --check
cargo clippy --tests --no-deps --all-features -- -D warnings
cargo check --target wasm32-unknown-unknown --all-features
```

## Publishing on GitHub Pages

The *Pages* workflow builds with trunk and commits the result to the `gh-pages` branch.

- **Release:** push a tag `vX.Y.Z` (bump `version` in `Cargo.toml` first), or create a GitHub release.
  The tag is published to `https://<user>.github.io/<repo>/vX.Y.Z/`, and the site root redirects to the newest release.
- **Old versions:** a release folder is never overwritten, so a cited URL keeps running the same code.
  All versions are listed at `/<repo>/versions.html`, which the app links to.
- **Preview:** run the workflow manually (*Actions → Pages → Run workflow*) to publish any branch to `/<repo>/dev/`.

```sh
git tag v0.1.0 && git push origin v0.1.0
```

One-time setup, after the first deploy has created the branch:
*Settings → Pages → Build and deployment → Deploy from a branch → `gh-pages` / root*.
Pages on private repos needs a paid GitHub plan.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT),
at your option.
