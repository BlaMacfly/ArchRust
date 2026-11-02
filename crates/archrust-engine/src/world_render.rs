//! Rendu de l'eau et de l'herbe du monde (AgcmWater, AgcmGrass), appelé par [`crate::render::Renderer`].
//!
//! * **Surfaces d'eau** (`water.wgsl`, `vs_water`/`fs_water`) — quadrilatères de `WaterStatusT1.ini`
//!   en coordonnées monde : texture de la couche 1 × couleur prélit éclairée (formule de RenderWare :
//!   prélit + ambiante × 0,3 + soleil × 0,7 × N·L), plus la texture de la couche 2 (seconde étape
//!   additive du MatFX « dual »), alpha du sommet, mélange alpha, profondeur testée sans écriture.
//! * **Vagues** (`nature.wgsl`, `vs_particle`/`fs_particle`) — quadrilatères texturés × couleur,
//!   mélange alpha source + destination (`FUN_140586ed0` : SRCALPHA / ONE).
//! * **Herbe** (`grass.wgsl`, `vs_grass`/`fs_grass`) — touffes instanciées : une plage de 12
//!   sommets par gabarit, une instance par brin (matrice 3 × 3, position, phase et amplitude du
//!   balancement). Le balancement est calculé dans le nuanceur (`FUN_140592b40` : inclinaison
//!   autour de X), seulement en deçà de la distance du niveau 0.
//!
//! Les données de l'image sont décrites par [`WorldDraw`], rempli par `world.rs`.

use crate::render::{GpuTexture, Vertex};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Sommet d'une surface d'eau : UV des deux couches (défilement déjà appliqué).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct WaterVertex {
    pub position: [f32; 3],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    pub color: [f32; 4],
}

impl WaterVertex {
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4];
}

/// Sommet d'une touffe dans le repère du brin.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct TuftVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

impl TuftVertex {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
}

/// Brin : colonnes de la matrice S·RotX·RotY du brin (xyz) et position (w des trois colonnes),
/// puis (phase initiale en degrés, vitesse en degrés par ms, amplitude en degrés, distance du
/// balancement).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GrassInstance {
    pub c0: [f32; 4],
    pub c1: [f32; 4],
    pub c2: [f32; 4],
    pub sway: [f32; 4],
}

impl GrassInstance {
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4];
}

/// Lot de surfaces d'eau d'un même type (mêmes textures).
#[derive(Debug, Clone, Default)]
pub struct WaterBatch {
    pub texture1: Option<String>,
    pub texture2: Option<String>,
    pub vertices: Vec<WaterVertex>,
}

/// Lot de vagues d'une même texture.
#[derive(Debug, Clone, Default)]
pub struct WaveBatch {
    pub texture: Option<String>,
    pub vertices: Vec<Vertex>,
}

/// Herbe chargée : sommets de tous les gabarits, brins triés par gabarit et lots
/// (texture, plage de sommets, plage d'instances). `version` change à chaque reconstruction.
#[derive(Debug, Clone, Default)]
pub struct GrassDraw {
    pub version: u64,
    pub vertices: Vec<TuftVertex>,
    pub instances: Vec<GrassInstance>,
    pub batches: Vec<(Option<String>, std::ops::Range<u32>, std::ops::Range<u32>)>,
}

