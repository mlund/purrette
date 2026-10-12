//! Geometry of a layer. Add a style by adding a variant, its arm in [`Representation::build`],
//! and listing it in [`super::LayerKind::representations`].

use std::rc::Rc;
use std::sync::Arc;

use cgkitten::Atom;
use glam::Vec3;

use super::coloring::Item;
use super::element::Element;
use super::grid::CellGrid;
use super::mesh::{self, Mesh};
use super::multipole::Moments;
use super::{Context, PROBE_RADIUS, Pos, glyphs};

/// Licorice stick radius (Å); joints use the same so bonds meet in smooth elbows.
const STICK_RADIUS: f32 = 0.25;
/// Added to summed covalent radii; the usual tolerance for distance-based bonding (Å).
const BOND_TOLERANCE: f32 = 0.4;
/// Backbone tube radius (Å).
const TRACE_RADIUS: f32 = 0.4;
/// Consecutive Cα farther apart than this are a gap in the model, not a bond (Å).
const MAX_CA_STEP: f32 = 4.2;
/// Spline samples per residue: smooth enough without inflating the stick count.
const TRACE_SAMPLES: usize = 4;
/// Arrow length per dipole (Å per e·Å): proteins' 10–500 e·Å then span roughly their
/// own size.
const ARROW_PER_DIPOLE: f32 = 1.0;
const ARROW_RADIUS: f32 = 0.6;
const ARROW_HEAD: f32 = 4.0;
/// Bulge of the lobes relative to their mean radius: clear shape, never inside-out.
const LOBE_AMPLITUDE: f64 = 0.5;
/// Subdivisions of the direction sphere: 2562 vertices, smooth at any zoom.
const SPHERE_DETAIL: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    /// One sphere per bead, radius σ/2.
    Beads,
    /// Space-filling atoms at their van der Waals radii.
    VanDerWaals,
    /// Bonds as sticks, coloured half by each atom.
    Licorice,
    /// Smooth tube through the Cα of each chain.
    Trace,
    /// Molecular surface, sampling the potential an ion just outside would feel.
    Surface,
    /// Arrow from the centre of mass along the dipole.
    Arrow,
    /// Angular surface r(n̂) ∝ 1 + ½ f(n̂)/max|f|, f = n̂ᵀΘn̂: bulges where Θ is positive.
    Lobes,
    /// Sphere enclosing the molecule, coloured by f(n̂).
    Sphere,
    /// Θ's principal axes, length ∝ |eigenvalue|.
    Axes,
}

impl Representation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beads => "Beads",
            Self::VanDerWaals => "van der Waals",
            Self::Licorice => "Licorice",
            Self::Trace => "Backbone trace",
            Self::Surface => "Surface",
            Self::Arrow => "Arrow",
            Self::Lobes => "Lobes",
            Self::Sphere => "Sphere",
            Self::Axes => "Principal axes",
        }
    }

    /// Whether this draws a surface that contour lines can go on.
    pub fn has_contours(self) -> bool {
        matches!(self, Self::Surface | Self::Lobes | Self::Sphere)
    }

    pub(super) fn build(self, ctx: &Context) -> Geometry {
        let atoms = &ctx.input.atoms;
        match self {
            Self::Beads => Geometry {
                items: Arc::clone(&ctx.bead_items),
                balls: ctx
                    .input
                    .beads
                    .iter()
                    .zip(&ctx.bead_radii)
                    .enumerate()
                    .map(|(item, (b, &radius))| Ball {
                        item,
                        center: b.pos(),
                        radius,
                    })
                    .collect(),
                ..Default::default()
            },
            Self::VanDerWaals => Geometry {
                items: Arc::clone(&ctx.atom_items),
                balls: atom_balls(atoms, |i| ctx.elements[i].vdw_radius()),
                ..Default::default()
            },
            Self::Licorice => Geometry {
                items: Arc::clone(&ctx.atom_items),
                balls: atom_balls(atoms, |_| STICK_RADIUS),
                sticks: bonds(atoms, &ctx.elements)
                    .into_iter()
                    .map(|(a, b)| Stick {
                        items: [a, b],
                        ends: [atoms[a].pos(), atoms[b].pos()],
                        radius: STICK_RADIUS,
                    })
                    .collect(),
                ..Default::default()
            },
            Self::Trace => Geometry {
                items: Arc::clone(&ctx.atom_items),
                ..trace(atoms)
            },
            Self::Surface => Geometry {
                shapes: surface(ctx)
                    .map_err(|e| log::warn!("surface: {e}"))
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            Self::Arrow => arrow(&ctx.moments),
            Self::Lobes | Self::Sphere | Self::Axes => quadrupole(self, &ctx.moments, ctx.extent),
        }
    }
}

