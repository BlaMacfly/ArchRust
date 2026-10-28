// Ciel d'Archonia (AgcmEventNature) : dôme en dégradé SkyTop → SkyBottom, étoiles, soleil (lune),
// deux couches de nuages qui défilent, bande d'horizon (`Circumstance`). Un triangle plein écran
// dessiné avant la scène ; la direction de vue vient de l'inverse de la projection sans translation.
// Les couches sont dans l'ordre de `SkyLayer` (soleil, nuages, horizon) ; jusqu'à huit pendant un
// fondu croisé.

struct Sky {
    inv_view_proj: mat4x4<f32>,
    top: vec4<f32>,
    // w : opacité des étoiles.
    bottom: vec4<f32>,
    // xyz : direction du soleil, w : demi-taille angulaire.
    sun: vec4<f32>,
    // x : pas de scintillement (100 ms).
    params: vec4<f32>,
    layer_color: array<vec4<f32>, 8>,
    // x : nature (0 aucune, 1 nuage, 2 horizon, 3 soleil), yz : décalage, w : échelle.
    layer_info: array<vec4<f32>, 8>,
};
@group(0) @binding(0) var<uniform> sky: Sky;

@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var t2: texture_2d<f32>;
@group(1) @binding(3) var t3: texture_2d<f32>;
@group(1) @binding(4) var t4: texture_2d<f32>;
@group(1) @binding(5) var t5: texture_2d<f32>;
@group(1) @binding(6) var t6: texture_2d<f32>;
@group(1) @binding(7) var t7: texture_2d<f32>;
@group(1) @binding(8) var s_sky: sampler;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
    var out: VertexOut;
    let p = vec2<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0);
    // Profondeur inversée : 0 = le plus loin.
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

const PI: f32 = 3.14159265;

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// Couche `i` (texture `t`) posée sur la couleur `col` pour la direction `dir`.
fn layer(col: vec3<f32>, i: u32, t: texture_2d<f32>, dir: vec3<f32>) -> vec3<f32> {
    let info = sky.layer_info[i];
    let color = sky.layer_color[i];
    let kind = i32(info.x + 0.5);
    var uv = vec2<f32>(0.0, 0.0);
    var weight = 0.0;
    if (kind == 1) {
        // Nuages : plan au-dessus de la caméra, effacé vers l'horizon.
        let h = max(dir.y, 0.0);
        uv = dir.xz / (h + 0.12) * info.w + info.yz;
        weight = smoothstep(0.0, 0.25, dir.y);
    } else if (kind == 2) {
        // Bande d'horizon : tour complet en azimut, haut transparent (v = 0) → horizon (v = 1).
        let az = atan2(dir.z, dir.x);
        uv = vec2<f32>(az / (2.0 * PI) * info.w, 1.0 - clamp(dir.y / 0.35, 0.0, 1.0));
        weight = 1.0;
    } else if (kind == 3) {
        let s = normalize(sky.sun.xyz);
        let right = normalize(cross(s, vec3<f32>(0.0, 1.0, 0.0)));
        let up = cross(right, s);
        uv = vec2<f32>(dot(dir, right), -dot(dir, up)) / sky.sun.w * 0.5 + 0.5;
        if (dot(dir, s) > 0.0 && uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0) {
            weight = 1.0;
        }
    }
    // Échantillonnage hors des branches (dérivées uniformes).
    let tex = textureSampleLevel(t, s_sky, uv, 0.0);
    let a = clamp(tex.a * color.a * weight, 0.0, 1.0);
    return mix(col, tex.rgb * color.rgb, a);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let near = sky.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let mid = sky.inv_view_proj * vec4<f32>(in.ndc, 0.5, 1.0);
    let dir = normalize(mid.xyz / mid.w - near.xyz / near.w);
    // Dégradé du dôme selon l'élévation ; sous l'horizon, la couleur du bas.
    let elev = asin(clamp(dir.y, 0.0, 1.0)) / (PI * 0.5);
    var col = mix(sky.bottom.rgb, sky.top.rgb, elev);
    // Étoiles : points sur une grille de directions ; toutes les 100 ms, 20 % prennent un alpha
    // de 0x91 à 0xFE (FUN_14057e3a0).
    if (sky.bottom.w > 0.0 && dir.y > 0.0) {
        let g = dir * 90.0;
        let cell = floor(g);
        if (hash3(cell) < 0.035) {
            let jitter = vec3<f32>(hash3(cell + 17.1), hash3(cell + 31.7), hash3(cell + 47.3)) * 0.6 + 0.2;
            let star = normalize(cell + jitter);
            let d = acos(clamp(dot(dir, star), -1.0, 1.0));
            var alpha = 254.0 / 255.0;
            let tick = sky.params.x;
            if (hash3(cell + vec3<f32>(tick, tick * 0.37, 1.0)) < 0.2) {
                alpha = (145.0 + hash3(cell + vec3<f32>(tick * 0.51, 3.0, tick)) * 109.0) / 255.0;
            }
            let point = 1.0 - smoothstep(0.0008, 0.0022, d);
            col = mix(col, vec3<f32>(1.0, 1.0, 1.0), point * alpha * sky.bottom.w * smoothstep(0.0, 0.1, dir.y));
        }
    }
    col = layer(col, 0u, t0, dir);
    col = layer(col, 1u, t1, dir);
    col = layer(col, 2u, t2, dir);
    col = layer(col, 3u, t3, dir);
    col = layer(col, 4u, t4, dir);
    col = layer(col, 5u, t5, dir);
    col = layer(col, 6u, t6, dir);
    col = layer(col, 7u, t7, dir);
    return vec4<f32>(col, 1.0);
}
