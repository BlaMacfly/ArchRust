//! Décalques au sol d'Archonia, ajoutés aux lots d'effets de l'image (`Renderer::effects`) :
//!
//! - **Ombre ronde** (AgcmShadow, `ShadowCircle.bmp` de `Texture\Etc\`, FUN_1404d9be0) : posée sur le
//!   sol sous chaque personnage visible par FUN_1404d9d60, qui découpe la boîte de l'ombre en cases de
//!   400 unités suivant les hauteurs du terrain relevées de 4 unités, texture étirée sur la boîte. Le
//!   mélange est `source × 0`, `destination × (1 − couleur source)` (états RenderWare 10 = 1, 11 = 4),
//!   la couleur des sommets valant `|+0x16e0| × 255` (0,9 par défaut, FUN_1404d8210) : le sol est
//!   assombri de 90 % de la luminance de la texture. ArchRust en tire une texture noire dont l'alpha
//!   est cette luminance × 0,9, en mélange alpha (même résultat pour une texture grise). Boîte :
//!   sphère englobante du gabarit (`BOUNDING_SPHERE` de `charactertemplateclient.ini`, centre ± rayon,
//!   déduit). Coupée avec l'option `Shadow` à 0.
//! - **Repère de destination** (AgcmTargeting, `targeting.ini` `[Textures1]` 0 `target.png`,
//!   FUN_1404e2780) : planche de 70 × 70 (`DAT_140e4a100`) posée au point cliqué quand le joueur part à
//!   pied (mode 1), retirée à l'arrivée. Les `[Textures2]` (`ET_Selection_128/256/512`, `ET_Alkadia_*`
//!   pour l'ArchLord) servent au ciblage de zone des compétences (FUN_1404e5280 : 128 sous 400, 256
//!   sous 700, 512 au-delà, centrées sur le joueur) : Archonia ne dessine pas de cercle sous la cible
//!   choisie.

use super::App;
use crate::render::{EffectDraw, Vertex};
use crate::sky_render::EffectBlend;
use glam::Vec3;
use std::collections::HashMap;

/// Nom de la texture d'ombre préparée (noire, alpha = luminance × intensité).
const SHADOW_TEXTURE: &str = "archrust:shadowcircle";
/// Repère de destination (`targeting.ini`, `[Textures1]` 0).
const TARGET_TEXTURE: &str = "target.png";
/// Intensité de l'ombre (`AgcmShadow+0x16e0`, 0,9 à la construction).
const SHADOW_INTENSITY: f32 = 0.9;
/// Hauteur au-dessus du sol (`DAT_140e4a024`).
const LIFT: f32 = 4.0;
/// Côté du repère de destination (`DAT_140e4a100`).
const TARGET_SIZE: f32 = 70.0;
/// Pas de la grille qui suit le sol (Archonia : cases de 400 ; ArchRust subdivise plus finement).
const STEP: f32 = 100.0;

#[derive(Default)]
pub struct DecalsGlue {
    /// Texture d'ombre envoyée au GPU (`None` : pas encore tentée).
    shadow_ready: Option<bool>,
    /// (centre local x, z, rayon) de la sphère englobante par TID.
    spheres: HashMap<i32, Option<(f32, f32, f32)>>,
}

/// Sommets d'une planche couchée sur le sol entre `min` et `max` (x, z), hauteurs données par `height`.
pub fn ground_quad(min: (f32, f32), max: (f32, f32), color: [f32; 4], height: impl Fn(f32, f32) -> Option<f32>) -> Vec<Vertex> {
    let (w, d) = (max.0 - min.0, max.1 - min.1);
    if w <= 0.0 || d <= 0.0 {
        return Vec::new();
    }
    let (nx, nz) = ((w / STEP).ceil().max(1.0) as usize, (d / STEP).ceil().max(1.0) as usize);
    let point = |i: usize, j: usize| {
        let (u, v) = (i as f32 / nx as f32, j as f32 / nz as f32);
        let (x, z) = (min.0 + u * w, min.1 + v * d);
        height(x, z).map(|h| Vertex { position: [x, h + LIFT, z], normal: [0.0, 1.0, 0.0], uv: [u, v], color })
    };
    let mut out = Vec::with_capacity(nx * nz * 6);
    for j in 0..nz {
        for i in 0..nx {
            let (Some(a), Some(b), Some(c), Some(e)) = (point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)) else { continue };
            out.extend_from_slice(&[a, b, c, a, c, e]);
        }
    }
    out
}

/// Texture d'ombre : noire, alpha = luminance de `ShadowCircle.bmp` × intensité.
pub fn shadow_rgba(rgba: &[u8], intensity: f32) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let lum = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.0;
            [0, 0, 0, (lum * intensity * 255.0).round().clamp(0.0, 255.0) as u8]
        })
        .collect()
}

