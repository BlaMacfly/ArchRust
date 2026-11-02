// Eau du monde (AgcmWater, `WaterStatusT1.ini`), voir `world_render.rs`.
//
// Couche 1 × couleur prélit éclairée + couche 2 (étage additif du MatFX « dual »). Éclairage
// RenderWare d'un matériau prélit (ambiante 0,3, diffuse 0,7, `FUN_14058bcc0`) : prélit +
// ambiante × 0,3 + soleil × 0,7 × max(N·L, 0), borné à 1 ; normale (0, 1, 0).

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    camera_pos: vec4<f32>,
    light_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
    point_pos: array<vec4<f32>, 8>,
    point_color: array<vec4<f32>, 8>,
    point_count: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var t_one: texture_2d<f32>;
@group(1) @binding(1) var s_one: sampler;
@group(2) @binding(0) var t_two: texture_2d<f32>;
@group(2) @binding(1) var s_two: sampler;

fn fog_factor(p: vec3<f32>) -> f32 {
    let dist = distance(p, globals.camera_pos.xyz);
    return clamp((dist - globals.fog_range.x) / max(globals.fog_range.y - globals.fog_range.x, 1.0), 0.0, globals.fog_range.z);
}

fn directional() -> vec3<f32> {
    return select(1.0 - globals.ambient.rgb, globals.light_color.rgb, globals.light_color.w > 0.0);
}

struct WaterIn {
    @location(0) position: vec3<f32>,
    @location(1) uv1: vec2<f32>,
    @location(2) uv2: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv1: vec2<f32>,
    @location(1) uv2: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_pos: vec3<f32>,
};

@vertex
fn vs_water(v: WaterIn) -> WaterOut {
    var out: WaterOut;
    out.clip = globals.view_proj * vec4<f32>(v.position, 1.0);
    out.uv1 = v.uv1;
    out.uv2 = v.uv2;
    let l = normalize(globals.light_dir.xyz);
    let lit = v.color.rgb + globals.ambient.rgb * 0.3 + directional() * 0.7 * max(l.y, 0.0);
    out.color = vec4<f32>(min(lit, vec3<f32>(1.0)), v.color.a);
    out.world_pos = v.position;
    return out;
}

@fragment
fn fs_water(in: WaterOut) -> @location(0) vec4<f32> {
    let one = textureSample(t_one, s_one, in.uv1);
    let two = textureSample(t_two, s_two, in.uv2);
    let color = min(one.rgb * in.color.rgb + two.rgb * two.a, vec3<f32>(1.0));
    return vec4<f32>(mix(color, globals.fog_color.rgb, fog_factor(in.world_pos)), one.a * in.color.a);
}
