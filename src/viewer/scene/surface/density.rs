// Vendored from bemtzmann (https://github.com/mlund/bemtzmann, MIT OR Apache-2.0),
// src/geometry/molecular_surface/density.rs at e3e3e0fa61cee83c67809133944ae97f05b6ca92.
// Changes across the vendored files: errors as String, serial density sampling (no
// rayon on wasm), unit tests and the test-only gradient dropped (covered upstream).

//! Gaussian molecular-surface density evaluator.
//!
//! Defines
//! ρ(r) = Σᵢ exp(d · (1 − |r − rᵢ|² / Rᵢ²))
//! and its analytical gradient. Atoms whose contribution falls below a small
//! ε at the query point are skipped via a KD-tree radius query.
//!
//! The mesh is the isolevel ρ = ρ₀; the gradient direction `−∇ρ / ‖∇ρ‖` is the
//! outward normal (density is high inside the molecule, low outside).

type Result<T> = std::result::Result<T, String>;

/// Map a kd-tree failure into a structured bemtzmann error. The kd-tree only
/// fails on `WrongDimension` (statically impossible — we always use 3D) or
/// `NonFiniteCoordinate` (caller-side bug — `validate_inputs` should have
/// caught this). Either way, surface as `NonManifoldMesh` rather than
/// panicking.
fn kdtree_err(e: kdtree::ErrorKind) -> String {
    format!("kd-tree internal failure: {e:?}")
}

/// Returns the squared radius beyond which a sphere of decay `decay` and
/// radius `radius` contributes less than `eps` to ρ.
///
/// Solving exp(d·(1 − r²/R²)) < ε gives r² > R² · (1 − ln(ε)/d). Used both to
/// size grid padding (boundary cells must be far enough from atoms that
/// ρ ≪ ρ₀) and as the per-atom KD-tree query radius during ρ evaluation.
pub(super) fn cutoff_radius_sq(radius: f64, decay: f64, eps: f64) -> f64 {
    debug_assert!(radius > 0.0 && decay > 0.0 && (0.0..1.0).contains(&eps));
    radius * radius * (1.0 - eps.ln() / decay)
}

/// Build a flat-array KD-tree over atom positions. The stored value at each
/// point is the atom index, so cutoff queries return the indices we need to
/// look up the corresponding radius.
pub(super) fn build_atom_tree(
    positions: &[[f64; 3]],
) -> Result<kdtree::KdTree<f64, u32, [f64; 3]>> {
    let mut tree: kdtree::KdTree<f64, u32, [f64; 3]> = kdtree::KdTree::new(3);
    for (i, p) in positions.iter().enumerate() {
        tree.add(*p, i as u32).map_err(kdtree_err)?;
    }
    Ok(tree)
}

/// Evaluate ρ at a single point, skipping atoms beyond the system-wide cutoff.
///
/// `query_cutoff_sq` is the *largest* per-atom cutoff in the system (we'd
/// rather pay for a few small extra contributions than tune per-atom). Atoms
/// outside that radius contribute < `cutoff_eps` per the cutoff math and are
/// dropped via the KD-tree's `within` query.
pub(super) fn density_at(
    point: [f64; 3],
    radii_sq: &[f64],
    decay: f64,
    query_cutoff_sq: f64,
    tree: &kdtree::KdTree<f64, u32, [f64; 3]>,
) -> Result<f64> {
    let hits = tree
        .within(
            &point,
            query_cutoff_sq,
            &kdtree::distance::squared_euclidean,
        )
        .map_err(kdtree_err)?;
    let mut rho = 0.0_f64;
    for (dist_sq, &idx) in hits {
        let r_sq = radii_sq[idx as usize];
        rho += (decay * (1.0 - dist_sq / r_sq)).exp();
    }
    Ok(rho)
}
