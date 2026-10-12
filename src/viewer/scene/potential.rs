//! Screened Coulomb (Debye–Hückel) potential of the bead charges, the same model the
//! titration uses, so colours and titration agree.

use glam::Vec3;

use super::grid::CellGrid;
use super::{Pos, SceneInput};

/// Screened charges beyond this many Debye lengths add < e⁻³ of a near one, invisible in
/// the colour map; skipping them makes large assemblies tractable.
const CUTOFF_DEBYE_LENGTHS: f64 = 3.0;
/// Couplings weaker than this (kT/e per unit noise) are dropped; well below what a
/// colour map shows.
const MIN_COUPLING: f64 = 1e-3;
/// Strongest couplings kept per point: bounds memory and per-frame work even without salt,
/// where every site reaches every vertex; the nearest sites dominate the pulse anyway.
const MAX_COUPLINGS: usize = 32;

struct Source {
    position: Vec3,
    charge: f64,
    sigma: f64,
    /// Bead index, so the surface pulses with the same noise as the bead.
    bead: u32,
}

pub(super) struct Field {
    bjerrum: f64,
    /// Inverse Debye length (1/Å); 0 without salt.
    kappa: f64,
    sources: Vec<Source>,
    /// With screening: sources by cell of edge `cutoff`; without, every charge matters.
    grid: Option<(f32, CellGrid)>,
}

impl Field {
    pub fn new(input: &SceneInput) -> Self {
        let sources: Vec<Source> = input
            .beads
            .iter()
            .zip(&input.charge_variance)
            .enumerate()
            .filter(|(_, (b, v))| b.charge != 0.0 || **v > 0.0)
            .map(|(bead, (b, v))| Source {
                position: b.pos(),
                charge: b.charge,
                sigma: v.sqrt(),
                bead: bead as u32,
            })
            .collect();
        let kappa = if input.debye_length.is_finite() && input.debye_length > 0.0 {
            1.0 / input.debye_length
        } else {
            0.0
        };
        let grid = (kappa > 0.0).then(|| {
            let cutoff = (CUTOFF_DEBYE_LENGTHS / kappa) as f32;
            (
                cutoff,
                CellGrid::new(sources.iter().map(|s| s.position), cutoff),
            )
        });
        Self {
            bjerrum: input.bjerrum_length,
            kappa,
            sources,
            grid,
        }
    }

    /// Sources that can matter at `p`.
    fn near(&self, p: Vec3) -> Box<dyn Iterator<Item = &Source> + '_> {
        match &self.grid {
            None => Box::new(self.sources.iter()),
            Some((cutoff, grid)) => Box::new(
                grid.near(p)
                    .map(|i| &self.sources[i])
                    .filter(move |s| p.distance(s.position) <= *cutoff),
            ),
        }
    }

    /// λ_B e^{−κr}/r, the potential (kT/e) of a unit charge at distance r (Å).
    fn kernel(&self, r: f32) -> f64 {
        let r = f64::from(r).max(1e-6);
        self.bjerrum * (-self.kappa * r).exp() / r
    }

    /// Mean potential and its standard deviation (kT/e) at `p`, sites independent.
    pub fn at(&self, p: Vec3) -> (f64, f64) {
        let (mut mean, mut var) = (0.0, 0.0);
        for s in self.near(p) {
            let k = self.kernel(p.distance(s.position));
            mean += s.charge * k;
            var += (s.sigma * k).powi(2);
        }
        (mean, var.sqrt())
    }

    /// The strongest per-unit-noise contributions σⱼKⱼ(p) of fluctuating sites, as
    /// (bead, coupling), each divided by `scale`.
    pub fn couplings(&self, p: Vec3, scale: f64) -> Vec<(u32, f32)> {
        let mut c: Vec<(u32, f64)> = self
            .near(p)
            .filter(|s| s.sigma > 0.0)
            .map(|s| (s.bead, s.sigma * self.kernel(p.distance(s.position))))
            .filter(|(_, c)| c.abs() > MIN_COUPLING)
            .collect();
        if c.len() > MAX_COUPLINGS {
            c.select_nth_unstable_by(MAX_COUPLINGS, |a, b| b.1.abs().total_cmp(&a.1.abs()));
            c.truncate(MAX_COUPLINGS);
        }
        c.into_iter()
            .map(|(b, c)| (b, (c / scale) as f32))
            .collect()
    }
}
