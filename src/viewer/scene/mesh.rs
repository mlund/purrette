//! Molecular surface as a triangle mesh with vertex normals, via the vendored mesher.

use glam::Vec3;

use super::surface;

#[derive(Clone, Debug, Default)]
pub(super) struct Mesh {
    pub positions: Vec<Vec3>,
    /// Outward unit normals.
    pub normals: Vec<Vec3>,
    /// Outward (counter-clockwise) winding.
    pub triangles: Vec<[u32; 3]>,
}

/// Gaussian surface around spheres; spacing coarsens with size so large assemblies
/// still mesh in about a second.
pub(super) fn gaussian(centers: &[[f64; 3]], radii: &[f64]) -> Result<Mesh, String> {
    let spacing = match centers.len() {
        0..5_000 => 1.0,
        5_000..50_000 => 1.5,
        _ => 2.5,
    };
    let (vertices, triangles) = surface::build_mesh(centers, radii, spacing)?;
    let positions: Vec<Vec3> = vertices
        .iter()
        .map(|v| Vec3::from(v.map(|c| c as f32)))
        .collect();
    let normals = vertex_normals(&positions, &triangles);
    Ok(Mesh {
        positions,
        normals,
        triangles,
    })
}

/// Area-weighted face normals averaged per vertex: cheap and smooth on fine meshes.
pub(super) fn vertex_normals(positions: &[Vec3], triangles: &[[u32; 3]]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for t in triangles {
        let [a, b, c] = t.map(|i| positions[i as usize]);
        let n = (b - a).cross(c - a);
        for &i in t {
            normals[i as usize] += n;
        }
    }
    normals.iter().map(|n| n.normalize_or_zero()).collect()
}
