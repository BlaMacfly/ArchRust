//! Ciel d'Archonia (AgcmEventNature) : gabarits de `skyset.ini`, composition région / ciel local /
//! météo, transitions entre tranches horaires, nuages, étoiles, musique du gabarit.
//!
//! * **Gabarits** — `ini/skyset.ini`, une section par gabarit (`TID`, `NAME`), six tranches
//!   `00, 05, 07, 13, 19, 21` (0-4 h, 5-6 h, 7-12 h, 13-17 h, 18-20 h, 21-23 h, voir
//!   [`crate::zones::sky_slot`]) de 28 clés (lecteur `FUN_140582210`). Chaque clé présente met un
//!   bit du drapeau de la tranche ([`flag`]) : couleurs du ciel et lumières (1), brouillard (2),
//!   nuages / horizon / soleil (4), effets d'ambiance (8), musique (0x10), météo (0x20).
//! * **Composition** (`FUN_1405804b0`) — les six composantes sont copiées séparément : un ciel local
//!   (événement Nature des objets) ne remplace que les composantes que porte sa tranche, le reste
//!   vient de la région (`RegionTemplate.ini`, clé `SkySet`). Pendant un effet de nature, le gabarit
//!   météo (`NEFFECT_SKYSET`, 98 neige, 99 pluie) s'impose, sauf gabarit « abrité » (bit 0x20 de
//!   `WeatherEffect1`).
//! * **Transitions** (`FUN_14057ff20`) — à chaque changement de tranche ou de gabarit : 25 000 ms
//!   de **temps de jeu**, interpolation linéaire des couleurs et distances, fondu croisé des nuages,
//!   de la bande d'horizon et du soleil. Rien n'est interpolé à l'intérieur d'une tranche.
//! * **Nuages** — défilement = vitesse × Δ temps de jeu × 1,5 × k ; k n'est pas relevé (voir
//!   [`CLOUD_SCROLL`]).
//! * **Étoiles** (`FUN_14057e3a0`) — si `WeatherEffect1 & 2` et heure 0-5 ou 18-23 ; toutes les
//!   100 ms, 20 % des étoiles reçoivent un alpha de 0x91 à 0xFE.
//! * **Musique** — liste des `BackGroundMusic` distincts et non vides des six tranches, tirée au
//!   hasard, morceau suivant 10 s après la fin ([`MusicPlaylist`]) : elle ne suit pas l'heure.

use crate::zones::SKY_SLOTS;
use archrust_data::ini::{IniSection, IniTable};
use std::collections::HashMap;

/// Durée d'une transition de ciel, en millisecondes de temps de jeu.
pub const TRANSITION_MS: u64 = 25_000;
/// Défilement des nuages en unités de texture par (vitesse × ms de jeu) : `1,5 × k` d'Archonia, k
/// choisi pour qu'une couche de vitesse 0,5 traverse sa texture en ~4 min réelles à ×6.
pub const CLOUD_SCROLL: f32 = 1.5 * 1.0e-6;
/// Délai entre la fin d'un morceau et le suivant (`FUN_140580ab0`).
pub const MUSIC_GAP_SECS: f64 = 10.0;
/// Nouvel essai quand le gabarit n'a aucune musique.
pub const MUSIC_RETRY_SECS: f64 = 5.0;
/// Couches de ciel dessinées au plus (2 nuages + horizon + soleil, deux fois pendant un fondu).
pub const MAX_LAYERS: usize = 8;

/// Bits du drapeau d'une tranche (`u16` à +0 de la tranche).
pub mod flag {
    pub const COLORS: u16 = 0x01;
    pub const FOG: u16 = 0x02;
    pub const LAYERS: u16 = 0x04;
    pub const EFFECTS: u16 = 0x08;
    pub const MUSIC: u16 = 0x10;
    pub const WEATHER: u16 = 0x20;
}

/// Bits de `WeatherEffect1`.
pub mod weather_bits {
    /// Étoiles la nuit.
    pub const STARS: i32 = 0x02;
    /// Gabarit « abrité » : s'impose même pendant une météo (donjons).
    pub const SHELTERED: i32 = 0x20;
}

/// Couche texturée d'une tranche : indice `[Texture]` d'`alefskysetting.ini` (−1 ou négatif =
/// aucune), couleur RGBA et vitesse de défilement (nuages seulement).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerDef {
    pub texture: i32,
    pub color: [u8; 4],
    pub speed: f32,
}

impl Default for LayerDef {
    fn default() -> Self {
        Self { texture: -1, color: [255; 4], speed: 0.0 }
    }
}

/// Une tranche horaire d'un gabarit (0xF8 octets dans Archonia).
#[derive(Debug, Clone, PartialEq)]
pub struct SkySlot {
    pub flags: u16,
    pub sky_top: [f32; 3],
    pub sky_bottom: [f32; 3],
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_distance: f32,
    pub fog_far: f32,
    pub clouds: [LayerDef; 2],
    pub circumstance: LayerDef,
    pub sun: LayerDef,
    pub effect_interval: i32,
    /// `EffectIndex_NN` et `EffectRate_NN` (%).
    pub effects: [(i32, i32); 5],
    pub music: String,
    pub weather1: i32,
    pub weather2: i32,
}

