//! Rendu wgpu du ciel, des particules de météo et des reflets du soleil (voir `sky.wgsl`,
//! `nature.wgsl`). Les données viennent de [`crate::sky`] et [`crate::weather`] par la colle
//! `app_nature.rs` ; le moteur ([`crate::render::Renderer`]) appelle ces passes dans sa passe de scène
//! (ciel avant le sol, particules après les objets) puis dans une passe sans profondeur (reflets).

use crate::render::{GpuTexture, Vertex};
use crate::sky::LayerKind;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Couches de ciel du shader.
pub const SKY_LAYERS: usize = 8;

/// Couche texturée du ciel à dessiner (couleur en espace linéaire).
#[derive(Debug, Clone, PartialEq)]
pub struct SkyDrawLayer {
    pub kind: LayerKind,
    /// Texture déjà téléversée (nom du moteur de rendu).
    pub texture: String,
    pub color: [f32; 4],
    pub offset: [f32; 2],
    pub scale: f32,
}

/// Ciel d'une image.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyDraw {
    /// Haut et bas du dôme (linéaire).
    pub top: [f32; 3],
    pub bottom: [f32; 3],
    /// Opacité des étoiles (0 = aucune).
    pub stars: f32,
    pub sun_dir: Vec3,
    /// Demi-taille angulaire du soleil (tangente).
    pub sun_size: f32,
    /// Temps de jeu (scintillement des étoiles toutes les 100 ms).
    pub time_ms: u64,
    /// Dans l'ordre de dessin, au plus [`SKY_LAYERS`].
    pub layers: Vec<SkyDrawLayer>,
}

/// Reflet du soleil (`LensFlare.ini`) : quadrilatère écran posé sur la droite soleil → centre.
#[derive(Debug, Clone, PartialEq)]
pub struct FlareDraw {
    pub texture: String,
    /// Demi-taille, en hauteur d'écran NDC (`Size` de 0 à 1).
    pub half: f32,
    /// 1 = sur le soleil, 0 = centre de l'écran, −1 = à l'opposé.
    pub line_pos: f32,
    /// Couleur (linéaire) × opacité.
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    inv_view_proj: [[f32; 4]; 4],
    top: [f32; 4],
    bottom: [f32; 4],
    sun: [f32; 4],
    params: [f32; 4],
    layer_color: [[f32; 4]; SKY_LAYERS],
    layer_info: [[f32; 4]; SKY_LAYERS],
}

/// Échelle des deux couches de nuages et répétitions de la bande d'horizon sur un tour.
pub const CLOUD_SCALES: [f32; 2] = [0.35, 0.22];
pub const CIRCUMSTANCE_REPEAT: f32 = 4.0;
/// Demi-taille angulaire de l'astre (tangente).
pub const SUN_SIZE: f32 = 0.09;
/// Élévation de la lune / du soleil : la lumière directionnelle d'Archonia a une orientation fixe,
/// sans course ; l'astre est posé dans son azimut, à 30° au-dessus de l'horizon.
pub const SUN_ELEVATION_DEG: f32 = 30.0;

/// Couleur du fichier (gamma, 0-1) vers l'espace linéaire du rendu.
pub fn to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn lin3(c: [f32; 3]) -> [f32; 3] {
    c.map(to_linear)
}

/// Direction de l'astre : azimut de la lumière, élévation fixe.
pub fn sun_direction(light_dir: Vec3) -> Vec3 {
    let flat = Vec3::new(light_dir.x, 0.0, light_dir.z).normalize_or(Vec3::X);
    let e = SUN_ELEVATION_DEG.to_radians();
    (flat * e.cos() + Vec3::Y * e.sin()).normalize()
}

