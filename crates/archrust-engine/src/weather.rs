//! Météo d'Archonia (AgcmNatureEffect, AgcmEventNature) : effets de `natureeffect.ini`, état de
//! l'effet en cours (démarrage, arrêt en fondu de 5 s) et particules de pluie et de neige.
//!
//! * **Données** — `ini/natureeffect.ini` (`FUN_14059e5f0`, `FUN_14059f4d0`) : `[Textures]`
//!   `TEXTURE_NAME=n:fichier` (0 `NE_Snow`, 1 `NE_Rain`, 2 `NE_Pollen`), puis `[NatureEffectInfo]`
//!   des blocs séparés par `====…;` : `NEFFECT_ID`, `TEXTURE_ID`, `NEFFECT_TYPE` (0 flocons qui
//!   oscillent, 1-3 traînées, ≥ 4 refusé), `SIZE/SPEED/SWING/DENSITY/RED/GREEN/BLUE/ALPHA _MIN/_MAX`,
//!   `NEFFECT_SKYSET` (gabarit de ciel imposé : 98 neige, 99 pluie).
//! * **Paquets** — AgpmEventNature 0 → fondu de sortie, 3 → pluie (effet 1), 5 → neige
//!   (effet 0) ; AgpmNatureEffect `id` → démarrage net, −1 → fondu de sortie (facteur
//!   0,0002 par ms, 5 s). Au démarrage le ciel passe sur `NEFFECT_SKYSET` ; à l'arrêt il est
//!   recalculé d'après la position.
//! * **Particules** (`FUN_14059ff50`) — grille de 5 × 5 cases autour de la caméra, nombre par case
//!   tiré dans DENSITY, couleurs tirées parmi 32 teintes. Les unités de taille, de vitesse et de
//!   densité ne sont pas relevées : les facteurs [`CELL_SIZE`], [`PER_CELL`], [`SIZE_UNIT`],
//!   [`FALL_UNIT`] et [`SWING_UNIT`] sont choisis pour un rendu plausible.

use crate::sky::Rng;
use archrust_data::ini::IniTable;
use glam::Vec3;
use std::collections::HashMap;

/// Fondu de sortie : part de l'opacité perdue par milliseconde réelle (5 s).
pub const FADE_PER_MS: f32 = 0.0002;
/// Côté de la grille de particules (5 × 5 cases).
pub const GRID: i32 = 5;
/// Côté d'une case de la grille, en unités du monde (cm).
pub const CELL_SIZE: f32 = 1_000.0;
/// Particules par case pour une densité de 1.
pub const PER_CELL: f32 = 100.0;
/// Hauteur de la boîte de particules au-dessus et au-dessous de la caméra.
pub const BOX_ABOVE: f32 = 2_500.0;
pub const BOX_BELOW: f32 = 1_000.0;
/// Taille d'un flocon (et longueur d'une goutte) par unité de `NEFFECT_SIZE`.
pub const SIZE_UNIT: f32 = 40.0;
/// Chute par unité de `NEFFECT_SPEED`, en unités par seconde : flocons et traînées.
pub const FALL_UNIT: [f32; 2] = [40.0, 250.0];
/// Amplitude du balancement par unité de `NEFFECT_SWING`.
pub const SWING_UNIT: f32 = 20.0;
/// Rendu limité aux identifiants < 30 (`FUN_14059ff50`).
pub const MAX_EFFECT_ID: i32 = 30;

/// Un effet de `natureeffect.ini`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NatureEffectInfo {
    pub id: i32,
    pub texture: i32,
    pub kind: i32,
    pub size: (f32, f32),
    pub speed: (f32, f32),
    pub swing: (f32, f32),
    pub density: (f32, f32),
    pub red: (f32, f32),
    pub green: (f32, f32),
    pub blue: (f32, f32),
    pub alpha: (f32, f32),
    pub skyset: i32,
}