impl Default for SkySlot {
    /// Ciel de jour neutre (les valeurs de `FUN_14057dd50` ne sont pas relevées) : sert quand la
    /// région n'a pas de gabarit et sous la scène de connexion.
    fn default() -> Self {
        Self {
            flags: 0,
            sky_top: [0.204, 0.482, 0.792],
            sky_bottom: [0.886, 0.973, 1.0],
            ambient: [0.45, 0.45, 0.5],
            directional: [0.55, 0.55, 0.5],
            fog_color: [0.33, 0.475, 0.667],
            fog_distance: 30_000.0,
            fog_far: 90_000.0,
            clouds: [LayerDef::default(); 2],
            circumstance: LayerDef::default(),
            sun: LayerDef::default(),
            effect_interval: 0,
            effects: [(0, 0); 5],
            music: String::new(),
            weather1: 0,
            weather2: -1,
        }
    }
}

fn floats3(v: &str) -> Option<[f32; 3]> {
    let f: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    (f.len() >= 3).then(|| [f[0], f[1], f[2]])
}

fn rgba(v: &str) -> Option<[u8; 4]> {
    let f: Vec<i64> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    (f.len() >= 4).then(|| [f[0], f[1], f[2], f[3]].map(|c| c.clamp(0, 255) as u8))
}

impl SkySlot {
    /// Tranche `prefix` (`00`, `05`…) d'une section de `skyset.ini`.
    pub fn parse(section: &IniSection, prefix: &str) -> Self {
        let mut s = SkySlot::default();
        let get = |k: &str| section.get(&format!("{prefix}_{k}")).map(str::trim);
        let int = |k: &str| get(k).and_then(|v| v.parse::<i64>().ok()).map(|v| v as i32);
        let float = |k: &str| get(k).and_then(|v| v.parse::<f32>().ok());
        macro_rules! set {
            ($bit:expr, $field:expr, $value:expr) => {
                if let Some(v) = $value {
                    $field = v;
                    s.flags |= $bit;
                }
            };
        }
        set!(flag::COLORS, s.sky_top, get("SkyTop").and_then(floats3));
        set!(flag::COLORS, s.sky_bottom, get("SkyBottom").and_then(floats3));
        set!(flag::COLORS, s.ambient, get("Ambient").and_then(floats3));
        set!(flag::COLORS, s.directional, get("Directional").and_then(floats3));
        set!(flag::FOG, s.fog_color, get("FogColor").and_then(floats3));
        set!(flag::FOG, s.fog_distance, float("FogDistance"));
        set!(flag::FOG, s.fog_far, float("FogFarClip"));
        for (i, n) in ["1", "2"].iter().enumerate() {
            set!(flag::LAYERS, s.clouds[i].texture, int(&format!("CloudTexture{n}")));
            set!(flag::LAYERS, s.clouds[i].color, get(&format!("CloudColor{n}")).and_then(rgba));
            set!(flag::LAYERS, s.clouds[i].speed, float(&format!("CloudSpeed{n}")));
        }
        set!(flag::LAYERS, s.circumstance.texture, int("Circumstance"));
        set!(flag::LAYERS, s.circumstance.color, get("CircumstanceColor").and_then(rgba));
        set!(flag::LAYERS, s.sun.texture, int("Sun"));
        set!(flag::LAYERS, s.sun.color, get("SunColor").and_then(rgba));
        set!(flag::EFFECTS, s.effect_interval, int("EffectInterval"));
        for n in 0..5 {
            set!(flag::EFFECTS, s.effects[n].0, int(&format!("EffectIndex_{:02}", n + 1)));
            set!(flag::EFFECTS, s.effects[n].1, int(&format!("EffectRate_{:02}", n + 1)));
        }
        set!(flag::MUSIC, s.music, get("BackGroundMusic").map(str::to_string));
        set!(flag::WEATHER, s.weather1, int("WeatherEffect1"));
        set!(flag::WEATHER, s.weather2, int("WeatherEffect2"));
        s
    }

    /// Copie dans `self` les composantes que porte `over` (bits de son drapeau).
    pub fn overlay(&mut self, over: &SkySlot) {
        if over.flags & flag::COLORS != 0 {
            self.sky_top = over.sky_top;
            self.sky_bottom = over.sky_bottom;
            self.ambient = over.ambient;
            self.directional = over.directional;
        }
        if over.flags & flag::FOG != 0 {
            self.fog_color = over.fog_color;
            self.fog_distance = over.fog_distance;
            self.fog_far = over.fog_far;
        }
        if over.flags & flag::LAYERS != 0 {
            self.clouds = over.clouds;
            self.circumstance = over.circumstance;
            self.sun = over.sun;
        }
        if over.flags & flag::EFFECTS != 0 {
            self.effect_interval = over.effect_interval;
            self.effects = over.effects;
        }
        if over.flags & flag::MUSIC != 0 {
            self.music = over.music.clone();
        }
        if over.flags & flag::WEATHER != 0 {
            self.weather1 = over.weather1;
            self.weather2 = over.weather2;
        }
        self.flags |= over.flags;
    }

