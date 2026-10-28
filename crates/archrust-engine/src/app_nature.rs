//! Heure du monde, ciel, météo et musique du gabarit côté client (AgpmTimer 0x11, AgpmEventNature
//! 0x30, AgpmNatureEffect 0x47 ; AgcmEventNature, AgcmNatureEffect, AgcmLensFlare d'Archonia).
//!
//! * **Heure** — `AGPMTIMER` règle [`GameClock`] (vitesse et temps de jeu) ; la tranche horaire
//!   vient de l'heure de jeu.
//! * **Gabarit** — région du joueur (`RegionTemplate.ini` `SkySet`, 13 si elle n'en a pas, comme la
//!   table codée en dur d'Archonia), ciel local des objets (`obj%05d.ini`, `Function=11`) des 3 × 3
//!   tuiles autour du joueur, gabarit météo (`NEFFECT_SKYSET`) pendant un effet ; composition et
//!   transitions de 25 000 ms de jeu dans [`crate::sky`].
//! * **Rendu** — dôme, nuages, horizon, lune/soleil et étoiles ([`crate::sky_render`]) ; lumière
//!   ambiante et directionnelle, brouillard (`FogDistance` → `FogFarClip`, ramenés à la distance de
//!   tracé comme le fait Archonia avec son option de distance), couleurs converties de l'espace du
//!   fichier (gamma) vers l'espace linéaire du rendu. Les CVars d'AddOn `skyColor`, `ambientColor`
//!   et `lightDirection` gardent la main quand elles sont posées.
//! * **Météo** — particules de `natureeffect.ini` autour de la caméra, fondu de sortie de 5 s.
//! * **Musique** — liste de lecture du gabarit ([`MusicPlaylist`]) : météo s'il a de la musique,
//!   sinon ciel local, sinon région.
//! * **Options** — `Sky` (effets météo), `Fog`, `LensFlare`, `Nature` (densité des particules) de
//!   [`crate::options::VideoOptions`].

use super::{App, Phase};
use crate::assets::Assets;
use crate::render::Vertex;
use crate::sky::{self, FlareSpot, LocalSky, MusicPlaylist, SkyKey, SkyState, SkyTemplates};
use crate::sky_render::{build_sky_draw, lin3, sun_direction, to_linear, FlareDraw};
use crate::weather::{NatureEffects, Particles, Weather};
use crate::zones::{hour_of_ms, sky_slot, Zones, SKY_SLOTS};
use archrust_netclient::nature::NatureEvent;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Gabarit de ciel d'une région sans `SkySet` (table codée en dur d'Archonia, `FUN_14057b9e0`).
const DEFAULT_SKY_TEMPLATE: i32 = 13;

/// Données du client lues une fois.
#[derive(Default)]
struct NatureData {
    templates: SkyTemplates,
    sky_textures: HashMap<i32, String>,
    flare_textures: HashMap<i32, String>,
    flare_spots: Vec<FlareSpot>,
    effects: NatureEffects,
}

/// État de la colle (champ `nature` d'[`App`]).
#[derive(Default)]
pub(super) struct NatureGlue {
    data: Option<Arc<NatureData>>,
    sky: SkyState,
    weather: Weather,
    particles: Particles,
    particle_vertices: Vec<Vertex>,
    playlist: MusicPlaylist,
    /// Ciels locaux par tuile `obj%05d.ini`.
    local_skies: HashMap<u32, Arc<Vec<LocalSky>>>,
    /// Gabarits retenus à la dernière image : région, ciel local, météo (−1 = aucun).
    templates_now: (i32, i32, i32),
    /// Textures déjà demandées au moteur de rendu (présentes ou introuvables).
    textures_tried: HashSet<String>,
    in_world: bool,
}

/// Brouillard ramené à la distance de tracé (`FUN_14057e3a0` : si la distance de l'option est plus
/// courte que `FogFarClip`, le début est mis à l'échelle et la fin bornée).
pub(super) fn fog_bounds(start: f32, far: f32, view: f32) -> (f32, f32) {
    if view > 0.0 && view < far {
        (start * view / far, view)
    } else {
        (start, far)
    }
}

