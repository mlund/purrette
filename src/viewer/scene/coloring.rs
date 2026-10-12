//! Colour schemes. Add one by adding a variant, its arms here, and listing it in
//! [`super::LayerKind::colorings`].

use super::element::Element;

/// Linear RGBA, 0–1.
pub type Rgba = [f32; 4];

/// Charges at or beyond this saturate the map (e).
const CHARGE_SCALE: f64 = 1.0;
/// Largest σ of a one-proton site (f = ½), so the map saturates only there (e).
const SIGMA_SCALE: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coloring {
    /// Blue for positive, red for negative, light grey for neutral; pulses with the
    /// sampled charge fluctuation.
    Charge,
    /// Charge fluctuation σ, steady: grey (none) → gold → magenta (largest).
    Fluctuation,
    /// CPK-style element colours.
    Element,
    /// One colour per chain.
    Chain,
    /// Electric potential: red negative, blue positive; pulses with site fluctuations.
    Potential,
    /// Potential fluctuation σ_φ, steady, on the same scale as charge fluctuation.
    PotentialSpread,
    /// One neutral colour, for glyphs.
    Uniform,
    /// Quadrupole f(n̂) = n̂ᵀΘn̂: blue where positive, red where negative.
    Quadrupole,
}

/// What a colouring may look at, filled in by whichever representation built the item.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Item {
    /// Mean charge (e).
    pub charge: f64,
    /// Charge spread σ (e).
    pub sigma: f64,
    pub element: Element,
    /// Chain ordinal, stable across layers.
    pub chain: usize,
    /// Signed field value (potential, quadrupole) and a spread, in colour-map units.
    pub value: f64,
    pub spread: f64,
    /// Bead whose noise this item pulses with, if any.
    pub bead: Option<u32>,
}

impl Coloring {
    pub fn label(self) -> &'static str {
        match self {
            Self::Charge => "Charge",
            Self::Fluctuation => "Fluctuation σ",
            Self::Element => "Element",
            Self::Chain => "Chain",
            Self::Potential => "Potential φ",
            Self::PotentialSpread => "Fluctuation σ_φ",
            Self::Uniform => "Uniform",
            Self::Quadrupole => "Quadrupole f(n̂)",
        }
    }

    /// Whether colours change with the fluctuation animation.
    pub(super) fn pulses(self) -> bool {
        matches!(self, Self::Charge | Self::Potential)
    }

    /// `pulse` is this instant's displacement in units of σ (noise × amplitude).
    pub(super) fn paint(self, item: &Item, pulse: f64) -> Rgba {
        match self {
            Self::Charge => diverging(item.sigma.mul_add(pulse, item.charge) / CHARGE_SCALE),
            Self::Fluctuation => sequential(item.sigma / SIGMA_SCALE),
            Self::Element => {
                let [r, g, b] = item.element.color();
                [r, g, b, 1.0]
            }
            Self::Chain => categorical(item.chain),
            Self::Potential => diverging(item.value),
            Self::PotentialSpread => sequential(item.spread),
            Self::Uniform => [0.95, 0.70, 0.20, 1.0],
            Self::Quadrupole => diverging(item.value),
        }
    }
}

/// Red ← grey → blue, with `t` clamped to −1..1. Grey rather than white so
/// neutral items still show their shading.
fn diverging(t: f64) -> Rgba {
    const NEUTRAL: [f32; 3] = [0.80, 0.80, 0.80];
    const POSITIVE: [f32; 3] = [0.17, 0.32, 0.86];
    const NEGATIVE: [f32; 3] = [0.84, 0.15, 0.15];
    let t = t.clamp(-1.0, 1.0) as f32;
    let end = if t >= 0.0 { POSITIVE } else { NEGATIVE };
    let a = t.abs();
    let mix = |i: usize| NEUTRAL[i] + a * (end[i] - NEUTRAL[i]);
    [mix(0), mix(1), mix(2), 1.0]
}

/// Grey → gold → magenta for `t` in 0..1; distinct from the red/blue charge map so the
/// two are not confused.
fn sequential(t: f64) -> Rgba {
    const STOPS: [[f32; 3]; 3] = [[0.80, 0.80, 0.80], [0.96, 0.76, 0.18], [0.78, 0.13, 0.62]];
    let t = 2.0 * t.clamp(0.0, 1.0) as f32;
    let (a, b, f) = if t <= 1.0 {
        (STOPS[0], STOPS[1], t)
    } else {
        (STOPS[1], STOPS[2], t - 1.0)
    };
    let mix = |i: usize| a[i] + f * (b[i] - a[i]);
    [mix(0), mix(1), mix(2), 1.0]
}

/// Okabe–Ito palette (colour-blind safe), cycled for more than eight chains.
fn categorical(index: usize) -> Rgba {
    const PALETTE: [[f32; 3]; 8] = [
        [0.90, 0.62, 0.00],
        [0.34, 0.71, 0.91],
        [0.00, 0.62, 0.45],
        [0.94, 0.89, 0.26],
        [0.00, 0.45, 0.70],
        [0.84, 0.37, 0.00],
        [0.80, 0.47, 0.65],
        [0.60, 0.60, 0.60],
    ];
    let [r, g, b] = PALETTE[index % PALETTE.len()];
    [r, g, b, 1.0]
}
