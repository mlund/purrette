//! Damped harmonic oscillator, ẍ = −ω²x − 2ζωẋ, started from x = 1, ẋ = 0.
//!
//! Demo of a deep module: the UI sees only [`Oscillator`] and [`ParamError`];
//! the integrator is private and can change without touching callers.

mod integrator;

use std::fmt;

/// Why the parameters were rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamError {
    /// ω must be finite and > 0.
    Frequency(f64),
    /// ζ must be finite and ≥ 0; negative damping would grow without bound.
    Damping(f64),
}

impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frequency(w) => write!(f, "frequency ω must be > 0, got {w}"),
            Self::Damping(z) => write!(f, "damping ζ must be ≥ 0, got {z}"),
        }
    }
}

impl std::error::Error for ParamError {}

#[derive(Debug, Clone)]
pub struct Oscillator {
    omega: f64,
    zeta: f64,
    state: integrator::State,
    time: f64,
}

impl Oscillator {
    pub fn new(omega: f64, zeta: f64) -> Result<Self, ParamError> {
        if !(omega.is_finite() && omega > 0.0) {
            return Err(ParamError::Frequency(omega));
        }
        if !(zeta.is_finite() && zeta >= 0.0) {
            return Err(ParamError::Damping(zeta));
        }
        Ok(Self {
            omega,
            zeta,
            state: integrator::State::INITIAL,
            time: 0.0,
        })
    }

    /// Advances by `dt`. Fixed-step RK4, so `dt ≪ 1/ω` for accuracy.
    pub fn step(&mut self, dt: f64) {
        let (w, z) = (self.omega, self.zeta);
        self.state = integrator::rk4(self.state, dt, |x, v| -w * w * x - 2.0 * z * w * v);
        self.time += dt;
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn position(&self) -> f64 {
        self.state.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Underdamped closed form for x(0)=1, ẋ(0)=0.
    fn exact(omega: f64, zeta: f64, t: f64) -> f64 {
        let wd = omega * (1.0 - zeta * zeta).sqrt();
        (-zeta * omega * t).exp() * ((wd * t).cos() + zeta * omega / wd * (wd * t).sin())
    }

    fn run(omega: f64, zeta: f64, t_end: f64) -> Oscillator {
        let mut osc = Oscillator::new(omega, zeta).unwrap();
        let dt = 1e-3;
        for _ in 0..(t_end / dt).round() as usize {
            osc.step(dt);
        }
        osc
    }

    #[test]
    fn underdamped_matches_closed_form() {
        let osc = run(2.0, 0.1, 10.0);
        assert!((osc.time() - 10.0).abs() < 1e-9);
        assert!((osc.position() - exact(2.0, 0.1, 10.0)).abs() < 1e-9);
    }

    #[test]
    fn undamped_returns_after_one_period() {
        let period = std::f64::consts::TAU / 2.0;
        let osc = run(2.0, 0.0, period);
        assert!((osc.position() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_unphysical_parameters() {
        assert_eq!(
            Oscillator::new(0.0, 0.1).unwrap_err(),
            ParamError::Frequency(0.0)
        );
        assert_eq!(
            Oscillator::new(1.0, -0.1).unwrap_err(),
            ParamError::Damping(-0.1)
        );
    }
}
