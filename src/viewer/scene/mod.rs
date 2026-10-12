//! What the viewer shows, independent of egui and wgpu: geometry and colours per frame,
//! and what lies under the cursor.

// Without wgpu nothing draws frames, but picking and tests still use the scene.
#![cfg_attr(not(feature = "wgpu"), allow(dead_code))]

mod camera;
mod coloring;
mod element;
mod fluctuation;
mod glyphs;
mod grid;
mod mesh;
mod multipole;
mod potential;
mod representation;
// Vendored; kept close to upstream rather than restyled.
#[allow(clippy::chunks_exact_to_as_chunks)]
mod surface;

pub use camera::Camera;
pub use coloring::Coloring;
pub use representation::Representation;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use cgkitten::{Atom, Bead, BeadType, forcefield};
use glam::Vec3;

use coloring::{Item, Rgba};
use element::Element;
use representation::Geometry;

/// Probe radius (Å) of a small ion: the potential is shown where such an ion's centre can
/// be, one probe radius outside the surface, rather than on it, where nearby charges spike.
pub const PROBE_RADIUS: f32 = 1.4;

/// `MeshVertex::contour` when a layer has no contour lines.
pub const NO_CONTOUR: f32 = -1e30;
/// Contour lines per colour-map half range.
const CONTOUR_LEVELS: f64 = 5.0;

/// Atoms beyond which the potential surface starts hidden.
const LARGE_ASSEMBLY: usize = 50_000;

/// Radius for beads the force field does not cover (Å).
const DEFAULT_BEAD_RADIUS: f32 = 2.0;

/// Results the scene is built from.
#[derive(Clone, Debug, Default)]
pub struct SceneInput {
    /// Beads with their mean charges.
    pub beads: Vec<Bead>,
    /// Per-bead charge variance (e²), same order as `beads`.
    pub charge_variance: Vec<f64>,
    /// The all-atom structure the beads came from; empty if not available. Shared, as
    /// large assemblies have ~10⁵ atoms.
    pub atoms: Arc<[Atom]>,
    /// Bjerrum length (Å); 0 leaves out the potential layer.
    pub bjerrum_length: f64,
    /// Debye screening length (Å); infinite without salt.
    pub debye_length: f64,
}

/// What a layer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// The original all-atom structure.
    Atoms,
    /// Coarse-grained beads.
    Beads,
    /// Electric potential on the molecular surface.
    Potential,
    /// Dipole moment of the mean charges.
    Dipole,
    /// Quadrupole moment of the mean charges.
    Quadrupole,
}

impl LayerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Atoms => "All atoms",
            Self::Beads => "CG beads",
            Self::Potential => "Potential",
            Self::Dipole => "Dipole",
            Self::Quadrupole => "Quadrupole",
        }
    }

    /// Representations that make sense for this layer; the first is the default.
    pub fn representations(self) -> &'static [Representation] {
        match self {
            Self::Atoms => &[
                Representation::VanDerWaals,
                Representation::Licorice,
                Representation::Trace,
            ],
            Self::Beads => &[Representation::Beads],
            Self::Potential => &[Representation::Surface],
            Self::Dipole => &[Representation::Arrow],
            Self::Quadrupole => &[
                Representation::Lobes,
                Representation::Sphere,
                Representation::Axes,
            ],
        }
    }

    /// Colourings that make sense for this layer; the first is the default.
    pub fn colorings(self) -> &'static [Coloring] {
        match self {
            Self::Atoms => &[Coloring::Element, Coloring::Chain],
            Self::Beads => &[Coloring::Charge, Coloring::Fluctuation, Coloring::Chain],
            Self::Potential => &[Coloring::Potential, Coloring::PotentialSpread],
            Self::Dipole => &[Coloring::Uniform],
            Self::Quadrupole => &[Coloring::Quadrupole],
        }
    }
}

/// One user-selectable part of the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub kind: LayerKind,
    pub representation: Representation,
    pub coloring: Coloring,
    pub visible: bool,
    /// 0–1.
    pub opacity: f32,
    /// Iso-lines on surfaces (see [`Representation::has_contours`]).
    pub contours: bool,
}

