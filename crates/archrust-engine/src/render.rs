//! Contexte wgpu : surface, pipeline éclairé, maillages, textures, instances ; ombres projetées des
//! objets sur le sol (`shadow_render`, groupe 2 du terrain) et lueur des armures serties
//! ([`ArmorGlow`], groupe 3 du pipeline éclairé).

use anyhow::{anyhow, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

#[path = "render_upload_cache.rs"]
mod upload_cache;
#[path = "render_frustum.rs"]
mod frustum;
pub use frustum::Frustum;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

/// Sommet d'un secteur de sol : cinq jeux d'UV, un par étage de texture (base, masque 1,
/// couche 1, masque 2, couche 2 ; voir `terrain.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv: [[f32; 2]; TERRAIN_STAGES],
}

/// Étages de texture du sol pris en charge (base + deux couches masquées).
pub const TERRAIN_STAGES: usize = 5;

impl TerrainVertex {
    const ATTRS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2,
        4 => Float32x2, 5 => Float32x2, 6 => Float32x2, 7 => Float32x2
    ];
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

/// Plage d'indices d'un matériau du sol et ses textures liées.
pub struct TerrainPart {
    pub index_start: u32,
    pub index_count: u32,
    bind_group: wgpu::BindGroup,
}

/// Secteur de sol prêt à dessiner (sommets en coordonnées monde).
pub struct TerrainMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    pub parts: Vec<TerrainPart>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    ambient: [f32; 4],
    camera_pos: [f32; 4],
    light_color: [f32; 4],
    fog_color: [f32; 4],
    fog_range: [f32; 4],
    /// Lumières d'effet (`app_effects.rs`, type 5) : xyz position, w rayon ; rgb couleur ; x nombre.
    point_pos: [[f32; 4]; MAX_POINT_LIGHTS],
    point_color: [[f32; 4]; MAX_POINT_LIGHTS],
    point_count: [f32; 4],
    /// Ombres projetées (`shadow`, lues par `terrain.wgsl`) : monde → (u, v) de la texture d'ombre,
    /// et x = 1 si elles sont allumées.
    shadow_uv: [[f32; 4]; 4],
    shadow_params: [f32; 4],
}

/// Lumières ponctuelles appliquées au plus (tableaux `point_*` des shaders) : celles des effets
/// (au plus 8, `app_effects.rs`) puis celles des repères du décor (`decor.rs`) les plus proches.
pub const MAX_POINT_LIGHTS: usize = 16;

/// Lumière ponctuelle d'un effet d'Archonia (`AgcdEffLight`) : éclaire terrain, objets et
/// personnages dans sa sphère, en plus du soleil et de l'ambiante (voir `shader.wgsl`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLight {
    pub position: Vec3,
    pub radius: f32,
    /// Couleur (0–1).
    pub color: [f32; 3],
}

/// Tableaux des shaders pour `lights` (les [`MAX_POINT_LIGHTS`] premières) : positions et rayons,
/// couleurs, nombre.
fn pack_point_lights(lights: &[PointLight]) -> ([[f32; 4]; MAX_POINT_LIGHTS], [[f32; 4]; MAX_POINT_LIGHTS], [f32; 4]) {
    let (mut pos, mut color) = ([[0.0; 4]; MAX_POINT_LIGHTS], [[0.0; 4]; MAX_POINT_LIGHTS]);
    let n = lights.len().min(MAX_POINT_LIGHTS);
    for (i, l) in lights.iter().take(n).enumerate() {
        pos[i] = [l.position.x, l.position.y, l.position.z, l.radius];
        color[i] = [l.color[0], l.color[1], l.color[2], 1.0];
    }
    (pos, color, [n as f32, 0.0, 0.0, 0.0])
}

/// Brouillard par défaut (sans ciel d'Archonia) : de 30 000 à 90 000 unités, au plus 85 %.
pub const DEFAULT_FOG_COLOR: [f32; 4] = [0.55, 0.65, 0.8, 1.0];
pub const DEFAULT_FOG_RANGE: [f32; 4] = [30_000.0, 90_000.0, 0.85, 0.0];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceUniform {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// Lueur d'armure ([`ArmorGlow`], `shader.wgsl`) : échelle UV, translation UV ; sélection U et
    /// rotation ; sélection V et poids (0 : sans lueur) ; spéculaire et temps ; ambiante et diffuse
    /// du matériau.
    glow: [[f32; 4]; 6],
}

