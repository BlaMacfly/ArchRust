// Particules de météo (AgcmNatureEffect) et reflets du soleil (`LensFlare.ini`).
//
// * Particules : quadrilatères en coordonnées monde, texture `NE_*` × couleur de sommet, sans
//   éclairage ni brouillard, mélange alpha, profondeur testée sans écriture.
// * Reflets : quadrilatères en coordonnées écran (NDC), mélange additif. La normale de chaque
//   sommet porte la position écran du soleil : si la profondeur y est non nulle (profondeur
//   inversée : 0 = rien de dessiné), le soleil est caché et le reflet replié hors de l'écran.

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    camera_pos: vec4<f32>,
    light_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@group(2) @binding(0) var t_depth: texture_depth_2d;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_particle(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(v.position, 1.0);
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

@fragment
fn fs_particle(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t_diffuse, s_diffuse, in.uv);
    return vec4<f32>(tex.rgb * in.color.rgb, tex.a * in.color.a);
}

@vertex
fn vs_flare(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let size = vec2<f32>(textureDimensions(t_depth));
    let px = clamp(vec2<i32>((v.normal.xy * vec2<f32>(0.5, -0.5) + 0.5) * size), vec2<i32>(0, 0), vec2<i32>(size) - 1);
    let hidden = textureLoad(t_depth, px, 0) > 0.0;
    out.clip = select(vec4<f32>(v.position.xy, 0.0, 1.0), vec4<f32>(-4.0, -4.0, 0.0, 1.0), hidden);
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

@fragment
fn fs_flare(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t_diffuse, s_diffuse, in.uv);
    return vec4<f32>(tex.rgb * in.color.rgb * in.color.a, 1.0);
}
