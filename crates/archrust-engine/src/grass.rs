//! Herbe des secteurs détaillés (AgcmGrass) : brins de `world/grass/gr%d.dat`, touffes de
//! `GrassTemplate.ini`, balancement au vent.
//!
//! * **Chargement** — Archonia lit l'herbe d'un secteur quand il devient détaillé
//!   (`FUN_140592e10`, appel 2) : ArchRust aussi, et la jette avec le détail.
//! * **Option `NaturalObj`** (`FUN_140711350` → `FUN_140597390`) : 0 → aucune herbe ; 1 → type de
//!   distance 2 (`loddistancetype.txt`), 3 niveaux ; 2 et 3 → type 3, 4 niveaux. Les brins sont
//!   dessinés à tous les niveaux (le choix du niveau retombe sur le dernier, voir `archrust_data::lod`) ;
//!   seul le niveau 0 se balance, en deçà de `dist[type][0] × facteur MapView` (déduit : le calcul
//!   du niveau par AgcmRender n'a pas été suivi).
//! * **Brin** (`FUN_140593b40`) — matrice S·RotX·RotY (colonnes : RotY·RotX·S) avec des échelles
//!   tirées au hasard et une phase initiale aléatoire ; ArchRust remplace `rand()` par un hachage
//!   déterministe de la position.
//! * **Couleur** — Archonia écrit min(A × 0,5 + B, 1) dans la couleur ambiante des sommets à chaque
//!   changement de lumière (`0x140592cb0`, A et B : couleurs de la lumière du ciel, ordre
//!   inconnu) ; ArchRust prend A = soleil, B = ambiante, multipliés par l'ambiante globale.

use crate::assets::Assets;
use crate::water::{hash32, texture_stem};
use crate::world::tile_of;
use crate::world_render::{GrassDraw, GrassInstance, TuftVertex};
use archrust_data::grass::{blade_scale, tuft_vertices, GrassBlade, GrassFile, GrassTemplates};
use archrust_data::lod::LodDistances;
use glam::{Mat3, Vec3};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Réglage de l'option `NaturalObj` : (type de distance, nombre de niveaux), `None` = pas d'herbe.
pub fn natural_obj_setting(natural_obj: u8) -> Option<(usize, usize)> {
    match natural_obj {
        0 => None,
        1 => Some((2, 3)),
        _ => Some((3, 4)),
    }
}

/// Touffe prête d'un gabarit : texture et 12 sommets.
#[derive(Clone)]
struct Tuft {
    texture: Option<String>,
    vertices: [TuftVertex; 12],
    anim: bool,
    speed: f32,
    amount: f32,
}

#[derive(Default)]
pub struct GrassWorld {
    templates: Option<Arc<GrassTemplates>>,
    tufts: Vec<Option<Tuft>>,
    files: HashMap<u32, Option<Arc<GrassFile>>>,
    /// Brins des secteurs détaillés : (gabarit, instance).
    sectors: HashMap<(i32, i32), Vec<(u32, GrassInstance)>>,
    draw: Option<Arc<GrassDraw>>,
    version: u64,
    dirty: bool,
    /// Distance du balancement (niveau 0).
    sway_distance: f32,
}

/// Instance d'un brin, ou `None` s'il est écarté comme dans `FUN_140593b40`.
pub fn blade_instance(b: &GrassBlade, speed: f32, amount: f32, sway_distance: f32) -> Option<GrassInstance> {
    if b.rot_x > 360.0 || b.rot_y > 360.0 {
        return None;
    }
    let seed = hash32(b.position[0].to_bits() ^ b.position[2].to_bits().rotate_left(13) ^ b.template);
    let mut state = seed;
    let mut rand = || {
        state = hash32(state.wrapping_add(0x9e37_79b9));
        state >> 1
    };
    let phase = (rand() % 360) as f32;
    let [sx, sy, sz] = blade_scale(b.scale, &mut rand);
    let m = Mat3::from_rotation_y(b.rot_y.to_radians()) * Mat3::from_rotation_x(b.rot_x.to_radians()) * Mat3::from_diagonal(Vec3::new(sx, sy, sz));
    let p = b.position;
    Some(GrassInstance {
        c0: m.x_axis.extend(p[0]).to_array(),
        c1: m.y_axis.extend(p[1]).to_array(),
        c2: m.z_axis.extend(p[2]).to_array(),
        sway: [phase, speed, amount, sway_distance],
    })
}

