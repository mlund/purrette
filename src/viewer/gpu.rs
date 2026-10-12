//! wgpu side of the viewer: pipelines live in egui-wgpu's callback resources, and each
//! frame's instances are uploaded in `prepare`.

use std::ops::Range;

use eframe::egui_wgpu::{self, wgpu};

use super::scene::{Camera, Cylinder, Frame, MeshVertex, Sphere};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
}

impl From<&Camera> for CameraUniform {
    fn from(c: &Camera) -> Self {
        let (right, up) = c.basis();
        Self {
            view_proj: c.view_proj().to_cols_array_2d(),
            eye: c.eye().extend(1.0).into(),
            right: right.extend(0.0).into(),
            up: up.extend(0.0).into(),
        }
    }
}

const SPHERE_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4];

/// Explicit offsets: [`Cylinder`] pads after `end` to keep the colours aligned.
const CYLINDER_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
    attribute(wgpu::VertexFormat::Float32x3, 0, 0),
    attribute(wgpu::VertexFormat::Float32, 12, 1),
    attribute(wgpu::VertexFormat::Float32x3, 16, 2),
    attribute(wgpu::VertexFormat::Float32x4, 32, 3),
    attribute(wgpu::VertexFormat::Float32x4, 48, 4),
];

const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32];

const fn attribute(
    format: wgpu::VertexFormat,
    offset: u64,
    location: u32,
) -> wgpu::VertexAttribute {
    wgpu::VertexAttribute {
        format,
        offset,
        shader_location: location,
    }
}

/// Opaque and see-through pipelines for one shader: see-through ones skip depth writes
/// and draw last, so whatever lies behind them still shows.
struct Pipelines {
    opaque: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
}

impl Pipelines {
    fn new(
        rs: &egui_wgpu::RenderState,
        layout: &wgpu::PipelineLayout,
        label: &str,
        source: &str,
        vertices: wgpu::VertexBufferLayout<'_>,
        primitive: wgpu::PrimitiveState,
    ) -> Self {
        let shader = rs
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = |write_depth: bool| {
            rs.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(vertices.clone())],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: rs.target_format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive,
                    depth_stencil: Some(wgpu::DepthStencilState {
                        // Same mapping eframe uses for its own depth attachment, so formats
                        // can't drift.
                        format: egui_wgpu::depth_format_from_bits(super::DEPTH_BITS, 0)
                            .unwrap_or(wgpu::TextureFormat::Depth24Plus),
                        depth_write_enabled: Some(write_depth),
                        depth_compare: Some(wgpu::CompareFunction::Less),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
        };
        Self {
            opaque: pipeline(true),
            translucent: pipeline(false),
        }
    }

    /// Pipeline and element range for one pass over `count` elements, see-through ones
    /// from `translucent_from` on.
    fn pass(
        &self,
        see_through: bool,
        translucent_from: u32,
        count: u32,
    ) -> (&wgpu::RenderPipeline, Range<u32>) {
        if see_through {
            (&self.translucent, translucent_from..count)
        } else {
            (&self.opaque, 0..translucent_from)
        }
    }
}

/// Grow-only GPU buffer holding a frame's elements.
#[derive(Default)]
struct Buffer {
    buffer: Option<wgpu::Buffer>,
    count: u32,
    translucent_from: u32,
}

impl Buffer {
    fn upload<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        usage: wgpu::BufferUsages,
        items: &[T],
        translucent_from: usize,
    ) {
        self.count = items.len() as u32;
        self.translucent_from = translucent_from as u32;
        let bytes: &[u8] = bytemuck::cast_slice(items);
        if bytes.is_empty() {
            return;
        }
        // Reallocated only when a frame needs more room.
        if !self
            .buffer
            .as_ref()
            .is_some_and(|b| b.size() >= bytes.len() as u64)
        {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes.len().next_power_of_two() as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, bytes);
        }
    }
}

struct Resources {
    camera: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    spheres: (Pipelines, Buffer),
    cylinders: (Pipelines, Buffer),
    meshes: Pipelines,
    vertices: Buffer,
    indices: Buffer,
}