impl Layer {
    fn new(kind: LayerKind) -> Self {
        Self {
            kind,
            representation: kind.representations()[0],
            coloring: kind.colorings()[0],
            // Scene::new picks the one layer shown first.
            visible: false,
            opacity: 1.0,
            contours: false,
        }
    }
}

/// A sphere instance, laid out for the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Sphere {
    pub center: [f32; 3],
    pub radius: f32,
    pub color: Rgba,
}

/// A capsule (cylinder with rounded ends) instance, laid out for the GPU; each half
/// takes the colour of the item at that end.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Cylinder {
    pub start: [f32; 3],
    pub radius: f32,
    pub end: [f32; 3],
    /// Keeps the colours 16-byte aligned for the GPU.
    pub _pad: f32,
    pub start_color: Rgba,
    pub end_color: Rgba,
}

/// A surface vertex, laid out for the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: Rgba,
    /// Field value in contour steps (lines at integers), or [`NO_CONTOUR`].
    pub contour: f32,
}

/// Everything to draw at one instant.
///
/// Each list holds opaque items first; from the `translucent_*` index on they are
/// see-through and must be drawn after everything opaque, without writing depth, so
/// what lies behind them still shows.
#[derive(Default)]
pub struct Frame {
    pub spheres: Vec<Sphere>,
    pub translucent_spheres: usize,
    pub cylinders: Vec<Cylinder>,
    pub translucent_cylinders: usize,
    pub mesh_vertices: Vec<MeshVertex>,
    pub mesh_indices: Vec<u32>,
    pub translucent_indices: usize,
}

/// Item under the cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub label: String,
    /// Where the ray hit, Å.
    pub position: [f32; 3],
}

/// Position of a bead or atom in Å, as the viewer's f32 vector.
pub(super) trait Pos {
    fn pos(&self) -> Vec3;
}

impl Pos for Bead {
    fn pos(&self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }
}

impl Pos for Atom {
    fn pos(&self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }
}

/// Everything derived once from the input that representations and labels need.
pub(super) struct Context {
    input: SceneInput,
    moments: multipole::Moments,
    field: potential::Field,
    /// Half the CALVADOS σ: the size beads have in the simulations they are made for.
    bead_radii: Vec<f32>,
    elements: Vec<Element>,
    /// What colourings see of each atom and bead.
    atom_items: Arc<[Item]>,
    bead_items: Arc<[Item]>,
    /// Radius (Å) of the beads about their centre of mass, so glyphs scale with the molecule.
    extent: f32,
    /// The molecular surface depends only on the structure, so a recalculation reuses it.
    surface_mesh: OnceCell<Rc<mesh::Mesh>>,
}

impl Context {
    fn new(input: SceneInput) -> Self {
        let ff = forcefield::from_name("calvados3", Default::default())
            .ok()
            .flatten();
        let bead_radii = input
            .beads
            .iter()
            .map(|b| {
                ff.as_ref()
                    .and_then(|ff| ff.params(&b.res_name, b.bead_type))
                    .map_or(DEFAULT_BEAD_RADIUS, |p| 0.5 * p.sigma as f32)
            })
            .collect();
        let elements: Vec<Element> = input
            .atoms
            .iter()
            .map(|a| Element::parse(&a.element))
            .collect();
        // Chains numbered over atoms and beads together, so a chain keeps its colour
        // across layers.
        let mut chains: Vec<&str> = input
            .atoms
            .iter()
            .map(|a| a.chain_id.as_str())
            .chain(input.beads.iter().map(|b| b.chain_id.as_str()))
            .collect();
        chains.sort_unstable();
        chains.dedup();
        let chain = |id: &str| chains.binary_search(&id).unwrap_or(0);
        let atom_items = input
            .atoms
            .iter()
            .zip(&elements)
            .map(|(a, &element)| Item {
                element,
                chain: chain(&a.chain_id),
                ..Item::default()
            })
            .collect();
        let bead_items = input
            .beads
            .iter()
            .zip(&input.charge_variance)
            .enumerate()
            .map(|(i, (b, &v))| Item {
                charge: b.charge,
                sigma: v.sqrt(),
                chain: chain(&b.chain_id),
                bead: Some(i as u32),
                ..Item::default()
            })
            .collect();
        let moments = multipole::Moments::of(&input.beads);
        let extent = input
            .beads
            .iter()
            .map(|b| b.pos().distance(moments.center))
            .fold(5.0, f32::max);
        Self {
            field: potential::Field::new(&input),
            moments,
            bead_radii,
            elements,
            atom_items,
            bead_items,
            extent,
            surface_mesh: OnceCell::new(),
            input,
        }
    }
}

