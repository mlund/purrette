/// Phase-space point (position, velocity).
#[derive(Debug, Clone, Copy)]
pub(super) struct State {
    pub x: f64,
    pub v: f64,
}

impl State {
    pub const INITIAL: Self = Self { x: 1.0, v: 0.0 };
}

/// One classic RK4 step for ẍ = a(x, ẋ).
pub(super) fn rk4(s: State, dt: f64, a: impl Fn(f64, f64) -> f64) -> State {
    let deriv = |x: f64, v: f64| (v, a(x, v));
    let (k1x, k1v) = deriv(s.x, s.v);
    let (k2x, k2v) = deriv(s.x + 0.5 * dt * k1x, s.v + 0.5 * dt * k1v);
    let (k3x, k3v) = deriv(s.x + 0.5 * dt * k2x, s.v + 0.5 * dt * k2v);
    let (k4x, k4v) = deriv(s.x + dt * k3x, s.v + dt * k3v);
    State {
        x: s.x + dt / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x),
        v: s.v + dt / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v),
    }
}
