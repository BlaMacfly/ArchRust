//! Décor vivant d'Archonia : animations des objets du monde et lumières des repères du décor.
//!
//! * **Groupes d'un modèle d'objet** — `objecttemplate.ini` (lecteur AgcmObject `FUN_1406dfb80`) :
//!   le premier nombre de `DFF` (`%d:%s`), `ANIMATION` (`%d:%d:%d:%d:%s`) et `ANIM_SPEED` (`%d:%d:%f`)
//!   est l'**indice du groupe** (liste `+0x1f20`, `FUN_140891950`, créé au besoin par
//!   `FUN_1408918b0`) ; chaque groupe a son clump, son animation (`+0x28`, `FUN_14073d170`) et sa
//!   vitesse (`+0x30`). Tous les groupes sont dessinés (`FUN_1406e2f30`). Un seul modèle en a
//!   plusieurs : la roue à eau `수차` (TID 339, groupes 0 à 2, les groupes 1 et 2 animés).
//! * **Animation** — fichier `object\animation\<nom>.ean` (clés HAnim, comme les personnages ; les
//!   noms des données n'ont pas d'extension), posé sur la hiérarchie HAnim du clump. Rappel de mise
//!   à jour du clump (`FUN_1406ddff0`, inscrit par `FUN_1404c4090` pour chaque groupe rendu) :
//!   `RtAnimInterpolatorAddAnimTime(Δt_ms × vitesse × 0,001)` puis mise à jour des matrices de la
//!   hiérarchie (et de la peau). Le clip boucle sur sa propre durée. Δt = temps depuis la dernière
//!   mise à jour de **cet** objet (0 la première fois) : la phase de chaque objet part de son
//!   premier affichage ; ArchRust tient une horloge par modèle (objets d'un même modèle en phase).
//! * **Lumières du décor** — événement `Function=30` des objets placés (`obj%05d.ini`,
//!   `PointLightStart` … `PointLightEnd`), lu par AgcmEventPointLight (`FUN_14065e140`) : `Color`
//!   (entier, octet 0 = rouge, 1 = vert, 2 = bleu), `Radius`, `Flag`. Création (`FUN_14065dab0`) :
//!   lumière ponctuelle RenderWare (`RpLightCreate(0x80)`), couleur / 255, rayon `Radius`, posée à la
//!   position de l'objet ; `Flag & 0x1000000` : ajoutée à AgcmRender (éclaire terrain, objets,
//!   personnages) ; `& 0x2000000` : aussi à AgcmDynamicLightmap. Ni clignotement ni variation.
//!   **Horaire** : rappel 0 d'AgcmEventNature (`FUN_14065dd80`, ciel courant de `FUN_140581fd0`) :
//!   lumières allumées si le bit 0x10 de `WeatherEffect1` du ciel courant est nul, toutes éteintes
//!   sinon (`FUN_14065ddb0`). Le bit 0x10 marque les tranches de jour de quelques gabarits ; la
//!   plupart ne le portent jamais, leurs lumières restent allumées jour et nuit.
//!
//! Voir `docs/FORMATS.md`, « Décor animé et lumières du décor ».

use crate::animation::{load_clip_in, Clip};
use crate::assets::Assets;
use crate::render::{DrawItem, Mesh, PointLight, Renderer};
use archrust_data::rw::Clump;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Bits de `Flag` d'une lumière du décor (`FUN_14065dab0`).
pub const LIGHT_RENDER: u32 = 0x100_0000;
pub const LIGHT_DYNAMIC_LIGHTMAP: u32 = 0x200_0000;
/// Bit de `WeatherEffect1` qui éteint les lumières du décor (`FUN_14065dd80`).
pub const WEATHER_LIGHTS_OFF: i32 = 0x10;
/// Fonction d'événement d'ApmEventManager des lumières du décor (AgpmEventPointLight).
pub const POINT_LIGHT_FUNCTION: &str = "30";
/// Cadence de la déformation des peaux du décor (images par seconde au plus).
const SKIN_HZ: f32 = 30.0;

/// Les lumières du décor sont allumées pour ce `WeatherEffect1` du ciel courant.
pub fn lights_on(weather1: i32) -> bool {
    (weather1 as u32 >> 4) & 1 == 0
}

/// Groupe d'un modèle d'objet (`objecttemplate.ini`).
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateGroup {
    pub index: usize,
    pub dff: String,
    /// Fichier d'animation (`object/animation`), extension `.ean` ajoutée.
    pub animation: Option<String>,
    /// `ANIM_SPEED` (1 par défaut).
    pub speed: f32,
}