/// A layer's geometry and the representation it was built for.
type Built = (Representation, Rc<Geometry>);

pub struct Scene {
    ctx: Context,
    layers: Vec<Layer>,
    /// Geometry per layer, built when a layer is first drawn or picked while visible and
    /// again only if its representation changes: meshing a large assembly takes seconds.
    /// Interior, so `pick` can fill it too.
    geometry: RefCell<Vec<Option<Built>>>,
    noise: fluctuation::Noise,
}

impl Scene {
    pub fn new(input: SceneInput) -> Result<Self, String> {
        if input.beads.is_empty() {
            return Err("nothing to show".into());
        }
        if input.charge_variance.len() != input.beads.len() {
            return Err("charge variance does not match beads".into());
        }
        let mut layers = vec![Layer::new(LayerKind::Beads)];
        if !input.atoms.is_empty() {
            layers.insert(0, Layer::new(LayerKind::Atoms));
        }
        if input.bjerrum_length > 0.0 {
            layers.push(Layer::new(LayerKind::Potential));
        }
        layers.push(Layer::new(LayerKind::Dipole));
        layers.push(Layer::new(LayerKind::Quadrupole));
        // The potential is what users come for; beads when it is unavailable or, for large
        // assemblies, too slow to mesh without being asked.
        let first = if input.bjerrum_length > 0.0 && input.atoms.len() <= LARGE_ASSEMBLY {
            LayerKind::Potential
        } else {
            LayerKind::Beads
        };
        for layer in &mut layers {
            layer.visible = layer.kind == first;
        }
        Ok(Self {
            geometry: RefCell::new(vec![None; layers.len()]),
            noise: fluctuation::Noise::new(&input.charge_variance),
            ctx: Context::new(input),
            layers,
        })
    }

    /// Take over the view of `previous`, a scene of the same structure (a recalculation):
    /// its layer settings, and the geometry that depends only on the structure.
    pub fn with_view_of(mut self, previous: &Scene) -> Self {
        for layer in &mut self.layers {
            if let Some(old) = previous.layers.iter().find(|l| l.kind == layer.kind) {
                *layer = old.clone();
            }
        }
        if let Some(mesh) = previous.ctx.surface_mesh.get() {
            let _ = self.ctx.surface_mesh.set(Rc::clone(mesh));
        }
        // Atom geometry carries only elements and chains, not charges.
        let old = previous.geometry.borrow();
        let mut new = self.geometry.borrow_mut();
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.kind == LayerKind::Atoms
                && let Some(j) = previous
                    .layers
                    .iter()
                    .position(|l| l.kind == LayerKind::Atoms)
            {
                new[i] = old[j].clone();
            }
        }
        drop(new);
        self
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layers_mut(&mut self) -> &mut [Layer] {
        &mut self.layers
    }

    /// Whether frames change with time at this pulse amplitude: something visible pulses
    /// and something fluctuates. Lets a paused-looking view stop redrawing.
    pub fn animates(&self, amplitude: f64) -> bool {
        amplitude > 0.0
            && self.ctx.input.charge_variance.iter().any(|&v| v > 0.0)
            && self.layers.iter().any(|l| l.visible && l.coloring.pulses())
    }

    /// Bounding sphere (centre, radius) in Å of the molecule, the same whichever layers
    /// are shown, so toggling layers does not move the camera.
    pub fn bounds(&self) -> ([f32; 3], f32) {
        /// Covers the largest bead or atom sphere and the surface around it.
        const MARGIN: f32 = 4.0;
        let points = || {
            let input = &self.ctx.input;
            input
                .beads
                .iter()
                .map(Pos::pos)
                .chain(input.atoms.iter().map(Pos::pos))
        };
        let count = points().count().max(1);
        let center = points().sum::<Vec3>() / count as f32;
        let radius = points().map(|p| p.distance(center)).fold(0.0, f32::max);
        (center.into(), radius + MARGIN)
    }

