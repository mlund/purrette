# Viewer design notes (from studying molar_vis)

Source studied: github.com/yesint/molar_vis (egui 0.34, wgpu 29). It is Artistic-2.0;
these notes describe techniques only, no code was copied. Paths below are relative to
`crates/molar_vis_core/src/`.

## 1. Sphere and cylinder impostors

Files: `render/shaders/sphere.wgsl`, `cylinder.wgsl`, `lighting.wgsl`, `render/sphere.rs`.

- **Billboard**: no vertex buffer for the quad; four corners come from `vertex_index`
  (triangle strip, `draw(0..4, 0..n)`), the only buffer is per-instance. Sphere quad sits
  on the centre plane in *view space*, oversized 1.25× so the perspective silhouette is
  covered. Cylinder quad is built in the screen plane: one axis = tube axis projected
  onto the screen, other = screen perpendicular; length = projected half-length + radius,
  oversized 1.4×. This stays valid end-on (a `cross(axis, view)` strip degenerates).
- **Ray setup**: all in view space. Perspective: origin 0, dir = normalize(frag pos).
  Orthographic: origin = (x, y, 0), dir = −z. One uniform flag selects.
- **Sphere**: standard monic quadratic (`b = oc·d`, `c = |oc|²−r²`), discard on miss or
  t < 0. Normal = (hit − centre)/r.
- **Cylinder = capsule**: infinite cylinder (project ray and offset perpendicular to the
  axis, solve quadratic, keep hits with 0 ≤ h ≤ L) plus two hemispherical caps (sphere
  test at each end, keep only the outward half). Nearest of the three wins. Caps remove
  the dark seam where a bond met a separate atom sphere. Two-colour (half-bond) by
  `h < L/2`, with optional smooth blend width.
- **frag_depth**: project the view-space hit with the projection matrix, write z/w.
- **Early-Z trick**: vertex shader overrides `clip.z` to a per-instance *constant* that is
  the NDC depth of the nearest point (sphere: centre.z + r; capsule: max endpoint z + r),
  clamped to [0,1]. That is a lower bound on the true depth, so with
  `@early_depth_test(greater_equal)` the GPU may reject occluded fragments before the
  ray-cast. Per-vertex values would break it (screen-linear interpolation overshoots).
- **Lighting**: per-instance packed material (ambient, diffuse, specular, shininess as
  u8s in one u32). Blinn-Phong key light at a fixed view-space direction plus a weak fill
  light weighted by (1 − n·l). Optional GGX "studio" model.
- **Outline**: no geometry pass. A Fresnel-style edge darkening `colour·(1 − k(1−|n·v|)²)`
  per material, and a toon material that inks where |n·v| < width.
- **AO / shadows**: opaque pass writes an Rgba16Float view-normal target (MRT); a
  fullscreen pass (`shaders/ssao.wgsl`) reconstructs position from depth, samples a fixed
  128-tap spiral (no noise texture, no temporal filter), rejects neighbours below the
  tangent plane, and multiply-blends AO × shadow onto colour. Shadow map = same impostor
  shaders rendered from an ortho light camera (`fs_shadow`, depth only).

Adopt:
- Vertex-pulled quads + per-instance buffer, view-space rays, a perspective/ortho flag:
  minimal code, both projections for free.
- Make every tube (licorice, backbone trace, dipole shaft) one capsule shader with
  optional caps flags; a dipole arrowhead is a cone, add later as a third impostor.
- Always write the conservative `clip.z`; it costs nothing and is ready if WebGPU ever
  exposes early depth test. Skip the attribute itself (not in WebGPU).
- Start with Blinn-Phong + Fresnel edge darkening + depth cue. Defer SSAO: it needs MRT
  and an offscreen pass (see 3); edge darkening gives most of the shape cue cheaply.

## 2. Instancing and buffer layout

