//! Charge properties along one condition axis.
//!
//! Henderson-Hasselbalch is always computed alongside MC: it is free and shows
//! how much site-site coupling matters.

use std::fmt::Write as _;

use cgkitten::{Bead, ChargeCalc, Multipole, Titration};
use web_time::Duration;

use super::analysis::chain_label;
use super::setup::{Conditions, Setup};
use super::structure::Structure;
use super::{SEED, advance_for};

/// Most points a scan may have; guards against a mistyped step.
const MAX_POINTS: usize = 500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Ph,
    Salt,
    Temperature,
}

impl Axis {
    pub const ALL: [Self; 3] = [Self::Ph, Self::Salt, Self::Temperature];

    /// Axis label with unit.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ph => "pH",
            Self::Salt => "I (mol/L)",
            Self::Temperature => "T (K)",
        }
    }

    pub fn default_range(self) -> Range {
        match self {
            Self::Ph => Range::new(2.0, 12.0, 0.25),
            Self::Salt => Range::new(0.005, 0.5, 0.025),
            Self::Temperature => Range::new(278.15, 348.15, 5.0),
        }
    }

    fn apply(self, base: Conditions, x: f64) -> Conditions {
        match self {
            Self::Ph => Conditions { ph: x, ..base },
            Self::Salt => Conditions { salt: x, ..base },
            Self::Temperature => Conditions {
                temperature: x,
                ..base
            },
        }
    }

    fn csv_name(self) -> &'static str {
        match self {
            Self::Ph => "pH",
            Self::Salt => "ionic_strength_M",
            Self::Temperature => "temperature_K",
        }
    }
}

/// Inclusive start..=stop in steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

impl Range {
    pub const fn new(start: f64, stop: f64, step: f64) -> Self {
        Self { start, stop, step }
    }

    fn values(&self) -> Result<Vec<f64>, String> {
        let Self { start, stop, step } = *self;
        if !(step > 0.0 && start <= stop && start.is_finite() && stop.is_finite()) {
            return Err(format!(
                "need start ≤ stop and step > 0 (got {start}..{stop} step {step})"
            ));
        }
        // Rounded so 2..12 step 0.25 includes 12 despite float error.
        let n = ((stop - start) / step + 1e-9).floor() as usize + 1;
        if n > MAX_POINTS {
            return Err(format!("{n} points; at most {MAX_POINTS} allowed"));
        }
        Ok((0..n).map(|i| (i as f64).mul_add(step, start)).collect())
    }
}

/// One scan point.
#[derive(Clone, Debug)]
pub struct Row {
    pub x: f64,
    pub hh: Multipole,
    /// None when MC is off.
    pub mc: Option<Multipole>,
}

pub struct Scan {
    beads: Vec<Bead>,
    calcs: Vec<ChargeCalc>,
    axis: Axis,
    values: Vec<f64>,
    rows: Vec<Row>,
    /// MC for the point after `rows`; HH is computed when it starts.
    current: Option<(Multipole, Titration)>,
    header: String,
}

impl Scan {
    pub fn start(
        structure: &Structure,
        setup: &Setup,
        chains: &[String],
        base: Conditions,
        axis: Axis,
        range: Range,
    ) -> Result<Self, String> {
        let values = range.values()?;
        let calcs = values
            .iter()
            .map(|&x| setup.charge_calc(axis.apply(base, x)))
            .collect::<Result<Vec<_>, _>>()?;
        let beads = structure.beads(setup.policy, chains)?;
        let header = format!(
            "# {}\n# {} chains={}\n# scan {} with fixed {}\n# {}\n# MC seed {SEED:#x}; HH = Henderson-Hasselbalch\n",
            crate::provenance(),
            structure.name(),
            chain_label(chains),
            axis.csv_name(),
            base,
            setup.describe(),
        );
        Ok(Self {
            beads,
            calcs,
            axis,
            values,
            rows: Vec::new(),
            current: None,
            header,
        })
    }