/// Eau et herbe de l'image.
#[derive(Debug, Clone, Default)]
pub struct WorldDraw {
    pub water: Vec<WaterBatch>,
    pub waves: Vec<WaveBatch>,
    pub grass: Option<Arc<GrassDraw>>,
    /// Temps du balancement (ms, boucle) et couleur de l'herbe (`0x140592cb0`, rgb).
    pub grass_time_ms: f32,
    pub grass_color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GrassUniform {
    /// x : temps (ms) ; yzw : couleur d'éclairage de l'herbe.
    params: [f32; 4],
}

pub(crate) struct WorldRenderer {
    water_pipeline: wgpu::RenderPipeline,
    wave_pipeline: wgpu::RenderPipeline,
    grass_pipeline: wgpu::RenderPipeline,
    grass_uniform: wgpu::Buffer,
    grass_uniform_bg: wgpu::BindGroup,
    water_buf: Option<(wgpu::Buffer, usize)>,
    wave_buf: Option<(wgpu::Buffer, usize)>,
    /// Herbe téléversée : version, sommets, instances.
    grass_bufs: Option<(u64, wgpu::Buffer, wgpu::Buffer)>,
}

fn dynamic_buffer<T: Pod>(device: &wgpu::Device, queue: &wgpu::Queue, slot: &mut Option<(wgpu::Buffer, usize)>, data: &[T], label: &str) {
    let bytes = std::mem::size_of_val(data);
    if bytes == 0 {
        return;
    }
    if slot.as_ref().is_none_or(|(_, cap)| *cap < bytes) {
        let cap = bytes.next_power_of_two();
        *slot = Some((
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: cap as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            cap,
        ));
    }
    queue.write_buffer(&slot.as_ref().unwrap().0, 0, bytemuck::cast_slice(data));
}

/// Plages préparées pour `draw` : (textures, sommets).
pub(crate) struct WorldPrepared {
    water: Vec<(Arc<GpuTexture>, Arc<GpuTexture>, std::ops::Range<u32>)>,
    waves: Vec<(Arc<GpuTexture>, std::ops::Range<u32>)>,
    grass: Vec<(Arc<GpuTexture>, std::ops::Range<u32>, std::ops::Range<u32>)>,
}

impl WorldRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, globals_layout: &wgpu::BindGroupLayout, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let water_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(include_str!("water.wgsl").into()),
        });
        let grass_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grass"),
            source: wgpu::ShaderSource::Wgsl(include_str!("grass.wgsl").into()),
        });
        let nature_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nature (vagues)"),
            source: wgpu::ShaderSource::Wgsl(include_str!("nature.wgsl").into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grass uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let depth = |write: bool| {
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: write,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        let primitive = wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() };
        let water_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[globals_layout, texture_layout, texture_layout],
            push_constant_ranges: &[],
        });
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&water_layout),
            vertex: wgpu::VertexState {
                module: &water_shader,
                entry_point: Some("vs_water"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WaterVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &WaterVertex::ATTRS,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_shader,
                entry_point: Some("fs_water"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive,
            depth_stencil: depth(false),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let wave_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waves"),
            bind_group_layouts: &[globals_layout, texture_layout],
            push_constant_ranges: &[],
        });
        let wave_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("waves"),
            layout: Some(&wave_layout),
            vertex: wgpu::VertexState { module: &nature_shader, entry_point: Some("vs_particle"), buffers: &[Vertex::layout()], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &nature_shader,
                entry_point: Some("fs_particle"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive,
            depth_stencil: depth(false),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let grass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grass"),
            bind_group_layouts: &[globals_layout, texture_layout, &uniform_layout],
            push_constant_ranges: &[],
        });
        let grass_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grass"),
            layout: Some(&grass_layout),
            vertex: wgpu::VertexState {
                module: &grass_shader,
                entry_point: Some("vs_grass"),
                buffers: &[
                    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<TuftVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &TuftVertex::ATTRS },
                    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<GrassInstance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &GrassInstance::ATTRS },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &grass_shader,
                entry_point: Some("fs_grass"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive,
            depth_stencil: depth(true),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let grass_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grass uniform"),
            size: std::mem::size_of::<GrassUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grass_uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grass uniform"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: grass_uniform.as_entire_binding() }],
        });
        Self { water_pipeline, wave_pipeline, grass_pipeline, grass_uniform, grass_uniform_bg, water_buf: None, wave_buf: None, grass_bufs: None }
    }

    /// Téléverse les données de l'image et résout les textures (`textures` : nom en minuscules).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draw: &WorldDraw, textures: &HashMap<String, Arc<GpuTexture>>, white: &Arc<GpuTexture>, clear: &Arc<GpuTexture>) -> WorldPrepared {
        let tex = |n: &Option<String>, fallback: &Arc<GpuTexture>| n.as_ref().and_then(|n| textures.get(&n.to_ascii_lowercase())).unwrap_or(fallback).clone();
        let mut water_vertices: Vec<WaterVertex> = Vec::new();
        let mut water = Vec::new();
        for b in &draw.water {
            if b.vertices.is_empty() {
                continue;
            }
            let start = water_vertices.len() as u32;
            water_vertices.extend_from_slice(&b.vertices);
            // Couche 2 absente : texture noire transparente (rien n'est ajouté).
            water.push((tex(&b.texture1, white), tex(&b.texture2, clear), start..water_vertices.len() as u32));
        }
        dynamic_buffer(device, queue, &mut self.water_buf, &water_vertices, "water");
        let mut wave_vertices: Vec<Vertex> = Vec::new();
        let mut waves = Vec::new();
        for b in &draw.waves {
            if b.vertices.is_empty() {
                continue;
            }
            let start = wave_vertices.len() as u32;
            wave_vertices.extend_from_slice(&b.vertices);
            waves.push((tex(&b.texture, white), start..wave_vertices.len() as u32));
        }
        dynamic_buffer(device, queue, &mut self.wave_buf, &wave_vertices, "waves");
        let mut grass = Vec::new();
        if let Some(g) = draw.grass.as_ref().filter(|g| !g.instances.is_empty() && !g.vertices.is_empty()) {
            if self.grass_bufs.as_ref().is_none_or(|(v, _, _)| *v != g.version) {
                let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("grass tufts"), contents: bytemuck::cast_slice(&g.vertices), usage: wgpu::BufferUsages::VERTEX });
                let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("grass blades"), contents: bytemuck::cast_slice(&g.instances), usage: wgpu::BufferUsages::VERTEX });
                self.grass_bufs = Some((g.version, vb, ib));
            }
            for (t, verts, insts) in &g.batches {
                grass.push((tex(t, white), verts.clone(), insts.clone()));
            }
            let c = draw.grass_color;
            queue.write_buffer(&self.grass_uniform, 0, bytemuck::bytes_of(&GrassUniform { params: [draw.grass_time_ms, c[0], c[1], c[2]] }));
        }
        WorldPrepared { water, waves, grass }
    }

    /// Herbe (après les objets opaques).
    pub fn draw_grass<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, globals: &'a wgpu::BindGroup, p: &'a WorldPrepared) {
        let Some((_, vb, ib)) = &self.grass_bufs else { return };
        if p.grass.is_empty() {
            return;
        }
        pass.set_pipeline(&self.grass_pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(2, &self.grass_uniform_bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_vertex_buffer(1, ib.slice(..));
        for (t, verts, insts) in &p.grass {
            pass.set_bind_group(1, &t.bind_group, &[]);
            pass.draw(verts.clone(), insts.clone());
        }
    }

    /// Surfaces d'eau puis vagues (transparents, après l'herbe).
    pub fn draw_water<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, globals: &'a wgpu::BindGroup, p: &'a WorldPrepared) {
        if let (Some((buf, _)), false) = (&self.water_buf, p.water.is_empty()) {
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(0, globals, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            for (t1, t2, range) in &p.water {
                pass.set_bind_group(1, &t1.bind_group, &[]);
                pass.set_bind_group(2, &t2.bind_group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        if let (Some((buf, _)), false) = (&self.wave_buf, p.waves.is_empty()) {
            pass.set_pipeline(&self.wave_pipeline);
            pass.set_bind_group(0, globals, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            for (t, range) in &p.waves {
                pass.set_bind_group(1, &t.bind_group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
    }
}