/// Groupes d'un modèle d'objet, dans l'ordre des indices. Vide si le modèle n'est pas dessiné
/// (voir `objects::template_model`).
pub fn template_groups(assets: &Assets, tid: u32) -> Vec<TemplateGroup> {
    if crate::objects::template_model(assets, tid).is_none() {
        return Vec::new();
    }
    let Ok(table) = assets.table("ini", "objecttemplate.ini") else { return Vec::new() };
    let Some(t) = table.section(&tid.to_string()) else { return Vec::new() };
    parse_groups(t.entries.iter().map(|(k, v)| (k.as_str(), v.as_str())))
}

/// Groupes à partir des colonnes positionnelles 10 (`DFF`), 26 (`ANIMATION`) et 27 (`ANIM_SPEED`).
pub fn parse_groups<'a>(entries: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<TemplateGroup> {
    let mut groups: Vec<TemplateGroup> = Vec::new();
    fn group(groups: &mut Vec<TemplateGroup>, index: usize) -> &mut TemplateGroup {
        if let Some(i) = groups.iter().position(|g| g.index == index) {
            return &mut groups[i];
        }
        groups.push(TemplateGroup { index, dff: String::new(), animation: None, speed: 1.0 });
        groups.last_mut().unwrap()
    }
    for (k, v) in entries {
        let f: Vec<&str> = v.split(':').map(str::trim).collect();
        let Some(index) = f.first().and_then(|i| i.parse::<usize>().ok()) else { continue };
        match k {
            "10" if f.len() >= 2 && !f[1].is_empty() => group(&mut groups, index).dff = f[1].to_ascii_lowercase(),
            "26" if f.len() >= 5 && !f[4].is_empty() => {
                let name = f[4].to_ascii_lowercase();
                group(&mut groups, index).animation = Some(if name.contains('.') { name } else { format!("{name}.ean") });
            }
            "27" if f.len() >= 3 => {
                if let Ok(s) = f[2].parse::<f32>() {
                    group(&mut groups, index).speed = s;
                }
            }
            _ => {}
        }
    }
    groups.retain(|g| !g.dff.is_empty());
    groups.sort_by_key(|g| g.index);
    groups
}

/// Pièce d'un groupe animé : maillage, cadre de son atomic, peau éventuelle.
struct Piece {
    mesh: Arc<Mesh>,
    frame: usize,
}

/// Groupe animé prêt : clump, clip, vitesse, inverse du cadre racine, pièces et pose courante.
struct AnimGroup {
    clump: Arc<Clump>,
    clip: Option<Arc<Clip>>,
    speed: f32,
    root_inverse: Mat4,
    pieces: Vec<Piece>,
    /// Repères des cadres (espace de l'objet, racine annulée) dans la pose courante.
    world: Vec<Mat4>,
    /// Repères de la pose du fichier (peaux : sommets déformés dans l'espace de leur cadre).
    bind: Vec<Mat4>,
}

/// Modèle d'objet animé : ses groupes et son horloge (secondes de clip).
pub struct AnimatedModel {
    groups: Vec<AnimGroup>,
    time: f32,
    since_skin: f32,
}

impl AnimatedModel {
    /// Modèle animé du TID (au moins un groupe avec un clip lisible et une hiérarchie), sinon `None`.
    fn load(renderer: &mut Renderer, assets: &Assets, tid: u32) -> Option<AnimatedModel> {
        let groups = template_groups(assets, tid);
        if !groups.iter().any(|g| g.animation.is_some()) {
            return None;
        }
        let mut out = Vec::new();
        for g in groups {
            let clump = match assets.clump("object", &g.dff) {
                Ok(c) => c,
                Err(e) => {
                    log::debug!("décor TID {tid} groupe {} ({}) : {e:#}", g.index, g.dff);
                    continue;
                }
            };
            let has_hierarchy = clump.frames.iter().any(|f| f.hanim.as_ref().is_some_and(|h| !h.nodes.is_empty()));
            let clip = g.animation.as_deref().filter(|_| has_hierarchy).and_then(|f| load_clip_in(assets, "object/animation", f));
            let root_inverse = clump.frames.first().map(|f| f.local_matrix().inverse()).unwrap_or(Mat4::IDENTITY);
            let mut pieces = Vec::new();
            for atomic in &clump.atomics {
                let Some(geom) = clump.geometries.get(atomic.geometry as usize) else { continue };
                if geom.vertices.is_empty() {
                    continue;
                }
                let mut mesh = crate::scene::build_geometry_mesh(renderer, assets, geom);
                if geom.skin.is_some() && has_hierarchy {
                    mesh.skin = Some(Arc::new(crate::animation::SkinBinding { clump: clump.clone(), geometry: atomic.geometry as usize, base: crate::scene::geometry_vertices(geom), wearer: None }));
                }
                pieces.push(Piece { mesh: Arc::new(mesh), frame: atomic.frame as usize });
            }
            let bind: Vec<Mat4> = clump.world_matrices().into_iter().map(|m| root_inverse * m).collect();
            out.push(AnimGroup { world: bind.clone(), bind, clump, clip, speed: g.speed, root_inverse, pieces });
        }
        out.iter().any(|g| g.clip.is_some()).then_some(AnimatedModel { groups: out, time: 0.0, since_skin: f32::MAX })
    }