/// `natureeffect.ini` lu.
#[derive(Debug, Clone, Default)]
pub struct NatureEffects {
    /// Indice → nom de texture sans extension.
    pub textures: HashMap<i32, String>,
    pub effects: Vec<NatureEffectInfo>,
}

impl NatureEffects {
    pub fn parse(table: &IniTable) -> Self {
        let mut out = NatureEffects::default();
        if let Some(s) = table.section("Textures") {
            for v in s.get_all("TEXTURE_NAME") {
                if let Some((n, f)) = v.split_once(':') {
                    if let Ok(n) = n.trim().parse() {
                        out.textures.insert(n, crate::sky::strip_ext(f));
                    }
                }
            }
        }
        let Some(s) = table.section("NatureEffectInfo") else { return out };
        let mut cur: Option<NatureEffectInfo> = None;
        for (k, v) in &s.entries {
            let key = k.trim().to_ascii_uppercase();
            if key.is_empty() {
                continue; // séparateur `====…;`
            }
            if key == "NEFFECT_ID" {
                if let Some(e) = cur.take() {
                    out.effects.push(e);
                }
                cur = Some(NatureEffectInfo { id: v.trim().parse().unwrap_or(-1), skyset: 0, ..Default::default() });
                continue;
            }
            let Some(e) = cur.as_mut() else { continue };
            let f: f32 = v.trim().trim_end_matches(';').parse().unwrap_or(0.0);
            let i = f as i32;
            match key.as_str() {
                "TEXTURE_ID" => e.texture = i,
                "NEFFECT_TYPE" => e.kind = i,
                "NEFFECT_SKYSET" => e.skyset = i,
                _ => {
                    let (name, max) = match key.rsplit_once('_') {
                        Some((n, "MIN")) => (n, false),
                        Some((n, "MAX")) => (n, true),
                        _ => continue,
                    };
                    let field = match name {
                        "NEFFECT_SIZE" => &mut e.size,
                        "NEFFECT_SPEED" => &mut e.speed,
                        "NEFFECT_SWING" => &mut e.swing,
                        "NEFFECT_DENSITY" => &mut e.density,
                        "NEFFECT_RED" => &mut e.red,
                        "NEFFECT_GREEN" => &mut e.green,
                        "NEFFECT_BLUE" => &mut e.blue,
                        "NEFFECT_ALPHA" => &mut e.alpha,
                        _ => continue,
                    };
                    if max {
                        field.1 = f;
                    } else {
                        field.0 = f;
                    }
                }
            }
        }
        if let Some(e) = cur {
            out.effects.push(e);
        }
        out
    }

    pub fn get(&self, id: i32) -> Option<&NatureEffectInfo> {
        self.effects.iter().find(|e| e.id == id)
    }

    /// Texture (nom sans extension) d'un effet.
    pub fn texture_of(&self, id: i32) -> Option<&str> {
        self.get(id).and_then(|e| self.textures.get(&e.texture)).map(String::as_str)
    }
}

/// Effet de nature en cours.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Weather {
    pub effect: Option<i32>,
    /// Opacité de l'effet (1 = plein, décroît pendant l'arrêt).
    pub fade: f32,
    pub stopping: bool,
}

impl Weather {
    /// Météo d'AgpmEventNature (`FUN_140579990`).
    pub fn on_weather(&mut self, weather: i8) {
        match weather {
            0 => self.stop(),
            3 => self.start(1),
            5 => self.start(0),
            _ => {}
        }
    }

    /// Effet d'AgpmNatureEffect (`FUN_14059e070`) : −1 = arrêt en fondu.
    pub fn on_effect(&mut self, id: i32) {
        if id < 0 {
            self.stop();
        } else {
            self.start(id);
        }
    }

    /// Effacement net puis démarrage.
    pub fn start(&mut self, id: i32) {
        *self = Weather { effect: Some(id), fade: 1.0, stopping: false };
    }

    pub fn stop(&mut self) {
        if self.effect.is_some() {
            self.stopping = true;
        }
    }