impl App {
    /// Sphère englobante du gabarit (`BOUNDING_SPHERE` : `x:y:z:rayon`).
    fn decal_sphere(&mut self, tid: i32) -> Option<(f32, f32, f32)> {
        let assets = &self.assets;
        *self.decals.spheres.entry(tid).or_insert_with(|| {
            let table = assets.table("ini", "charactertemplateclient.ini").ok()?;
            let key = table.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("BOUNDING_SPHERE")).map(|(k, _)| k.clone())?;
            let v: Vec<f32> = table.section(&tid.to_string())?.get(&key)?.split(':').filter_map(|s| s.trim().parse().ok()).collect();
            (v.len() == 4 && v[3] > 0.0).then(|| (v[0], v[2], v[3]))
        })
    }

    /// Ombres et repère de destination de l'image, après les effets.
    pub(super) fn decals_frame(&mut self) {
        let in_world = self.phase == super::Phase::InWorld;
        {
            let Some(gpu) = self.gpu.as_mut() else { return };
            // Lots de l'image précédente retirés (les effets ne réécrivent pas toujours la liste).
            gpu.renderer.effects.retain(|d| !matches!(d.texture.as_deref(), Some(SHADOW_TEXTURE | TARGET_TEXTURE)));
            if !in_world {
                return;
            }
            if self.decals.shadow_ready.is_none() {
                let found = self.assets.texture_found("ShadowCircle.bmp");
                let ok = match self.assets.texture("ShadowCircle.bmp") {
                    Ok(img) if found => {
                        gpu.renderer.upload_texture(SHADOW_TEXTURE, img.width, img.height, &shadow_rgba(&img.data, SHADOW_INTENSITY));
                        true
                    }
                    _ => {
                        log::warn!("ombres : ShadowCircle.bmp introuvable");
                        false
                    }
                };
                self.decals.shadow_ready = Some(ok);
            }
        }
        let mut draws = Vec::new();
        if self.decals.shadow_ready == Some(true) && self.options.video.shadow > 0 {
            let limit = self.options.video.char_distance();
            let eye = self.camera.eye();
            let chars: Vec<(i32, Vec3, f32)> = self.player.iter().chain(self.entities.iter()).filter(|e| e.cid >= 0).map(|e| (e.tid, e.position, e.turn_y)).collect();
            let mut vertices = Vec::new();
            for (tid, pos, turn) in chars {
                if limit.is_some_and(|d| pos.distance(eye) > d) {
                    continue;
                }
                let Some((cx, cz, r)) = self.decal_sphere(tid) else { continue };
                // Centre local tourné avec le personnage.
                let (s, c) = turn.to_radians().sin_cos();
                let center = (pos.x + cx * c + cz * s, pos.z - cx * s + cz * c);
                let (terrain, objects, assets) = (&self.terrain, &self.objects, &self.assets);
                vertices.extend(ground_quad((center.0 - r, center.1 - r), (center.0 + r, center.1 + r), [1.0; 4], |x, z| Self::ground_height(terrain, objects, assets, x, z)));
            }
            if !vertices.is_empty() {
                draws.push(EffectDraw { texture: Some(SHADOW_TEXTURE.into()), blend: EffectBlend::Alpha, vertices });
            }
        }
        if self.walking {
            if let Some(dest) = self.player.as_ref().map(|p| p.destination) {
                let half = TARGET_SIZE / 2.0;
                let (terrain, objects, assets) = (&self.terrain, &self.objects, &self.assets);
                let vertices = ground_quad((dest.x - half, dest.z - half), (dest.x + half, dest.z + half), [1.0; 4], |x, z| Self::ground_height(terrain, objects, assets, x, z));
                if !vertices.is_empty() {
                    draws.push(EffectDraw { texture: Some(TARGET_TEXTURE.into()), blend: EffectBlend::Alpha, vertices });
                }
            }
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        if draws.iter().any(|d| d.texture.as_deref() == Some(TARGET_TEXTURE)) {
            crate::scene::ensure_texture(&mut gpu.renderer, &self.assets, TARGET_TEXTURE);
        }
        // Sous les effets : au début de la liste, avec les mélanges alpha.
        if !draws.is_empty() {
            draws.append(&mut gpu.renderer.effects);
            gpu.renderer.effects = draws;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_follows_the_ground_and_spans_uv() {
        let v = ground_quad((0.0, 0.0), (200.0, 100.0), [1.0; 4], |x, _| Some(x * 0.1));
        // 2 × 1 cases de 100 : 12 sommets, hauteur du sol + 4, UV de 0 à 1.
        assert_eq!(v.len(), 12);
        assert!(v.iter().all(|p| (p.position[1] - (p.position[0] * 0.1 + LIFT)).abs() < 1e-4));
        assert!(v.iter().any(|p| p.uv == [1.0, 1.0]) && v.iter().any(|p| p.uv == [0.0, 0.0]));
        // Hors du terrain : rien.
        assert!(ground_quad((0.0, 0.0), (100.0, 100.0), [1.0; 4], |_, _| None).is_empty());
    }

    #[test]
    fn shadow_texture_darkens_by_luminance() {
        let px = shadow_rgba(&[255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255], SHADOW_INTENSITY);
        assert_eq!(px, vec![0, 0, 0, 230, 0, 0, 0, 0, 0, 0, 0, 115]);
    }
}