    /// Avance l'horloge de `dt` secondes et recalcule la pose (cadres ; peaux à 30 Hz au plus).
    fn advance(&mut self, renderer: &Renderer, dt: f32) {
        self.time += dt;
        self.since_skin += dt;
        let skin_now = self.since_skin >= 1.0 / SKIN_HZ;
        if skin_now {
            self.since_skin = 0.0;
        }
        for g in &mut self.groups {
            let Some(clip) = &g.clip else { continue };
            let pose = clip.sample(self.time * g.speed);
            g.world = g.clump.world_matrices_with_pose(&pose).into_iter().map(|m| g.root_inverse * m).collect();
            if skin_now {
                for p in &g.pieces {
                    if let Some(binding) = &p.mesh.skin {
                        if let Some(v) = crate::animation::deform(binding, &pose) {
                            renderer.write_vertices(&p.mesh, &v);
                        }
                    }
                }
            }
        }
    }

    /// Maillages et repères (espace de l'objet) de la pose courante.
    pub fn pieces(&self) -> impl Iterator<Item = (&Arc<Mesh>, Mat4)> {
        self.groups.iter().flat_map(|g| {
            g.pieces.iter().map(move |p| {
                // Peau : sommets déjà dans l'espace de leur cadre (pose de bind de ce cadre).
                let m = if p.mesh.skin.is_some() { g.bind.get(p.frame) } else { g.world.get(p.frame) };
                (&p.mesh, m.copied().unwrap_or(Mat4::IDENTITY))
            })
        })
    }
}

/// Lumière d'un repère du décor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecorLight {
    pub position: Vec3,
    pub radius: f32,
    pub color: [u8; 3],
    pub flags: u32,
}

impl DecorLight {
    /// Lumière pour le moteur de rendu (couleur / 255).
    pub fn point(&self) -> PointLight {
        PointLight { position: self.position, radius: self.radius, color: self.color.map(|c| c as f32 / 255.0) }
    }
}

/// Lumières (événement `Function=30`) des objets d'une tuile `obj%05d.ini`.
pub fn tile_lights(assets: &Assets, tile: u32) -> Vec<DecorLight> {
    let Ok(table) = assets.table("ini", &format!("obj{tile:05}.ini")) else { return Vec::new() };
    table.sections.iter().flat_map(|s| section_lights(s.entries.iter().map(|(k, v)| (k.as_str(), v.as_str())))).collect()
}

/// Lumières d'une section d'objet placé : la position de l'objet (`Position=x,y,z`) et chaque bloc
/// `PointLightStart` … `PointLightEnd` d'une fonction 30 (`Color`, `Radius`, `Flag`).
pub fn section_lights<'a>(entries: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<DecorLight> {
    let mut position = None;
    let mut out = Vec::new();
    let mut function = String::new();
    let mut cur: Option<DecorLight> = None;
    for (k, v) in entries {
        let v = v.trim();
        if k.eq_ignore_ascii_case("Position") {
            let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if p.len() == 3 {
                position = Some(Vec3::new(p[0], p[1], p[2]));
            }
        } else if k.eq_ignore_ascii_case("Function") {
            function = v.to_string();
        } else if k.eq_ignore_ascii_case("PointLightStart") && function == POINT_LIGHT_FUNCTION {
            cur = Some(DecorLight { position: Vec3::ZERO, radius: 0.0, color: [0; 3], flags: 0 });
        } else if k.eq_ignore_ascii_case("PointLightEnd") {
            out.extend(cur.take());
        } else if let Some(l) = cur.as_mut() {
            if k.eq_ignore_ascii_case("Color") {
                let c = v.parse::<i64>().unwrap_or(0) as u32;
                l.color = [c as u8, (c >> 8) as u8, (c >> 16) as u8];
            } else if k.eq_ignore_ascii_case("Radius") {
                l.radius = v.parse().unwrap_or(0.0);
            } else if k.eq_ignore_ascii_case("Flag") {
                l.flags = v.parse::<i64>().unwrap_or(0) as u32;
            }
        }
    }
    let Some(position) = position else { return Vec::new() };
    out.into_iter().map(|l| DecorLight { position, ..l }).filter(|l| l.radius > 0.0).collect()
}