/// Ciel à dessiner pour une image de [`crate::sky::SkyState`] : couches dans l'ordre soleil,
/// nuages, horizon ; `texture` donne le nom de texture chargée d'un indice d'`alefskysetting.ini`
/// (`None` : couche omise).
pub fn build_sky_draw(
    frame: &crate::sky::SkyFrame,
    hour: u32,
    game_ms: u64,
    sun_dir: Vec3,
    cloud_offset: [f32; 2],
    mut texture: impl FnMut(i32) -> Option<String>,
) -> SkyDraw {
    let order = |k: LayerKind| match k {
        LayerKind::Sun => 0,
        LayerKind::Cloud(_) => 1,
        LayerKind::Circumstance => 2,
    };
    let mut sorted = frame.layers.clone();
    sorted.sort_by_key(|l| order(l.kind));
    let mut layers = Vec::new();
    for l in sorted {
        if layers.len() >= SKY_LAYERS {
            break;
        }
        let Some(name) = texture(l.texture) else { continue };
        let (offset, scale) = match l.kind {
            LayerKind::Cloud(i) => {
                let o = cloud_offset[i as usize % 2];
                ([o, o * 0.5], CLOUD_SCALES[i as usize % 2])
            }
            LayerKind::Circumstance => ([0.0, 0.0], CIRCUMSTANCE_REPEAT),
            LayerKind::Sun => ([0.0, 0.0], 1.0),
        };
        let c = l.color;
        layers.push(SkyDrawLayer { kind: l.kind, texture: name, color: [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3]], offset, scale });
    }
    SkyDraw {
        top: lin3(frame.sky_top),
        bottom: lin3(frame.sky_bottom),
        stars: if crate::sky::stars_visible(frame.weather1, hour) { 1.0 } else { 0.0 },
        sun_dir,
        sun_size: SUN_SIZE,
        time_ms: game_ms,
        layers,
    }
}

/// Inverse de la vue-projection centrée sur la caméra (sans translation), pour retrouver la
/// direction de chaque pixel sans perte de précision loin de l'origine du monde.
pub fn camera_relative_inverse(view_proj: Mat4, camera_pos: Vec3) -> Mat4 {
    (view_proj * Mat4::from_translation(camera_pos)).inverse()
}

/// Position écran (NDC) d'une direction vue depuis la caméra, si elle est devant.
pub fn project_direction(view_proj: Mat4, camera_pos: Vec3, dir: Vec3) -> Option<[f32; 2]> {
    let p = view_proj * (camera_pos + dir.normalize_or_zero() * 1_000.0).extend(1.0);
    (p.w > 1e-4).then(|| [p.x / p.w, p.y / p.w])
}

/// Quadrilatères écran des reflets, au format des sommets du moteur (la normale porte la position
/// écran du soleil pour le test d'occultation de `nature.wgsl`).
pub fn flare_vertices(flares: &[FlareDraw], sun: [f32; 2], aspect: f32) -> Vec<(String, [Vertex; 6])> {
    flares
        .iter()
        .map(|f| {
            let (hx, hy) = (f.half / aspect.max(0.01), f.half);
            let center = [sun[0] * f.line_pos, sun[1] * f.line_pos];
            let v = |dx: f32, dy: f32, u: f32, w: f32| Vertex {
                position: [center[0] + dx * hx, center[1] + dy * hy, 0.0],
                normal: [sun[0], sun[1], 0.0],
                uv: [u, w],
                color: f.color,
            };
            let (a, b, c, d) = (v(-1.0, -1.0, 0.0, 1.0), v(1.0, -1.0, 1.0, 1.0), v(1.0, 1.0, 1.0, 0.0), v(-1.0, 1.0, 0.0, 0.0));
            (f.texture.clone(), [a, b, c, a, c, d])
        })
        .collect()
}