    /// Frame at `t` (s); `amplitude` scales the sampled charge fluctuations (1 = as
    /// sampled, 0 = mean charges). Equal arguments give equal frames.
    pub fn frame(&self, t: f64, amplitude: f64) -> Frame {
        let noise = self
            .animates(amplitude)
            .then(|| self.noise.at(t, amplitude));
        let mut frame = Frame::default();
        // Opaque layers first, then see-through ones, which are drawn after.
        for see_through in [false, true] {
            if see_through {
                frame.translucent_spheres = frame.spheres.len();
                frame.translucent_cylinders = frame.cylinders.len();
                frame.translucent_indices = frame.mesh_indices.len();
            }
            for (index, layer) in self.layers.iter().enumerate() {
                if (layer.opacity < 1.0) != see_through {
                    continue;
                }
                if let Some(geometry) = self.geometry(index) {
                    draw(&mut frame, layer, &geometry, noise.as_deref());
                }
            }
        }
        frame
    }

    pub fn pick(&self, camera: &Camera, pixel: [f32; 2]) -> Option<Pick> {
        let (origin, dir) = camera.ray(pixel);
        // (distance, layer kind, item, surface normal) of the nearest hit.
        let mut nearest: Option<(f32, LayerKind, usize, Vec3)> = None;
        let mut consider = |hit: Option<(f32, LayerKind, usize, Vec3)>| {
            if let Some(hit) = hit
                && nearest.is_none_or(|n| hit.0 < n.0)
            {
                nearest = Some(hit);
            }
        };
        for (index, layer) in self.layers.iter().enumerate() {
            let Some(geometry) = self.geometry(index) else {
                continue;
            };
            for ball in &geometry.balls {
                let hit = ray_sphere(origin, dir, ball.center, ball.radius);
                consider(hit.map(|d| (d, layer.kind, ball.item, Vec3::ZERO)));
            }
            for shape in &geometry.shapes {
                let mesh = &shape.mesh;
                for t in &mesh.triangles {
                    let [a, b, c] = t.map(|k| mesh.positions[k as usize]);
                    let hit = ray_triangle(origin, dir, a, b, c);
                    consider(hit.map(|d| (d, layer.kind, 0, (b - a).cross(c - a))));
                }
            }
        }
        let (distance, kind, item, normal) = nearest?;
        let hit = origin + dir * distance;
        Some(Pick {
            label: self.describe(kind, item, hit, normal.normalize_or_zero()),
            position: hit.into(),
        })
    }
}

impl Scene {
    /// Visible layer `i`'s geometry, built on first use; None while hidden.
    fn geometry(&self, i: usize) -> Option<Rc<Geometry>> {
        let layer = &self.layers[i];
        if !layer.visible {
            return None;
        }
        let mut cache = self.geometry.borrow_mut();
        let current = cache[i]
            .as_ref()
            .filter(|(built_for, _)| *built_for == layer.representation);
        if let Some((_, geometry)) = current {
            return Some(Rc::clone(geometry));
        }
        let geometry = Rc::new(layer.representation.build(&self.ctx));
        cache[i] = Some((layer.representation, Rc::clone(&geometry)));
        Some(geometry)
    }

    /// Hover label for what a layer shows at `hit`: item `item` of an atom or bead layer,
    /// or the point on a surface or glyph with outward `normal`.
    fn describe(&self, kind: LayerKind, item: usize, hit: Vec3, normal: Vec3) -> String {
        let (ctx, moments) = (&self.ctx, &self.ctx.moments);
        match kind {
            LayerKind::Atoms => {
                let a = &ctx.input.atoms[item];
                format!("{} {} · {} · {}", a.res_name, a.res_seq, a.chain_id, a.name)
            }
            LayerKind::Beads => bead_label(&ctx.input.beads, item, ctx.input.charge_variance[item]),
            LayerKind::Potential => {
                let (phi, spread) = ctx.field.at(hit + normal * PROBE_RADIUS);
                format!("surface · φ = {phi:+.2} ± {spread:.2} kT/e")
            }
            LayerKind::Dipole => format!("dipole · |μ| = {:.1} e·Å", moments.dipole.length()),
            LayerKind::Quadrupole => {
                let n = (hit - moments.center).normalize_or_zero();
                let norm = moments
                    .quadrupole
                    .to_cols_array()
                    .iter()
                    .map(|x| x * x)
                    .sum::<f32>()
                    .sqrt();
                format!(
                    "quadrupole · f(n̂) = {:+.0} e·Å² · ‖Θ‖ = {norm:.0} e·Å²",
                    moments.angular(n)
                )
            }
        }
    }
}

