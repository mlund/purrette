//! Charge properties of one structure at one set of conditions.

use cgkitten::topology::Topology;
use cgkitten::{Bead, BeadType, ChargeResult, Multipole, Titration, format_topology, format_xyz};
use web_time::Duration;

use super::setup::{Conditions, Setup};
use super::structure::Structure;
use super::{SEED, advance_for};

/// A running or finished calculation.
pub struct Analysis {
    beads: Vec<Bead>,
    setup: Setup,
    conditions: Conditions,
    name: String,
    stem: String,
    chains: Vec<String>,
    state: State,
}

enum State {
    Running(Titration),
    Done(Box<Properties>),
    Failed,
}

/// Results plus ready-to-save files.
pub struct Properties {
    pub multipole: Multipole,
    /// Da.
    pub mass: f64,
    pub num_beads: usize,
    pub num_sites: usize,
    pub conditions: Conditions,
    /// Coarse-grained coordinates (XYZ).
    pub xyz: String,
    /// Faunus topology (YAML).
    pub topology: String,
    /// Base name for saved files.
    pub stem: String,
    // Kept for the 3D viewer: per-bead positions, charge variance and snapshots.
    #[allow(dead_code)]
    beads: Vec<Bead>,
    #[allow(dead_code)]
    charges: ChargeResult,
}

impl Analysis {
    pub fn start(
        structure: &Structure,
        setup: &Setup,
        chains: &[String],
        conditions: Conditions,
    ) -> Result<Self, String> {
        let calc = setup.charge_calc(conditions)?;
        setup.force_field()?; // fail now, not after the titration
        let beads = structure.beads(setup.policy, chains)?;
        Ok(Self {
            state: State::Running(calc.start(&beads, SEED)),
            beads,
            setup: setup.clone(),
            conditions,
            name: structure.name().to_owned(),
            stem: structure.stem().to_owned(),
            chains: chains.to_vec(),
        })
    }

    /// Work for about `budget`; the result once finished.
    pub fn advance(&mut self, budget: Duration) -> Result<Option<&Properties>, String> {
        // Taken out so `finish` can consume it; stays Failed if exporting errs.
        self.state = match std::mem::replace(&mut self.state, State::Failed) {
            State::Running(mut t) => {
                if advance_for(&mut t, budget) {
                    State::Done(Box::new(self.build_properties(t.finish())?))
                } else {
                    State::Running(t)
                }
            }
            other => other,
        };
        Ok(self.properties())
    }

    pub fn properties(&self) -> Option<&Properties> {
        match &self.state {
            State::Done(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running(_))
    }

    pub fn progress(&self) -> f64 {
        match &self.state {
            State::Running(t) => t.progress(),
            _ => 1.0,
        }
    }

    fn build_properties(&self, charges: ChargeResult) -> Result<Properties, String> {
        let charged = charges.apply(&self.beads);
        let topo = Topology::new(&charged, self.setup.merge_tolerance);
        let comment = format!(
            "{} | {} chains={} | {} | {}",
            crate::provenance(),
            self.name,
            chain_label(&self.chains),
            self.conditions,
            self.setup.describe(),
        );
        let xyz = format_xyz(&charged, topo.bead_names(), &comment)?;
        let multi_bead = charged.iter().any(|b| b.bead_type == BeadType::Virtual);
        let ff = self.setup.force_field()?;
        let topology =
            format!("# {comment}\n") + &format_topology(&topo, ff.as_deref(), multi_bead);
        Ok(Properties {
            multipole: charges.multipole.clone(),
            mass: self.beads.iter().map(|b| b.mass).sum(),
            num_beads: self.beads.len(),
            num_sites: self
                .beads
                .iter()
                .filter(|b| b.bead_type.is_titratable())
                .count(),
            conditions: self.conditions,
            xyz,
            topology,
            stem: self.stem.clone(),
            beads: charged,
            charges,
        })
    }
}

pub(crate) fn chain_label(chains: &[String]) -> String {
    if chains.is_empty() {
        "all".into()
    } else {
        chains.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::structure::tests::structure;

    fn finish(mut a: Analysis) -> Properties {
        loop {
            if a.advance(Duration::from_millis(50)).unwrap().is_some() {
                let State::Done(p) = a.state else {
                    unreachable!()
                };
                return *p;
            }
        }
    }

    #[test]
    fn hh_and_mc_complete_with_exports() {
        for mc_sweeps in [0, 500] {
            let setup = Setup {
                mc_sweeps,
                ..Default::default()
            };
            let a = Analysis::start(&structure(), &setup, &[], Conditions::default()).unwrap();
            let p = finish(a);
            assert!(p.num_sites > 0);
            assert!(p.mass > 0.0);
            assert!(p.xyz.contains(&crate::provenance()));
            assert!(p.topology.contains("atoms"));
            assert_eq!(p.stem, "test");
        }
    }

    #[test]
    fn bad_conditions_fail_before_running() {
        let c = Conditions {
            temperature: -1.0,
            ..Default::default()
        };
        assert!(Analysis::start(&structure(), &Setup::default(), &[], c).is_err());
    }
}
