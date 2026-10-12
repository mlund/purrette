//! Dipole and quadrupole of the mean bead charges about the centre of mass, the same
//! convention cgkitten reports, so the glyphs match the numbers shown.

use cgkitten::Bead;
use glam::{Mat3, Vec3};

use super::Pos;

pub(super) struct Moments {
    pub center: Vec3,
    /// e·Å.
    pub dipole: Vec3,
    /// Traceless (Buckingham) Θ = ½ Σ q (3ddᵀ − d²I), e·Å².
    pub quadrupole: Mat3,
}

impl Moments {
    pub fn of(beads: &[Bead]) -> Self {
        let mass: f64 = beads.iter().map(|b| b.mass).sum();
        let center = if mass > 0.0 {
            beads.iter().map(|b| b.pos() * b.mass as f32).sum::<Vec3>() / mass as f32
        } else {
            beads.iter().map(Pos::pos).sum::<Vec3>() / beads.len().max(1) as f32
        };
        let mut dipole = Vec3::ZERO;
        let mut primitive = Mat3::ZERO;
        for b in beads {
            let (q, d) = (b.charge as f32, b.pos() - center);
            dipole += q * d;
            primitive += Mat3::from_cols(d * d.x, d * d.y, d * d.z) * q;
        }
        let trace = primitive.x_axis.x + primitive.y_axis.y + primitive.z_axis.z;
        let quadrupole = (primitive * 3.0 - Mat3::IDENTITY * trace) * 0.5;
        Self {
            center,
            dipole,
            quadrupole,
        }
    }

    /// f(n̂) = n̂ᵀΘn̂: how the quadrupole looks from direction n̂.
    pub fn angular(&self, n: Vec3) -> f32 {
        n.dot(self.quadrupole * n)
    }

    /// Eigenvalues and unit eigenvectors of Θ (Jacobi rotations; 3×3 symmetric).
    pub fn principal_axes(&self) -> [(f32, Vec3); 3] {
        let mut a = self.quadrupole.to_cols_array_2d().map(|c| c.map(f64::from));
        let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for _ in 0..50 {
            let (p, q) = [(0, 1), (0, 2), (1, 2)]
                .into_iter()
                .max_by(|x, y| a[x.0][x.1].abs().total_cmp(&a[y.0][y.1].abs()))
                .unwrap_or((0, 1));
            if a[p][q].abs() < 1e-12 {
                break;
            }
            let theta = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
            let (s, c) = theta.sin_cos();
            for column in &mut a {
                let (kp, kq) = (column[p], column[q]);
                column[p] = c * kp - s * kq;
                column[q] = s * kp + c * kq;
            }
            let (cp, cq) = (a[p], a[q]);
            a[p] = [0, 1, 2].map(|k| c * cp[k] - s * cq[k]);
            a[q] = [0, 1, 2].map(|k| s * cp[k] + c * cq[k]);
            for row in &mut v {
                let (vp, vq) = (row[p], row[q]);
                row[p] = c * vp - s * vq;
                row[q] = s * vp + c * vq;
            }
        }
        [0, 1, 2].map(|i| {
            let axis = Vec3::new(v[0][i] as f32, v[1][i] as f32, v[2][i] as f32);
            (a[i][i] as f32, axis.normalize_or_zero())
        })
    }
}