    /// Gabarit « abrité » (bit 0x20 de `WeatherEffect1`).
    pub fn sheltered(&self) -> bool {
        self.weather1 >= 0 && self.weather1 & weather_bits::SHELTERED != 0
    }
}

/// Gabarit de ciel : six tranches horaires.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyTemplate {
    pub tid: i32,
    pub name: String,
    pub slots: [SkySlot; 6],
}

impl SkyTemplate {
    pub fn parse(section: &IniSection) -> Option<Self> {
        let tid = section.get_i64("TID").or_else(|| section.name.trim().parse().ok())? as i32;
        Some(Self {
            tid,
            name: section.get("NAME").unwrap_or_default().to_string(),
            slots: SKY_SLOTS.map(|p| SkySlot::parse(section, p)),
        })
    }

    /// Musiques distinctes et non vides des six tranches (`sound/bgm/…`, minuscules), dans l'ordre.
    pub fn music(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.slots {
            let m = s.music.trim();
            if m.is_empty() || m == "-" {
                continue;
            }
            let path = format!("sound/bgm/{}", m.to_ascii_lowercase());
            if !out.contains(&path) {
                out.push(path);
            }
        }
        out
    }

    /// La tranche porte-t-elle de la musique quelque part ?
    pub fn has_music(&self) -> bool {
        self.slots.iter().any(|s| s.flags & flag::MUSIC != 0 && !s.music.trim().is_empty())
    }
}

/// Tous les gabarits de `skyset.ini`.
#[derive(Debug, Default, Clone)]
pub struct SkyTemplates {
    pub templates: HashMap<i32, SkyTemplate>,
}

impl SkyTemplates {
    pub fn parse(table: &IniTable) -> Self {
        Self { templates: table.sections.iter().filter_map(SkyTemplate::parse).map(|t| (t.tid, t)).collect() }
    }

    pub fn get(&self, tid: i32) -> Option<&SkyTemplate> {
        self.templates.get(&tid)
    }
}

/// Textures du ciel (`ini/alefskysetting.ini`, `[Texture]` `NN=SKY000NN.tif`) : indice → nom de
/// texture sans extension.
pub fn parse_sky_textures(table: &IniTable) -> HashMap<i32, String> {
    let Some(s) = table.section("Texture") else { return HashMap::new() };
    s.entries
        .iter()
        .filter_map(|(k, v)| Some((k.trim().parse().ok()?, strip_ext(v))))
        .collect()
}

/// Nom de fichier sans extension (`SKY00002.tif` → `SKY00002`).
pub fn strip_ext(name: &str) -> String {
    let n = name.trim();
    n.rsplit_once('.').map_or(n, |(a, _)| a).to_string()
}

/// Reflet du soleil (`ini/LensFlare.ini`, `[Setting]` `SET=taille,position,r,g,b,a,texture`) :
/// position sur la droite soleil → centre de l'écran (1 = soleil, 0 = centre, −1 = opposé).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlareSpot {
    pub size: f32,
    pub line_pos: f32,
    pub color: [u8; 4],
    pub texture: i32,
}