    /// Work for about `budget`; true once all points are done.
    pub fn advance(&mut self, budget: Duration) -> bool {
        let deadline = web_time::Instant::now() + budget;
        while self.rows.len() < self.values.len() {
            let i = self.rows.len();
            let x = self.values[i];
            let calc = &self.calcs[i];
            if calc.mc == 0 {
                let hh = calc.run(&self.beads).multipole;
                self.rows.push(Row { x, hh, mc: None });
            } else {
                let (hh, mut t) = self.current.take().unwrap_or_else(|| {
                    let hh = calc.clone().mc(0).run(&self.beads).multipole;
                    (hh, calc.start(&self.beads, SEED))
                });
                let left = deadline.saturating_duration_since(web_time::Instant::now());
                if !advance_for(&mut t, left) {
                    self.current = Some((hh, t));
                    return false;
                }
                let mc = Some(t.finish().multipole);
                self.rows.push(Row { x, hh, mc });
            }
            if web_time::Instant::now() >= deadline {
                break;
            }
        }
        self.is_done()
    }

    pub fn is_done(&self) -> bool {
        self.rows.len() == self.values.len()
    }

    pub fn progress(&self) -> f64 {
        let partial = self.current.as_ref().map_or(0.0, |(_, t)| t.progress());
        (self.rows.len() as f64 + partial) / self.values.len() as f64
    }

    pub fn axis(&self) -> Axis {
        self.axis
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Finished rows as CSV, with provenance and settings as `#` comments.
    pub fn csv(&self) -> String {
        let mut out = self.header.clone();
        let cols = |m: &str| format!("Z_{m},C_{m},mu_{m},mu_std_{m},Q_{m},Q_std_{m}");
        let _ = write!(out, "{},{}", self.axis.csv_name(), cols("HH"));
        let has_mc = self.rows.iter().any(|r| r.mc.is_some());
        if has_mc {
            let _ = write!(out, ",{}", cols("MC"));
        }
        out.push('\n');
        let fields = |m: &Multipole| {
            format!(
                "{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
                m.charge,
                m.capacitance(),
                m.dipole,
                m.dipole_std(),
                m.quadrupole,
                m.quadrupole_std()
            )
        };
        for r in &self.rows {
            let _ = write!(out, "{},{}", r.x, fields(&r.hh));
            if let Some(mc) = &r.mc {
                let _ = write!(out, ",{}", fields(mc));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::structure::tests::structure;

    fn run(setup: &Setup, axis: Axis, range: Range) -> Scan {
        let mut scan =
            Scan::start(&structure(), setup, &[], Conditions::default(), axis, range).unwrap();
        while !scan.advance(Duration::from_millis(5)) {}
        scan
    }

    #[test]
    fn range_includes_stop() {
        let v = Range::new(2.0, 12.0, 0.25).values().unwrap();
        assert_eq!(v.len(), 41);
        assert!((v[40] - 12.0).abs() < 1e-12);
        assert!(Range::new(1.0, 0.0, 0.1).values().is_err());
        assert!(Range::new(0.0, 1.0, 0.0).values().is_err());
        assert!(Range::new(0.0, 1e6, 1.0).values().is_err());
    }

    #[test]
    fn ph_scan_charge_decreases() {
        let setup = Setup {
            mc_sweeps: 0,
            ..Default::default()
        };
        let scan = run(&setup, Axis::Ph, Range::new(2.0, 12.0, 2.0));
        let z: Vec<f64> = scan.rows().iter().map(|r| r.hh.charge).collect();
        assert_eq!(z.len(), 6);
        assert!(z.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn mc_scan_fills_both_columns() {
        let setup = Setup {
            mc_sweeps: 300,
            ..Default::default()
        };
        let scan = run(&setup, Axis::Salt, Range::new(0.01, 0.1, 0.03));
        assert!(scan.rows().iter().all(|r| r.mc.is_some()));
        let csv = scan.csv();
        let header = csv.lines().find(|l| !l.starts_with('#')).unwrap();
        assert_eq!(header.split(',').count(), 13);
        assert_eq!(csv.lines().filter(|l| !l.starts_with('#')).count(), 5);
        assert!((scan.progress() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_point_rejected_up_front() {
        let r = Scan::start(
            &structure(),
            &Setup::default(),
            &[],
            Conditions::default(),
            Axis::Temperature,
            Range::new(-10.0, 300.0, 10.0),
        );
        assert!(r.is_err());
    }
}
