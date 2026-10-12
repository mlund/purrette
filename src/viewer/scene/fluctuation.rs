//! Smooth unit Gaussian noise ξᵢ(t) per fluctuating bead, for animating charge fluctuations.
//!
//! A random-phase sum of cosines with normally distributed frequencies: stationary,
//! unit variance, close to Gaussian, with autocorrelation exp(−t²/2τ²). Being a pure
//! function of t, frames are reproducible and pausing is just holding t.

/// Cosines per bead; enough for a near-Gaussian marginal without visible periodicity.
const MODES: usize = 16;
/// Correlation time (s): short enough that a fluctuating site visibly flickers.
const TAU: f64 = 0.3;
/// Fixed so the same result always animates the same way.
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// (angular frequency, phase) per mode, drawn once; a frame only evaluates cosines.
type Modes = [(f64, f64); MODES];

pub(super) struct Noise {
    /// None for beads that do not fluctuate.
    modes: Vec<Option<Box<Modes>>>,
}

impl Noise {
    pub fn new(charge_variance: &[f64]) -> Self {
        let modes = charge_variance
            .iter()
            .enumerate()
            .map(|(bead, &v)| (v > 0.0).then(|| Box::new(modes(bead))))
            .collect();
        Self { modes }
    }

    /// ξ for every bead at time `t` (s), times `amplitude`; 0 where nothing fluctuates.
    pub fn at(&self, t: f64, amplitude: f64) -> Vec<f64> {
        self.modes
            .iter()
            .map(|m| {
                m.as_ref().map_or(0.0, |m| {
                    let sum: f64 = m
                        .iter()
                        .map(|&(omega, phase)| (omega * t + phase).cos())
                        .sum();
                    // Each cosine has variance ½.
                    amplitude * sum * (2.0 / MODES as f64).sqrt()
                })
            })
            .collect()
    }
}

fn modes(bead: usize) -> Modes {
    std::array::from_fn(|k| {
        let key = SEED ^ ((bead as u64) << 8 | k as u64);
        let (u1, u2, u3) = (uniform(key, 1), uniform(key, 2), uniform(key, 3));
        // Box–Muller: Gaussian frequencies give a Gaussian autocorrelation.
        let omega = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos() / TAU;
        (omega, std::f64::consts::TAU * u3)
    })
}

/// Deterministic uniform in (0, 1] from a key (splitmix64).
fn uniform(key: u64, stream: u64) -> f64 {
    let mut z = key.wrapping_add(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 11) as f64 + 1.0) / (1u64 << 53) as f64
}
