//! Small meshes for glyphs: arrow heads and the directional surfaces of multipoles.

use std::collections::HashMap;

use glam::Vec3;

use super::mesh::Mesh;

/// Cone from a base disc centred at `base` to `tip`; outward winding.
pub(super) fn cone(base: Vec3, tip: Vec3, radius: f32) -> Mesh {
    const SEGMENTS: u32 = 24;
    let axis = (tip - base).normalize_or_zero();
    let u = axis.any_orthonormal_vector();
    let v = axis.cross(u);
    let mut positions = vec![tip, base];
    let mut normals = vec![axis, -axis];
    let slope = radius / tip.distance(base).max(1e-6);
    for k in 0..SEGMENTS {
        let a = std::f32::consts::TAU * k as f32 / SEGMENTS as f32;
        let radial = u * a.cos() + v * a.sin();
        positions.push(base + radial * radius);
        normals.push((radial + axis * slope).normalize());
    }
    let mut triangles = Vec::new();
    for k in 0..SEGMENTS {
        let (a, b) = (2 + k, 2 + (k + 1) % SEGMENTS);
        triangles.push([0, a, b]);
        triangles.push([1, b, a]);
    }
    Mesh {
        positions,
        normals,
        triangles,
    }
}

/// Unit sphere from a subdivided icosahedron: even sampling of directions.
pub(super) fn icosphere(subdivisions: u32) -> Mesh {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut positions: Vec<Vec3> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .map(|p| Vec3::from(p).normalize())
    .to_vec();
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..subdivisions {
        let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
        let mut midpoint = |a: u32, b: u32, positions: &mut Vec<Vec3>| {
            *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                positions.push(((positions[a as usize] + positions[b as usize]) * 0.5).normalize());
                positions.len() as u32 - 1
            })
        };
        triangles = triangles
            .iter()
            .flat_map(|&[a, b, c]| {
                let ab = midpoint(a, b, &mut positions);
                let bc = midpoint(b, c, &mut positions);
                let ca = midpoint(c, a, &mut positions);
                [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]
            })
            .collect();
    }
    Mesh {
        normals: positions.clone(),
        positions,
        triangles,
    }
}
