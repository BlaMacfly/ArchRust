//! Eau des secteurs chargés (AgcmWater, rendu `WaterStatusT1.ini` d'Archonia) : surfaces et vagues.
//!
//! * **Surfaces** — chaque rectangle de `world/water/wt%d.dat` devient un quadrilatère à sa hauteur
//!   (`FUN_14058bcc0`) : UV = cellule / `TILESIZE_L1`, couleur prélit `RED…ALPHA_L1`, textures
//!   `WaterTextures[WATERFILE_L1]` et `[WATERFILE_L2]`. Les UV de chaque couche défilent de
//!   `U_L*`, `V_L*` à chaque image (`FUN_14058cb70`) ; Archonia avance d'un pas par image, ArchRust
//!   suppose 60 images par seconde (cadence d'Archonia inconnue). Les surfaces sont dessinées pour
//!   tous les secteurs chargés, grossiers compris.
//! * **Vagues** — seulement pour les secteurs détaillés, comme Archonia : simulation de
//!   `FUN_140590d70` (voir [`WaveState::step`]), texture `WaveTextures[WAVEFILE]`.
//! * **Mode** — Archonia ramène l'option `Water` à un mode ≥ 2 (`FUN_1405909a0`) : l'eau à shaders
//!   de `HWaterStatus.ini` n'est jamais dessinée ; ArchRust non plus.

use crate::assets::Assets;
use crate::render::Vertex;
use crate::world::{sector_origin, tile_of, CELL_SIZE};
use crate::world_render::{WaterBatch, WaterVertex, WaveBatch};
use archrust_data::water::{SectorWater, Wave, WaterFile, WaterStatusT1, WaterStatusT1Table};
use glam::{Mat4, Quat, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Nom de texture d'une entrée de table (`water_nomal_01.bmp` → `water_nomal_01`) : le lecteur
/// de textures essaie les extensions lui-même.
pub fn texture_stem(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        return None;
    }
    Some(n.rsplit_once('.').map_or(n, |(s, _)| s).to_ascii_lowercase())
}

/// Générateur déterministe à la place de `rand()` d'Archonia.
pub(crate) fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// État d'une vague (champs de la structure de 0x198 octets d'Archonia).
#[derive(Debug, Clone)]
pub struct WaveState {
    pub wave: Wave,
    /// Temps courant (ms) depuis le départ (`+0x28`).
    time: i32,
    position: Vec3,
    velocity: (f32, f32),
    scale: [f32; 2],
    scale_rate: [f32; 2],
    bob: f32,
    bob_down: bool,
    alpha: f32,
    alpha_rate: f32,
    fading: bool,
    seed: u32,
}

impl WaveState {
    /// Départ (`FUN_140590c70`) : vitesse (cos a, −sin a) × vitesse, échelle 1, alpha 0.
    pub fn new(wave: Wave, status: &WaterStatusT1, seed: u32) -> Self {
        let a = wave.angle.to_radians();
        let life = status.wave_lifetime.max(1) as f32;
        WaveState {
            wave,
            time: wave.start_time,
            position: Vec3::from_array(wave.position),
            velocity: (a.cos() * wave.speed, -a.sin() * wave.speed),
            scale: [1.0, 1.0],
            scale_rate: [2.0 / life * status.wave_scale_x, 2.0 / life * status.wave_scale_z],
            bob: 0.0,
            bob_down: false,
            alpha: 0.0,
            alpha_rate: 1.666_666_6 / life,
            fading: false,
            seed,
        }
    }