/// Décor vivant chargé autour du joueur (champ `decor` de `objects::WorldObjects`).
#[derive(Default)]
pub struct Decor {
    /// Modèles animés par TID (`None` : sans animation jouable).
    models: HashMap<u32, Option<Arc<std::sync::Mutex<AnimatedModel>>>>,
    /// Objets animés placés, par secteur.
    placed: HashMap<(i32, i32), Vec<(Arc<std::sync::Mutex<AnimatedModel>>, Mat4)>>,
    /// Lumières des tuiles lues, par secteur.
    lights: HashMap<(i32, i32), Vec<DecorLight>>,
    last: Option<std::time::Instant>,
}

impl Decor {
    /// Modèle animé du TID, chargé une fois.
    pub fn model(&mut self, renderer: &mut Renderer, assets: &Assets, tid: u32) -> Option<Arc<std::sync::Mutex<AnimatedModel>>> {
        if let Some(m) = self.models.get(&tid) {
            return m.clone();
        }
        let m = AnimatedModel::load(renderer, assets, tid).map(|m| Arc::new(std::sync::Mutex::new(m)));
        self.models.insert(tid, m.clone());
        m
    }

    pub fn set_sector(&mut self, key: (i32, i32), placed: Vec<(Arc<std::sync::Mutex<AnimatedModel>>, Mat4)>) {
        self.placed.insert(key, placed);
    }

    pub fn add_tile_lights(&mut self, lights: Vec<DecorLight>) {
        for l in lights {
            let key = crate::world::sector_of(l.position.x, l.position.z);
            self.lights.entry(key).or_default().push(l);
        }
    }

    pub fn retain_sectors(&mut self, keep: impl Fn(&(i32, i32)) -> bool) {
        self.placed.retain(|k, _| keep(k));
    }

