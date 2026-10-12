# Purrette

[![Run in browser](https://img.shields.io/badge/run-in%20browser-blue)](https://mlund.github.io/purrette/)
[![CI](https://github.com/mlund/purrette/actions/workflows/ci.yml/badge.svg)](https://github.com/mlund/purrette/actions/workflows/ci.yml)

Charge properties of proteins in the browser, built on
[cgkitten](https://github.com/mlund/cgkitten): coarse-graining and Monte Carlo titration
with screened electrostatics.

- **Structure:** fetch by PDB ID, open, or drop a PDB/mmCIF file; pick chains and the
  cgkitten options. Shows net charge ⟨Z⟩, capacitance C = ⟨Z²⟩ − ⟨Z⟩², dipole and quadrupole
  moments, and saves the coarse-grained structure (.xyz) and Faunus topology (.yaml).
- **Scan:** the same quantities vs pH, ionic strength or temperature, Monte Carlo next to
  Henderson-Hasselbalch, saved as CSV.

Everything runs locally; structures never leave the browser. Results are reproducible: MC
uses a fixed seed, and saved files record the Purrette and cgkitten versions and all settings.

## Run and test locally

Needs stable Rust (see `rust-version` in `Cargo.toml`). For the browser build you also need the
wasm target and [trunk](https://trunkrs.dev):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Native:

```sh
cargo run --release                     # wgpu renderer, with plots
cargo run --release --no-default-features --features plot   # glow renderer
```

Browser:

```sh
trunk serve --release                   # http://127.0.0.1:8080
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

## Releasing

Purrette is versioned on its own; the cgkitten it uses is pinned in `Cargo.toml` and
`Cargo.lock`. Set `version` in `Cargo.toml`, commit, and push a matching tag `vX.Y.Z`.
The deploy workflow builds that tag once, attaches the build to the GitHub release, and
republishes the site with all releases at `https://mlund.github.io/purrette/vX.Y.Z/`;
the header's version menu switches between them, so a cited version keeps running the
same code.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT),
at your option.
