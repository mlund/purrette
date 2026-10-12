//! Calculation options, mirroring the cgkitten command line.

use cgkitten::forcefield::{self, HydrophobicScaling};
use cgkitten::residue::PkaOverrides;
use cgkitten::{ChargeCalc, CoarseGrain, MultiBead, SingleBead};

/// Charge configurations kept per run; enough for a smooth, non-repeating pulse.
const SNAPSHOTS: usize = 32;

/// Coarse-graining resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Backbone bead plus a side-chain bead at each titratable charge centre.
    #[default]
    Multi,
    /// One bead per residue.
    Single,
}

impl Policy {
    pub const ALL: [Self; 2] = [Self::Multi, Self::Single];

    pub fn label(self) -> &'static str {
        match self {
            Self::Multi => "Multi-bead",
            Self::Single => "Single bead",
        }
    }

    pub(crate) fn coarse_grain(self) -> &'static dyn CoarseGrain {
        match self {
            Self::Multi => &MultiBead,
            Self::Single => &SingleBead,
        }
    }
}

/// Pair potential written to the topology; does not affect titration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ForceField {
    #[default]
    Calvados3,
    KimHummer,
    Pasquier,
    None,
}

impl ForceField {
    pub const ALL: [Self; 4] = [Self::Calvados3, Self::KimHummer, Self::Pasquier, Self::None];

    pub fn label(self) -> &'static str {
        match self {
            Self::Calvados3 => "CALVADOS 3",
            Self::KimHummer => "Kim-Hummer",
            Self::Pasquier => "Pasquier",
            Self::None => "None",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Calvados3 => "calvados3",
            Self::KimHummer => "kimhummer",
            Self::Pasquier => "pasquier",
            Self::None => "none",
        }
    }
}

/// Which hydrophobic parameter to scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scaling {
    #[default]
    None,
    Lambda,
    Epsilon,
}

impl Scaling {
    pub const ALL: [Self; 3] = [Self::None, Self::Lambda, Self::Epsilon];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lambda => "λ",
            Self::Epsilon => "ε",
        }
    }
}

/// Solution conditions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    pub ph: f64,
    /// Ionic strength, mol/L.
    pub salt: f64,
    /// Kelvin.
    pub temperature: f64,
}

impl Default for Conditions {
    fn default() -> Self {
        Self {
            ph: 7.0,
            salt: 0.1,
            temperature: 298.15,
        }
    }
}

impl Conditions {
    fn validate(&self) -> Result<(), String> {
        if !(0.0..=14.0).contains(&self.ph) {
            return Err(format!("pH must be in 0–14, got {}", self.ph));
        }
        if !(self.salt >= 0.0 && self.salt.is_finite()) {
            return Err(format!("ionic strength must be ≥ 0, got {}", self.salt));
        }
        if !(self.temperature > 0.0 && self.temperature.is_finite()) {
            return Err(format!(
                "temperature must be > 0 K, got {}",
                self.temperature
            ));
        }
        Ok(())
    }
}

impl std::fmt::Display for Conditions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pH={} I={} M T={} K",
            self.ph, self.salt, self.temperature
        )
    }
}

/// Everything a run needs besides the structure and conditions.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub policy: Policy,
    pub force_field: ForceField,
    pub scaling: Scaling,
    pub scaling_factor: f64,
    /// Relative charge tolerance for merging titratable site types.
    pub merge_tolerance: f64,
    /// MC sweeps per site; 0 = Henderson-Hasselbalch only.
    pub mc_sweeps: usize,
    /// Effective pKa per group, editable in place; defaults are not stored as overrides.
    pub pka: Vec<(&'static str, f64)>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            policy: Policy::default(),
            force_field: ForceField::default(),
            scaling: Scaling::default(),
            scaling_factor: 1.0,
            merge_tolerance: 0.02,
            mc_sweeps: 10_000,
            pka: default_pka(),
        }
    }
}

/// Default pKa for every titratable group.
pub fn default_pka() -> Vec<(&'static str, f64)> {
    PkaOverrides::default().table().collect()
}

impl Setup {
    /// Only edited values become overrides, so output headers show what changed.
    fn pka_overrides(&self) -> Result<PkaOverrides, String> {
        let mut overrides = PkaOverrides::default();
        for ((name, pka), (_, default)) in self.pka.iter().zip(default_pka()) {
            if *pka != default {
                overrides.insert(name, *pka)?;
            }
        }
        Ok(overrides)
    }

    pub(crate) fn charge_calc(&self, c: Conditions) -> Result<ChargeCalc, String> {
        c.validate()?;
        Ok(ChargeCalc {
            ph: c.ph,
            temperature: c.temperature,
            ionic_strength: c.salt,
            mc: self.mc_sweeps,
            pka_overrides: self.pka_overrides()?,
            snapshots: SNAPSHOTS,
        })
    }

    pub(crate) fn force_field(&self) -> Result<Option<Box<dyn forcefield::ForceField>>, String> {
        let scaling = match self.scaling {
            Scaling::None => HydrophobicScaling::NoScale,
            Scaling::Lambda => HydrophobicScaling::ScaleLambda(self.scaling_factor),
            Scaling::Epsilon => HydrophobicScaling::ScaleEpsilon(self.scaling_factor),
        };
        forcefield::from_name(self.force_field.name(), scaling)
    }

    /// One-line summary for file headers.
    pub(crate) fn describe(&self) -> String {
        let pka = self
            .pka_overrides()
            .map(|o| o.to_string())
            .unwrap_or_else(|e| e);
        format!(
            "cg={} ff={} scaling={}:{} merge_tol={} mc={} pKa: {pka}",
            self.policy.label(),
            self.force_field.name(),
            self.scaling.label(),
            self.scaling_factor,
            self.merge_tolerance,
            self.mc_sweeps,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_produce_no_overrides() {
        assert!(Setup::default().pka_overrides().unwrap().is_empty());
    }

    #[test]
    fn edited_pka_becomes_override() {
        let mut setup = Setup::default();
        let his = setup.pka.iter_mut().find(|(n, _)| *n == "HIS").unwrap();
        his.1 = 6.0;
        let calc = setup.charge_calc(Conditions::default()).unwrap();
        assert!(!calc.pka_overrides.is_empty());
    }

    #[test]
    fn rejects_bad_conditions() {
        let bad = Conditions {
            ph: 15.0,
            ..Default::default()
        };
        assert!(Setup::default().charge_calc(bad).is_err());
    }

    #[test]
    fn lambda_scaling_rejected_for_kim_hummer() {
        let setup = Setup {
            force_field: ForceField::KimHummer,
            scaling: Scaling::Lambda,
            ..Default::default()
        };
        assert!(setup.force_field().is_err());
    }
}