    /// Fondu de sortie, en millisecondes réelles.
    pub fn update(&mut self, dt_ms: f32) {
        if self.stopping {
            self.fade -= FADE_PER_MS * dt_ms.max(0.0);
            if self.fade <= 0.0 {
                *self = Weather::default();
            }
        }
    }

    /// Gabarit de ciel imposé (`NEFFECT_SKYSET`) : tant que l'effet dure et n'est pas en arrêt.
    /// Un gabarit 0 (effets de réserve 4 à 10) est traité comme « aucun ».
    pub fn sky_template(&self, effects: &NatureEffects) -> Option<i32> {
        let id = self.effect.filter(|_| !self.stopping)?;
        effects.get(id).map(|e| e.skyset).filter(|&s| s > 0)
    }
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    pos: Vec3,
    fall: f32,
    swing: f32,
    phase: f32,
    size: f32,
    color: [f32; 4],
}

/// Particules de l'effet en cours, autour de la caméra.
#[derive(Debug)]
pub struct Particles {
    effect: Option<i32>,
    kind: i32,
    items: Vec<Particle>,
    rng: Rng,
    time: f32,
}

impl Default for Particles {
    fn default() -> Self {
        Self { effect: None, kind: 0, items: Vec::new(), rng: Rng::seeded(), time: 0.0 }
    }
}

fn wrap(v: f32, center: f32, half: f32) -> f32 {
    center + (v - center + half).rem_euclid(2.0 * half) - half
}

impl Particles {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// (Re)crée les particules de l'effet `id` autour de `camera` ; `density` multiplie la densité
    /// (option `Nature`). `None` vide tout.
    pub fn ensure(&mut self, info: Option<&NatureEffectInfo>, camera: Vec3, density: f32) {
        let id = info.map(|i| i.id);
        if id == self.effect && (id.is_none() || !self.items.is_empty()) {
            return;
        }
        self.effect = id;
        self.items.clear();
        let Some(info) = info.filter(|i| i.id < MAX_EFFECT_ID && (0..4).contains(&i.kind)) else { return };
        self.kind = info.kind;
        let rng = &mut self.rng;
        // 32 teintes tirées une fois, comme Archonia.
        let palette: Vec<[f32; 4]> = (0..32)
            .map(|_| {
                [info.red, info.green, info.blue, info.alpha].map(|(a, b)| (rng.range(a, b) / 255.0).clamp(0.0, 1.0))
            })
            .collect();
        let half = GRID as f32 * CELL_SIZE * 0.5;
        let fall_unit = FALL_UNIT[(info.kind > 0) as usize];
        for cx in 0..GRID {
            for cz in 0..GRID {
                let n = (rng.range(info.density.0, info.density.1).abs() * PER_CELL * density).round() as usize;
                for _ in 0..n {
                    let pos = Vec3::new(
                        camera.x - half + (cx as f32 + rng.unit()) * CELL_SIZE,
                        camera.y - BOX_BELOW + rng.unit() * (BOX_ABOVE + BOX_BELOW),
                        camera.z - half + (cz as f32 + rng.unit()) * CELL_SIZE,
                    );
                    self.items.push(Particle {
                        pos,
                        fall: rng.range(info.speed.0, info.speed.1) * fall_unit,
                        swing: rng.range(info.swing.0, info.swing.1) * SWING_UNIT,
                        phase: rng.unit() * std::f32::consts::TAU,
                        size: rng.range(info.size.0, info.size.1) * SIZE_UNIT,
                        color: palette[rng.next_u32() as usize % palette.len()],
                    });
                }
            }
        }
    }

