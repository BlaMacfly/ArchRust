// Sol multi-couches d'Archonia : texture de base puis jusqu'à deux couches fondues par masque.
//
// L'étage n d'un matériau de secteur lit le jeu d'UV n de la géométrie (voir `docs/FORMATS.md`,
// « Textures du sol ») : 0 = base, 1 = masque de la couche 1 (alpha d'un atlas 4×4), 2 = couche 1,
// 3 = masque de la couche 2, 4 = couche 2. Un étage absent reçoit une texture neutre (masque
// transparent, couche blanche) qui laisse la couleur inchangée.

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    camera_pos: vec4<f32>,
    light_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
    // Lumières des effets (AgcdEffLight), voir `shader.wgsl`.
    point_pos: array<vec4<f32>, 8>,
    point_color: array<vec4<f32>, 8>,
    point_count: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var t_base: texture_2d<f32>;
@group(1) @binding(1) var t_mask1: texture_2d<f32>;
@group(1) @binding(2) var t_layer1: texture_2d<f32>;
@group(1) @binding(3) var t_mask2: texture_2d<f32>;
@group(1) @binding(4) var t_layer2: texture_2d<f32>;
@group(1) @binding(5) var s_terrain: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) uv2: vec2<f32>,
    @location(6) uv3: vec2<f32>,
    @location(7) uv4: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) uv2: vec2<f32>,
    @location(6) uv3: vec2<f32>,
    @location(7) uv4: vec2<f32>,
};

// Lumières ponctuelles des effets (type 5 d'AgcmEff2) : apport diffus rgb × max(0, N·L) × att(d),
// même calcul que `shader.wgsl` (atténuation `1 − d/R`, choix d'ArchRust).
fn point_lighting(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = min(u32(globals.point_count.x), 8u);
    for (var i = 0u; i < count; i = i + 1u) {
        let pr = globals.point_pos[i];
        let d = pr.xyz - p;
        let dist = length(d);
        if (dist < pr.w && dist > 1e-3) {
            sum = sum + globals.point_color[i].rgb * max(dot(n, d / dist), 0.0) * (1.0 - dist / pr.w);
        }
    }
    return sum;
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(v.position, 1.0);
    out.normal = v.normal;
    out.color = v.color;
    out.world_pos = v.position;
    out.uv0 = v.uv0;
    out.uv1 = v.uv1;
    out.uv2 = v.uv2;
    out.uv3 = v.uv3;
    out.uv4 = v.uv4;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var c = textureSample(t_base, s_terrain, in.uv0).rgb;
    let m1 = textureSample(t_mask1, s_terrain, in.uv1).a;
    c = mix(c, textureSample(t_layer1, s_terrain, in.uv2).rgb, m1);
    let m2 = textureSample(t_mask2, s_terrain, in.uv3).a;
    c = mix(c, textureSample(t_layer2, s_terrain, in.uv4).rgb, m2);
    let n = normalize(in.normal);
    let l = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let directional = select(1.0 - globals.ambient.rgb, globals.light_color.rgb, globals.light_color.w > 0.0);
    let light = globals.ambient.rgb + vec3<f32>(diffuse) * directional + point_lighting(in.world_pos, n);
    let dist = distance(in.world_pos, globals.camera_pos.xyz);
    let fog = clamp((dist - globals.fog_range.x) / max(globals.fog_range.y - globals.fog_range.x, 1.0), 0.0, globals.fog_range.z);
    let color = c * in.color.rgb * light;
    return vec4<f32>(mix(color, globals.fog_color.rgb, fog), 1.0);
}
