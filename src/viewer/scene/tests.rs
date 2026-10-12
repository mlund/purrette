//! Tests at the Scene seam: build from a hand-made input, observe `frame` and `pick`.

use cgkitten::{Bead, BeadType};

use glam::Vec3;

use super::*;

fn bead(x: f64, charge: f64, res_name: &str, res_seq: i32) -> Bead {
    Bead {
        x,
        y: 0.0,
        z: 0.0,
        charge,
        mass: 100.0,
        res_name: res_name.into(),
        chain_id: "A".into(),
        res_seq,
        bead_type: BeadType::Residue,
    }
}

/// A cation (LYS) left of an anion (GLU), 10 Å apart on x.
fn ion_pair() -> SceneInput {
    SceneInput {
        beads: vec![bead(-5.0, 1.0, "LYS", 1), bead(5.0, -1.0, "GLU", 2)],
        charge_variance: vec![0.0, 0.0],
        ..Default::default()
    }
}

fn camera_on(scene: &Scene) -> Camera {
    let mut camera = Camera::new([400.0, 300.0]);
    let (center, radius) = scene.bounds();
    camera.fit(center, radius);
    camera
}

#[test]
fn beads_are_spheres_coloured_by_charge_and_named_on_pick() {
    let scene = Scene::new(ion_pair()).unwrap();
    let camera = camera_on(&scene);

    let frame = scene.frame(0.0, 1.0);
    assert_eq!(frame.spheres.len(), 2);
    let [cation, anion] = [frame.spheres[0], frame.spheres[1]];
    assert!(cation.color[2] > cation.color[0], "positive is blue");
    assert!(anion.color[0] > anion.color[2], "negative is red");

    let pixel = camera.project([-5.0, 0.0, 0.0]).unwrap();
    let pick = scene.pick(&camera, pixel).unwrap();
    assert!(pick.label.starts_with("LYS 1 · A"), "{}", pick.label);
    assert!(pick.label.contains("+1.00"), "{}", pick.label);

    assert!(scene.pick(&camera, [1.0, 1.0]).is_none(), "corner is empty");
}

#[test]
fn hidden_layers_draw_nothing_and_cannot_be_picked() {
    let mut scene = Scene::new(ion_pair()).unwrap();
    let camera = camera_on(&scene);
    let pixel = camera.project([-5.0, 0.0, 0.0]).unwrap();

    for layer in scene.layers_mut() {
        layer.visible = false;
    }
    assert!(scene.frame(0.0, 1.0).spheres.is_empty());
    assert!(scene.pick(&camera, pixel).is_none());

    for layer in scene.layers_mut() {
        layer.visible = true;
    }
    assert_eq!(scene.frame(0.0, 1.0).spheres.len(), 2);
    assert!(scene.pick(&camera, pixel).is_some());
}

#[test]
fn charge_colour_pulses_only_where_the_charge_fluctuates() {
    let input = SceneInput {
        charge_variance: vec![0.25, 0.0],
        ..ion_pair()
    };
    let colours = |scene: &mut Scene, t: f64| {
        let f = scene.frame(t, 1.0);
        (f.spheres[0].color, f.spheres[1].color)
    };
    let mut scene = Scene::new(input.clone()).unwrap();
    let times = [0.0, 0.7, 1.9, 3.4, 5.2];
    let frames: Vec<_> = times.iter().map(|&t| colours(&mut scene, t)).collect();

    let fluctuating: Vec<_> = frames.iter().map(|f| f.0).collect();
    assert!(fluctuating.windows(2).any(|w| w[0] != w[1]), "σ > 0 pulses");
    assert!(frames.iter().all(|f| f.1 == frames[0].1), "σ = 0 is steady");

    let mut again = Scene::new(input).unwrap();
    assert_eq!(
        colours(&mut again, 1.9),
        frames[2],
        "same input, same frames"
    );
}