    /// Une image de `dt` ms (`FUN_140590d70`).
    pub fn step(&mut self, status: &WaterStatusT1, dt: i32) {
        let before = self.time;
        self.time += dt;
        if before < 0 && self.time < 1 {
            return;
        }
        let life = status.wave_lifetime.max(1) as f32;
        let dtf = dt as f32;
        let f = self.time as f32 / life;
        if f >= 0.7 {
            self.bob += dtf * 0.015;
        } else {
            self.bob += if self.bob_down { -dtf * 0.01 } else { dtf * 0.01 };
            if self.bob > 6.0 {
                self.bob_down = true;
            } else if self.bob < -6.0 {
                self.bob_down = false;
            }
        }
        if !self.fading {
            self.alpha += dtf * self.alpha_rate;
            if self.alpha > 1.0 {
                self.alpha = 1.0;
                self.fading = true;
                self.alpha_rate = 2.5 / life;
            }
        } else {
            self.alpha = (self.alpha - dtf * self.alpha_rate).max(0.0);
        }
        if f < 1.0 {
            let k = if f < 0.5 {
                1.0
            } else if f < 0.6 {
                0.9
            } else {
                0.7
            };
            self.position.x += dtf * self.velocity.0 * k;
            self.position.z += dtf * self.velocity.1 * k;
            // Table `+0x3ec0` : sin(i°), indice f × 180.
            let s = ((f * 180.0) as i32 as f32).to_radians().sin();
            self.scale[0] += self.scale_rate[0] * s * dtf;
            self.scale[1] += self.scale_rate[1] * s * dtf;
        } else {
            self.position = Vec3::from_array(self.wave.position);
            self.time = self.wave.start_time;
            self.scale = [1.0, 1.0];
            self.alpha = 0.0;
            self.fading = false;
            self.bob = 0.0;
            self.alpha_rate = 1.666_666_6 / life;
            self.seed = hash32(self.seed);
            self.bob_down = self.seed & 1 == 1;
        }
    }

    /// Visible (temps de départ atteint et alpha non nul).
    pub fn visible(&self) -> bool {
        self.time >= 1 && self.alpha > 0.0
    }

    /// Les 6 sommets (monde) de la vague (`FUN_14058c200`) : ±demi-hauteur en X, ±demi-largeur
    /// en Z, matrice S(sx, 1, sz) · RotY(angle) · T(position + bob).
    pub fn vertices(&self, status: &WaterStatusT1) -> [Vertex; 6] {
        let (d, c) = (self.wave.half_height as f32, self.wave.half_width as f32);
        let m = Mat4::from_translation(self.position + Vec3::new(0.0, self.bob, 0.0))
            * Mat4::from_quat(Quat::from_rotation_y(self.wave.angle.to_radians()))
            * Mat4::from_scale(Vec3::new(self.scale[0], 1.0, self.scale[1]));
        let [r, g, b] = status.wave_color.map(|v| v as f32 / 255.0);
        let a = (self.alpha * 255.0) as u8 as f32 / 255.0;
        let v = |x: f32, z: f32, u: f32, w: f32| Vertex {
            position: m.transform_point3(Vec3::new(x, 0.0, z)).to_array(),
            normal: [0.0, 1.0, 0.0],
            uv: [u, w],
            color: [r, g, b, a],
        };
        [v(-d, -c, 0.0, 0.0), v(d, -c, 1.0, 0.0), v(-d, c, 0.0, 1.0), v(d, -c, 1.0, 0.0), v(d, c, 1.0, 1.0), v(-d, c, 0.0, 1.0)]
    }
}

/// Eau d'un secteur chargé.
struct SectorState {
    water: SectorWater,
    waves: Vec<WaveState>,
    detailed: bool,
}

/// Eau autour du joueur.
#[derive(Default)]
pub struct WaterWorld {
    table: Option<Arc<WaterStatusT1Table>>,
    files: HashMap<u32, Option<Arc<WaterFile>>>,
    sectors: HashMap<(i32, i32), SectorState>,
    /// Temps écoulé (ms) pour le défilement des UV.
    elapsed_ms: f64,
}

/// Eau d'un secteur lue dans le pack `world/water`.
pub fn sector_water(assets: &Assets, files: &mut HashMap<u32, Option<Arc<WaterFile>>>, sx: i32, sy: i32) -> Option<SectorWater> {
    let tile = tile_of(sx, sy);
    let file = files
        .entry(tile)
        .or_insert_with(|| {
            let bytes = assets.read_packed("world/water", &format!("wt{tile}.dat")).ok()?;
            WaterFile::parse(&bytes).inspect_err(|e| log::warn!("wt{tile}.dat : {e}")).ok().map(Arc::new)
        })
        .clone()?;
    let s = file.sector(sx.rem_euclid(16) as usize, sy.rem_euclid(16) as usize)?;
    (!s.rects.is_empty() || !s.waves.is_empty()).then(|| s.clone())
}