/// What a layer draws. Balls and sticks index `items`; shape vertices carry their own.
#[derive(Clone, Debug, Default)]
pub(super) struct Geometry {
    pub items: Arc<[Item]>,
    pub balls: Vec<Ball>,
    pub sticks: Vec<Stick>,
    pub shapes: Vec<Shape>,
}

/// A sphere standing for `items[item]`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ball {
    pub item: usize,
    pub center: Vec3,
    pub radius: f32,
}

/// A capsule joining two items.
#[derive(Clone, Copy, Debug)]
pub(super) struct Stick {
    pub items: [usize; 2],
    pub ends: [Vec3; 2],
    pub radius: f32,
}

/// A coloured mesh: a surface or a glyph.
#[derive(Clone, Debug, Default)]
pub(super) struct Shape {
    pub mesh: Rc<Mesh>,
    /// Per vertex; empty colours every vertex with `Item::default()`.
    pub items: Vec<Item>,
    /// Per vertex, how each bead's noise shifts the item's value; for pulsing surfaces.
    pub couplings: Option<Couplings>,
}

/// Sparse per-vertex couplings: `entries[offsets[v]..offsets[v + 1]]` as (bead, weight),
/// so a frame only sums the sites near each vertex.
#[derive(Clone, Debug, Default)]
pub(super) struct Couplings {
    pub offsets: Vec<u32>,
    pub entries: Vec<(u32, f32)>,
}

impl Couplings {
    pub fn at(&self, vertex: usize) -> &[(u32, f32)] {
        &self.entries[self.offsets[vertex] as usize..self.offsets[vertex + 1] as usize]
    }
}

fn atom_balls(atoms: &[Atom], radius: impl Fn(usize) -> f32) -> Vec<Ball> {
    atoms
        .iter()
        .enumerate()
        .map(|(item, a)| Ball {
            item,
            center: a.pos(),
            radius: radius(item),
        })
        .collect()
}