#[test]
fn side_chain_beads_are_named_after_their_residue() {
    let mut site = bead(5.0, -1.0, "O", 2); // cgkitten names sites by charge-centre element
    site.bead_type = BeadType::Virtual;
    site.y = 8.0;
    let input = SceneInput {
        beads: vec![bead(5.0, 0.0, "GLU", 2), site],
        charge_variance: vec![0.0, 0.0],
        ..Default::default()
    };
    let scene = Scene::new(input).unwrap();
    let camera = camera_on(&scene);
    let pixel = camera.project([5.0, 8.0, 0.0]).unwrap();
    let label = scene.pick(&camera, pixel).unwrap().label;
    assert!(label.starts_with("GLU 2 · A · side chain"), "{label}");
}

#[test]
fn pulse_amplitude_scales_the_fluctuation() {
    let input = SceneInput {
        charge_variance: vec![0.25, 0.0],
        ..ion_pair()
    };
    let scene = Scene::new(input).unwrap();
    let steady = scene.frame(0.0, 0.0).spheres[0].color;
    assert!(
        [0.7, 1.9, 3.4]
            .iter()
            .all(|&t| scene.frame(t, 0.0).spheres[0].color == steady)
    );

    let swing = |amplitude: f64| {
        let blues: Vec<f32> = (0..50)
            .map(|k| scene.frame(k as f64 * 0.3, amplitude).spheres[0].color[2])
            .collect();
        blues.iter().cloned().fold(f32::MIN, f32::max)
            - blues.iter().cloned().fold(f32::MAX, f32::min)
    };
    assert!(swing(3.0) > swing(1.0));
}

#[test]
fn fluctuation_colouring_is_steady_and_strongest_where_sigma_is_largest() {
    let mut beads = ion_pair().beads;
    beads.push(bead(15.0, 0.0, "ALA", 3));
    let input = SceneInput {
        beads,
        charge_variance: vec![0.2, 0.01, 0.0],
        ..Default::default()
    };
    let mut scene = Scene::new(input).unwrap();
    let layer = &mut scene.layers_mut()[0];
    assert!(layer.kind.colorings().contains(&Coloring::Fluctuation));
    layer.coloring = Coloring::Fluctuation;

    let first: Vec<_> = scene
        .frame(0.0, 1.0)
        .spheres
        .iter()
        .map(|s| s.color)
        .collect();
    for t in [0.4, 1.3, 2.9] {
        let now: Vec<_> = scene
            .frame(t, 1.0)
            .spheres
            .iter()
            .map(|s| s.color)
            .collect();
        assert_eq!(now, first, "static, no pulsing");
    }
    let distance = |a: [f32; 4], b: [f32; 4]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>();
    let [big, small, none] = [first[0], first[1], first[2]];
    assert!(distance(big, none) > distance(small, none));
    assert!(distance(small, none) > 0.0);
}

fn atom(name: &str, element: &str, x: f64, y: f64) -> cgkitten::Atom {
    cgkitten::Atom {
        record_name: "ATOM".into(),
        name: name.into(),
        alt_loc: String::new(),
        res_name: "GLU".into(),
        chain_id: "A".into(),
        res_seq: 2,
        i_code: String::new(),
        x,
        y,
        z: 0.0,
        element: element.into(),
    }
}

#[test]
fn atoms_layer_shows_vdw_spheres_by_element_and_names_atoms() {
    let input = SceneInput {
        atoms: vec![
            atom("CD", "C", 0.0, 10.0),
            atom("OE1", "O", 4.0, 10.0),
            atom("OE2", "O", 8.0, 10.0),
        ]
        .into(),
        ..ion_pair()
    };
    let mut scene = Scene::new(input).unwrap();
    let layer = scene
        .layers_mut()
        .iter_mut()
        .find(|l| l.kind == LayerKind::Atoms)
        .expect("an atoms layer when atoms are given");
    assert!(!layer.visible, "off by default: beads are the main view");
    layer.visible = true;
    for other in scene
        .layers_mut()
        .iter_mut()
        .filter(|l| l.kind != LayerKind::Atoms)
    {
        other.visible = false;
    }

    let spheres = scene.frame(0.0, 1.0).spheres.to_vec();
    assert_eq!(spheres.len(), 3);
    let [carbon, oxygen, oxygen2] = [spheres[0], spheres[1], spheres[2]];
    assert_eq!(oxygen.color, oxygen2.color, "same element, same colour");
    assert_ne!(carbon.color, oxygen.color);
    assert!(carbon.radius > oxygen.radius, "Bondi: C 1.70 Å > O 1.52 Å");

    let camera = camera_on(&scene);
    let pixel = camera.project([4.0, 10.0, 0.0]).unwrap();
    let label = scene.pick(&camera, pixel).unwrap().label;
    assert_eq!(label, "GLU 2 · A · OE1");
}

