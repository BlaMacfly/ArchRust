//! Rendu GPU des ombres projetées (voir `shadow` pour les relevés d'AgcmShadow2) : texture
//! 512 × 512 à un canal, silhouettes des objets vues depuis la lumière (pas de tampon de profondeur,
//! comme la caméra d'Archonia dont `+0x88` est nul), valeur `INTENSITY` × alpha de la texture.
//! La texture est lue par `terrain.wgsl` (groupe 2) ; la passe n'est refaite qu'à une demande
//! ([`ShadowRequest`]), pas à chaque image.

use crate::render::{GpuTexture, Mesh, Vertex};
use glam::Mat4;
use std::collections::HashMap;
use std::sync::Arc;

const PASS_SHADER: &str = r#"
// Passe d'ombre : silhouettes des objets dans la texture d'ombre (voir `shadow_render.rs`).
struct Pass {
    clip: mat4x4<f32>,
    // x : intensité (130/255).
    params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> pass_u: Pass;
@group(0) @binding(1) var<storage, read> models: array<mat4x4<f32>>;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_main(v: VertexIn, @builtin(instance_index) i: u32) -> VertexOut {
    var out: VertexOut;
    out.clip = pass_u.clip * (models[i] * vec4<f32>(v.position, 1.0));
    out.uv = v.uv;
    out.alpha = v.color.a;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Même test alpha que le rendu des objets (`shader.wgsl`) : feuillages découpés.
    let a = textureSample(t_diffuse, s_diffuse, in.uv).a * in.alpha;
    if (a < 0.3) {
        discard;
    }
    return vec4<f32>(pass_u.params.x, 0.0, 0.0, a);
}
"#;

/// Prise demandée : matrice de découpage de la caméra de lumière et objets (maillage, matrice monde).
pub struct ShadowRequest {
    pub clip: Mat4,
    pub items: Vec<(Arc<Mesh>, Mat4)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PassUniform {
    clip: [[f32; 4]; 4],
    params: [f32; 4],
}

pub struct ShadowRenderer {
    view: wgpu::TextureView,
    /// Groupe 2 du terrain : texture d'ombre et son échantillonneur (bords bloqués, linéaire).
    pub sample_layout: wgpu::BindGroupLayout,
    pub sample_bg: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    pass_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    models: wgpu::Buffer,
    models_capacity: usize,
    pass_bg: wgpu::BindGroup,
}

impl ShadowRenderer {
    pub fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let size = crate::shadow::TEXTURE_SIZE;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow2"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        // Filtrage linéaire (`+0x50` du raster = 2, rwFILTERLINEAR) ; bords bloqués : CHOIX
        // d'ArchRust (mode d'adressage d'Archonia non précisé), le shader du sol ignore hors de [0, 1].
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow2"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow2 sample"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sample_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow2 sample"),
            layout: &sample_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let pass_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow2 pass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow2"),
            source: wgpu::ShaderSource::Wgsl(PASS_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow2"),
            bind_group_layouts: &[&pass_layout, texture_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow2"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                // Silhouettes cumulées comme le noir sur blanc d'Archonia inversé : 1 − Π(1 − alpha).
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::R8Unorm, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow2 pass"),
            size: std::mem::size_of::<PassUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (models, pass_bg) = Self::make_models(device, &pass_layout, &uniform, 256);
        ShadowRenderer { view, sample_layout, sample_bg, pipeline, pass_layout, uniform, models, models_capacity: 256, pass_bg }
    }

    fn make_models(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, uniform: &wgpu::Buffer, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
        let models = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow2 models"),
            size: (capacity * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow2 pass"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: models.as_entire_binding() },
            ],
        });
        (models, bg)
    }

    /// Refait la texture d'ombre pour `request` dans `encoder` (avant la scène) : effacée à 0 puis
    /// silhouettes des objets.
    pub fn encode(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, request: &ShadowRequest, textures: &HashMap<String, Arc<GpuTexture>>, white: &Arc<GpuTexture>) {
        let n = request.items.len();
        if n > self.models_capacity {
            let capacity = n.next_power_of_two();
            let (models, bg) = Self::make_models(device, &self.pass_layout, &self.uniform, capacity);
            self.models = models;
            self.pass_bg = bg;
            self.models_capacity = capacity;
        }
        let uniform = PassUniform { clip: request.clip.to_cols_array_2d(), params: [crate::shadow::INTENSITY, 0.0, 0.0, 0.0] };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        if n > 0 {
            let models: Vec<[[f32; 4]; 4]> = request.items.iter().map(|(_, m)| m.to_cols_array_2d()).collect();
            queue.write_buffer(&self.models, 0, bytemuck::cast_slice(&models));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow2"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.pass_bg, &[]);
        for (i, (mesh, _)) in request.items.iter().enumerate() {
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            for part in &mesh.parts {
                let tex = part.texture.as_ref().and_then(|t| textures.get(&t.to_ascii_lowercase())).unwrap_or(white);
                pass.set_bind_group(1, &tex.bind_group, &[]);
                pass.draw_indexed(part.index_start..part.index_start + part.index_count, 0, i as u32..i as u32 + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Shader de la passe d'ombre valide (naga), comme ceux de `sky_render`.
    #[test]
    fn shader_de_passe_valide() {
        let module = wgpu::naga::front::wgsl::parse_str(super::PASS_SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(super::PASS_SHADER)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }
}