impl InstanceUniform {
    fn new(item: &DrawItem, time: f32) -> Self {
        let glow = item.mesh.glow.lock().unwrap().as_ref().map_or([[0.0; 4]; 6], |g| g.uniform(time));
        InstanceUniform { model: item.model.to_cols_array_2d(), color: item.tint, glow }
    }
}

/// Lueur de l'armure de corps sertie (`ee_ss_fx.txt`, matériau `uvAnim.fx` d'Archonia, voir
/// `app_stones.rs`) : second étage de texture animé posé sur les pièces de l'armure.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmorGlow {
    /// Texture animée (`SOUL_*`).
    pub texture: String,
    /// `UV Scale`, `Uv Translation` (vitesse par seconde), `UV Rotation` (degrés par seconde).
    pub scale: [f32; 2],
    pub velocity: [f32; 2],
    pub rotation: f32,
    /// `UV Anim weight`.
    pub weight: f32,
    /// `U Selection vector`, `V Selection vector` (trois premières composantes, `float3` du shader).
    pub u_select: [f32; 3],
    pub v_select: [f32; 3],
    /// `AMBIENT`, `DIFFUSE`, `SPECULAR` du matériau (0–1).
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
}

impl ArmorGlow {
    /// Paramètres du shader (voir `InstanceUniform::glow`) au temps `time` (secondes).
    pub fn uniform(&self, time: f32) -> [[f32; 4]; 6] {
        let v3 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
        [
            [self.scale[0], self.scale[1], self.velocity[0], self.velocity[1]],
            v3(self.u_select, self.rotation),
            // Poids nul = pas de lueur pour le shader : un poids de 0 lu dans la table reste « allumé ».
            v3(self.v_select, self.weight.max(1e-6)),
            v3(self.specular, time),
            v3(self.ambient, 0.0),
            v3(self.diffuse, 0.0),
        ]
    }
}

/// Sous-maillage : plage d'indices + texture.
pub struct SubMesh {
    pub index_start: u32,
    pub index_count: u32,
    pub texture: Option<String>,
    pub color: [f32; 4],
}

pub struct Mesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub parts: Vec<SubMesh>,
    pub bounds_center: Vec3,
    pub bounds_radius: f32,
    /// Liaison au squelette d'une géométrie skinnée du client, pour l'animation
    /// (voir `animation`). `None` pour les maillages statiques.
    pub skin: Option<Arc<crate::animation::SkinBinding>>,
    /// Pièce d'un objet porté (arme, bouclier, lance) : nœud de la main et matrice d'attache, voir
    /// `worn`. `None` pour les autres maillages.
    pub worn: Option<crate::worn::Worn>,
    /// TID de l'objet d'équipement dont vient la pièce (armures, voir `scene`), `None` sinon.
    pub item_tid: Option<i32>,
    /// Lueur d'armure sertie posée par `app_stones.rs` (`None` : aucune).
    pub glow: std::sync::Mutex<Option<Arc<ArmorGlow>>>,
}