/// Pipelines et ressources du ciel.
pub struct SkyRenderer {
    sky_pipeline: wgpu::RenderPipeline,
    sky_uniform: wgpu::Buffer,
    sky_uniform_bg: wgpu::BindGroup,
    sky_tex_layout: wgpu::BindGroupLayout,
    /// Groupe des textures courantes (noms dans l'ordre des couches).
    sky_tex_bg: Option<(Vec<String>, wgpu::BindGroup)>,
    particle_pipeline: wgpu::RenderPipeline,
    flare_pipeline: wgpu::RenderPipeline,
    depth_layout: wgpu::BindGroupLayout,
    /// Tampons de sommets dynamiques (particules, reflets) et leur capacité en sommets.
    particle_buf: Option<(wgpu::Buffer, usize)>,
    flare_buf: Option<(wgpu::Buffer, usize)>,
    /// Effets : un pipeline par mélange (indice = `EffectBlend as usize`) et leur tampon.
    effect_pipelines: [wgpu::RenderPipeline; 5],
    effect_buf: Option<(wgpu::Buffer, usize)>,
}

/// Mélange d'un effet, d'après `AgcdEffRenderBase` (`switch` de FUN_1408f8030 d'Archonia : états
/// RenderWare `SRCBLEND` (10) / `DESTBLEND` (11) et `D3DRS_BLENDOP` (0xab)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectBlend {
    /// Source alpha, 1 − source alpha (types 0, 1, 4, 8 et défaut).
    Alpha = 0,
    /// Source alpha + destination (type 3, le plus courant).
    AddAlpha = 1,
    /// Source + destination (type 2).
    AddOne = 2,
    /// Destination − source alpha (type 7).
    RevSubAlpha = 3,
    /// Destination − source (type 6).
    RevSubOne = 4,
}

impl EffectBlend {
    pub const ALL: [EffectBlend; 5] = [EffectBlend::Alpha, EffectBlend::AddAlpha, EffectBlend::AddOne, EffectBlend::RevSubAlpha, EffectBlend::RevSubOne];

    /// Mélange d'une valeur des données. Les types 5 et 9 (couleur source / 1 − couleur source) et
    /// les soustractions 10 à 13, rares, retombent sur le mélange alpha.
    pub fn from_data(v: u32) -> Self {
        match v {
            2 => EffectBlend::AddOne,
            3 => EffectBlend::AddAlpha,
            6 => EffectBlend::RevSubOne,
            7 => EffectBlend::RevSubAlpha,
            _ => EffectBlend::Alpha,
        }
    }

    fn factors(self) -> (wgpu::BlendFactor, wgpu::BlendFactor, wgpu::BlendOperation) {
        use wgpu::{BlendFactor as F, BlendOperation as O};
        match self {
            EffectBlend::Alpha => (F::SrcAlpha, F::OneMinusSrcAlpha, O::Add),
            EffectBlend::AddAlpha => (F::SrcAlpha, F::One, O::Add),
            EffectBlend::AddOne => (F::One, F::One, O::Add),
            EffectBlend::RevSubAlpha => (F::SrcAlpha, F::One, O::ReverseSubtract),
            EffectBlend::RevSubOne => (F::One, F::One, O::ReverseSubtract),
        }
    }
}