/// Textures (`[Textures]` `FN=n:fichier`) et reflets de `LensFlare.ini`.
pub fn parse_lens_flare(table: &IniTable) -> (HashMap<i32, String>, Vec<FlareSpot>) {
    let textures = table
        .section("Textures")
        .map(|s| {
            s.get_all("FN")
                .filter_map(|v| {
                    let (n, f) = v.split_once(':')?;
                    Some((n.trim().parse().ok()?, strip_ext(f)))
                })
                .collect()
        })
        .unwrap_or_default();
    let spots = table
        .section("Setting")
        .map(|s| {
            s.get_all("SET")
                .filter_map(|v| {
                    let f: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (f.len() >= 7).then(|| FlareSpot {
                        size: f[0],
                        line_pos: f[1],
                        color: [f[2], f[3], f[4], f[5]].map(|c| c.clamp(0.0, 255.0) as u8),
                        texture: f[6] as i32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (textures, spots)
}

/// Tranche composée pour une position : région, ciel local par-dessus, météo sauf abri.
pub fn compose(region: Option<&SkySlot>, local: Option<&SkySlot>, weather: Option<&SkySlot>) -> SkySlot {
    let mut s = region.cloned().unwrap_or_default();
    if let Some(l) = local {
        s.overlay(l);
    }
    if let Some(w) = weather {
        if !s.sheltered() {
            s.overlay(w);
        }
    }
    s
}

/// Étoiles visibles : bit 2 de `WeatherEffect1` et heure 0-5 ou 18-23 (`FUN_14057e3a0`).
pub fn stars_visible(weather1: i32, hour: u32) -> bool {
    weather1 >= 0 && weather1 & weather_bits::STARS != 0 && (hour % 24 <= 5 || hour % 24 >= 18)
}

// ------------------------------------------------------------------------------------------------
// Ciels locaux (événements Nature des objets)

/// Ciel local d'un objet : `ApmEventManager` `Function=11`, condition sphère
/// (`CondAreaSphereRadius`), gabarit `SkyTemplateID`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSky {
    pub x: f32,
    pub z: f32,
    pub radius: f32,
    pub template: i32,
}

/// Fonction d'événement Nature d'ApmEventManager.
pub const NATURE_FUNCTION: i64 = 11;

/// Ciels locaux d'une table `obj%05d.ini`.
pub fn parse_local_skies(table: &IniTable) -> Vec<LocalSky> {
    let mut out = Vec::new();
    for s in &table.sections {
        let Some(pos) = s.get("Position").and_then(floats3) else { continue };
        // Les événements se suivent dans la section : `Function=` ouvre un bloc.
        let mut in_nature = false;
        let mut radius = None;
        let mut template = None;
        let mut flush = |radius: &mut Option<f32>, template: &mut Option<i32>| {
            if let (Some(r), Some(t)) = (radius.take(), template.take()) {
                if r > 0.0 && t >= 0 {
                    out.push(LocalSky { x: pos[0], z: pos[2], radius: r, template: t });
                }
            }
        };
        for (k, v) in &s.entries {
            if k.eq_ignore_ascii_case("Function") {
                if in_nature {
                    flush(&mut radius, &mut template);
                }
                in_nature = v.trim().parse::<i64>().ok() == Some(NATURE_FUNCTION);
                radius = None;
                template = None;
            } else if in_nature && k.eq_ignore_ascii_case("CondAreaSphereRadius") {
                radius = v.trim().parse().ok();
            } else if in_nature && k.eq_ignore_ascii_case("SkyTemplateID") {
                template = v.trim().parse().ok();
            }
        }
        if in_nature {
            flush(&mut radius, &mut template);
        }
    }
    out
}

/// Ciel local à la position (x, z) : la plus petite sphère (distance horizontale) qui la contient,
/// à égalité la plus proche. Archonia passe par une grille de secteurs pondérée (`FUN_140579a50`,
/// `FUN_14057d1e0`) dont le détail n'est pas relevé.
pub fn pick_local_sky(skies: &[LocalSky], x: f32, z: f32) -> Option<i32> {
    skies
        .iter()
        .filter_map(|s| {
            let d2 = (s.x - x).powi(2) + (s.z - z).powi(2);
            (d2 <= s.radius * s.radius).then_some((s.radius, d2, s.template))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, t)| t)
}

// ------------------------------------------------------------------------------------------------
// Image du ciel et transitions

/// Nature d'une couche texturée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// Couche de nuages 0 ou 1 (défilement propre).
    Cloud(u8),
    /// Bande d'horizon (`Circumstance`).
    Circumstance,
    Sun,
}

/// Couche à dessiner : texture d'`alefskysetting.ini`, couleur 0-1 (alpha × poids du fondu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameLayer {
    pub kind: LayerKind,
    pub texture: i32,
    pub color: [f32; 4],
}

/// État du ciel à un instant (couleurs dans l'espace du fichier, 0-1).
#[derive(Debug, Clone, PartialEq)]
pub struct SkyFrame {
    pub sky_top: [f32; 3],
    pub sky_bottom: [f32; 3],
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_distance: f32,
    pub fog_far: f32,
    pub layers: Vec<FrameLayer>,
    pub weather1: i32,
}

impl SkyFrame {
    pub fn from_slot(s: &SkySlot) -> Self {
        let mut layers = Vec::new();
        let mut push = |kind, d: &LayerDef| {
            if d.texture >= 0 && d.color[3] > 0 {
                layers.push(FrameLayer { kind, texture: d.texture, color: d.color.map(|c| c as f32 / 255.0) });
            }
        };
        push(LayerKind::Cloud(0), &s.clouds[0]);
        push(LayerKind::Cloud(1), &s.clouds[1]);
        push(LayerKind::Circumstance, &s.circumstance);
        push(LayerKind::Sun, &s.sun);
        Self {
            sky_top: s.sky_top,
            sky_bottom: s.sky_bottom,
            ambient: s.ambient,
            directional: s.directional,
            fog_color: s.fog_color,
            fog_distance: s.fog_distance,
            fog_far: s.fog_far,
            layers,
            weather1: s.weather1,
        }
    }

    /// Mélange linéaire `self` → `to` à `t` (0-1) ; couches en fondu croisé.
    pub fn blend(&self, to: &SkyFrame, t: f32) -> SkyFrame {
        let t = t.clamp(0.0, 1.0);
        let l3 = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        let mut layers: Vec<FrameLayer> = self
            .layers
            .iter()
            .map(|l| FrameLayer { color: [l.color[0], l.color[1], l.color[2], l.color[3] * (1.0 - t)], ..*l })
            .chain(to.layers.iter().map(|l| FrameLayer { color: [l.color[0], l.color[1], l.color[2], l.color[3] * t], ..*l }))
            .filter(|l| l.color[3] > 0.004)
            .collect();
        if layers.len() > MAX_LAYERS {
            layers.sort_by(|a, b| b.color[3].total_cmp(&a.color[3]));
            layers.truncate(MAX_LAYERS);
        }
        SkyFrame {
            sky_top: l3(self.sky_top, to.sky_top),
            sky_bottom: l3(self.sky_bottom, to.sky_bottom),
            ambient: l3(self.ambient, to.ambient),
            directional: l3(self.directional, to.directional),
            fog_color: l3(self.fog_color, to.fog_color),
            fog_distance: self.fog_distance + (to.fog_distance - self.fog_distance) * t,
            fog_far: self.fog_far + (to.fog_far - self.fog_far) * t,
            layers,
            weather1: if t < 1.0 { self.weather1 } else { to.weather1 },
        }
    }
}

/// Ce qui détermine la cible du ciel : gabarits (région, local, météo ; −1 = aucun) et tranche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyKey {
    pub region: i32,
    pub local: i32,
    pub weather: i32,
    pub slot: usize,
}

/// Ciel courant avec sa transition et le défilement des nuages.
#[derive(Debug, Default, Clone)]
pub struct SkyState {
    key: Option<SkyKey>,
    target: Option<SkyFrame>,
    cloud_speed: [f32; 2],
    from: Option<SkyFrame>,
    start_ms: u64,
    last_ms: Option<u64>,
    /// Décalage de texture des deux couches de nuages.
    pub cloud_offset: [f32; 2],
}

impl SkyState {
    pub fn key(&self) -> Option<SkyKey> {
        self.key
    }

    /// Nouvelle cible ; une cible différente de la précédente lance une transition de
    /// [`TRANSITION_MS`] depuis l'image courante (la toute première s'applique d'emblée).
    pub fn set_target(&mut self, key: SkyKey, slot: &SkySlot, game_ms: u64) {
        if self.key == Some(key) {
            return;
        }
        self.from = self.target.is_some().then(|| self.frame(game_ms));
        self.start_ms = game_ms;
        self.key = Some(key);
        self.target = Some(SkyFrame::from_slot(slot));
        self.cloud_speed = [slot.clouds[0].speed, slot.clouds[1].speed];
    }

    /// Oublie le ciel (sortie du monde) : la prochaine cible s'appliquera sans transition.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Avance des nuages selon le temps de jeu écoulé (temps reculé ou saut de plus d'une heure :
    /// ignoré, le serveur a renvoyé l'heure).
    pub fn advance(&mut self, game_ms: u64) {
        if let Some(last) = self.last_ms {
            let dt = game_ms.wrapping_sub(last);
            if game_ms >= last && dt <= 3_600_000 {
                for i in 0..2 {
                    self.cloud_offset[i] = (self.cloud_offset[i] + self.cloud_speed[i] * dt as f32 * CLOUD_SCROLL).rem_euclid(1.0);
                }
            }
        }
        self.last_ms = Some(game_ms);
    }

    /// Avancement de la transition (1 = terminée ; temps reculé : terminée).
    pub fn progress(&self, game_ms: u64) -> f32 {
        if self.from.is_none() || game_ms < self.start_ms {
            return 1.0;
        }
        ((game_ms - self.start_ms) as f64 / TRANSITION_MS as f64).min(1.0) as f32
    }

    /// Image du ciel à `game_ms`.
    pub fn frame(&self, game_ms: u64) -> SkyFrame {
        let target = self.target.clone().unwrap_or_else(|| SkyFrame::from_slot(&SkySlot::default()));
        match &self.from {
            Some(from) => {
                let t = self.progress(game_ms);
                if t >= 1.0 {
                    target
                } else {
                    from.blend(&target, t)
                }
            }
            None => target,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Musique

/// Petit générateur pseudo-aléatoire (xorshift32) pour les tirages du ciel et de la météo.
#[derive(Debug, Clone, Copy)]
pub struct Rng(pub u32);

impl Rng {
    pub fn seeded() -> Self {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() ^ d.as_secs() as u32).unwrap_or(1);
        Self(t | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Réel dans [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Réel entre `a` et `b` (dans un sens ou dans l'autre).
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
}

/// Musique de fond d'Archonia : morceau tiré au hasard dans la liste du gabarit, joué une fois,
/// le suivant 10 s après la fin ; liste vide : nouvel essai 5 s plus tard.
#[derive(Debug, Clone)]
pub struct MusicPlaylist {
    candidates: Vec<String>,
    current: Option<String>,
    /// Numéro du tirage : un même morceau retiré doit être relancé.
    generation: u32,
    next_at: Option<f64>,
    rng: Rng,
}

impl Default for MusicPlaylist {
    fn default() -> Self {
        Self { candidates: Vec::new(), current: None, generation: 0, next_at: None, rng: Rng::seeded() }
    }
}

impl MusicPlaylist {
    pub fn with_seed(seed: u32) -> Self {
        Self { rng: Rng(seed | 1), ..Self::default() }
    }

    fn pick(&mut self, now: f64) {
        self.generation = self.generation.wrapping_add(1);
        if self.candidates.is_empty() {
            self.current = None;
            self.next_at = Some(now + MUSIC_RETRY_SECS);
        } else {
            let i = self.rng.next_u32() as usize % self.candidates.len();
            self.current = Some(self.candidates[i].clone());
            self.next_at = None;
        }
    }

    /// Liste du gabarit courant ; une liste différente relance un tirage aussitôt.
    pub fn set_candidates(&mut self, list: Vec<String>, now: f64) {
        if list != self.candidates || (self.current.is_none() && self.next_at.is_none()) {
            self.candidates = list;
            self.pick(now);
        }
    }

    /// À chaque image : `finished` = le morceau courant est terminé (ou illisible).
    pub fn update(&mut self, finished: bool, now: f64) {
        match self.next_at {
            Some(t) if now >= t => self.pick(now),
            Some(_) => {}
            None if finished && self.current.is_some() => self.next_at = Some(now + MUSIC_GAP_SECS),
            None => {}
        }
    }

    /// Morceau à jouer (`None` pendant le silence entre deux morceaux) et numéro du tirage.
    pub fn current(&self) -> Option<(&str, u32)> {
        if self.next_at.is_some() {
            return None;
        }
        self.current.as_deref().map(|c| (c, self.generation))
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    pub fn clear(&mut self) {
        self.candidates.clear();
        self.current = None;
        self.next_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY: &str = "[0]\nModuleData1=AgpmEventNature\nTID=0\nNAME=test\nModuleData2=AgcmEventNature\n\
00_SkyTop=0.1,0.2,0.3\n00_SkyBottom=0.2,0.2,0.3\n00_Ambient=0.1,0.1,0.1\n00_Directional=0.2,0.2,0.2\n\
00_CloudTexture1=2\n00_CloudSpeed1=0.5\n00_CloudColor1=43,56,65,74\n00_CloudTexture2=10\n00_CloudSpeed2=0.125\n00_CloudColor2=67,86,100,52\n\
00_Circumstance=0\n00_CircumstanceColor=33,49,65,255\n00_Sun=1\n00_SunColor=255,215,160,255\n\
00_FogColor=0.1,0.1,0.2\n00_FogDistance=18400\n00_FogFarClip=51600\n00_BackGroundMusic=HFT_001.mp3\n00_WeatherEffect1=10\n00_WeatherEffect2=-1\n\
05_BackGroundMusic=HFT_006.mp3\n07_SkyTop=0.5,0.5,1\n07_Sun=-16777216\n07_SunColor=255,255,255,0\n07_BackGroundMusic=hft_001.MP3\n13_BackGroundMusic=\n19_BackGroundMusic=-\nEnumEnd=0\n\
[42]\nTID=42\n00_FogColor=1,0,0\n00_FogDistance=100\n00_FogFarClip=200\n00_WeatherEffect1=42\n\
[99]\nTID=99\n00_SkyTop=0,0,0\n00_SkyBottom=0,0,0\n00_Ambient=0,0,0\n00_Directional=0,0,0\n00_WeatherEffect1=4\n";

    fn templates() -> SkyTemplates {
        SkyTemplates::parse(&IniTable::parse(SKY))
    }

    #[test]
    fn gabarit_lu_avec_ses_drapeaux() {
        let t = templates();
        let s = &t.get(0).unwrap().slots[0];
        assert_eq!(s.flags, flag::COLORS | flag::FOG | flag::LAYERS | flag::MUSIC | flag::WEATHER);
        assert_eq!(s.sky_top, [0.1, 0.2, 0.3]);
        assert_eq!(s.clouds[0], LayerDef { texture: 2, color: [43, 56, 65, 74], speed: 0.5 });
        assert_eq!(s.clouds[1].texture, 10);
        assert_eq!(s.sun.texture, 1);
        assert_eq!((s.fog_distance, s.fog_far), (18400.0, 51600.0));
        assert_eq!(s.weather1, 10);
        // Tranche 07 : soleil négatif = aucun ; seul le bit des couleurs et des couches.
        let s7 = &t.get(0).unwrap().slots[2];
        assert!(s7.sun.texture < 0);
        assert_eq!(s7.flags & flag::FOG, 0);
        assert!(SkyFrame::from_slot(s7).layers.iter().all(|l| l.kind != LayerKind::Sun));
        let f = SkyFrame::from_slot(s);
        assert_eq!(f.layers.len(), 4);
    }

    #[test]
    fn musique_des_six_tranches_sans_doublon() {
        let t = templates();
        assert_eq!(t.get(0).unwrap().music(), vec!["sound/bgm/hft_001.mp3".to_string(), "sound/bgm/hft_006.mp3".to_string()]);
        assert!(t.get(42).unwrap().music().is_empty());
        assert!(!t.get(42).unwrap().has_music());
    }

    #[test]
    fn composition_par_composante_et_abri() {
        let t = templates();
        let region = &t.get(0).unwrap().slots[0];
        let local = &t.get(42).unwrap().slots[0];
        let rain = &t.get(99).unwrap().slots[0];
        // Le ciel local ne remplace que le brouillard et la météo.
        let s = compose(Some(region), Some(local), None);
        assert_eq!(s.fog_color, [1.0, 0.0, 0.0]);
        assert_eq!(s.sky_top, region.sky_top);
        assert_eq!(s.clouds, region.clouds);
        // Pluie : couleurs du gabarit météo…
        let s = compose(Some(region), None, Some(rain));
        assert_eq!(s.sky_top, [0.0, 0.0, 0.0]);
        assert_eq!(s.fog_color, region.fog_color);
        // … sauf sous un gabarit abrité (42 = 0x20 | 8 | 2).
        let s = compose(Some(region), Some(local), Some(rain));
        assert_eq!(s.sky_top, region.sky_top);
        // Sans gabarit : ciel par défaut.
        assert_eq!(compose(None, None, None), SkySlot::default());
    }

    #[test]
    fn transition_lineaire_de_25_secondes_de_jeu() {
        let t = templates();
        let a = &t.get(0).unwrap().slots[0];
        let b = &t.get(0).unwrap().slots[2];
        let mut st = SkyState::default();
        let k = |slot| SkyKey { region: 0, local: -1, weather: -1, slot };
        st.set_target(k(0), a, 1_000);
        assert_eq!(st.progress(1_000), 1.0);
        assert_eq!(st.frame(1_000).sky_top, a.sky_top);
        st.set_target(k(2), b, 10_000);
        // Même cible : pas de nouvelle transition.
        st.set_target(k(2), b, 20_000);
        assert_eq!(st.progress(10_000), 0.0);
        let mid = st.frame(22_500);
        assert!((st.progress(22_500) - 0.5).abs() < 1e-6);
        assert!((mid.sky_top[0] - 0.3).abs() < 1e-6);
        assert!((mid.sky_top[2] - 0.65).abs() < 1e-6);
        // Fondu croisé : nuages de a à moitié, soleil de a à moitié, b sans soleil.
        let sun: Vec<_> = mid.layers.iter().filter(|l| l.kind == LayerKind::Sun).collect();
        assert_eq!(sun.len(), 1);
        assert!((sun[0].color[3] - 0.5).abs() < 1e-6);
        assert_eq!(st.frame(35_000).sky_top, b.sky_top);
        assert_eq!(st.frame(99_000).sky_top, b.sky_top);
        // Temps reculé (nouvel AGPMTIMER) : transition considérée terminée.
        assert_eq!(st.progress(5_000), 1.0);
        // Transition interrompue : repart de l'image courante.
        st.set_target(k(0), a, 50_000);
        st.set_target(k(2), b, 62_500);
        let f = st.frame(62_500);
        assert!((f.sky_top[2] - 0.65).abs() < 1e-6);
        assert!(f.layers.len() <= MAX_LAYERS);
    }

    #[test]
    fn nuages_au_temps_de_jeu() {
        let t = templates();
        let mut st = SkyState::default();
        st.set_target(SkyKey { region: 0, local: -1, weather: -1, slot: 0 }, &t.get(0).unwrap().slots[0], 0);
        st.advance(0);
        st.advance(100_000);
        assert!((st.cloud_offset[0] - 0.5 * 100_000.0 * CLOUD_SCROLL).abs() < 1e-6);
        assert!((st.cloud_offset[1] - 0.125 * 100_000.0 * CLOUD_SCROLL).abs() < 1e-6);
        let before = st.cloud_offset;
        st.advance(50_000); // temps reculé
        st.advance(50_000 + 2 * 3_600_000); // saut
        assert_eq!(st.cloud_offset, before);
    }

    #[test]
    fn etoiles_la_nuit_si_bit_2() {
        assert!(stars_visible(10, 0));
        assert!(stars_visible(10, 5));
        assert!(!stars_visible(10, 6));
        assert!(!stars_visible(10, 17));
        assert!(stars_visible(26, 18));
        assert!(stars_visible(26, 23));
        assert!(!stars_visible(24, 23));
        assert!(!stars_visible(-1, 23));
    }

    #[test]
    fn ciels_locaux_des_objets() {
        let t = IniTable::parse(
            "[1]\nModuleData1=ApmObject\nTID=468\nPosition=-791600.0,943.7,-644200.0\nModuleData5=ApmEventManager\nFunction=11\nEventID=1\nCondType=2\nCondAreaSphereRadius=51200\nCondEnd=0\nNatureStart=0\nSkyTemplateID=92\nNatureEnd=0\n\
[2]\nTID=1\nPosition=0,0,0\nFunction=8\nCondAreaSphereRadius=100\nSkyTemplateID=3\nFunction=11\nCondAreaSphereRadius=1000\nSkyTemplateID=7\n\
[3]\nTID=2\nPosition=100,0,0\nFunction=11\nCondAreaSphereRadius=300\nSkyTemplateID=8\n",
        );
        let skies = parse_local_skies(&t);
        assert_eq!(skies.len(), 3);
        assert_eq!(skies[0], LocalSky { x: -791_600.0, z: -644_200.0, radius: 51_200.0, template: 92 });
        assert_eq!(skies[1].template, 7);
        assert_eq!(pick_local_sky(&skies, -791_600.0 + 51_000.0, -644_200.0), Some(92));
        assert_eq!(pick_local_sky(&skies, -791_600.0 + 52_000.0, -644_200.0), None);
        // Deux sphères : la plus petite l'emporte.
        assert_eq!(pick_local_sky(&skies, 150.0, 0.0), Some(8));
        assert_eq!(pick_local_sky(&skies, -500.0, 0.0), Some(7));
    }

    #[test]
    fn textures_du_ciel_et_reflets() {
        let t = IniTable::parse("[Texture]\n01=SKY00001.tif\n10=SKY00010.tif\n00=SKY00000.tif\n");
        let m = parse_sky_textures(&t);
        assert_eq!(m.get(&10).map(String::as_str), Some("SKY00010"));
        assert_eq!(m.len(), 3);
        let t = IniTable::parse("[Textures]\nFN=0:LF_Glow.tif\n;FN=9:LF_Sec4.tif\nFN=7:LF_Sec6.tif\n[Setting]\nNumber=2\nSET=0.3,1.0,250,250,250,120,0\nSET=0.43,-0.3,250,250,250,50,7\n");
        let (tex, spots) = parse_lens_flare(&t);
        assert_eq!(tex.len(), 2);
        assert_eq!(tex.get(&7).map(String::as_str), Some("LF_Sec6"));
        assert_eq!(spots.len(), 2);
        assert_eq!(spots[1], FlareSpot { size: 0.43, line_pos: -0.3, color: [250, 250, 250, 50], texture: 7 });
    }

    #[test]
    fn musique_tiree_puis_suivante_dix_secondes_apres_la_fin() {
        let mut p = MusicPlaylist::with_seed(7);
        let list = vec!["sound/bgm/a.mp3".to_string(), "sound/bgm/b.mp3".to_string()];
        p.set_candidates(list.clone(), 0.0);
        let (first, g1) = p.current().map(|(c, g)| (c.to_string(), g)).unwrap();
        assert!(list.contains(&first));
        // Même liste : rien ne change.
        p.set_candidates(list.clone(), 1.0);
        assert_eq!(p.current().unwrap().1, g1);
        p.update(false, 100.0);
        assert_eq!(p.current().unwrap().1, g1);
        // Fin du morceau : silence 10 s puis nouveau tirage.
        p.update(true, 200.0);
        assert!(p.current().is_none());
        p.update(true, 209.0);
        assert!(p.current().is_none());
        p.update(true, 210.0);
        let (_, g2) = p.current().unwrap();
        assert_ne!(g1, g2);
        // Liste vide : rien, nouvel essai.
        p.set_candidates(Vec::new(), 300.0);
        assert!(p.current().is_none());
        p.set_candidates(list.clone(), 301.0);
        assert!(p.current().is_some());
        // Les deux morceaux finissent par sortir.
        let mut seen = std::collections::HashSet::new();
        for i in 0..50 {
            let now = 1000.0 + i as f64 * 20.0;
            p.update(true, now);
            p.update(true, now + 10.0);
            if let Some((c, _)) = p.current() {
                seen.insert(c.to_string());
            }
        }
        assert_eq!(seen.len(), 2);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : 159 gabarits, le 13 d'Anchorville avec
    /// ses étoiles la nuit, les gabarits météo 98 et 99 ; textures du ciel lisibles. Avec
    /// `ARCHRUST_SKY_DUMP=<dossier>`, les textures sont écrites en PNG.
    #[test]
    fn gabarits_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = crate::assets::Assets::new(Some(dir.into()), &[]);
        let t = SkyTemplates::parse(&assets.table("ini", "skyset.ini").unwrap());
        assert!(t.templates.len() >= 150, "{} gabarits", t.templates.len());
        let anchor = t.get(13).unwrap();
        assert!(stars_visible(anchor.slots[0].weather1, 2) || stars_visible(anchor.slots[5].weather1, 22));
        assert!(t.get(98).is_some() && t.get(99).is_some());
        let tex = parse_sky_textures(&assets.table("ini", "alefskysetting.ini").unwrap());
        assert!(tex.len() >= 7);
        let (lf, spots) = parse_lens_flare(&assets.table("ini", "LensFlare.ini").unwrap());
        assert_eq!(spots.len(), 14);
        let dump = std::env::var_os("ARCHRUST_SKY_DUMP").map(std::path::PathBuf::from);
        let names: Vec<String> = tex.values().cloned().chain(lf.values().cloned()).chain(["NE_Snow", "NE_Rain", "NE_Pollen"].map(String::from)).collect();
        for name in names {
            let img = assets.texture(&name).unwrap();
            let placeholder = crate::assets::placeholder_texture(&name.to_ascii_lowercase());
            let alpha_min = img.data.chunks_exact(4).map(|p| p[3]).min().unwrap_or(0);
            println!("{name} : {}×{} alpha min {alpha_min}{}", img.width, img.height, if img.data == placeholder.data { " (SUBSTITUT)" } else { "" });
            if let Some(d) = &dump {
                let _ = image::save_buffer(d.join(format!("{name}.png")), &img.data, img.width, img.height, image::ColorType::Rgba8);
            }
        }
    }
}
