// Pipeline éclairé simple d'ArchRust : lumière directionnelle + ambiante + lumières ponctuelles des effets,
// texture diffuse, couleur de sommet (prélit RenderWare), test alpha ; lueur des armures serties
// (second étage `uvAnim.fx`, groupe 3, voir `app_stones.rs`).

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
    // Lumières des effets (AgcdEffLight) : xyz position, w rayon ; rgb couleur ; x nombre (≤ 8).
    point_pos: array<vec4<f32>, 16>,
    point_color: array<vec4<f32>, 16>,
    point_count: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct Instance {
    model: mat4x4<f32>,
    color: vec4<f32>,
    // Lueur d'armure sertie (`uvAnim.fx` d'Archonia, voir `app_stones.rs`) : échelle UV xy,
    // translation UV par seconde zw ; sélection U xyz, rotation w (°/s) ; sélection V xyz, poids w
    // (0 : sans lueur) ; spéculaire rgb, temps w (s) ; ambiante et diffuse du matériau.
    glow_uv: vec4<f32>,
    glow_u: vec4<f32>,
    glow_v: vec4<f32>,
    glow_spec: vec4<f32>,
    glow_ambient: vec4<f32>,
    glow_diffuse: vec4<f32>,
};
@group(2) @binding(0) var<uniform> inst: Instance;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
// Texture animée de la lueur (`SOUL_*`), blanche sans lueur.
@group(3) @binding(0) var t_glow: texture_2d<f32>;
@group(3) @binding(1) var s_glow: sampler;

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
    // Position dans le repère du maillage (sommets skinnés compris) : `netPosition` d'`uvAnim.fx`.
    @location(4) model_pos: vec3<f32>,
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

// Lumières ponctuelles des effets (type 5 d'AgcmEff2) : apport diffus rgb × max(0, N·L) × att(d),
// ajouté au soleil et à l'ambiante. Atténuation `1 − d/R` : CHOIX d'ArchRust, celle du pilote D3D9
// de RenderWare n'a pas été retrouvée dans Archonia (inconnue).
fn point_lighting(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = min(u32(globals.point_count.x), 16u);
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
    let world = inst.model * vec4<f32>(v.position, 1.0);
    out.clip = globals.view_proj * world;
    out.normal = safe_normal((inst.model * vec4<f32>(v.normal, 0.0)).xyz);
    out.uv = v.uv;
    out.color = v.color * inst.color;
    out.world_pos = world.xyz;
    out.model_pos = v.position;
    return out;
}

// Coordonnées de la texture animée (`VS` d'`uvAnim.fx`) : `rot(rotation × temps) × (échelle ×
// (pos·uSel, pos·vSel)) + temps × translation`.
fn glow_uv(p: vec3<f32>) -> vec2<f32> {
    let t = inst.glow_spec.w;
    let a = radians(inst.glow_u.w * t);
    let s = sin(a);
    let c = cos(a);
    let uv = inst.glow_uv.xy * vec2<f32>(dot(p, inst.glow_u.xyz), dot(p, inst.glow_v.xyz));
    return vec2<f32>(c * uv.x - s * uv.y, s * uv.x + c * uv.y) + t * inst.glow_uv.zw;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t_diffuse, s_diffuse, in.uv);
    let anim = textureSample(t_glow, s_glow, glow_uv(in.model_pos));
    if (tex.a * in.color.a < 0.3) {
        discard;
    }
    let n = safe_normal(in.normal);
    let l = normalize(globals.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let directional = select(1.0 - globals.ambient.rgb, globals.light_color.rgb, globals.light_color.w > 0.0);
    let points = point_lighting(in.world_pos, n);
    let light = globals.ambient.rgb + vec3<f32>(diffuse) * directional + points;
    // Brouillard linéaire du ciel (FogDistance → FogFarClip), ou léger brouillard par défaut.
    let dist = distance(in.world_pos, globals.camera_pos.xyz);
    let fog = clamp((dist - globals.fog_range.x) / max(globals.fog_range.y - globals.fog_range.x, 1.0), 0.0, globals.fog_range.z);
    var color = tex.rgb * in.color.rgb * light;
    if (inst.glow_v.w > 0.0) {
        // Lueur d'armure, pixel shader `PS` d'`uvAnim.fx` (ps_1_1) : `sat(N·L × diffuse + ambiante)
        // × base`, fondu vers la texture animée par `alpha × poids` (`lrp`), puis `mad_x2` :
        // `2 × (couleur + (N·H)^8 × spéculaire × lumière)`.
        let lit = clamp(globals.ambient.rgb * inst.glow_ambient.rgb + vec3<f32>(diffuse) * directional * inst.glow_diffuse.rgb + points, vec3<f32>(0.0), vec3<f32>(1.0));
        let base = mix(tex.rgb * in.color.rgb * lit, anim.rgb, clamp(anim.a * inst.glow_v.w, 0.0, 1.0));
        let h = safe_normal(l + safe_normal(globals.camera_pos.xyz - in.world_pos));
        var nh = clamp(dot(n, h), 0.0, 1.0);
        nh = nh * nh;
        nh = nh * nh;
        nh = nh * nh;
        color = clamp(2.0 * (base + nh * inst.glow_spec.rgb * directional), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(mix(color, globals.fog_color.rgb, fog), tex.a * in.color.a);
}