impl GrassWorld {
    fn templates(&mut self, assets: &Assets) -> Arc<GrassTemplates> {
        if self.templates.is_none() {
            let t = assets.table("ini", "GrassTemplate.ini").map(|t| GrassTemplates::parse(&t)).unwrap_or_default();
            self.tufts = t
                .templates
                .iter()
                .enumerate()
                .map(|(i, tpl)| {
                    if tpl.width <= 0 && tpl.height <= 0 {
                        return None;
                    }
                    let texture = t.textures.get(tpl.texture).and_then(|n| texture_stem(n));
                    let size = texture.as_ref().and_then(|n| assets.texture(n).ok()).map_or((256, 256), |img| (img.width, img.height));
                    let mut state = hash32(i as u32 + 1);
                    let v = tuft_vertices(tpl, size, || {
                        state = hash32(state);
                        state >> 1
                    });
                    Some(Tuft {
                        texture,
                        vertices: v.map(|v| TuftVertex { position: v.position, uv: v.uv }),
                        anim: tpl.anim_type == 0,
                        speed: tpl.anim_speed,
                        amount: tpl.anim_amount,
                    })
                })
                .collect();
            self.templates = Some(Arc::new(t));
        }
        self.templates.clone().unwrap()
    }

    /// Distance du balancement d'après `NaturalObj` et le facteur `MapView`.
    pub fn set_options(&mut self, lod: &LodDistances, natural_obj: u8, map_factor: f32) -> bool {
        let sway = natural_obj_setting(natural_obj).map_or(0.0, |(kind, _)| lod.dist[kind][0] as f32 * map_factor);
        if (sway - self.sway_distance).abs() > 0.5 {
            self.sway_distance = sway;
            for list in self.sectors.values_mut() {
                for (_, i) in list.iter_mut() {
                    i.sway[3] = sway;
                }
            }
            self.dirty = true;
        }
        natural_obj_setting(natural_obj).is_some()
    }

    /// Secteurs détaillés voulus : lit les nouveaux, oublie les autres.
    pub fn set_sectors(&mut self, assets: &Assets, detailed: &HashSet<(i32, i32)>) {
        if !assets.has_client() {
            return;
        }
        let before = self.sectors.len();
        self.sectors.retain(|k, _| detailed.contains(k));
        let mut changed = before != self.sectors.len();
        self.templates(assets);
        for &(sx, sy) in detailed {
            if self.sectors.contains_key(&(sx, sy)) {
                continue;
            }
            let tile = tile_of(sx, sy);
            let file = self
                .files
                .entry(tile)
                .or_insert_with(|| {
                    let bytes = assets.read_packed("world/grass", &format!("gr{tile}.dat")).ok()?;
                    GrassFile::parse(&bytes).inspect_err(|e| log::warn!("gr{tile}.dat : {e}")).ok().map(Arc::new)
                })
                .clone();
            let mut list = Vec::new();
            if let Some(f) = file {
                for g in f.sector(sx.rem_euclid(16) as usize, sy.rem_euclid(16) as usize) {
                    for b in &g.blades {
                        let Some(Some(t)) = self.tufts.get(b.template as usize) else { continue };
                        let (speed, amount) = if t.anim { (t.speed, t.amount) } else { (0.0, 0.0) };
                        if let Some(i) = blade_instance(b, speed, amount, self.sway_distance) {
                            list.push((b.template, i));
                        }
                    }
                }
            }
            self.sectors.insert((sx, sy), list);
            changed = true;
        }
        let tiles: HashSet<u32> = detailed.iter().map(|&(x, y)| tile_of(x, y)).collect();
        self.files.retain(|t, _| tiles.contains(t));
        if changed {
            self.dirty = true;
        }
    }

    /// Brins chargés.
    pub fn blade_count(&self) -> usize {
        self.sectors.values().map(Vec::len).sum()
    }

