//! Uniform cell grid for neighbour queries within a fixed cutoff: linear in the number
//! of points, and simpler than a tree for the one cutoff each caller uses.

use std::collections::HashMap;

use glam::Vec3;

pub(super) struct CellGrid {
    size: f32,
    cells: HashMap<[i32; 3], Vec<u32>>,
}

impl CellGrid {
    /// Cells of edge `size`, which must be at least the largest query distance.
    pub fn new(points: impl IntoIterator<Item = Vec3>, size: f32) -> Self {
        let mut cells: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
        for (i, p) in points.into_iter().enumerate() {
            cells.entry(cell(p, size)).or_default().push(i as u32);
        }
        Self { size, cells }
    }

    /// Indices in the 27 cells around `p`: a superset of those within `size`.
    pub fn near(&self, p: Vec3) -> impl Iterator<Item = usize> + '_ {
        let [x, y, z] = cell(p, self.size);
        (-1..=1)
            .flat_map(move |dx| {
                (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [x + dx, y + dy, z + dz]))
            })
            .filter_map(|c| self.cells.get(&c))
            .flatten()
            .map(|&i| i as usize)
    }
}

fn cell(p: Vec3, size: f32) -> [i32; 3] {
    (p / size).floor().as_ivec3().to_array()
}