/// Append a layer's instances and mesh to `frame`; `noise` per bead when animating.
fn draw(frame: &mut Frame, layer: &Layer, geometry: &Geometry, noise: Option<&[f64]>) {
    let pulses = layer.coloring.pulses();
    let bead_noise = |bead: u32| noise.map_or(0.0, |n| n[bead as usize]);
    let paint = |item: &Item, pulse: f64| {
        let mut c = layer.coloring.paint(item, pulse);
        c[3] *= layer.opacity;
        c
    };
    // Once per item, not per ball/stick end: ~10⁵-atom structures must stay interactive.
    let colors: Vec<Rgba> = if geometry.balls.is_empty() && geometry.sticks.is_empty() {
        Vec::new()
    } else {
        geometry
            .items
            .iter()
            .map(|item| paint(item, item.bead.filter(|_| pulses).map_or(0.0, bead_noise)))
            .collect()
    };
    frame
        .spheres
        .extend(geometry.balls.iter().map(|ball| Sphere {
            center: ball.center.into(),
            radius: ball.radius,
            color: colors[ball.item],
        }));
    frame
        .cylinders
        .extend(geometry.sticks.iter().map(|stick| Cylinder {
            start: stick.ends[0].into(),
            radius: stick.radius,
            end: stick.ends[1].into(),
            _pad: 0.0,
            start_color: colors[stick.items[0]],
            end_color: colors[stick.items[1]],
        }));
    for shape in &geometry.shapes {
        let base = frame.mesh_vertices.len() as u32;
        let mesh = &shape.mesh;
        for (v, (&p, &n)) in mesh.positions.iter().zip(&mesh.normals).enumerate() {
            let mut item = shape.items.get(v).copied().unwrap_or_default();
            if pulses && let Some(c) = &shape.couplings {
                item.value += c
                    .at(v)
                    .iter()
                    .map(|&(b, w)| f64::from(w) * bead_noise(b))
                    .sum::<f64>();
            }
            frame.mesh_vertices.push(MeshVertex {
                position: p.into(),
                normal: n.into(),
                color: paint(&item, 0.0),
                contour: if layer.contours && layer.representation.has_contours() {
                    (item.value * CONTOUR_LEVELS) as f32
                } else {
                    NO_CONTOUR
                },
            });
        }
        frame
            .mesh_indices
            .extend(mesh.triangles.iter().flatten().map(|&i| base + i));
    }
}

/// Distance along the ray to the first hit on a sphere, if any.
fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let half_b = oc.dot(dir);
    let disc = half_b * half_b - (oc.length_squared() - radius * radius);
    if disc < 0.0 {
        return None;
    }
    let t = -half_b - disc.sqrt();
    (t > 0.0).then_some(t)
}

/// Distance along the ray to triangle abc (either side), if hit (Möller–Trumbore).
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = origin - a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    if u < 0.0 || v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) / det;
    (t > 0.0).then_some(t)
}

fn bead_label(beads: &[Bead], index: usize, variance: f64) -> String {
    let b = &beads[index];
    // Site beads are named by their charge-centre element; show the residue instead.
    let res_name = beads
        .iter()
        .find(|o| {
            matches!(o.bead_type, BeadType::Residue | BeadType::Titratable)
                && o.chain_id == b.chain_id
                && o.res_seq == b.res_seq
        })
        .map_or(b.res_name.as_str(), |o| o.res_name.as_str());
    let kind = match b.bead_type {
        BeadType::Residue | BeadType::Titratable => "residue",
        BeadType::Virtual => "side chain",
        BeadType::Ntr => "N-terminus",
        BeadType::Ctr => "C-terminus",
        BeadType::Ion => "ion",
    };
    format!(
        "{res_name} {} · {} · {kind} · q = {:+.2} ± {:.2} e",
        b.res_seq,
        b.chain_id,
        b.charge,
        variance.sqrt()
    )
}

#[cfg(test)]
mod tests;