/// Covalently bonded pairs (i < j) by distance, within a chain only: chains are separate
/// molecules, however close.
fn bonds(atoms: &[Atom], elements: &[Element]) -> Vec<(usize, usize)> {
    // No bond is longer than this, so each pair lies in neighbouring cells.
    let max_bond = 2.0 * Element::MAX_COVALENT_RADIUS + BOND_TOLERANCE;
    let grid = CellGrid::new(atoms.iter().map(Pos::pos), max_bond);
    let mut pairs = Vec::new();
    for (i, a) in atoms.iter().enumerate() {
        for j in grid.near(a.pos()).filter(|&j| j > i) {
            let b = &atoms[j];
            let limit =
                elements[i].covalent_radius() + elements[j].covalent_radius() + BOND_TOLERANCE;
            if a.chain_id == b.chain_id && a.pos().distance(b.pos()) < limit {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// Catmull–Rom tube through each chain's Cα, broken at gaps. Spheres at the Cα make
/// residues pickable.
fn trace(atoms: &[Atom]) -> Geometry {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (i, a) in atoms.iter().enumerate() {
        if a.name != "CA" || !cgkitten::residue::is_amino_acid(&a.res_name) {
            continue;
        }
        match runs.last_mut() {
            Some(run)
                if run.last().is_some_and(|&j| {
                    atoms[j].chain_id == a.chain_id
                        && atoms[j].pos().distance(a.pos()) <= MAX_CA_STEP
                }) =>
            {
                run.push(i);
            }
            _ => runs.push(vec![i]),
        }
    }

    let mut geometry = Geometry::default();
    for run in &runs {
        let points: Vec<Vec3> = run.iter().map(|&i| atoms[i].pos()).collect();
        geometry
            .balls
            .extend(run.iter().zip(&points).map(|(&item, &center)| Ball {
                item,
                center,
                radius: TRACE_RADIUS,
            }));
        let p = |k: isize| points[k.clamp(0, points.len() as isize - 1) as usize];
        for seg in 0..points.len().saturating_sub(1) {
            let s = seg as isize;
            let (p0, p1, p2, p3) = (p(s - 1), p(s), p(s + 1), p(s + 2));
            // Exact control points at the ends, so neighbouring segments share them.
            let sample = |k: usize| match k {
                0 => p1,
                k if k == TRACE_SAMPLES => p2,
                k => catmull_rom(p0, p1, p2, p3, k as f32 / TRACE_SAMPLES as f32),
            };
            for k in 0..TRACE_SAMPLES {
                let item = run[if 2 * k < TRACE_SAMPLES { seg } else { seg + 1 }];
                geometry.sticks.push(Stick {
                    items: [item, item],
                    ends: [sample(k), sample(k + 1)],
                    radius: TRACE_RADIUS,
                });
            }
        }
    }
    geometry
}

fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

/// Molecular surface (around atoms when known, else beads) with the potential one probe
/// radius outside each vertex, as items in colour-map units.
fn surface(ctx: &Context) -> Result<Shape, String> {
    let mesh = match ctx.surface_mesh.get() {
        Some(mesh) => Rc::clone(mesh),
        None => {
            let (centers, radii): (Vec<[f64; 3]>, Vec<f64>) = if ctx.input.atoms.is_empty() {
                ctx.input
                    .beads
                    .iter()
                    .zip(&ctx.bead_radii)
                    .map(|(b, &r)| ([b.x, b.y, b.z], f64::from(r)))
                    .unzip()
            } else {
                ctx.input
                    .atoms
                    .iter()
                    .zip(&ctx.elements)
                    .map(|(a, e)| ([a.x, a.y, a.z], f64::from(e.vdw_radius())))
                    .unzip()
            };
            let mesh = Rc::new(mesh::gaussian(&centers, &radii)?);
            Rc::clone(ctx.surface_mesh.get_or_init(|| mesh))
        }
    };
    let probes: Vec<Vec3> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .map(|(&p, &n)| p + n * PROBE_RADIUS)
        .collect();
    let (mean, spread): (Vec<f64>, Vec<f64>) = probes.iter().map(|&p| ctx.field.at(p)).unzip();
    // |φ| and σ_φ that saturate their colour maps, so most of the surface uses the range.
    let scale = percentile95(mean.iter().map(|m| m.abs())).max(0.5);
    let spread_scale = percentile95(spread.iter().copied()).max(0.05);
    let mut couplings = Couplings {
        offsets: vec![0],
        entries: Vec::new(),
    };
    for &p in &probes {
        couplings.entries.extend(ctx.field.couplings(p, scale));
        couplings.offsets.push(couplings.entries.len() as u32);
    }
    let items = mean
        .iter()
        .zip(&spread)
        .map(|(m, s)| Item {
            value: m / scale,
            spread: s / spread_scale,
            ..Item::default()
        })
        .collect();
    Ok(Shape {
        mesh,
        items,
        couplings: Some(couplings),
    })
}

fn percentile95(values: impl Iterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = values.collect();
    v.sort_by(f64::total_cmp);
    v.get(v.len() * 95 / 100).copied().unwrap_or(0.0)
}

fn arrow(m: &Moments) -> Geometry {
    let length = m.dipole.length() * ARROW_PER_DIPOLE;
    let dir = m.dipole.normalize_or_zero();
    let tip = m.center + dir * length;
    let neck = m.center + dir * (length - ARROW_HEAD).max(0.0);
    Geometry {
        items: Arc::new([Item::default()]),
        sticks: vec![Stick {
            items: [0, 0],
            ends: [m.center, neck],
            radius: ARROW_RADIUS,
        }],
        shapes: vec![Shape {
            mesh: Rc::new(glyphs::cone(neck, tip, 2.5 * ARROW_RADIUS)),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn quadrupole(style: Representation, m: &Moments, extent: f32) -> Geometry {
    let axes = m.principal_axes();
    let largest = axes.iter().map(|(l, _)| l.abs()).fold(0.0, f32::max);
    // A neutral, isotropic molecule has no shape to show; avoid dividing by zero.
    let scale = if largest > 0.0 { largest } else { 1.0 };
    if style == Representation::Axes {
        return Geometry {
            // Each axis coloured by the sign of its eigenvalue.
            items: axes
                .iter()
                .map(|(lambda, _)| Item {
                    value: f64::from(lambda.signum()),
                    ..Item::default()
                })
                .collect(),
            sticks: axes
                .iter()
                .enumerate()
                .map(|(item, &(lambda, axis))| {
                    let half = axis * extent * lambda.abs() / scale;
                    Stick {
                        items: [item, item],
                        ends: [m.center - half, m.center + half],
                        radius: ARROW_RADIUS,
                    }
                })
                .collect(),
            ..Default::default()
        };
    }
    let mut mesh = glyphs::icosphere(SPHERE_DETAIL);
    let values: Vec<f64> = mesh
        .normals
        .iter()
        .map(|&n| f64::from(m.angular(n) / scale))
        .collect();
    for ((p, &n), &v) in mesh.positions.iter_mut().zip(&mesh.normals).zip(&values) {
        let radius = match style {
            Representation::Lobes => 0.6 * extent * (1.0 + LOBE_AMPLITUDE * v) as f32,
            _ => extent + PROBE_RADIUS,
        };
        *p = m.center + n * radius;
    }
    if style == Representation::Lobes {
        mesh.normals = mesh::vertex_normals(&mesh.positions, &mesh.triangles);
    }
    Geometry {
        shapes: vec![Shape {
            mesh: Rc::new(mesh),
            items: values
                .into_iter()
                .map(|value| Item {
                    value,
                    ..Item::default()
                })
                .collect(),
            couplings: None,
        }],
        ..Default::default()
    }
}