#[test]
fn licorice_draws_sticks_only_between_bonded_atoms_in_one_chain() {
    let at = |name: &str, element: &str, res_seq: i32, chain: &str, x: f64, y: f64| {
        let mut a = atom(name, element, x, y);
        a.res_seq = res_seq;
        a.chain_id = chain.into();
        a
    };
    // Backbone of residue 2 and the N of residue 3, with real bond lengths (Å).
    let atoms = vec![
        at("N", "N", 2, "A", 0.0, 0.0),
        at("CA", "C", 2, "A", 1.46, 0.0),
        at("C", "C", 2, "A", 1.46 + 1.52, 0.0),
        at("O", "O", 2, "A", 1.46 + 1.52, 1.23),
        at("N", "N", 3, "A", 1.46 + 1.52 + 1.33, 0.0), // peptide bond to C
        at("O", "O", 9, "B", 1.46 + 1.52 + 1.33, -1.4), // close, but another chain
        at("CA", "C", 5, "A", 20.0, 0.0),              // far from everything
    ];
    let mut scene = Scene::new(SceneInput {
        atoms: atoms.into(),
        ..ion_pair()
    })
    .unwrap();
    for layer in scene.layers_mut() {
        layer.visible = layer.kind == LayerKind::Atoms;
        if layer.visible {
            assert!(
                layer
                    .kind
                    .representations()
                    .contains(&Representation::Licorice)
            );
            layer.representation = Representation::Licorice;
        }
    }

    let frame = scene.frame(0.0, 1.0);
    let mut bonds: Vec<[[i32; 2]; 2]> = frame
        .cylinders
        .iter()
        .map(|c| {
            let r = |p: [f32; 3]| [(p[0] * 100.0).round() as i32, (p[1] * 100.0).round() as i32];
            let mut ends = [r(c.start), r(c.end)];
            ends.sort();
            ends
        })
        .collect();
    bonds.sort();
    assert_eq!(
        bonds,
        vec![
            [[0, 0], [146, 0]],     // N–CA
            [[146, 0], [298, 0]],   // CA–C
            [[298, 0], [298, 123]], // C=O
            [[298, 0], [431, 0]],   // peptide C–N
        ]
    );
    let peptide = frame
        .cylinders
        .iter()
        .find(|c| c.end[0] > 4.0 || c.start[0] > 4.0)
        .unwrap();
    assert_ne!(
        peptide.start_color, peptide.end_color,
        "each half takes its atom's colour"
    );
    assert_eq!(
        frame.spheres.len(),
        7,
        "a joint per atom, so ends are rounded"
    );
}