    /// Données de rendu, reconstruites après un changement.
    pub fn draw(&mut self) -> Option<Arc<GrassDraw>> {
        if self.dirty {
            self.dirty = false;
            self.version += 1;
            let mut by_template: HashMap<u32, Vec<GrassInstance>> = HashMap::new();
            for list in self.sectors.values() {
                for (t, i) in list {
                    by_template.entry(*t).or_default().push(*i);
                }
            }
            let mut g = GrassDraw { version: self.version, ..Default::default() };
            let mut keys: Vec<u32> = by_template.keys().copied().collect();
            keys.sort_unstable();
            for t in keys {
                let Some(Some(tuft)) = self.tufts.get(t as usize) else { continue };
                let v0 = g.vertices.len() as u32;
                g.vertices.extend_from_slice(&tuft.vertices);
                let i0 = g.instances.len() as u32;
                g.instances.extend_from_slice(&by_template[&t]);
                g.batches.push((tuft.texture.clone(), v0..v0 + 12, i0..g.instances.len() as u32));
            }
            self.draw = Some(Arc::new(g));
        }
        self.draw.clone()
    }

    /// Textures des touffes (à téléverser).
    pub fn textures(&self) -> Vec<String> {
        self.tufts.iter().flatten().filter_map(|t| t.texture.clone()).collect::<HashSet<_>>().into_iter().collect()
    }
}

/// Couleur d'éclairage de l'herbe : min(soleil × 0,5 + ambiante, 1) × ambiante (voir l'en-tête).
pub fn grass_color(ambient: [f32; 4], directional: [f32; 3]) -> [f32; 3] {
    let mut c = [0.0; 3];
    for i in 0..3 {
        c[i] = (directional[i] * 0.5 + ambient[i]).min(1.0) * ambient[i];
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blade_matrix_and_options() {
        let b = GrassBlade { template: 3, position: [100.0, 5.0, -50.0], rot_x: 0.0, rot_y: 90.0, scale: 1.0 };
        let i = blade_instance(&b, 0.075, 5.0, 12000.0).unwrap();
        assert_eq!([i.c0[3], i.c1[3], i.c2[3]], [100.0, 5.0, -50.0]);
        // RotY 90° : +X du brin vers −Z, à l'échelle tirée (0,9 à 1,09).
        assert!(i.c0[2] < -0.89 && i.c0[2] > -1.1 && i.c0[0].abs() < 1e-5, "{:?}", i.c0);
        assert!(i.sway[0] < 360.0 && i.sway[3] == 12000.0);
        assert!(blade_instance(&GrassBlade { rot_x: 400.0, ..b }, 0.0, 0.0, 0.0).is_none());
        assert_eq!(natural_obj_setting(0), None);
        assert_eq!(natural_obj_setting(1), Some((2, 3)));
        assert_eq!(natural_obj_setting(3), Some((3, 4)));
        let c = grass_color([0.5, 0.5, 0.5, 1.0], [1.0, 1.0, 1.0]);
        assert!((c[0] - 0.5).abs() < 1e-6);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : herbe autour du secteur (324, 385), au nord
    /// d'Anchorville (3186 brins ; la ville elle-même n'en a presque pas).
    #[test]
    fn game_grass() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut g = GrassWorld::default();
        let lod = LodDistances { dist: [[15000; 5]; 10] };
        assert!(g.set_options(&lod, 1, 0.8));
        let wanted: HashSet<(i32, i32)> = (323..=325).flat_map(|x| (384..=386).map(move |y| (x, y))).collect();
        g.set_sectors(&assets, &wanted);
        assert!(g.blade_count() > 3000, "{} brins", g.blade_count());
        let d = g.draw().unwrap();
        assert!(!d.batches.is_empty());
        assert_eq!(d.instances.len(), g.blade_count());
        let tex = g.textures();
        assert!(tex.iter().any(|t| t == "grass_hu_01"), "{tex:?}");
        let img = assets.texture("grass_hu_01").unwrap();
        assert!(img.width >= 64 && img.from_patch.is_none());
        // Image lue (le substitut est uni) : des pixels transparents et opaques.
        assert!(img.data.chunks(4).any(|p| p[3] < 40) && img.data.chunks(4).any(|p| p[3] > 200));
    }
}