impl NatureGlue {
    fn data(&mut self, assets: &Assets) -> Arc<NatureData> {
        self.data
            .get_or_insert_with(|| {
                let table = |f: &str| assets.table("ini", f).ok();
                let templates = table("skyset.ini").map(|t| SkyTemplates::parse(&t)).unwrap_or_default();
                let sky_textures = table("alefskysetting.ini").map(|t| sky::parse_sky_textures(&t)).unwrap_or_default();
                let (flare_textures, flare_spots) = table("LensFlare.ini").map(|t| sky::parse_lens_flare(&t)).unwrap_or_default();
                let effects = table("natureeffect.ini").map(|t| NatureEffects::parse(&t)).unwrap_or_default();
                log::info!(
                    "ciel : {} gabarits (skyset.ini), {} textures, {} reflets, {} effets de nature",
                    templates.templates.len(),
                    sky_textures.len(),
                    flare_spots.len(),
                    effects.effects.len()
                );
                Arc::new(NatureData { templates, sky_textures, flare_textures, flare_spots, effects })
            })
            .clone()
    }

    fn local_sky_at(&mut self, assets: &Assets, pos: Vec3) -> Option<i32> {
        let (sx, sy) = crate::world::sector_of(pos.x, pos.z);
        let mut tiles = Vec::with_capacity(9);
        for dx in [-16, 0, 16] {
            for dy in [-16, 0, 16] {
                let t = crate::world::tile_of(sx + dx, sy + dy);
                if !tiles.contains(&t) {
                    tiles.push(t);
                }
            }
        }
        let mut all = Vec::new();
        for t in tiles {
            let skies = self
                .local_skies
                .entry(t)
                .or_insert_with(|| {
                    let list = assets.table("ini", &format!("obj{t:05}.ini")).map(|tab| sky::parse_local_skies(&tab)).unwrap_or_default();
                    if !list.is_empty() {
                        log::debug!("ciel : {} ciel(s) local(aux) dans obj{t:05}.ini", list.len());
                    }
                    Arc::new(list)
                })
                .clone();
            all.extend(skies.iter().copied());
        }
        sky::pick_local_sky(&all, pos.x, pos.z)
    }
}

impl App {
    /// Paquets AgpmTimer, AgpmEventNature, AgpmNatureEffect.
    pub(super) fn nature_event(&mut self, ev: NatureEvent) {
        match ev {
            NatureEvent::Timer { speed, time_ms } => {
                self.game_clock.receive(speed, time_ms);
                let t = self.game_clock.now_ms();
                log::info!(
                    "heure du jeu : {t} ms (vitesse ×{}), soit {} h (tranche skyset « {} »)",
                    self.game_clock.speed(),
                    hour_of_ms(t),
                    SKY_SLOTS[sky_slot(hour_of_ms(t))]
                );
                // Musique et ambiance à revoir tout de suite avec l'heure du serveur.
                self.zone_check_at = None;
            }
            NatureEvent::Weather(w) => {
                log::info!("météo (AgpmEventNature) : {w}");
                self.nature.weather.on_weather(w);
            }
            NatureEvent::Effect(id) => {
                log::info!("effet de nature (AgpmNatureEffect) : {id}");
                self.nature.weather.on_effect(id);
            }
        }
    }

    /// `/ciel` : heure, vitesse, gabarits, tranche, météo. `/effetnature <id|-1>` : effet de nature
    /// local, comme `SetNatureEffect` des scripts d'Archonia (sans rien envoyer au serveur ; la météo
    /// MJ d'ArchCore passe par la discussion).
    pub(super) fn nature_slash(&mut self, cmd: &str, arg: &str) -> bool {
        match cmd {
            "/ciel" | "/sky" => {
                let t = self.game_clock.now_ms();
                let (region, local, weather) = self.nature.templates_now;
                let line = format!(
                    "[système] heure du jeu {:02}:{:02} (×{}, {}) ; tranche « {} » ; gabarit {region}, local {local}, météo {weather} ; effet {:?}{}",
                    hour_of_ms(t),
                    (t / 60_000) % 60,
                    self.game_clock.speed(),
                    if self.game_clock.is_set() { "serveur" } else { "défaut d'Archonia" },
                    SKY_SLOTS[sky_slot(hour_of_ms(t))],
                    self.nature.weather.effect,
                    if self.nature.weather.stopping { " (fin en fondu)" } else { "" }
                );
                self.ui.push_chat(line);
                true
            }
            "/effetnature" | "/natureeffect" => {
                match arg.trim().parse::<i32>() {
                    Ok(id) => {
                        self.nature.weather.on_effect(id);
                        self.ui.push_chat(format!("[système] effet de nature local : {id}"));
                    }
                    Err(_) => self.ui.push_chat("[système] /effetnature <id de natureeffect.ini | -1>".to_string()),
                }
                true
            }
            _ => false,
        }
    }