    /// Avance les modèles animés qui ont des objets placés (une fois par modèle).
    pub fn animate(&mut self, renderer: &Renderer) {
        let now = std::time::Instant::now();
        let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32()).min(0.5);
        self.last = Some(now);
        let mut seen: Vec<*const std::sync::Mutex<AnimatedModel>> = Vec::new();
        for (m, _) in self.placed.values().flatten() {
            let p = Arc::as_ptr(m);
            if seen.contains(&p) {
                continue;
            }
            seen.push(p);
            m.lock().unwrap().advance(renderer, dt);
        }
    }

    /// Objets animés à dessiner à moins de `max_distance` de `eye`.
    pub fn draw_items(&self, eye: Vec3, max_distance: f32, out: &mut Vec<DrawItem>) {
        for (model, world) in self.placed.values().flatten() {
            if world.w_axis.truncate().distance(eye) > max_distance {
                continue;
            }
            let m = model.lock().unwrap();
            out.extend(m.pieces().map(|(mesh, local)| DrawItem { mesh: mesh.clone(), model: *world * local, tint: [1.0; 4], character: false }));
        }
    }

    /// Lumières du décor éclairant le rendu (`Flag & 0x1000000`) dans les secteurs à moins de
    /// `radius` secteurs de `center`, de la plus proche de `eye` à la plus lointaine.
    pub fn lights_near(&self, center: Vec3, radius: i32, eye: Vec3) -> Vec<PointLight> {
        let (cx, cy) = crate::world::sector_of(center.x, center.z);
        let mut out: Vec<&DecorLight> = Vec::new();
        for sy in cy - radius..=cy + radius {
            for sx in cx - radius..=cx + radius {
                if let Some(list) = self.lights.get(&(sx, sy)) {
                    out.extend(list.iter().filter(|l| l.flags & LIGHT_RENDER != 0));
                }
            }
        }
        // Lumières dont la sphère touche l'œil d'abord, puis par distance au bord de la sphère.
        out.sort_by(|a, b| (a.position.distance(eye) - a.radius).total_cmp(&(b.position.distance(eye) - b.radius)));
        out.into_iter().map(DecorLight::point).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groupes_de_la_roue_a_eau() {
        // TID 339 `수차` : trois groupes, les groupes 1 et 2 animés.
        let entries = [("10", "0:H0015300.ECL"), ("10", "1:H0015301.ECL"), ("26", "1:0:0:0:E0001531"), ("27", "1:0:1.000000"), ("10", "2:H0015302.ECL"), ("26", "2:0:0:0:E0001532"), ("27", "2:0:0.500000"), ("23", "7")];
        let g = parse_groups(entries.into_iter());
        assert_eq!(g.len(), 3);
        assert_eq!((g[0].dff.as_str(), g[0].animation.as_deref()), ("h0015300.ecl", None));
        assert_eq!(g[1].animation.as_deref(), Some("e0001531.ean"));
        assert_eq!(g[2].speed, 0.5);
    }

    #[test]
    fn lumiere_d_un_repere() {
        // `obj02024.ini` : Color=1606082 (0x1881C2 : R 0xC2, V 0x81, B 0x18), Radius, Flag 0x3FFFFFF.
        let entries = [("TID", "1034"), ("Position", "-1000,200,3000"), ("Function", "30"), ("EventID", "1073742018"), ("PointLightStart", "0"), ("ModuleData6", "AgcmEventPointLight"), ("Color", "1606082"), ("Radius", "937.609375"), ("Flag", "67108863"), ("EnumEnd", "0"), ("PointLightEnd", "0")];
        let l = section_lights(entries.into_iter());
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].color, [0xC2, 0x81, 0x18]);
        assert_eq!(l[0].position, Vec3::new(-1000.0, 200.0, 3000.0));
        assert!((l[0].radius - 937.609).abs() < 0.01);
        assert_eq!(l[0].flags & (LIGHT_RENDER | LIGHT_DYNAMIC_LIGHTMAP), LIGHT_RENDER | LIGHT_DYNAMIC_LIGHTMAP);
        // Une fonction 11 (ciel local) n'est pas une lumière.
        let other = [("Position", "0,0,0"), ("Function", "11"), ("PointLightStart", "0"), ("Radius", "10"), ("PointLightEnd", "0")];
        assert!(section_lights(other.into_iter()).is_empty());
    }

    #[test]
    fn horaire_des_lumieres() {
        assert!(lights_on(0));
        assert!(lights_on(10));
        assert!(lights_on(42));
        assert!(!lights_on(24));
        assert!(!lights_on(25));
        assert!(!lights_on(-1));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : modèles animés du décor (hors PNJ et
    /// repères) dont le clip `object/animation` se lit, et lumières des repères.
    #[test]
    fn decor_anime_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let table = assets.table("ini", "objecttemplate.ini").unwrap();
        let (mut animated, mut clips, mut skinned, mut rigid, mut moving) = (0, 0, 0, 0, 0);
        for s in &table.sections {
            let Ok(tid) = s.name.trim().parse::<u32>() else { continue };
            let groups = template_groups(&assets, tid);
            if !groups.iter().any(|g| g.animation.is_some()) {
                continue;
            }
            animated += 1;
            for g in groups.iter() {
                let Some(a) = &g.animation else { continue };
                let clip = load_clip_in(&assets, "object/animation", a);
                if clip.is_some() {
                    clips += 1;
                }
                if let Ok(c) = assets.clump("object", &g.dff) {
                    // La pose à mi-clip déplace au moins un cadre (clip apparié à la hiérarchie).
                    if let Some(clip) = &clip {
                        let (bind, posed) = (c.world_matrices(), c.world_matrices_with_pose(&clip.sample(clip.duration * 0.37)));
                        if bind.iter().zip(&posed).any(|(a, b)| !a.abs_diff_eq(*b, 1e-2)) {
                            moving += 1;
                        }
                    }
                    if c.geometries.iter().any(|g| g.skin.is_some()) {
                        skinned += 1;
                    } else {
                        rigid += 1;
                    }
                }
            }
        }
        eprintln!("décor animé : {animated} modèles, {clips} clips lus, {moving} qui bougent, {skinned} groupes à peau, {rigid} rigides");
        assert!(animated > 100, "{animated}");
        // Quelques clips (TID 1200-1204, 7736…) ne bougent pas les cadres du modèle.
        assert!(moving * 10 >= clips * 8, "{moving} poses animées pour {clips} clips");
        assert!(clips * 10 >= animated * 9, "{clips} clips pour {animated} modèles");
        let lights = tile_lights(&assets, 2024);
        eprintln!("obj02024 : {} lumières", lights.len());
        assert!(!lights.is_empty());
    }
}
