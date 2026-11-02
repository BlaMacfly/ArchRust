// Herbe du monde (AgcmGrass, `GrassTemplate.ini`), voir `world_render.rs`.
//
// Touffe (repère du brin) → matrice S·RotX·RotY du brin → inclinaison RotX du balancement (niveau 0,
// `FUN_140592730`) → position. Couleur : texture × min(2 × couleur d'éclairage, 1) (`MODULATE2X`).

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    camera_pos: vec4<f32>,
    light_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
    point_pos: array<vec4<f32>, 16>,
    point_color: array<vec4<f32>, 16>,
    point_count: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var t_grass: texture_2d<f32>;
@group(1) @binding(1) var s_grass: sampler;

struct GrassParams {
    // x : temps (ms) ; yzw : couleur d'éclairage.
    params: vec4<f32>,
};
@group(2) @binding(0) var<uniform> grass: GrassParams;

fn fog_factor(p: vec3<f32>) -> f32 {
    let dist = distance(p, globals.camera_pos.xyz);
    return clamp((dist - globals.fog_range.x) / max(globals.fog_range.y - globals.fog_range.x, 1.0), 0.0, globals.fog_range.z);
}

struct GrassIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) c0: vec4<f32>,
    @location(3) c1: vec4<f32>,
    @location(4) c2: vec4<f32>,
    @location(5) sway: vec4<f32>,
};

struct GrassOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
};

@vertex
fn vs_grass(v: GrassIn) -> GrassOut {
    var out: GrassOut;
    let origin = vec3<f32>(v.c0.w, v.c1.w, v.c2.w);
    var p = v.c0.xyz * v.position.x + v.c1.xyz * v.position.y + v.c2.xyz * v.position.z;
    if (distance(origin, globals.camera_pos.xyz) < v.sway.w) {
        // Table de 256 sinus d'Archonia, indice round(phase · 256 / 360) & 0xff.
        let phase = (v.sway.x + grass.params.x * v.sway.y) % 360.0;
        let index = f32(u32(round(phase * 0.7111111)) & 255u);
        let tilt = radians(sin(index * 6.2831853 / 256.0) * v.sway.z);
        let c = cos(tilt);
        let s = sin(tilt);
        p = vec3<f32>(p.x, p.y * c - p.z * s, p.y * s + p.z * c);
    }
    let world = p + origin;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = v.uv;
    out.world_pos = world;
    return out;
}

@fragment
fn fs_grass(in: GrassOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t_grass, s_grass, in.uv);
    if (tex.a < 0.3) {
        discard;
    }
    let color = tex.rgb * min(2.0 * grass.params.yzw, vec3<f32>(1.0));
    return vec4<f32>(mix(color, globals.fog_color.rgb, fog_factor(in.world_pos)), tex.a);
}