    /// Musique du gabarit pour `update_audio` : `None` = pas de liste (CVar `worldMusic`),
    /// `Some(None)` = silence entre deux morceaux, `Some(Some((morceau, tirage)))`.
    pub(super) fn nature_music(&mut self) -> Option<Option<(String, u32)>> {
        let data = self.nature.data(&self.assets);
        let (region, local, weather) = self.nature.templates_now;
        let has = |tid: i32| data.templates.get(tid).filter(|t| t.has_music());
        let sheltered = |tid: i32| data.templates.get(tid).is_some_and(|t| t.slots.iter().any(|s| s.sheltered()));
        let chosen = has(weather)
            .filter(|_| !sheltered(local) && !sheltered(region))
            .or_else(|| has(local))
            .or_else(|| data.templates.get(region));
        let list = chosen.map(|t| t.music()).unwrap_or_default();
        if list.is_empty() {
            self.nature.playlist.clear();
            return None;
        }
        let now = self.started.elapsed().as_secs_f64();
        self.nature.playlist.set_candidates(list, now);
        self.nature.playlist.update(self.audio.music_finished(), now);
        Some(self.nature.playlist.current().map(|(c, g)| (c.to_string(), g)))
    }

    /// Texture du ciel ou de la météo sur le GPU (chargée au premier besoin) ; `false` si elle
    /// manque au client.
    fn nature_texture(&mut self, name: &str) -> bool {
        let Some(g) = self.gpu.as_mut() else { return false };
        if g.renderer.texture(name).is_some() {
            return true;
        }
        let key = name.to_ascii_lowercase();
        if !self.nature.textures_tried.insert(key.clone()) {
            return false;
        }
        match self.assets.texture(name) {
            Ok(img) if img.data != crate::assets::placeholder_texture(&key).data => {
                g.renderer.upload_texture(&key, img.width, img.height, &img.data);
                true
            }
            _ => {
                log::warn!("ciel : texture {name} introuvable");
                false
            }
        }
    }

    /// Hors du monde : rendu par défaut, horloge et météo oubliées.
    fn leave_nature(&mut self) {
        self.nature.in_world = false;
        self.game_clock.clear();
        self.nature.sky.reset();
        self.nature.weather = Weather::default();
        self.nature.particles.ensure(None, Vec3::ZERO, 1.0);
        self.nature.playlist.clear();
        self.nature.templates_now = (-1, -1, -1);
        if let Some(g) = self.gpu.as_mut() {
            let r = &mut g.renderer;
            r.sky_draw = None;
            r.particles = None;
            r.flares.clear();
            r.fog_color = crate::render::DEFAULT_FOG_COLOR;
            r.fog_range = crate::render::DEFAULT_FOG_RANGE;
            r.light_color = [0.0; 4];
            r.ambient = [0.45, 0.45, 0.5, 1.0];
            r.clear_color = wgpu::Color { r: 0.35, g: 0.45, b: 0.6, a: 1.0 };
        }
    }