    /// Chute et balancement (`dt` en secondes réelles), particules ramenées dans la boîte qui suit
    /// la caméra.
    pub fn update(&mut self, dt: f32, camera: Vec3) {
        let dt = dt.clamp(0.0, 0.25);
        self.time += dt;
        let half = GRID as f32 * CELL_SIZE * 0.5;
        let height = BOX_ABOVE + BOX_BELOW;
        for p in &mut self.items {
            p.pos.y -= p.fall * dt;
            if self.kind == 0 {
                let w = 0.8 + p.phase * 0.1;
                p.pos.x += (self.time * w + p.phase).cos() * p.swing * w * dt;
                p.pos.z += (self.time * w * 0.7 + p.phase).sin() * p.swing * w * dt;
            }
            p.pos.x = wrap(p.pos.x, camera.x, half);
            p.pos.z = wrap(p.pos.z, camera.z, half);
            p.pos.y = wrap(p.pos.y, camera.y + (BOX_ABOVE - BOX_BELOW) * 0.5, height * 0.5);
        }
    }

    /// Quadrilatères (6 sommets chacun) face à la caméra : flocons carrés, gouttes étirées le long
    /// de la chute. `alpha` = fondu de l'effet.
    pub fn vertices(&self, camera: Vec3, alpha: f32, out: &mut Vec<crate::render::Vertex>) {
        out.clear();
        if alpha <= 0.0 {
            return;
        }
        for p in &self.items {
            let to_cam = (camera - p.pos).normalize_or_zero();
            let (right, up) = if self.kind == 0 {
                let right = Vec3::Y.cross(to_cam).normalize_or(Vec3::X);
                (right * p.size * 0.5, to_cam.cross(right).normalize_or(Vec3::Y) * p.size * 0.5)
            } else {
                // Traînée verticale : largeur fixe, longueur = taille.
                let flat = Vec3::new(to_cam.x, 0.0, to_cam.z);
                let right = Vec3::Y.cross(flat).normalize_or(Vec3::X);
                (right * 2.5, Vec3::Y * p.size * 0.5)
            };
            let color = [p.color[0], p.color[1], p.color[2], p.color[3] * alpha];
            let corner = |sx: f32, sy: f32, u: f32, v: f32| crate::render::Vertex {
                position: (p.pos + right * sx + up * sy).to_array(),
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
                color,
            };
            let (a, b, c, d) = (corner(-1.0, -1.0, 0.0, 1.0), corner(1.0, -1.0, 1.0, 1.0), corner(1.0, 1.0, 1.0, 0.0), corner(-1.0, 1.0, 0.0, 0.0));
            out.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NE: &str = "[Textures]\nTEXTURE_NAME=0:NE_Snow.tif\nTEXTURE_NAME=1:NE_Rain.tif\nTEXTURE_NAME=2:NE_Pollen.tif\n[NatureEffectInfo]\n\
NEFFECT_ID=0\nTEXTURE_ID=0\nNEFFECT_TYPE=0\nNEFFECT_SIZE_MIN=0.1\nNEFFECT_SIZE_MAX=0.4\nNEFFECT_SPEED_MIN=2.5\nNEFFECT_SPEED_MAX=3.5\nNEFFECT_SWING_MIN=0.1\nNEFFECT_SWING_MAX=5\nNEFFECT_DENSITY_MIN=0.3\nNEFFECT_DENSITY_MAX=0.6\n\
NEFFECT_RED_MIN=255\nNEFFECT_RED_MAX=255\nNEFFECT_GREEN_MIN=255\nNEFFECT_GREEN_MAX=255\nNEFFECT_BLUE_MIN=255\nNEFFECT_BLUE_MAX=255\nNEFFECT_ALPHA_MIN=100\nNEFFECT_ALPHA_MAX=255\nNEFFECT_SKYSET=98\n\
========================================;\nNEFFECT_ID=1\nTEXTURE_ID=1\nNEFFECT_TYPE=1\nNEFFECT_SIZE_MIN=3\nNEFFECT_SIZE_MAX=6\nNEFFECT_SPEED_MIN=2\nNEFFECT_SPEED_MAX=4\nNEFFECT_DENSITY_MIN=0.1\nNEFFECT_DENSITY_MAX=0.2\nNEFFECT_ALPHA_MIN=5\nNEFFECT_ALPHA_MAX=15\nNEFFECT_SKYSET=99\n\
========================================;\nNEFFECT_ID=4\nTEXTURE_ID=0\nNEFFECT_TYPE=0\nNEFFECT_SKYSET=0\n";

    #[test]
    fn natureeffect_lu() {
        let ne = NatureEffects::parse(&IniTable::parse(NE));
        assert_eq!(ne.effects.len(), 3);
        let snow = ne.get(0).unwrap();
        assert_eq!((snow.kind, snow.skyset, snow.size, snow.density), (0, 98, (0.1, 0.4), (0.3, 0.6)));
        assert_eq!(snow.alpha, (100.0, 255.0));
        let rain = ne.get(1).unwrap();
        assert_eq!((rain.kind, rain.texture, rain.skyset), (1, 1, 99));
        assert_eq!(ne.texture_of(1), Some("NE_Rain"));
        assert_eq!(ne.texture_of(0), Some("NE_Snow"));
    }

    #[test]
    fn meteo_demarrage_et_fondu_de_cinq_secondes() {
        let ne = NatureEffects::parse(&IniTable::parse(NE));
        let mut w = Weather::default();
        w.on_weather(3);
        assert_eq!(w.effect, Some(1));
        assert_eq!(w.sky_template(&ne), Some(99));
        w.on_weather(5);
        assert_eq!((w.effect, w.fade), (Some(0), 1.0));
        assert_eq!(w.sky_template(&ne), Some(98));
        // Valeurs sans effet visible.
        w.on_weather(1);
        w.on_weather(4);
        assert_eq!(w.effect, Some(0));
        // Arrêt : le ciel revient aussitôt, les particules s'effacent en 5 s.
        w.on_effect(-1);
        assert_eq!(w.sky_template(&ne), None);
        w.update(2_500.0);
        assert!((w.fade - 0.5).abs() < 1e-6);
        w.update(2_500.0);
        assert_eq!(w, Weather::default());
        // Effet de réserve à gabarit 0 : pas de changement de ciel.
        w.on_effect(4);
        assert_eq!(w.sky_template(&ne), None);
        w.on_weather(0);
        assert!(w.stopping);
    }

    #[test]
    fn particules_autour_de_la_camera() {
        let ne = NatureEffects::parse(&IniTable::parse(NE));
        let mut p = Particles { rng: Rng(12345), ..Default::default() };
        let cam = Vec3::new(10_000.0, 500.0, -20_000.0);
        p.ensure(ne.get(0), cam, 1.0);
        // 25 cases × 30 à 60 flocons.
        assert!((750..=1500).contains(&p.len()), "{}", p.len());
        let n = p.len();
        p.ensure(ne.get(0), cam, 1.0);
        assert_eq!(p.len(), n);
        let half = GRID as f32 * CELL_SIZE * 0.5;
        let cam2 = cam + Vec3::new(30_000.0, 0.0, 0.0);
        for _ in 0..40 {
            p.update(0.25, cam2);
        }
        assert!(p.items.iter().all(|q| (q.pos.x - cam2.x).abs() <= half + 1.0 && (q.pos.z - cam2.z).abs() <= half + 1.0));
        assert!(p.items.iter().all(|q| q.pos.y >= cam2.y - BOX_BELOW - 1.0 && q.pos.y <= cam2.y + BOX_ABOVE + 1.0));
        let mut v = Vec::new();
        p.vertices(cam2, 0.5, &mut v);
        assert_eq!(v.len(), p.len() * 6);
        assert!(v.iter().all(|x| x.color[3] <= 0.5 + 1e-6));
        // Pluie : moins dense ; option Nature réduite.
        p.ensure(ne.get(1), cam, 0.5);
        assert!((100..=300).contains(&p.len()), "{}", p.len());
        p.ensure(None, cam, 1.0);
        assert!(p.is_empty());
    }
}