#[test]
fn trace_is_one_continuous_tube_per_chain_through_every_alpha_carbon() {
    let ca = |chain: &str, res_seq: i32, x: f64, y: f64| {
        let mut a = atom("CA", "C", x, y);
        a.chain_id = chain.into();
        a.res_seq = res_seq;
        a.res_name = "ALA".into();
        a
    };
    // A zig-zag of four Cα 3.8 Å apart in chain A, three in chain B far away; a side-chain
    // atom that the trace must ignore.
    let chain_a = [(0.0, 0.0), (3.0, 2.33), (6.0, 0.0), (9.0, 2.33)];
    let chain_b = [(0.0, 30.0), (3.8, 30.0), (7.6, 30.0)];
    let mut atoms: Vec<_> = chain_a
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| ca("A", i as i32, x, y))
        .collect();
    atoms.extend(
        chain_b
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| ca("B", i as i32, x, y)),
    );
    atoms.push(atom("CB", "C", 1.0, -1.5));
    let mut scene = Scene::new(SceneInput {
        atoms: atoms.into(),
        ..ion_pair()
    })
    .unwrap();
    for layer in scene.layers_mut() {
        layer.visible = layer.kind == LayerKind::Atoms;
        if layer.visible {
            assert!(
                layer
                    .kind
                    .representations()
                    .contains(&Representation::Trace)
            );
            layer.representation = Representation::Trace;
        }
    }

    let frame = scene.frame(0.0, 1.0);
    let key = |p: [f32; 3]| {
        [
            (p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
            (p[2] * 1000.0).round() as i64,
        ]
    };
    let mut degree: std::collections::HashMap<[i64; 3], usize> = Default::default();
    let mut links: Vec<([i64; 3], [i64; 3])> = Vec::new();
    for c in &frame.cylinders {
        *degree.entry(key(c.start)).or_default() += 1;
        *degree.entry(key(c.end)).or_default() += 1;
        links.push((key(c.start), key(c.end)));
    }
    // Paths: every point joins two sticks except the two ends of each chain.
    assert_eq!(
        degree.values().filter(|&&d| d == 1).count(),
        4,
        "two chains, two ends each"
    );
    assert!(degree.values().all(|&d| d <= 2), "no branches");
    // Every Cα lies on the tube; the side chain does not.
    for &(x, y) in chain_a.iter().chain(&chain_b) {
        assert!(
            degree.contains_key(&key([x as f32, y as f32, 0.0])),
            "Cα at ({x}, {y})"
        );
    }
    assert!(!degree.contains_key(&key([1.0, -1.5, 0.0])));
    // Two separate components: nothing links chain A (y < 10) with chain B (y = 30).
    assert!(
        links
            .iter()
            .all(|(a, b)| (a[1] < 10_000) == (b[1] < 10_000))
    );
}

#[test]
fn chain_colouring_separates_chains() {
    let mut b = atom("CA", "C", 0.0, 30.0);
    b.chain_id = "B".into();
    let atoms = vec![atom("CA", "C", 0.0, 0.0), atom("N", "N", 1.0, 0.0), b];
    let mut scene = Scene::new(SceneInput {
        atoms: atoms.into(),
        ..ion_pair()
    })
    .unwrap();
    for layer in scene.layers_mut() {
        layer.visible = layer.kind == LayerKind::Atoms;
        if layer.visible {
            assert!(layer.kind.colorings().contains(&Coloring::Chain));
            layer.coloring = Coloring::Chain;
        }
    }
    let s = scene.frame(0.0, 1.0).spheres.to_vec();
    assert_eq!(s[0].color, s[1].color, "same chain, any element");
    assert_ne!(s[0].color, s[2].color);
}

#[test]
fn potential_surface_is_closed_and_reports_debye_huckel_potential() {
    let (bjerrum, debye) = (7.1, 10.0);
    let input = SceneInput {
        beads: vec![bead(0.0, 1.0, "LYS", 1)],
        charge_variance: vec![0.0],
        atoms: Default::default(),
        bjerrum_length: bjerrum,
        debye_length: debye,
    };
    let mut scene = Scene::new(input).unwrap();
    for layer in scene.layers_mut() {
        layer.visible = layer.kind == LayerKind::Potential;
    }
    assert!(
        scene
            .layers_mut()
            .iter()
            .any(|l| l.kind == LayerKind::Potential)
    );

    let frame = scene.frame(0.0, 1.0);
    let triangles = frame.mesh_indices.chunks(3).collect::<Vec<_>>();
    assert!(triangles.len() > 50);
    let mut edges: std::collections::HashMap<(u32, u32), usize> = Default::default();
    for t in &triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    assert!(
        edges.values().all(|&n| n == 2),
        "closed: every edge shared by two triangles"
    );
    let mean_rgb = |i: usize| {
        frame.mesh_vertices.iter().map(|v| v.color[i]).sum::<f32>()
            / frame.mesh_vertices.len() as f32
    };
    assert!(mean_rgb(2) > mean_rgb(0), "positive potential is blue");

    let camera = camera_on(&scene);
    let pick = scene.pick(&camera, [200.0, 150.0]).unwrap();
    let phi: f64 = pick
        .label
        .split("φ = ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.replace('−', "-").replace('+', "").parse().ok())
        .unwrap_or_else(|| panic!("no potential in '{}'", pick.label));
    let hit = Vec3::from(pick.position).length() as f64;
    let r = hit + PROBE_RADIUS as f64;
    let expected = bjerrum * (-r / debye).exp() / r;
    assert!(
        (phi - expected).abs() < 0.03 * expected,
        "φ = {phi}, expected {expected:.3}"
    );
}

fn with_potential(input: SceneInput) -> SceneInput {
    SceneInput {
        bjerrum_length: 7.1,
        debye_length: 10.0,
        ..input
    }
}

#[test]
fn default_view_is_the_potential_alone_at_full_opacity() {
    let input = with_potential(SceneInput {
        atoms: vec![atom("CA", "C", 0.0, 0.0)].into(),
        ..ion_pair()
    });
    let mut scene = Scene::new(input).unwrap();
    let shown: Vec<_> = scene
        .layers_mut()
        .iter()
        .filter(|l| l.visible)
        .cloned()
        .collect();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].kind, LayerKind::Potential);
    assert_eq!(shown[0].opacity, 1.0);
}