Files: `render/sphere.rs`, `render/cylinder.rs`, `render/mesh.rs`, `render.rs`
(`upload_buf`, `update_buf`), `scene.rs` (dirty flags).

- Sphere instance, 32 bytes, `#[repr(C)] Pod`: centre `[f32;3]`, radius, RGBA8 colour
  `u32`, material `u32`, pick id `[u32;2]`. Colour alpha carries opacity. Unpacked in WGSL
  with shifts. Cylinder adds second endpoint, second colour, blend width, strand offset.
- Mesh vertex: position, normal, RGBA8 colour, material.
- One `RepGpu` per representation holding optional sphere/cylinder/line/point/mesh
  buffers. Every representation reduces to these primitive lists (`GeometryData`).
- Upload policy: buffers are `VERTEX | COPY_DST`. If new data fits the existing buffer,
  `queue.write_buffer` in place and update the count (capacity kept when shrinking);
  otherwise reallocate. No partial-range updates.
- Two dirty levels per rep: `geom_dirty` (style/selection/colour change → rebuild +
  re-create) and `coords_dirty` (trajectory frame → rebuild data, write in place).
- Redraw: scene is re-rendered only if geometry, camera (compared by `PartialEq`), size
  or a view flag changed, or something animates; otherwise the cached texture is shown.
- Selection glow pulse is a uniform scalar (`sin(t)` on the CPU), never a rebuild;
  `request_repaint` only while something pulses.

Adopt:
- Same 32-byte sphere layout; replace the pick id with a `u32` instance flags word
  (pulse on/off, phase, layer id) since our picking is CPU.
- Pulsing colours: pass `time` in the camera uniform and modulate in the shader from
  per-instance flags. Keeps buffers static; repaint continuously only while any pulse is
  active.
- Capacity-reusing `write_buffer` is enough; frames of a CG system are small.

## 3. egui-wgpu integration

Files: `render.rs` (top comment, `Targets`, `render_core`), `app/viewport.rs`,
`launch.rs`.

- **No `CallbackTrait`.** The scene is rendered into its own offscreen colour +
  Depth32Float (+ normal, OIT accum/reveal) textures, registered once with
  `renderer.write().register_native_texture`, and shown with `egui::Image`. Reason given:
  egui's pass has no depth attachment they control, and they need MRT and custom depth.
- Resources live in a `SceneRenderer` owned by the app (not in `callback_resources`);
  it is built from `cc.wgpu_render_state` and re-created/resized when the viewport size
  changes.
- **SSAA**: offscreen targets are k× the viewport; egui's linear sampling downsamples.
  This is their only AA, since MSAA cannot smooth `discard`-defined silhouettes.
- `early_z_wgpu_options()`: a `WgpuConfiguration` whose device descriptor requests
  `SHADER_EARLY_DEPTH_TEST` (and timestamp queries when profiling) only if the adapter
  has it, reusing eframe's default limits (webgl2 limits on the GL backend, 2D texture
  limit 8192). The renderer then injects `@early_depth_test` into the WGSL string only
  if the device got the feature. Native only.
- Pass order per frame: shadow map → opaque (+ background gradient) → SSAO/shadow
  multiply → transparent into weighted-blended OIT targets (depth test, no write) →
  fullscreen OIT resolve → selection glow (depth `<=`, blend over) → hover lens.
- **Transparency**: weighted-blended OIT (McGuire–Bavoil). Weight uses *linear* eye depth
  normalised to the molecule's front/back range, steeply biased to the front. OIT is
  skipped when nothing is transparent; without independent blend (WebGL2) transparent reps
  fall back to plain alpha blending in the opaque pass (order-dependent).

Adopt:
- Use render-to-texture instead of a paint callback. It gives our own depth format, MRT
  (needed for OIT), SSAA for impostor edges with MSAA 1, and lets the 3D scene skip
  rendering when only egui widgets change. eframe's `depth_buffer` then becomes unneeded.