fn dynamic_buffer(device: &wgpu::Device, slot: &mut Option<(wgpu::Buffer, usize)>, queue: &wgpu::Queue, vertices: &[Vertex], label: &str) {
    let need = vertices.len().max(1);
    if slot.as_ref().is_none_or(|(_, cap)| *cap < need) {
        let cap = need.next_power_of_two();
        *slot = Some((
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (cap * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            cap,
        ));
    }
    if !vertices.is_empty() {
        queue.write_buffer(&slot.as_ref().unwrap().0, 0, bytemuck::cast_slice(vertices));
    }
}

impl SkyRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
        texture_layout: &wgpu::BindGroupLayout,
        vertex_layout: wgpu::VertexBufferLayout<'static>,
    ) -> Self {
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });
        let nature_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nature"),
            source: wgpu::ShaderSource::Wgsl(include_str!("nature.wgsl").into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..SKY_LAYERS as u32)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: SKY_LAYERS as u32,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let sky_tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("sky textures"), entries: &entries });
        let sky_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniform"),
            size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky uniform"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sky_uniform.as_entire_binding() }],
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[&uniform_layout, &sky_tex_layout],
            push_constant_ranges: &[],
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState { module: &sky_shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("flare depth"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let nature_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nature"),
            bind_group_layouts: &[globals_layout, texture_layout, &depth_layout],
            push_constant_ranges: &[],
        });
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[globals_layout, texture_layout],
            push_constant_ranges: &[],
        });
        let particle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&particle_layout),
            vertex: wgpu::VertexState {
                module: &nature_shader,
                entry_point: Some("vs_particle"),
                buffers: std::slice::from_ref(&vertex_layout),
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &nature_shader,
                entry_point: Some("fs_particle"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        // Effets d'Archonia (`app_effects.rs`) : même nuanceur que les particules, un pipeline par
        // mélange d'`AgcdEffRenderBase` (voir `EffectBlend`).
        let effect_pipelines = EffectBlend::ALL.map(|blend| {
            let (src, dst, op) = blend.factors();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("effects"),
                layout: Some(&particle_layout),
                vertex: wgpu::VertexState {
                    module: &nature_shader,
                    entry_point: Some("vs_particle"),
                    buffers: std::slice::from_ref(&vertex_layout),
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &nature_shader,
                    entry_point: Some("fs_particle"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent { src_factor: src, dst_factor: dst, operation: op },
                            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Greater,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        });
        let flare_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("flare"),
            layout: Some(&nature_layout),
            vertex: wgpu::VertexState {
                module: &nature_shader,
                entry_point: Some("vs_flare"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &nature_shader,
                entry_point: Some("fs_flare"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(additive), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self {
            sky_pipeline,
            sky_uniform,
            sky_uniform_bg,
            sky_tex_layout,
            sky_tex_bg: None,
            particle_pipeline,
            flare_pipeline,
            depth_layout,
            particle_buf: None,
            flare_buf: None,
            effect_pipelines,
            effect_buf: None,
        }
    }

    /// Téléverse les sommets de tous les effets de l'image (lots mis bout à bout).
    pub fn prepare_effects(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[Vertex]) {
        dynamic_buffer(device, &mut self.effect_buf, queue, vertices, "effects");
    }

    /// Un lot d'effets (sommets `range` du tampon de `prepare_effects`).
    pub fn draw_effect(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup, texture: &wgpu::BindGroup, blend: EffectBlend, range: std::ops::Range<u32>) {
        if let Some((buf, _)) = &self.effect_buf {
            if !range.is_empty() {
                pass.set_pipeline(&self.effect_pipelines[blend as usize]);
                pass.set_bind_group(0, globals, &[]);
                pass.set_bind_group(1, texture, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(range, 0..1);
            }
        }
    }

    /// Prépare le ciel de l'image (uniformes, groupe de textures).
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_sky(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        textures: &HashMap<String, Arc<GpuTexture>>,
        white: &Arc<GpuTexture>,
        view_proj: Mat4,
        camera_pos: Vec3,
        sky: &SkyDraw,
    ) {
        let mut u = SkyUniform::zeroed();
        u.inv_view_proj = camera_relative_inverse(view_proj, camera_pos).to_cols_array_2d();
        u.top = [sky.top[0], sky.top[1], sky.top[2], 1.0];
        u.bottom = [sky.bottom[0], sky.bottom[1], sky.bottom[2], sky.stars];
        let s = sky.sun_dir.normalize_or(Vec3::Y);
        u.sun = [s.x, s.y, s.z, sky.sun_size.max(1e-3)];
        u.params = [((sky.time_ms / 100) % 4096) as f32, 0.0, 0.0, 0.0];
        let mut names = Vec::with_capacity(SKY_LAYERS);
        for i in 0..SKY_LAYERS {
            match sky.layers.get(i) {
                Some(l) => {
                    u.layer_color[i] = l.color;
                    let kind = match l.kind {
                        LayerKind::Cloud(_) => 1.0,
                        LayerKind::Circumstance => 2.0,
                        LayerKind::Sun => 3.0,
                    };
                    u.layer_info[i] = [kind, l.offset[0], l.offset[1], l.scale];
                    names.push(l.texture.to_ascii_lowercase());
                }
                None => names.push(String::new()),
            }
        }
        queue.write_buffer(&self.sky_uniform, 0, bytemuck::bytes_of(&u));
        if self.sky_tex_bg.as_ref().map(|(n, _)| n) != Some(&names) {
            let views: Vec<Arc<GpuTexture>> = names.iter().map(|n| textures.get(n).cloned().unwrap_or_else(|| white.clone())).collect();
            let mut entries: Vec<wgpu::BindGroupEntry> =
                views.iter().enumerate().map(|(i, t)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(&t.view) }).collect();
            entries.push(wgpu::BindGroupEntry { binding: SKY_LAYERS as u32, resource: wgpu::BindingResource::Sampler(sampler) });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("sky textures"), layout: &self.sky_tex_layout, entries: &entries });
            self.sky_tex_bg = Some((names, bg));
        }
    }

    /// Ciel préparé, en premier dans la passe de scène.
    pub fn draw_sky(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some((_, bg)) = &self.sky_tex_bg {
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_uniform_bg, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Téléverse les sommets des particules ; rend leur nombre.
    pub fn prepare_particles(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[Vertex]) -> u32 {
        dynamic_buffer(device, &mut self.particle_buf, queue, vertices, "particles");
        vertices.len() as u32
    }

    /// Particules, après les objets (le groupe 0 des globales est déjà lié).
    pub fn draw_particles(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup, texture: &wgpu::BindGroup, count: u32) {
        if let Some((buf, _)) = &self.particle_buf {
            if count > 0 {
                pass.set_pipeline(&self.particle_pipeline);
                pass.set_bind_group(0, globals, &[]);
                pass.set_bind_group(1, texture, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..count, 0..1);
            }
        }
    }

    /// Téléverse les reflets (6 sommets chacun, dans l'ordre de `textures`).
    pub fn prepare_flares(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[Vertex]) {
        dynamic_buffer(device, &mut self.flare_buf, queue, vertices, "flares");
    }

    pub fn depth_bind_group(&self, device: &wgpu::Device, depth: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flare depth"),
            layout: &self.depth_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(depth) }],
        })
    }

    /// Reflets, dans une passe sans profondeur : un quadrilatère (6 sommets) par texture.
    pub fn draw_flares(&self, pass: &mut wgpu::RenderPass<'_>, globals: &wgpu::BindGroup, depth: &wgpu::BindGroup, textures: &[&wgpu::BindGroup]) {
        let Some((buf, _)) = &self.flare_buf else { return };
        pass.set_pipeline(&self.flare_pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(2, depth, &[]);
        pass.set_vertex_buffer(0, buf.slice(..));
        for (i, tex) in textures.iter().enumerate() {
            pass.set_bind_group(1, *tex, &[]);
            let start = i as u32 * 6;
            pass.draw(start..start + 6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(src: &str) {
        let module = wgpu::naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn shaders_valides() {
        validate(include_str!("sky.wgsl"));
        validate(include_str!("nature.wgsl"));
        validate(include_str!("shader.wgsl"));
        validate(include_str!("terrain.wgsl"));
    }

    #[test]
    fn couleurs_lineaires_et_astre() {
        assert_eq!(to_linear(0.0), 0.0);
        assert!((to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!((to_linear(0.5) - 0.214).abs() < 1e-3);
        assert_eq!(to_linear(2.0), 1.0);
        let s = sun_direction(Vec3::new(0.4, 0.8, 0.3));
        assert!((s.length() - 1.0).abs() < 1e-5);
        assert!((s.y - 0.5).abs() < 1e-5);
        assert!((s.x / s.z - 0.4 / 0.3).abs() < 1e-4);
    }

    /// Rendu sans affichage du ciel (adaptateur sans surface ; ignoré s'il n'y en a pas) : les
    /// pipelines du ciel, des particules et des reflets passent la validation wgpu, le bas de
    /// l'image (sous l'horizon) a la couleur `SkyBottom`, le haut tire vers `SkyTop`. Avec
    /// `ARCHRUST_GAME_DIR`, le gabarit 13 (Anchorville) est rendu à 2 h, 6 h, 9 h, 15 h, 19 h et 22 h
    /// avec ses textures ; avec `ARCHRUST_SKY_DUMP=<dossier>`, les images sont écrites en PNG.
    #[test]
    fn rendu_hors_ecran() {
        use crate::sky::{SkyFrame, SkySlot, SkyTemplates};
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
        let Some(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("pas d'adaptateur graphique : rendu hors écran non vérifié");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default(), None)).unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let uniform = |vis| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[uniform(wgpu::ShaderStages::VERTEX_FRAGMENT)] });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let mut sky = SkyRenderer::new(&device, format, &globals_layout, &texture_layout, Vertex::layout());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let upload = |name: &str, w, h, data: &[u8]| Arc::new(crate::render::upload_rgba(&device, &queue, &texture_layout, &sampler, name, w, h, data));
        let white = upload("__white", 1, 1, &[255; 4]);
        let globals = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 160, usage: wgpu::BufferUsages::UNIFORM, mapped_at_creation: false });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &globals_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }] });
        // Données du client : gabarits et textures (sinon un ciel par défaut sans couche).
        let assets = std::env::var_os("ARCHRUST_GAME_DIR").map(|d| crate::assets::Assets::new(Some(d.into()), &[]));
        let mut textures: HashMap<String, Arc<GpuTexture>> = HashMap::new();
        let mut cases: Vec<(String, SkySlot, u32)> = vec![("defaut".into(), SkySlot::default(), 12)];
        let mut tex_names = HashMap::new();
        if let Some(a) = &assets {
            let t = SkyTemplates::parse(&a.table("ini", "skyset.ini").unwrap());
            tex_names = crate::sky::parse_sky_textures(&a.table("ini", "alefskysetting.ini").unwrap());
            for name in tex_names.values() {
                let img = a.texture(name).unwrap();
                textures.insert(name.to_ascii_lowercase(), upload(name, img.width, img.height, &img.data));
            }
            for hour in [2, 6, 9, 15, 19, 22] {
                cases.push((format!("gabarit13-{hour:02}h"), t.get(13).unwrap().slots[crate::zones::sky_slot(hour)].clone(), hour));
            }
        }
        let (w, h) = (256u32, 160u32);
        let eye = Vec3::new(-478_000.0, 3_000.0, -68_000.0);
        let sun = sun_direction(Vec3::new(0.4, 0.8, 0.3));
        // Vue tournée vers l'astre, un peu relevée : horizon au tiers bas de l'image.
        let look = Vec3::new(sun.x, 0.25, sun.z).normalize();
        let view = Mat4::look_at_rh(eye, eye + look, Vec3::Y);
        let vp = Mat4::perspective_rh(1.2, w as f32 / h as f32, 1_000_000.0, 10.0) * view;
        let dump = std::env::var_os("ARCHRUST_SKY_DUMP").map(std::path::PathBuf::from);
        for (label, slot, hour) in cases {
            let frame = SkyFrame::from_slot(&slot);
            let draw = build_sky_draw(&frame, hour, hour as u64 * 3_600_000, sun, [0.1, 0.3], |i| tex_names.get(&i).cloned());
            sky.prepare_sky(&device, &queue, &sampler, &textures, &white, vp, eye, &draw);
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let depth = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let (tv, dv) = (target.create_view(&Default::default()), depth.create_view(&Default::default()));
            // Une particule et un reflet, pour valider leurs pipelines.
            let p = Vertex { position: (eye + look * 500.0).to_array(), normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0], color: [1.0; 4] };
            let n = sky.prepare_particles(&device, &queue, &[p, p, p]);
            let flares = flare_vertices(&[FlareDraw { texture: "__white".into(), half: 0.05, line_pos: 0.5, color: [0.2; 4] }], [0.0, 0.0], 1.6);
            sky.prepare_flares(&device, &queue, &flares[0].1);
            let mut enc = device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &tv, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                sky.draw_sky(&mut pass);
                sky.draw_particles(&mut pass, &globals_bg, &white.bind_group, n);
            }
            {
                let depth_bg = sky.depth_bind_group(&device, &dv);
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &tv, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                sky.draw_flares(&mut pass, &globals_bg, &depth_bg, &[&white.bind_group]);
            }
            let row = (4 * w).div_ceil(256) * 256;
            let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            queue.submit(Some(enc.finish()));
            buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            device.poll(wgpu::Maintain::Wait);
            let data = buf.slice(..).get_mapped_range();
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                rgba.extend_from_slice(&data[(y * row) as usize..(y * row + 4 * w) as usize]);
            }
            drop(data);
            let px = |x: u32, y: u32| -> [u8; 3] {
                let i = ((y * w + x) * 4) as usize;
                [rgba[i], rgba[i + 1], rgba[i + 2]]
            };
            // Coin bas gauche : sous l'horizon, couleur du bas (8 bits, gamma) sauf bande d'horizon.
            if slot.circumstance.texture < 0 {
                let want = slot.sky_bottom.map(|c| (c * 255.0).round());
                let got = px(2, h - 2);
                assert!((0..3).all(|i| (got[i] as f32 - want[i]).abs() <= 3.0), "{label} : bas {got:?} au lieu de {want:?}");
                let top = px(2, 1);
                let tw = slot.sky_top.map(|c| c * 255.0);
                assert!((0..3).all(|i| (top[i] as f32 - tw[i]).abs() <= (got[i] as f32 - tw[i]).abs() + 1.0), "{label} : haut {top:?}");
            }
            if let Some(d) = &dump {
                image::save_buffer(d.join(format!("ciel-{label}.png")), &rgba, w, h, image::ColorType::Rgba8).unwrap();
            }
        }
        let err = pollster::block_on(device.pop_error_scope());
        assert!(err.is_none(), "validation wgpu : {err:?}");
    }

    #[test]
    fn direction_et_projection() {
        let eye = Vec3::new(-800_000.0, 1_000.0, -640_000.0);
        let view = Mat4::look_at_rh(eye, eye + Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        let vp = Mat4::perspective_rh(1.0, 1.5, 100_000.0, 10.0) * view;
        // Centre de l'écran : la direction de visée.
        let inv = camera_relative_inverse(vp, eye);
        let near = inv * glam::Vec4::new(0.0, 0.0, 1.0, 1.0);
        let mid = inv * glam::Vec4::new(0.0, 0.0, 0.5, 1.0);
        let dir = (mid.truncate() / mid.w - near.truncate() / near.w).normalize();
        assert!((dir - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-3, "{dir}");
        let c = project_direction(vp, eye, Vec3::new(0.0, 0.0, -1.0)).unwrap();
        assert!(c[0].abs() < 1e-3 && c[1].abs() < 1e-3);
        assert!(project_direction(vp, eye, Vec3::new(0.0, 0.0, 1.0)).is_none());
        let v = flare_vertices(&[FlareDraw { texture: "lf_glow".into(), half: 0.25, line_pos: 1.0, color: [1.0; 4] }], [0.5, 0.5], 2.0);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].1[0].position, [0.375, 0.25, 0.0]);
        // À l'opposé du soleil.
        let v = flare_vertices(&[FlareDraw { texture: "lf_glow".into(), half: 0.0, line_pos: -1.0, color: [1.0; 4] }], [0.5, 0.5], 2.0);
        assert_eq!(v[0].1[0].position, [-0.5, -0.5, 0.0]);
        assert_eq!(v[0].1[0].normal, [0.5, 0.5, 0.0]);
    }
}
