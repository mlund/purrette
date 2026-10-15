// Triangle meshes (molecular surface) with per-vertex colour and normal.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) contour: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) contour: f32,
};

@vertex
fn vs_main(v: Vertex) -> VertexOut {
    var out: VertexOut;
    out.clip = camera.view_proj * vec4<f32>(v.position, 1.0);
    out.world = v.position;
    out.normal = v.normal;
    out.color = v.color;
    out.contour = v.contour;
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    // Same headlight as the impostors, so all layers read alike.
    let light = normalize(camera.eye.xyz - v.world);
    let diffuse = abs(dot(normalize(v.normal), light));
    let shade = 0.35 + 0.65 * diffuse;
    var rgb = v.color.rgb * shade;
    // Iso-lines at integer contour values, about a pixel wide at any zoom.
    // Derivatives must be taken outside the branch: Chrome rejects them in
    // non-uniform control flow, invalidating the whole frame.
    let width = max(fwidth(v.contour), 1e-6);
    if (v.contour > -1e29) {
        let distance = abs(fract(v.contour + 0.5) - 0.5);
        let line = 1.0 - smoothstep(0.5 * width, 1.5 * width, distance);
        rgb = mix(rgb, vec3<f32>(1.0), 0.8 * line);
    }
    return vec4<f32>(rgb, v.color.a);
}