impl Resources {
    fn upload(&mut self, frame: &Frame, device: &wgpu::Device, queue: &wgpu::Queue) {
        let vertex = wgpu::BufferUsages::VERTEX;
        self.spheres.1.upload(
            device,
            queue,
            vertex,
            &frame.spheres,
            frame.translucent_spheres,
        );
        self.cylinders.1.upload(
            device,
            queue,
            vertex,
            &frame.cylinders,
            frame.translucent_cylinders,
        );
        self.vertices
            .upload(device, queue, vertex, &frame.mesh_vertices, 0);
        self.indices.upload(
            device,
            queue,
            wgpu::BufferUsages::INDEX,
            &frame.mesh_indices,
            frame.translucent_indices,
        );
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'static>, see_through: bool) {
        pass.set_bind_group(0, &self.bind_group, &[]);
        // Impostors: four corners per instance.
        for (pipelines, instances) in [&self.spheres, &self.cylinders] {
            let (pipeline, range) =
                pipelines.pass(see_through, instances.translucent_from, instances.count);
            if let Some(buffer) = &instances.buffer
                && !range.is_empty()
            {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..4, range);
            }
        }
        let (pipeline, range) = self.meshes.pass(
            see_through,
            self.indices.translucent_from,
            self.indices.count,
        );
        if let (Some(vertices), Some(indices)) = (&self.vertices.buffer, &self.indices.buffer)
            && !range.is_empty()
        {
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(range, 0, 0..1);
        }
    }
}

/// Sets up pipelines once; false if the app runs without wgpu.
pub fn register(cc: &eframe::CreationContext<'_>) -> bool {
    let Some(rs) = &cc.wgpu_render_state else {
        return false;
    };
    let device = &rs.device;
    let camera = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera"),
        size: std::mem::size_of::<CameraUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera"),
        layout: &bind_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera.as_entire_binding(),
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("viewer"),
        bind_group_layouts: &[Some(&bind_layout)],
        immediate_size: 0,
    });
    let instanced =
        |stride: usize, attributes: &'static [wgpu::VertexAttribute]| wgpu::VertexBufferLayout {
            array_stride: stride as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };
    let strip = wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleStrip,
        ..Default::default()
    };
    let spheres = Pipelines::new(
        rs,
        &layout,
        "spheres",
        include_str!("spheres.wgsl"),
        instanced(std::mem::size_of::<Sphere>(), &SPHERE_ATTRIBUTES),
        strip,
    );
    let cylinders = Pipelines::new(
        rs,
        &layout,
        "capsules",
        include_str!("capsules.wgsl"),
        instanced(std::mem::size_of::<Cylinder>(), &CYLINDER_ATTRIBUTES),
        strip,
    );
    let meshes = Pipelines::new(
        rs,
        &layout,
        "mesh",
        include_str!("mesh.wgsl"),
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MESH_ATTRIBUTES,
        },
        // Closed, outward-wound surfaces: hiding back faces avoids double-blending the
        // far side when see-through.
        wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
    );
    rs.renderer.write().callback_resources.insert(Resources {
        camera,
        bind_group,
        spheres: (spheres, Buffer::default()),
        cylinders: (cylinders, Buffer::default()),
        meshes,
        vertices: Buffer::default(),
        indices: Buffer::default(),
    });
    true
}

/// One frame's draw calls, painted inside the viewer's rect.
pub struct Draw {
    pub camera: Camera,
    /// None redraws what was last uploaded: an unchanged frame (e.g. paused) costs no
    /// rebuild or upload, which matters for 10⁵–10⁶ instances.
    pub frame: Option<Frame>,
}

impl egui_wgpu::CallbackTrait for Draw {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get_mut::<Resources>() {
            let camera = CameraUniform::from(&self.camera);
            queue.write_buffer(&r.camera, 0, bytemuck::bytes_of(&camera));
            if let Some(frame) = &self.frame {
                r.upload(frame, device, queue);
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<Resources>() {
            // Everything opaque first, so see-through items blend over all of it.
            r.draw(pass, false);
            r.draw(pass, true);
        }
    }
}