- Keep the renderer in an app-owned struct; one `SceneRenderer::render(scene, camera)`
  call per changed frame.
- Layer opacity: start with per-layer sorted alpha (opaque first, then transparent layers
  far to near, depth write off). Move to WBOIT when translucent surfaces over beads look
  wrong; it is ~2 targets + one fullscreen pass.
- Do not port `early_z_wgpu_options`; WebGPU lacks the feature and we target the browser.

## 4. Backbone trace / cartoon

File: `geometry/cartoon.rs`.

- CPU triangle mesh, not impostors. Catmull-Rom spline through Cα with VMD's tension
  (tangent = (P₂−P₀)/1.25), 12 samples per residue, elliptical cross-section rings
  extruded along it; section shape morphs by secondary structure (tube vs flat ribbon,
  arrowheads on strands). β-strand control points are neighbour-averaged to remove pleat.
- **CG (Martini)**: trace = `BB` beads, orientation reference = `SC1`. Helices are
  rebuilt analytically: local axis by windowed centroid + low-pass, a parallel-transported
  frame, unwrapped and smoothed phase, ribbon laid on a uniform spiral around the axis.
- Chains split at gaps and at periodic-box jumps; a ghost control point extends the
  ribbon to the boundary and is rendered dashed.

Adopt:
- Our trace has no SS and one bead per residue: a Catmull-Rom spline through beads
  rendered as a chain of capsule impostors (short segments) needs no mesh and reuses the
  cylinder shader. Switch to an extruded mesh only if segment joints show.
- Split the trace on chain breaks and box jumps (minimum-image check between beads).

## 5. Molecular surface

File: `geometry/surface.rs`, `render/shaders/mesh.wgsl`.

- SES on a grid: rasterise SAS solid (vdW + probe) → exact separable Euclidean distance
  transform (Felzenszwalb–Huttenlocher) from inside voxels to solvent → isosurface at
  `distance − probe = 0` via naive Surface Nets (one vertex per straddling cell,
  watertight). Voxel count capped by coarsening spacing.
- Smoothing: separable [1,2,1] blur of the distance field before extraction, then mesh
  relaxation projected back onto the field. Normals = field gradient at the final vertex.
- Colour: nearest atom per vertex, then Laplacian (1-ring) smoothing along the mesh so
  colours do not bleed across crevices. Colours baked into vertices on the CPU.

Adopt:
- Same pipeline is a good fit (simple, robust, no tables). Store per-vertex scalar
  potential in its own buffer created `STORAGE | VERTEX`; the compute pass writes it and
  the mesh pipeline binds it as a second vertex buffer. Colour-map in the vertex shader
  from a uniform (range, palette), so recolouring never re-uploads geometry.
- Quadrupole angular surface: build as a parametric sphere mesh with radius from the
  angular function; same mesh pipeline and colour-map path.

## 6. Picking

Files: `pick.rs`, `render.rs` (`request_pick`, `poll_pick`), `app/viewport.rs`.

- Native: GPU id buffer. Impostors re-rendered with `fs_pick` into an Rg32Uint target,
  scissored to the single cursor pixel, copied to a buffer, `map_async`, polled next
  frames (1–2 frame lag). Re-pick only when cursor pixel or view changed.
- wasm: CPU ray cast (`pick::pick`): cursor ray from inverse view-proj; analytic
  ray-sphere against displayed atoms (incl. periodic images), Möller–Trumbore against
  cached mesh triangles; nearest wins; result maps back to a real atom index.

Adopt:
- CPU ray cast only. CG systems are thousands of beads; a linear scan is microseconds,
  synchronous and identical on native and web. Add a uniform grid only if profiling
  says so. Test the same radii the shader draws, so hover matches pixels.

## 7. Camera

File: `camera.rs`.

- State: target, orientation quaternion, distance, scene radius, fov, projection, optional
  pivot. Orbit: incremental rotation about the *current* camera up/right by pointer delta
  × constant (trackball-like, no gimbal lock), not a sphere-projected arcball.