    /// À chaque image, après les CVars : ciel, lumières, brouillard, météo.
    pub(super) fn update_nature(&mut self, dt: f32) {
        if self.phase != Phase::InWorld {
            if self.nature.in_world {
                self.leave_nature();
            }
            return;
        }
        self.nature.in_world = true;
        let data = self.nature.data(&self.assets);
        let game_ms = self.game_clock.now_ms();
        let hour = hour_of_ms(game_ms);
        let slot = sky_slot(hour);
        let pos = self.player.as_ref().map(|p| p.position);
        // Gabarits : région, ciel local, météo.
        let region = self
            .zone_region
            .as_ref()
            .and_then(|z| Zones::sky_set(&self.assets, z.region))
            .map(|t| t as i32)
            .filter(|t| data.templates.get(*t).is_some())
            .unwrap_or(DEFAULT_SKY_TEMPLATE);
        let local = pos.and_then(|p| self.nature.local_sky_at(&self.assets, p)).filter(|t| data.templates.get(*t).is_some()).unwrap_or(-1);
        let weather = self.nature.weather.sky_template(&data.effects).filter(|t| data.templates.get(*t).is_some()).unwrap_or(-1);
        if self.nature.templates_now != (region, local, weather) {
            log::info!("ciel : gabarit {region} (région), local {local}, météo {weather}, {hour} h (tranche « {} »)", SKY_SLOTS[slot]);
            self.nature.templates_now = (region, local, weather);
        }
        let get = |tid: i32| data.templates.get(tid).map(|t| &t.slots[slot]);
        let target = sky::compose(get(region), get(local), get(weather));
        self.nature.sky.set_target(SkyKey { region, local, weather, slot }, &target, game_ms);
        self.nature.sky.advance(game_ms);
        let frame = self.nature.sky.frame(game_ms);
        // Météo : fondu en temps réel, particules autour de la caméra.
        self.nature.weather.update(dt * 1000.0);
        let eye = self.camera.eye();
        let effect = self.nature.weather.effect.filter(|_| self.options.video.sky);
        let info = effect.and_then(|id| data.effects.get(id));
        self.nature.particles.ensure(info, eye, self.options.video.nature_density());
        self.nature.particles.update(dt, eye);
        let particle_texture = effect.and_then(|id| data.effects.texture_of(id)).map(str::to_string);
        // Ciel de l'image (textures chargées au premier besoin).
        let light_dir = self.gpu.as_ref().map_or(Vec3::new(0.4, 0.8, 0.3), |g| g.renderer.light_dir);
        let cloud_offset = self.nature.sky.cloud_offset;
        let sky_draw = build_sky_draw(&frame, hour, game_ms, sun_direction(light_dir), cloud_offset, |i| {
            data.sky_textures.get(&i).cloned().filter(|n| self.nature_texture(n))
        });
        let particle_texture = particle_texture.filter(|t| self.nature_texture(t));
        let mut flares = Vec::new();
        if self.options.video.lens_flare {
            let intensity = frame.directional.iter().copied().fold(0.0f32, f32::max).clamp(0.0, 1.0);
            for spot in &data.flare_spots {
                let Some(name) = data.flare_textures.get(&spot.texture).cloned() else { continue };
                if self.nature_texture(&name) {
                    let c = spot.color.map(|v| v as f32 / 255.0);
                    flares.push(FlareDraw {
                        texture: name,
                        half: spot.size * 0.5,
                        line_pos: spot.line_pos,
                        color: [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3] * intensity],
                    });
                }
            }
        }
        // CVars d'AddOn prioritaires.
        let cvar_set = |n: &str| !self.addons.cvar_floats(n).is_empty();
        let (sky_cvar, ambient_cvar) = (cvar_set("skyColor"), cvar_set("ambientColor"));
        let view = self.object_draw_distance();
        let fog_on = self.options.video.fog;
        let alpha = self.nature.weather.fade;
        // Tampon de sommets repris au moteur de rendu (ou gardé ici) d'une image à l'autre.
        let mut vertices = self
            .gpu
            .as_mut()
            .and_then(|g| g.renderer.particles.take())
            .map(|(_, v)| v)
            .unwrap_or_else(|| std::mem::take(&mut self.nature.particle_vertices));
        self.nature.particles.vertices(eye, alpha, &mut vertices);
        let Some(g) = self.gpu.as_mut() else {
            self.nature.particle_vertices = vertices;
            return;
        };
        let r = &mut g.renderer;
        r.sky_draw = Some(sky_draw);
        if !sky_cvar {
            let b = lin3(frame.sky_bottom);
            r.clear_color = wgpu::Color { r: b[0] as f64, g: b[1] as f64, b: b[2] as f64, a: 1.0 };
        }
        if !ambient_cvar {
            let a = lin3(frame.ambient);
            r.ambient = [a[0], a[1], a[2], 1.0];
        }
        let d = lin3(frame.directional);
        r.light_color = [d[0], d[1], d[2], 1.0];
        let f = lin3(frame.fog_color);
        r.fog_color = [f[0], f[1], f[2], 1.0];
        let (start, end) = fog_bounds(frame.fog_distance, frame.fog_far, view);
        r.fog_range = [start, end.max(start + 1.0), if fog_on { 1.0 } else { 0.0 }, 0.0];
        r.flares = flares;
        r.particles = match particle_texture {
            Some(t) if !vertices.is_empty() => Some((t, vertices)),
            _ => {
                self.nature.particle_vertices = vertices;
                None
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brouillard_borne_par_la_distance_de_trace() {
        // FogDistance 18 400, FogFarClip 51 600 (gabarit 0, minuit).
        assert_eq!(fog_bounds(18_400.0, 51_600.0, 100_000.0), (18_400.0, 51_600.0));
        let (s, e) = fog_bounds(18_400.0, 51_600.0, 25_800.0);
        assert_eq!((s, e), (9_200.0, 25_800.0));
    }
}