#[test]
fn without_a_potential_the_beads_are_shown() {
    let mut scene = Scene::new(ion_pair()).unwrap();
    let shown: Vec<_> = scene
        .layers_mut()
        .iter()
        .filter(|l| l.visible)
        .map(|l| l.kind)
        .collect();
    assert_eq!(shown, vec![LayerKind::Beads]);
}

#[test]
fn recalculated_scene_keeps_the_view_settings() {
    let input = with_potential(ion_pair());
    let mut before = Scene::new(input.clone()).unwrap();
    for layer in before.layers_mut() {
        layer.visible = layer.kind == LayerKind::Beads;
        if layer.kind == LayerKind::Beads {
            layer.coloring = Coloring::Fluctuation;
            layer.opacity = 0.5;
        }
    }
    let mut after = Scene::new(input).unwrap().with_view_of(&before);
    assert_eq!(after.layers_mut(), before.layers_mut());
}

/// Only the given layer shown.
fn show_only(scene: &mut Scene, kind: LayerKind) {
    for layer in scene.layers_mut() {
        layer.visible = layer.kind == kind;
    }
}

#[test]
fn dipole_arrow_starts_at_the_centre_and_scales_with_the_moment() {
    // ±q at x = ±5 Å: μ = 10 q e·Å along x, about the centre of mass at the origin.
    let pair = |q: f64| SceneInput {
        beads: vec![bead(5.0, q, "LYS", 1), bead(-5.0, -q, "GLU", 2)],
        ..ion_pair()
    };
    let shaft = |q: f64| {
        let mut scene = Scene::new(pair(q)).unwrap();
        assert!(
            scene
                .layers_mut()
                .iter()
                .any(|l| l.kind == LayerKind::Dipole)
        );
        show_only(&mut scene, LayerKind::Dipole);
        let frame = scene.frame(0.0, 1.0);
        assert!(!frame.mesh_indices.is_empty(), "arrow head");
        assert_eq!(frame.cylinders.len(), 1, "one shaft");
        let c = frame.cylinders[0];
        (Vec3::from(c.start), Vec3::from(c.end))
    };
    let (start, end) = shaft(1.0);
    assert!(start.length() < 1e-4, "from the centre of mass");
    assert!(
        end.x > 0.0 && end.y.abs() < 1e-4 && end.z.abs() < 1e-4,
        "along μ"
    );
    let (_, end_doubled) = shaft(2.0);
    assert!(end_doubled.x > end.x * 1.5, "longer for a larger moment");
}

/// +q at z = ±2 Å and −2q at the origin: Θzz = 2a²q = 8, Θxx = Θyy = −4 (e·Å²).
fn linear_quadrupole() -> SceneInput {
    let at_z = |z: f64, q: f64, seq: i32| Bead {
        z,
        ..bead(0.0, q, "LYS", seq)
    };
    SceneInput {
        beads: vec![at_z(2.0, 1.0, 1), at_z(0.0, -2.0, 2), at_z(-2.0, 1.0, 3)],
        charge_variance: vec![0.0; 3],
        ..Default::default()
    }
}

fn quadrupole_scene(representation: Representation, contours: bool) -> Scene {
    let mut scene = Scene::new(linear_quadrupole()).unwrap();
    show_only(&mut scene, LayerKind::Quadrupole);
    let layer = scene
        .layers_mut()
        .iter_mut()
        .find(|l| l.kind == LayerKind::Quadrupole)
        .unwrap();
    assert!(layer.kind.representations().contains(&representation));
    layer.representation = representation;
    layer.contours = contours;
    scene
}