pub struct GpuTexture {
    pub bind_group: wgpu::BindGroup,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Cible de rendu hors écran (voir [`Renderer::create_offscreen`]) : couleur lisible comme texture
/// (à enregistrer auprès d'egui) et sa profondeur.
pub struct OffscreenTarget {
    pub view: wgpu::TextureView,
    depth: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Lot d'un effet : texture (nom RenderWare ; absente = blanc), mélange et triangles.
pub struct EffectDraw {
    pub texture: Option<String>,
    pub blend: crate::sky_render::EffectBlend,
    pub vertices: Vec<Vertex>,
}

/// Instance à dessiner : maillage + transformation.
pub struct DrawItem {
    pub mesh: Arc<Mesh>,
    pub model: Mat4,
    pub tint: [f32; 4],
    /// Pièce d'un personnage ou d'une créature : faces arrière cachées (voir `pipeline_culled`), qu'elle
    /// soit skinnée ou rigide (chapeau, arme, ailes accrochés à un os).
    pub character: bool,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    /// Même pipeline avec les faces arrière cachées, pour les maillages skinnés (personnages et
    /// créatures) : mode par défaut de RenderWare (`rwCULLMODECULLBACK`). Dessinées recto-verso,
    /// les faces intérieures des vêtements se disputaient la profondeur avec les faces avant
    /// pendant l'animation (taches noires qui clignotent).
    pipeline_culled: wgpu::RenderPipeline,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_layout: wgpu::BindGroupLayout,
    /// Masque neutre (alpha 0) des étages de sol absents.
    clear: Arc<GpuTexture>,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    instance_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    depth: wgpu::TextureView,
    pub textures: HashMap<String, Arc<GpuTexture>>,
    pub white: Arc<GpuTexture>,
    instance_pool: Vec<(wgpu::Buffer, wgpu::BindGroup, upload_cache::UploadCache)>,
    pub clear_color: wgpu::Color,
    /// Direction du soleil et lumière ambiante (CVars `lightDirection`, `ambientColor` des AddOns).
    pub light_dir: Vec3,
    pub ambient: [f32; 4],
    /// Couleur de la lumière directionnelle (`Directional` du ciel) ; `w = 0` : blanc atténué de
    /// l'ambiante (`1 − ambiante`), l'éclairage d'avant le ciel.
    pub light_color: [f32; 4],
    /// Brouillard : couleur (linéaire) et (début, fin, opacité maximale, —).
    pub fog_color: [f32; 4],
    pub fog_range: [f32; 4],
    /// Ciel d'Archonia, particules de météo (texture, sommets) et reflets du soleil de l'image
    /// (remplis par `app_nature.rs` ; `None`/vides : fond uni `clear_color`).
    pub sky_draw: Option<crate::sky_render::SkyDraw>,
    pub particles: Option<(String, Vec<Vertex>)>,
    pub flares: Vec<crate::sky_render::FlareDraw>,
    /// Effets d'Archonia de l'image (`app_effects.rs`) : lots (texture, mélange, sommets en
    /// coordonnées du monde), dessinés après les particules dans l'ordre de la liste.
    pub effects: Vec<EffectDraw>,
    /// Lumières de l'image : effets (`app_effects.rs`) puis repères du décor (`decor.rs`), au plus
    /// [`MAX_POINT_LIGHTS`].
    pub point_lights: Vec<PointLight>,
    /// Eau et herbe du monde de l'image (remplies par `world.rs`, voir `world_render`).
    pub world: crate::world_render::WorldDraw,
    world_renderer: crate::world_render::WorldRenderer,
    sky: crate::sky_render::SkyRenderer,
    /// Demande de capture de la prochaine image (lue dans `take_capture`).
    pub capture_next: bool,
    captured: Option<(u32, u32, Vec<u8>)>,
    /// Ombres projetées des objets (`shadow_render`) : prise à refaire avant la prochaine image, et
    /// matrice monde → texture de la dernière prise (`None` : ombres éteintes).
    shadow: crate::shadow_render::ShadowRenderer,
    pub shadow_request: Option<crate::shadow_render::ShadowRequest>,
    pub shadow_uv: Option<Mat4>,
    /// Horloge des lueurs d'armure (`Time` d'`uvAnim.fx`, en secondes).
    started: std::time::Instant,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow!("aucun adaptateur graphique compatible"))?;
        log::info!(
            "GPU : {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("archrust"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
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
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
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
        let instance_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instance"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // Lu aussi par le fragment (lueur d'armure).
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lit"),
            // Groupe 3 : texture animée de la lueur d'armure (blanc sans lueur).
            bind_group_layouts: &[&globals_layout, &texture_layout, &instance_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let lit = |cull_mode: Option<wgpu::Face>| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if cull_mode.is_some() { "lit-culled" } else { "lit" }),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Sommets des clumps en ordre trigonométrique (vérifié en jeu : rien ne disparaît).
                front_face: wgpu::FrontFace::Ccw,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let pipeline = lit(None);
        let pipeline_culled = lit(Some(wgpu::Face::Back));
        let shadow = crate::shadow_render::ShadowRenderer::new(&device, &texture_layout);
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });
        let mut terrain_entries: Vec<wgpu::BindGroupLayoutEntry> = (0..TERRAIN_STAGES as u32)
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
        terrain_entries.push(wgpu::BindGroupLayoutEntry {
            binding: TERRAIN_STAGES as u32,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let terrain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &terrain_entries,
        });
        let terrain_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain"),
                bind_group_layouts: &[&globals_layout, &terrain_layout, &shadow.sample_layout],
                push_constant_ranges: &[],
            });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&terrain_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_main"),
                buffers: &[TerrainVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let depth = Self::make_depth(&device, &config);
        let white = Arc::new(upload_rgba(
            &device,
            &queue,
            &texture_layout,
            &sampler,
            "__white",
            1,
            1,
            &[255, 255, 255, 255],
        ));
        let clear = Arc::new(upload_rgba(
            &device,
            &queue,
            &texture_layout,
            &sampler,
            "__clear",
            1,
            1,
            &[0, 0, 0, 0],
        ));
        let mut textures = HashMap::new();
        textures.insert("__white".to_string(), white.clone());
        let sky = crate::sky_render::SkyRenderer::new(&device, format, &globals_layout, &texture_layout, Vertex::layout());
        let world_renderer = crate::world_render::WorldRenderer::new(&device, format, &globals_layout, &texture_layout);
        Ok(Renderer {
            device,
            queue,
            surface,
            config,
            pipeline,
            pipeline_culled,
            terrain_pipeline,
            terrain_layout,
            clear,
            globals_buf,
            globals_bg,
            texture_layout,
            instance_layout,
            sampler,
            depth,
            textures,
            white,
            instance_pool: Vec::new(),
            clear_color: wgpu::Color {
                r: 0.35,
                g: 0.45,
                b: 0.6,
                a: 1.0,
            },
            light_dir: Vec3::new(0.4, 0.8, 0.3),
            ambient: [0.45, 0.45, 0.5, 1.0],
            light_color: [0.0; 4],
            fog_color: DEFAULT_FOG_COLOR,
            fog_range: DEFAULT_FOG_RANGE,
            sky_draw: None,
            particles: None,
            flares: Vec::new(),
            effects: Vec::new(),
            point_lights: Vec::new(),
            world: Default::default(),
            world_renderer,
            sky,
            capture_next: false,
            captured: None,
            shadow,
            shadow_request: None,
            shadow_uv: None,
            started: std::time::Instant::now(),
        })
    }

    /// Image capturée (RGBA8) si une capture a été demandée puis rendue.
    pub fn take_capture(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.captured.take()
    }

    /// Copie la surface rendue vers un tampon lisible par le CPU.
    fn capture(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> (wgpu::Buffer, u32) {
        let (w, h) = (self.config.width, self.config.height);
        let bytes_per_row = (4 * w).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: bytes_per_row as u64 * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        (buffer, bytes_per_row)
    }

    fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            // Lue aussi par les reflets du soleil (occultation, `nature.wgsl`).
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        tex.create_view(&Default::default())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::make_depth(&self.device, &self.config);
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    /// Téléverse une texture RGBA8 (un seul niveau) et l'enregistre sous `name`.
    pub fn upload_texture(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Arc<GpuTexture> {
        let gpu = Arc::new(upload_rgba(
            &self.device,
            &self.queue,
            &self.texture_layout,
            &self.sampler,
            name,
            width,
            height,
            rgba,
        ));
        self.textures.insert(name.to_ascii_lowercase(), gpu.clone());
        gpu
    }

    /// Texture par nom, sans allocation quand le nom est déjà en minuscules (pièces de `create_mesh`).
    fn texture_ref(&self, name: &str) -> Option<&Arc<GpuTexture>> {
        if name.bytes().any(|b| b.is_ascii_uppercase()) {
            self.textures.get(&name.to_ascii_lowercase())
        } else {
            self.textures.get(name)
        }
    }

    pub fn texture(&self, name: &str) -> Option<Arc<GpuTexture>> {
        self.textures.get(&name.to_ascii_lowercase()).cloned()
    }

    /// Texture animée de la lueur d'un maillage (blanc sans lueur ou texture absente).
    fn glow_texture(&self, mesh: &Mesh) -> Arc<GpuTexture> {
        mesh.glow.lock().unwrap().as_ref().and_then(|g| self.textures.get(&g.texture.to_ascii_lowercase()).cloned()).unwrap_or_else(|| self.white.clone())
    }

    pub fn create_mesh(&self, vertices: &[Vertex], indices: &[u32], mut parts: Vec<SubMesh>) -> Mesh {
        let (center, radius) = bounds(vertices);
        // Noms de textures en minuscules une fois pour toutes : la boucle de dessin les cherche sans allouer.
        for part in &mut parts {
            if let Some(t) = &mut part.texture {
                t.make_ascii_lowercase();
            }
        }
        Mesh {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    // COPY_DST : les sommets des personnages animés sont réécrits à chaque pose.
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("indices"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            parts,
            bounds_center: center,
            bounds_radius: radius,
            skin: None,
            worn: None,
            item_tid: None,
            glow: Default::default(),
        }
    }

    /// Maillage de sol : `parts` = (premier indice, nombre d'indices, textures des étages). Un étage
    /// sans texture, ou dont la texture n'est pas sur le GPU, reçoit la texture neutre : blanc pour
    /// la base et les couches, transparent pour les masques (étages impairs).
    pub fn create_terrain_mesh(
        &self,
        vertices: &[TerrainVertex],
        indices: &[u32],
        parts: Vec<(u32, u32, [Option<String>; TERRAIN_STAGES])>,
    ) -> TerrainMesh {
        let parts = parts
            .into_iter()
            .map(|(index_start, index_count, names)| {
                let views: Vec<Arc<GpuTexture>> = names
                    .iter()
                    .enumerate()
                    .map(|(stage, name)| {
                        name.as_ref()
                            .and_then(|n| self.texture(n))
                            .unwrap_or_else(|| {
                                if stage % 2 == 1 {
                                    self.clear.clone()
                                } else {
                                    self.white.clone()
                                }
                            })
                    })
                    .collect();
                let mut entries: Vec<wgpu::BindGroupEntry> = views
                    .iter()
                    .enumerate()
                    .map(|(i, t)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: wgpu::BindingResource::TextureView(&t.view),
                    })
                    .collect();
                entries.push(wgpu::BindGroupEntry {
                    binding: TERRAIN_STAGES as u32,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                });
                TerrainPart {
                    index_start,
                    index_count,
                    bind_group: self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("terrain"),
                        layout: &self.terrain_layout,
                        entries: &entries,
                    }),
                }
            })
            .collect();
        TerrainMesh {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain vertices"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain indices"),
                    contents: bytemuck::cast_slice(indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            parts,
        }
    }

    /// Réécrit les sommets d'un maillage (même nombre de sommets qu'à sa création).
    pub fn write_vertices(&self, mesh: &Mesh, vertices: &[Vertex]) {
        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        if bytes.len() as u64 <= mesh.vertices.size() {
            self.queue.write_buffer(&mesh.vertices, 0, bytes);
        }
    }

    fn ensure_instances(&mut self, count: usize) {
        while self.instance_pool.len() < count {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instance"),
                size: std::mem::size_of::<InstanceUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("instance"),
                layout: &self.instance_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            self.instance_pool.push((buf, bg, upload_cache::UploadCache::default()));
        }
    }

    /// Cible de rendu hors écran de `width × height` (format de la surface, lisible comme texture) :
    /// raster d'un aperçu comme celui du coiffeur d'Archonia (`AgcmCustomizeRender`).
    pub fn create_offscreen(&self, width: u32, height: u32) -> OffscreenTarget {
        let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        OffscreenTarget { view: color.create_view(&Default::default()), depth: depth.create_view(&Default::default()), width: size.width, height: size.height }
    }

    /// Dessine `items` dans `target` (profondeur inversée comme la scène), sans ciel, brouillard ni
    /// lumière d'effet, avec la lumière `light` (direction vers la lumière, ambiante, couleur
    /// directionnelle `w > 0`), sur le fond `clear`. Soumis aussitôt : les tampons communs (globales,
    /// instances) peuvent être réécrits ensuite par l'image principale.
    pub fn render_offscreen(&mut self, target: &OffscreenTarget, view_proj: Mat4, camera_pos: Vec3, items: &[DrawItem], light: (Vec3, [f32; 4], [f32; 4]), clear: wgpu::Color) {
        let (point_pos, point_color, point_count) = pack_point_lights(&[]);
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: light.0.extend(0.0).to_array(),
            ambient: light.1,
            camera_pos: camera_pos.extend(1.0).to_array(),
            light_color: light.2,
            fog_color: self.fog_color,
            fog_range: [f32::MAX, f32::MAX, 0.0, 0.0],
            point_pos,
            point_color,
            point_count,
            shadow_uv: Mat4::IDENTITY.to_cols_array_2d(),
            shadow_params: [0.0; 4],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.ensure_instances(items.len());
        let time = self.started.elapsed().as_secs_f32();
        for (i, item) in items.iter().enumerate() {
            let data = InstanceUniform::new(item, time);
            let (buf, _, cache) = &mut self.instance_pool[i];
            let bytes = bytemuck::bytes_of(&data);
            if cache.changed(bytes) {
                self.queue.write_buffer(buf, 0, bytes);
            }
        }
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("offscreen") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_pipeline(&self.pipeline_culled);
            for (i, item) in items.iter().enumerate() {
                pass.set_bind_group(2, &self.instance_pool[i].1, &[]);
                let glow = self.glow_texture(&item.mesh);
                pass.set_bind_group(3, &glow.bind_group, &[]);
                pass.set_vertex_buffer(0, item.mesh.vertices.slice(..));
                pass.set_index_buffer(item.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                for part in &item.mesh.parts {
                    let tex = part.texture.as_deref().and_then(|t| self.texture_ref(t)).unwrap_or(&self.white);
                    pass.set_bind_group(1, &tex.bind_group, &[]);
                    pass.draw_indexed(part.index_start..part.index_start + part.index_count, 0, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Dessine une image : scène 3D puis une fermeture pour l'interface (egui) sur le même encodeur.
    pub fn render(
        &mut self,
        view_proj: Mat4,
        camera_pos: Vec3,
        terrain: &[Arc<TerrainMesh>],
        items: &[DrawItem],
        ui: impl FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        let view = frame.texture.create_view(&Default::default());
        let (point_pos, point_color, point_count) = pack_point_lights(&self.point_lights);
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: self.light_dir.extend(0.0).to_array(),
            ambient: self.ambient,
            camera_pos: camera_pos.extend(1.0).to_array(),
            light_color: self.light_color,
            fog_color: self.fog_color,
            fog_range: self.fog_range,
            point_pos,
            point_color,
            point_count,
            shadow_uv: self.shadow_uv.unwrap_or(Mat4::IDENTITY).to_cols_array_2d(),
            shadow_params: [if self.shadow_uv.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        // Ciel, particules et reflets de l'image (voir `sky_render`).
        if let Some(sky) = &self.sky_draw {
            self.sky.prepare_sky(&self.device, &self.queue, &self.sampler, &self.textures, &self.white, view_proj, camera_pos, sky);
        }
        let particle_count = match &self.particles {
            Some((_, v)) if !v.is_empty() => self.sky.prepare_particles(&self.device, &self.queue, v),
            _ => 0,
        };
        let particle_tex = self.particles.as_ref().and_then(|(t, _)| self.textures.get(&t.to_ascii_lowercase()).cloned());
        // Effets : un seul tampon, un appel par lot.
        let mut effect_vertices: Vec<Vertex> = Vec::new();
        let mut effect_batches: Vec<(Arc<GpuTexture>, crate::sky_render::EffectBlend, std::ops::Range<u32>)> = Vec::new();
        for e in &self.effects {
            if e.vertices.is_empty() {
                continue;
            }
            let tex = e.texture.as_ref().and_then(|t| self.textures.get(&t.to_ascii_lowercase())).unwrap_or(&self.white).clone();
            let start = effect_vertices.len() as u32;
            effect_vertices.extend_from_slice(&e.vertices);
            effect_batches.push((tex, e.blend, start..effect_vertices.len() as u32));
        }
        if !effect_vertices.is_empty() {
            self.sky.prepare_effects(&self.device, &self.queue, &effect_vertices);
        }
        let sun_ndc = self
            .sky_draw
            .as_ref()
            .filter(|_| !self.flares.is_empty())
            .and_then(|s| crate::sky_render::project_direction(view_proj, camera_pos, s.sun_dir))
            .filter(|p| p[0].abs() <= 1.0 && p[1].abs() <= 1.0);
        let mut flare_textures: Vec<Arc<GpuTexture>> = Vec::new();
        if let Some(sun) = sun_ndc {
            let quads = crate::sky_render::flare_vertices(&self.flares, sun, self.aspect());
            let mut vertices = Vec::with_capacity(quads.len() * 6);
            for (name, q) in quads {
                if let Some(t) = self.textures.get(&name.to_ascii_lowercase()) {
                    flare_textures.push(t.clone());
                    vertices.extend_from_slice(&q);
                }
            }
            self.sky.prepare_flares(&self.device, &self.queue, &vertices);
        }
        let world_prepared = self.world_renderer.prepare(&self.device, &self.queue, &self.world, &self.textures, &self.white, &self.clear);
        self.ensure_instances(items.len());
        let time = self.started.elapsed().as_secs_f32();
        for (i, item) in items.iter().enumerate() {
            let data = InstanceUniform::new(item, time);
            let (buf, _, cache) = &mut self.instance_pool[i];
            let bytes = bytemuck::bytes_of(&data);
            if cache.changed(bytes) {
                self.queue.write_buffer(buf, 0, bytes);
            }
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        // Ombres projetées : prise refaite avant la scène quand elle est demandée (voir `shadow`).
        if let Some(request) = self.shadow_request.take() {
            self.shadow.encode(&self.device, &self.queue, &mut encoder, &request, &self.textures, &self.white);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    // Profondeur inversée (`OrbitCamera::render_view_proj`) : 0 = le plus loin.
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if self.sky_draw.is_some() {
                self.sky.draw_sky(&mut pass);
            }
            pass.set_bind_group(0, &self.globals_bg, &[]);
            if !terrain.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(2, &self.shadow.sample_bg, &[]);
                for mesh in terrain {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    for part in &mesh.parts {
                        pass.set_bind_group(1, &part.bind_group, &[]);
                        pass.draw_indexed(
                            part.index_start..part.index_start + part.index_count,
                            0,
                            0..1,
                        );
                    }
                }
            }
            let mut culled = false;
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(3, &self.white.bind_group, &[]);
            let mut glowing = false;
            let (mut last_mesh, mut last_texture): (Option<*const Mesh>, Option<*const GpuTexture>) = (None, None);
            for (i, item) in items.iter().enumerate() {
                // Personnages et créatures (pièces skinnées ou rigides) : faces arrière cachées ; le décor
                // reste recto-verso. Les pièces rigides accrochées aux os (chapeau, manteau, ailes)
                // clignotaient en noir comme les pièces skinnées avant elles.
                let want = item.character || item.mesh.skin.is_some();
                if want != culled {
                    culled = want;
                    pass.set_pipeline(if culled { &self.pipeline_culled } else { &self.pipeline });
                }
                let (_, bg, _) = &self.instance_pool[i];
                pass.set_bind_group(2, bg, &[]);
                // Lueur d'armure : texture animée en groupe 3 (blanc remis après).
                let glow = item.mesh.glow.lock().unwrap().is_some();
                if glow || glowing {
                    let tex = self.glow_texture(&item.mesh);
                    pass.set_bind_group(3, &tex.bind_group, &[]);
                    glowing = glow;
                }
                // Même maillage ou même texture que la pièce précédente : liaisons gardées.
                if !last_mesh.is_some_and(|m| std::ptr::eq(m, Arc::as_ptr(&item.mesh))) {
                    pass.set_vertex_buffer(0, item.mesh.vertices.slice(..));
                    pass.set_index_buffer(item.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    last_mesh = Some(Arc::as_ptr(&item.mesh));
                }
                for part in &item.mesh.parts {
                    let tex = part.texture.as_deref().and_then(|t| self.texture_ref(t)).unwrap_or(&self.white);
                    if !last_texture.is_some_and(|t| std::ptr::eq(t, Arc::as_ptr(tex))) {
                        pass.set_bind_group(1, &tex.bind_group, &[]);
                        last_texture = Some(Arc::as_ptr(tex));
                    }
                    pass.draw_indexed(
                        part.index_start..part.index_start + part.index_count,
                        0,
                        0..1,
                    );
                }
            }
            // Herbe puis eau (transparente) du monde, après les objets opaques.
            self.world_renderer.draw_grass(&mut pass, &self.globals_bg, &world_prepared);
            self.world_renderer.draw_water(&mut pass, &self.globals_bg, &world_prepared);
            // Particules de météo, après les objets opaques.
            if let Some(tex) = &particle_tex {
                self.sky.draw_particles(&mut pass, &self.globals_bg, &tex.bind_group, particle_count);
            }
            for (tex, blend, range) in &effect_batches {
                self.sky.draw_effect(&mut pass, &self.globals_bg, &tex.bind_group, *blend, range.clone());
            }
        }
        // Reflets du soleil, par-dessus la scène, cachés si le soleil l'est.
        if !flare_textures.is_empty() {
            let depth_bg = self.sky.depth_bind_group(&self.device, &self.depth);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("flares"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let groups: Vec<&wgpu::BindGroup> = flare_textures.iter().map(|t| &t.bind_group).collect();
            self.sky.draw_flares(&mut pass, &self.globals_bg, &depth_bg, &groups);
        }
        ui(&self.device, &self.queue, &mut encoder, &view);
        let capture = if self.capture_next {
            self.capture_next = false;
            Some(self.capture(&mut encoder, &frame.texture))
        } else {
            None
        };
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        if let Some((buffer, bytes_per_row)) = capture {
            let (w, h) = (self.config.width, self.config.height);
            let slice = buffer.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            self.device.poll(wgpu::Maintain::Wait);
            if let Ok(Ok(())) = rx.recv() {
                let data = slice.get_mapped_range();
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                let bgra = self.config.format == wgpu::TextureFormat::Bgra8UnormSrgb
                    || self.config.format == wgpu::TextureFormat::Bgra8Unorm;
                for row in 0..h {
                    let start = (row * bytes_per_row) as usize;
                    let line = &data[start..start + (w * 4) as usize];
                    if bgra {
                        for px in line.chunks_exact(4) {
                            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                        }
                    } else {
                        for px in line.chunks_exact(4) {
                            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                        }
                    }
                }
                drop(data);
                buffer.unmap();
                self.captured = Some((w, h, rgba));
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> GpuTexture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    // Chaîne de mipmaps calculée sur le CPU (moyenne 2×2) : sans elle, les textures 1024² du sol
    // vues de loin scintillent et forment un damier.
    let levels = mip_chain(width, height, rgba);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(name),
        size,
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (w, h, data)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(*h),
            },
            wgpu::Extent3d {
                width: *w,
                height: *h,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = tex.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(name),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    GpuTexture {
        bind_group,
        view,
        width,
        height,
    }
}

/// Niveaux de mipmap d'une image RGBA8 : l'image elle-même puis des moyennes 2×2 jusqu'à 1×1.
fn mip_chain(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, std::borrow::Cow<'_, [u8]>)> {
    let mut levels = vec![(width, height, std::borrow::Cow::Borrowed(rgba))];
    if rgba.len() < (4 * width * height) as usize {
        return levels;
    }
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = &levels.last().unwrap().2;
        let mut dst = vec![0u8; (4 * nw * nh) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (2 * x + dx).min(w - 1);
                        let sy = (2 * y + dy).min(h - 1);
                        sum += src[((sy * w + sx) * 4 + c) as usize] as u32;
                    }
                    dst[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        levels.push((nw, nh, std::borrow::Cow::Owned(dst)));
        (w, h) = (nw, nh);
    }
    levels
}

fn bounds(vertices: &[Vertex]) -> (Vec3, f32) {
    if vertices.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for v in vertices {
        let p = Vec3::from_array(v.position);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    (center, (max - center).length())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lumières ponctuelles : au plus seize dans les tableaux des shaders, taille de `Globals` multiple de 16.
    #[test]
    fn point_lights_pack() {
        let l = PointLight { position: Vec3::new(1.0, 2.0, 3.0), radius: 600.0, color: [0.6, 0.57, 1.0] };
        let (pos, color, count) = pack_point_lights(&[l; 20]);
        assert_eq!(count[0], MAX_POINT_LIGHTS as f32);
        assert_eq!(pos[7], [1.0, 2.0, 3.0, 600.0]);
        assert_eq!(color[0], [0.6, 0.57, 1.0, 1.0]);
        let (_, _, none) = pack_point_lights(&[]);
        assert_eq!(none[0], 0.0);
        assert_eq!(std::mem::size_of::<Globals>() % 16, 0);
    }
}
