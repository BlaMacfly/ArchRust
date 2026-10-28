// Pipeline éclairé simple d'ArchRust : lumière directionnelle + ambiante, texture diffuse,
// couleur de sommet (prélit RenderWare), test alpha.

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    camera_pos: vec4<f32>,
    // Lumière directionnelle du ciel (w = 0 : 1 − ambiante).
    light_color: vec4<f32>,
    fog_color: vec4<f32>,
    // Début, fin, opacité maximale du brouillard.
    fog_range: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct Instance {
    model: mat4x4<f32>,
    color: vec4<f32>,
};
@group(2) @binding(0) var<uniform> inst: Instance;

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
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_pos: vec3<f32>,
};

// normalize() d'un vecteur nul rend NaN, et un NaN propagé noircit toute la couleur (lumière
// ambiante comprise) : vecteur nul ou NaN → normale verticale. `!(x > seuil)` couvre aussi NaN.
fn safe_normal(n: vec3<f32>) -> vec3<f32> {
    let len2 = dot(n, n);
    if (!(len2 > 1e-12)) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return n * inverseSqrt(len2);
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let world = inst.model * vec4<f32>(v.position, 1.0);
    out.clip = globals.view_proj * world;
    out.normal = safe_normal((inst.model * vec4<f32>(v.normal, 0.0)).xyz);
    out.uv = v.uv;
    out.color = v.color * inst.color;
    out.world_pos = world.xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t_diffuse, s_diffuse, in.uv);
    if (tex.a * in.color.a < 0.3) {
        discard;
    }
    let n = safe_normal(in.normal);
    let l = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let directional = select(1.0 - globals.ambient.rgb, globals.light_color.rgb, globals.light_color.w > 0.0);
    let light = globals.ambient.rgb + vec3<f32>(diffuse) * directional;
    // Brouillard linéaire du ciel (FogDistance → FogFarClip), ou léger brouillard par défaut.
    let dist = distance(in.world_pos, globals.camera_pos.xyz);
    let fog = clamp((dist - globals.fog_range.x) / max(globals.fog_range.y - globals.fog_range.x, 1.0), 0.0, globals.fog_range.z);
    let color = tex.rgb * in.color.rgb * light;
    return vec4<f32>(mix(color, globals.fog_color.rgb, fog), tex.a * in.color.a);
}