#[test]
fn quadrupole_lobes_bulge_and_are_blue_along_the_positive_axis() {
    let scene = quadrupole_scene(Representation::Lobes, false);
    let frame = scene.frame(0.0, 1.0);
    let along = |axis: Vec3| {
        frame
            .mesh_vertices
            .iter()
            .max_by(|a, b| {
                let dir = |v: &MeshVertex| Vec3::from(v.position).normalize_or_zero().dot(axis);
                dir(a).total_cmp(&dir(b))
            })
            .copied()
            .unwrap()
    };
    let (z, x) = (along(Vec3::Z), along(Vec3::X));
    let (rz, rx) = (
        Vec3::from(z.position).length(),
        Vec3::from(x.position).length(),
    );
    assert!(rz > rx, "bulge along z ({rz}) beyond x ({rx})");
    assert!(z.color[2] > z.color[0], "f(ẑ) > 0 is blue");
    assert!(x.color[0] > x.color[2], "f(x̂) < 0 is red");
    assert!(frame.mesh_vertices.iter().all(|v| v.contour == NO_CONTOUR));
}

#[test]
fn quadrupole_axes_follow_the_eigenvalues() {
    let scene = quadrupole_scene(Representation::Axes, false);
    let frame = scene.frame(0.0, 1.0);
    assert_eq!(frame.cylinders.len(), 3);
    let mut sticks: Vec<(f32, Vec3)> = frame
        .cylinders
        .iter()
        .map(|c| {
            let d = Vec3::from(c.end) - Vec3::from(c.start);
            (d.length(), d.normalize())
        })
        .collect();
    sticks.sort_by(|a, b| b.0.total_cmp(&a.0));
    assert!(sticks[0].1.z.abs() > 0.999, "longest along z");
    assert!(
        (sticks[0].0 - 2.0 * sticks[1].0).abs() < 1e-3 * sticks[0].0,
        "|8| = 2·|−4|"
    );
    assert!((sticks[1].0 - sticks[2].0).abs() < 1e-3 * sticks[0].0);
}

#[test]
fn contours_mark_quadrupole_levels_on_the_sphere() {
    let scene = quadrupole_scene(Representation::Sphere, true);
    let frame = scene.frame(0.0, 1.0);
    let pole = frame
        .mesh_vertices
        .iter()
        .max_by(|a, b| a.position[2].total_cmp(&b.position[2]))
        .unwrap();
    let equator = frame
        .mesh_vertices
        .iter()
        .max_by(|a, b| a.position[0].total_cmp(&b.position[0]))
        .unwrap();
    assert!(
        pole.contour > 0.0 && equator.contour < 0.0,
        "levels follow f's sign"
    );
}

#[test]
fn hovering_multipole_glyphs_names_them_with_their_values() {
    let scene = quadrupole_scene(Representation::Sphere, false);
    scene.frame(0.0, 1.0);
    let mut camera = Camera::new([400.0, 300.0]);
    camera.fit([0.0; 3], 30.0);
    let label = scene.pick(&camera, [200.0, 150.0]).unwrap().label;
    assert!(label.starts_with("quadrupole"), "{label}");
    assert!(label.contains("e·Å²"), "{label}");
}

#[test]
fn see_through_layers_come_after_opaque_ones() {
    let mut scene = Scene::new(SceneInput {
        atoms: vec![atom("CA", "C", 0.0, 0.0), atom("N", "N", 1.4, 0.0)].into(),
        ..ion_pair()
    })
    .unwrap();
    for layer in scene.layers_mut() {
        layer.visible = matches!(layer.kind, LayerKind::Atoms | LayerKind::Beads);
        if layer.kind == LayerKind::Atoms {
            layer.opacity = 0.3;
        }
    }
    let frame = scene.frame(0.0, 1.0);
    let start = frame.translucent_spheres;
    assert_eq!(start, 2, "the two beads first, drawn with depth");
    assert!(frame.spheres[start..].iter().all(|s| s.color[3] < 1.0));
    assert!(frame.spheres[..start].iter().all(|s| s.color[3] == 1.0));
}