impl WaterWorld {
    fn table(&mut self, assets: &Assets) -> Arc<WaterStatusT1Table> {
        self.table
            .get_or_insert_with(|| Arc::new(assets.table("ini", "WaterStatusT1.ini").map(|t| WaterStatusT1Table::parse(&t)).unwrap_or_default()))
            .clone()
    }

    /// Secteurs voulus (`true` = détaillé) : lit les nouveaux, oublie les autres, crée ou retire
    /// les vagues selon le détail.
    pub fn set_sectors(&mut self, assets: &Assets, wanted: &HashMap<(i32, i32), bool>) {
        if !assets.has_client() {
            return;
        }
        self.sectors.retain(|k, _| wanted.contains_key(k));
        let table = self.table(assets);
        for (&(sx, sy), &detailed) in wanted {
            if !self.sectors.contains_key(&(sx, sy)) {
                let Some(water) = sector_water(assets, &mut self.files, sx, sy) else { continue };
                self.sectors.insert((sx, sy), SectorState { water, waves: Vec::new(), detailed: false });
            }
            let s = self.sectors.get_mut(&(sx, sy)).unwrap();
            if s.detailed != detailed {
                s.detailed = detailed;
                s.waves = if detailed {
                    s.water
                        .waves
                        .iter()
                        .enumerate()
                        .filter_map(|(i, w)| {
                            let st = table.status(w.status)?;
                            Some(WaveState::new(*w, st, hash32((sx as u32) << 20 ^ (sy as u32) << 8 ^ i as u32)))
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            }
        }
        // Tuiles lointaines oubliées.
        let tiles: std::collections::HashSet<u32> = wanted.keys().map(|&(x, y)| tile_of(x, y)).collect();
        self.files.retain(|t, _| tiles.contains(t));
    }

    /// Avance le défilement et les vagues de `dt` ms.
    pub fn update(&mut self, dt_ms: u32) {
        self.elapsed_ms = (self.elapsed_ms + dt_ms as f64) % 3.6e9;
        let Some(table) = self.table.clone() else { return };
        let dt = dt_ms.min(250) as i32;
        for s in self.sectors.values_mut() {
            for w in &mut s.waves {
                if let Some(st) = table.status(w.wave.status) {
                    w.step(st, dt);
                }
            }
        }
    }

    pub fn loaded_count(&self) -> usize {
        self.sectors.len()
    }

    /// Lots de l'image : surfaces par type d'eau, vagues par texture.
    pub fn draw(&self) -> (Vec<WaterBatch>, Vec<WaveBatch>) {
        let Some(table) = self.table.as_ref() else { return Default::default() };
        let frames = self.elapsed_ms * 0.06;
        let mut surfaces: HashMap<u32, WaterBatch> = HashMap::new();
        let mut waves: HashMap<i32, WaveBatch> = HashMap::new();
        let tex_of = |list: &[String], i: i32| usize::try_from(i).ok().and_then(|i| list.get(i)).and_then(|n| texture_stem(n));
        for (&(sx, sy), s) in &self.sectors {
            let origin = sector_origin(sx, sy);
            for r in &s.water.rects {
                let Some(st) = table.status(r.status) else { continue };
                if !(0..=1).contains(&st.wt_type) || r.width <= 0 || r.height <= 0 {
                    continue;
                }
                let batch = surfaces.entry(r.status).or_insert_with(|| WaterBatch {
                    texture1: tex_of(&table.water_textures, st.water_file_l1),
                    texture2: tex_of(&table.water_textures, st.water_file_l2),
                    vertices: Vec::new(),
                });
                let scroll = |uv: [f32; 2]| [((uv[0] as f64 * frames) % 1.0) as f32, ((uv[1] as f64 * frames) % 1.0) as f32];
                let (o1, o2) = (scroll(st.uv_l1), scroll(st.uv_l2));
                let tile = st.tile_size_l1.max(1) as f32;
                let color = st.color_l1.map(|c| c as f32 / 255.0);
                let corner = |i: i16, j: i16| {
                    let p = Vec3::new(origin.x + i as f32 * CELL_SIZE, r.y, origin.z + j as f32 * CELL_SIZE);
                    let uv = [i as f32 / tile, j as f32 / tile];
                    WaterVertex {
                        position: p.to_array(),
                        uv1: [uv[0] + o1[0], uv[1] + o1[1]],
                        uv2: [uv[0] + o2[0], uv[1] + o2[1]],
                        color,
                    }
                };
                let (x0, z0, x1, z1) = (r.x, r.z, r.x + r.width, r.z + r.height);
                let q = [corner(x0, z0), corner(x1, z0), corner(x0, z1), corner(x1, z1)];
                batch.vertices.extend_from_slice(&[q[0], q[1], q[2], q[1], q[3], q[2]]);
            }
            for w in s.waves.iter().filter(|w| w.visible()) {
                let Some(st) = table.status(w.wave.status) else { continue };
                waves
                    .entry(st.wave_file)
                    .or_insert_with(|| WaveBatch { texture: tex_of(&table.wave_textures, st.wave_file), vertices: Vec::new() })
                    .vertices
                    .extend_from_slice(&w.vertices(st));
            }
        }
        (surfaces.into_values().collect(), waves.into_values().collect())
    }

    /// Noms des textures utilisées (à téléverser).
    pub fn textures(&self) -> Vec<String> {
        let Some(t) = self.table.as_ref() else { return Vec::new() };
        t.water_textures.iter().chain(&t.wave_textures).filter_map(|n| texture_stem(n)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> WaterStatusT1 {
        WaterStatusT1 { wave_lifetime: 1000, wave_scale_x: 1.5, wave_scale_z: 1.5, wave_color: [60, 60, 60], ..Default::default() }
    }

    #[test]
    fn wave_cycle() {
        let wave = Wave { status: 0, cell_x: 0, cell_z: 0, half_width: 160, half_height: 80, start_time: -100, angle: 90.0, speed: 0.1, position: [0.0, 0.0, 0.0] };
        let st = status();
        let mut w = WaveState::new(wave, &st, 1);
        w.step(&st, 50);
        assert!(!w.visible(), "attente avant le départ");
        w.step(&st, 100);
        assert!(w.visible());
        // Angle 90° : vitesse (cos, −sin) = (0, −0,1) par ms.
        assert!(w.position.z < 0.0 && w.position.x.abs() < 1e-3, "{:?}", w.position);
        let (mut peak, mut grown): (f32, f32) = (0.0, 0.0);
        for _ in 0..55 {
            w.step(&st, 16);
            peak = peak.max(w.alpha);
            grown = grown.max(w.scale[0]);
        }
        assert_eq!(peak, 1.0);
        assert!(grown > 1.5, "{grown}");
        // Fin du cycle : retour au départ.
        for _ in 0..40 {
            w.step(&st, 16);
        }
        assert!(w.time < 1000);
        let v = w.vertices(&st);
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn stems() {
        assert_eq!(texture_stem("WATER_NOMAL_02.BMP").as_deref(), Some("water_nomal_02"));
        assert_eq!(texture_stem(" "), None);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : eau de la tuile 1717 et ses textures.
    #[test]
    fn game_water() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut files = HashMap::new();
        // Tuile 1717 = secteurs x 272..287, z 272..287 ; le secteur local (15, 0) a des vagues.
        let s = sector_water(&assets, &mut files, 287, 272).expect("eau du secteur (15, 0) de wt1717");
        assert!(!s.rects.is_empty() && !s.waves.is_empty());
        assert_eq!((s.rects[0].x, s.rects[0].width), (0, 16));
        let mut world = WaterWorld::default();
        let wanted: HashMap<(i32, i32), bool> = [((287, 272), true), ((284, 272), false)].into_iter().collect();
        world.set_sectors(&assets, &wanted);
        assert_eq!(world.loaded_count(), 2);
        world.update(5000);
        let (surfaces, _) = world.draw();
        assert!(surfaces.iter().map(|b| b.vertices.len()).sum::<usize>() >= 12);
        let t = world.textures();
        assert!(t.contains(&"water_nomal_01".to_string()), "{t:?}");
        for name in ["water_nomal_01", "water_nomal_02", "water_wave"] {
            assert!(assets.texture_found(name), "{name}");
        }
    }
}
