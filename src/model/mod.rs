//! Domain logic, free of UI so it can be tested natively.
//!
//! Titrations run in time-boxed chunks so the browser's single thread can keep
//! drawing; callers pass a per-frame budget.

mod analysis;
mod scan;
mod setup;
mod structure;

pub use analysis::{Analysis, Properties};
pub use scan::{Axis, Range, Row, Scan};
pub use setup::{Conditions, ForceField, Policy, Scaling, Setup, default_pka};
pub use structure::{Structure, rcsb_url};

use cgkitten::Titration;
use web_time::{Duration, Instant};

/// Fixed so results are reproducible across runs and machines.
const SEED: u64 = 0x5EED_CA75;

/// Sweeps between clock checks; small enough to respect a ~10 ms budget.
const CHUNK: usize = 10;

/// True once finished.
fn advance_for(t: &mut Titration, budget: Duration) -> bool {
    let start = Instant::now();
    loop {
        if t.advance(CHUNK) {
            return true;
        }
        if start.elapsed() >= budget {
            return false;
        }
    }
}