- Zoom: logarithmic on distance, shifted toward the cursor. Pan moves target. Roll on
  Shift-drag. A "translate z" moves the rig without changing distance, for slabbing.
- Near/far derived each frame from distance ± scene radius (near ≥ 2 % of radius).
  Ortho extents match the perspective view at the focal distance, so toggling is seamless.
- Fit: frame a bbox along its longest axis; a tighter fit tests every rendered sphere
  against the frustum planes for the actual aspect.

Adopt:
- Same quaternion orbit model; plain and robust. Derive near/far from the bounding
  sphere (we never hard-code planes). Keep `Camera: PartialEq` so "did it move?" is a
  comparison, driving the render-skip in 2.

## 8. Layer / representation model and UI

Files: `scene.rs` (`Representation`), `geometry.rs` (`RepKind`, `RepParams`, `build`),
`color.rs` (`ColorMethod`), `app/rep_panel.rs`.

- `Representation` = selection text + compiled selection, `kind`, `params` (enum with
  one variant per kind, carrying only its knobs; switching kind replaces it), `color`
  scheme enum, `material` (lighting + opacity), `visible`, dirty flags, `RepGpu`.
- One builder `match` turns (atoms, selection, params, colour) into `GeometryData`; the
  renderer never knows rep kinds.
- UI: a row per rep with style/colour pickers and an expandable tabbed settings panel
  (Style / Traj / Periodic / Color). Any `changed()` sets `geom_dirty`; colours are baked
  into instances so a colour change is a rebuild.
- New representation: add a `RepKind` and `RepParams` variant, a builder arm, and a
  style-options UI arm.

Adopt:
- Layer { visible, repr: Repr (enum with params), colouring: Colouring, opacity }.
  Builders produce primitive lists; renderer stays representation-agnostic.
- Keep opacity and pulse out of the baked data (uniform/flags), so toggling them is
  render-only. Visibility is render-only too: skip the draw, keep buffers.

## 9. wasm handling

Files: `launch.rs` (`run_web`), `Cargo.toml`, `render.rs` (`SceneRenderer::new`).

- eframe `WebRunner` started inside `spawn_local`; default `WebOptions`; startup errors
  are written into a `#loading` element. wgpu `webgl` feature enabled as WebGL2 fallback.
- Capability checks at runtime, not cfg: `INDEPENDENT_BLEND` → OIT, downlevel
  `COMPUTE_SHADERS` → GPU ray tracer, `SHADER_EARLY_DEPTH_TEST` → early-Z. cfg-gated:
  GPU picking, background worker threads (rayon), Python.
- GPU readback on wasm is polled each frame instead of a blocking `device.poll`.

Adopt:
- Gate the compute potential on the downlevel `COMPUTE_SHADERS` flag and keep a CPU
  fallback, since the glow build and WebGL2 lack compute.
- Feature-detect once in renderer construction and store booleans; never assume.

## 10. Performance tricks

Sources: `docs/RENDERING_PERFORMANCE_PLAN.md`, `render.rs`, `scene.rs`, `geometry/*`.

- Render-skip: idle frames cost no GPU work; the offscreen texture is reused.
- Skip whole passes when unused (OIT, SSAO, glow, shadow); shadow map reused by a key
  of light camera + geometry revisions.
- Capacity-reusing buffer writes; coordinate-only updates keep structure caches
  (topology, SS, cartoon inputs) and only rebuild positions.
- Surface/cartoon built on capped background jobs (native, ≤ 2, cancel on edit).
- Grid EDT and blur parallelised along slabs; voxel budget cap.
- Conservative early-Z for impostors; vertex-pulled quads (no quad buffer).

Adopt:
- Render-skip + per-layer dirty levels first; they matter more than shader speed for an
  interactive UI app. Add timestamp profiling only when a frame budget is missed.
