//! Effets visuels et sonores d'Archonia (`AgcmEff2`) : jeux d'effets de `effect/ini/new/*.eff`
//! (décodés par `archrust_data::effset`), joués sur un personnage ou en un point.
//!
//! Déclencheurs (tables du client, comme `AgcmEventEffect`) :
//! * montée de niveau : lignes `LV_UP` d'`ini/ee_common_char.txt` (jeux 3610 et 3611) ;
//! * compétence apprise : ligne `LEARN_SK` (jeu 3614, avec son son `EA0044.WAV`) ;
//! * lancement d'une compétence (AGPMSKILL op 7) : le nom de la compétence (`SKILLTEMPLATE.ini`) donne
//!   son rang dans les `UsableSkillTName<i>` du lanceur (`CharacterTemplatePublic.ini`), puis les
//!   `SKILL_EFFECT_DATA` de `charactertemplateskill.ini` de ce rang (`rang:équipement:n°:EID:x:y:z:
//!   échelle:nœud:délai`), pour l'ensemble d'animation de l'arme portée (sinon le premier) ;
//! * coup porté (0x41 réussi) : effets de condition 0x100 du modèle de la cible
//!   (`charactertemplateeventeffect.ini`, « Defualt(Exp)_mob »…) ; mort : condition 0x200
//!   (« mob_dead2 »…). Le sens de ces deux drapeaux est **déduit des noms des jeux** ;
//! * commande de développement `/effet <EID> [cid]`.
//!
//! Rendu (voir `docs/FORMATS.md`, « Effets ») : planches (`AgcdEffBoard`, carré de ±7,064 unités,
//! FUN_1408d2e20, face à la caméra si le drapeau 1 de la base est mis — déduit des données), planches au
//! sol, objets (`effect/clump/K*.ECL`), sons ; animations couleur, échelle, rotation (vitesse en
//! degrés par seconde, cumulée comme FUN_1408f4020), déplacement linéaire, coordonnées de texture
//! (image par paliers, FUN_1408f45b0). Mélanges : `sky_render::EffectBlend`. Les systèmes de
//! particules, traînées, lumières et cadres mobiles ne sont pas encore dessinés.
//!
//! Options GFX (`OP_Effect`, `OP_TurnOffAllEffects`) : niveau 0 = aucun effet ; l'interface (montée de
//! niveau, apprentissage) demande `EffectUI` ≥ 1 ; les compétences des autres joueurs se masquent.

use super::App;
use crate::assets::Assets;
use crate::render::{EffectDraw, Vertex};
use crate::sky_render::EffectBlend;
use archrust_data::effset::{anim_kind, base_kind, AnimKeys, EffAnim, EffBase, EffectSetData};
use archrust_data::rw::Clump;
use archrust_netclient::skills::{learn_result, SkillEvent};
use archrust_netclient::WorldEvent;
use archrust_protocol::schema::action_status;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Demi-côté de la planche d'Archonia (0x40e20c4a, FUN_1408d2e20).
const BOARD_HALF: f32 = 7.064;
/// Durée d'un jeu sans durée propre ni base bornée.
const DEFAULT_LIFE_MS: u32 = 3000;
/// Au plus autant d'effets joués à la fois (les plus anciens partent).
const MAX_ACTIVE: usize = 64;
/// Drapeaux de condition d'`AgcmEventEffect` (déduits des données, voir l'en-tête).
const COND_STRUCK: u32 = 0x100;
const COND_DEAD: u32 = 0x200;

/// Effet placé : jeu, décalage dans le repère du personnage, échelle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub eid: u32,
    pub offset: Vec3,
    pub scale: f32,
}

/// Lit `EID:x:y:z:échelle[:…]` (le début d'une donnée d'effet après ses indices).
fn placed_from(fields: &[&str]) -> Option<Placed> {
    let num = |i: usize| fields.get(i).and_then(|v| v.trim().parse::<f32>().ok());
    let eid = fields.first()?.trim().parse::<u32>().ok().filter(|e| *e > 0)?;
    Some(Placed { eid, offset: Vec3::new(num(1).unwrap_or(0.0), num(2).unwrap_or(0.0), num(3).unwrap_or(0.0)), scale: num(4).filter(|s| *s > 0.0).unwrap_or(1.0) })
}

/// Catégorie d'un effet, pour les niveaux d'`OP_Effect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Ui,
    Combat,
    Skill,
}

#[derive(Debug, Clone, Copy)]
enum Anchor {
    Character(i32),
}

struct Instance {
    set: Arc<EffectSetData>,
    born: Instant,
    anchor: Anchor,
    /// Dernière position connue (le personnage peut sortir de la vue).
    last: (Vec3, f32),
    offset: Vec3,
    scale: f32,
    sounded: Vec<bool>,
    life: u32,
}

#[derive(Default)]
pub struct EffectsGlue {
    active: Vec<Instance>,
    sets: HashMap<u32, Option<Arc<EffectSetData>>>,
    clumps: HashMap<String, Option<Arc<Clump>>>,
    common: Option<HashMap<String, Vec<Placed>>>,
    skill_cache: HashMap<(i32, i32, u32), Vec<Placed>>,
    event_cache: HashMap<(i32, u32), Vec<Placed>>,
}

/// `ee_common_char.txt` : NAME → effets (EID, OFFSET_X/Y/Z, SCALE).
fn load_common(assets: &Assets) -> HashMap<String, Vec<Placed>> {
    let text = super::npc_glue::client_text_in(assets, "ini", "ee_common_char.txt");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (Some(name), Some(eid)) = (col("NAME"), col("EID")) else { return HashMap::new() };
    let (x, y, z, s) = (col("OFFSET_X"), col("OFFSET_Y"), col("OFFSET_Z"), col("SCALE"));
    let mut out: HashMap<String, Vec<Placed>> = HashMap::new();
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let get = |i: Option<usize>| i.and_then(|i| c.get(i)).copied().unwrap_or("");
        if let Some(p) = placed_from(&[get(Some(eid)), get(x), get(y), get(z), get(s)]) {
            out.entry(get(Some(name)).trim().to_ascii_uppercase()).or_default().push(p);
        }
    }
    out
}

/// Colonne nommée d'une table positionnelle.
fn column(table: &archrust_data::IniTable, name: &str) -> Option<String> {
    table.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(name)).map(|(k, _)| k.clone())
}

/// Effets de la compétence `skill_tid` lancée par le modèle `char_tid` avec l'ensemble d'animation
/// `equip` (voir l'en-tête du module).
pub fn skill_effects(assets: &Assets, char_tid: i32, skill_tid: i32, equip: u32) -> Vec<Placed> {
    let name = (|| {
        let t = assets.table("ini", "SKILLTEMPLATE.ini").ok()?;
        let c = column(&t, "Name")?;
        t.section(&skill_tid.to_string())?.get(&c).map(str::to_owned)
    })();
    let Some(name) = name else { return Vec::new() };
    let rank = (|| {
        let t = assets.table("ini", "CharacterTemplatePublic.ini").ok()?;
        let header = t.section("")?;
        let sec = t.section(&char_tid.to_string())?;
        header.entries.iter().filter_map(|(k, v)| Some((v.strip_prefix("UsableSkillTName")?.parse::<u32>().ok()?, k))).find_map(|(i, k)| sec.get(k).filter(|s| s.trim().eq_ignore_ascii_case(name.trim())).map(|_| i))
    })();
    let Some(rank) = rank else { return Vec::new() };
    let Ok(t) = assets.table("ini", "charactertemplateskill.ini") else { return Vec::new() };
    let (Some(header), Some(sec)) = (t.section(""), t.section(&char_tid.to_string())) else { return Vec::new() };
    let data_cols: std::collections::HashSet<&str> = header.entries.iter().filter(|(_, v)| v.starts_with("SKILL_EFFECT_DATA")).map(|(k, _)| k.as_str()).collect();
    // (équipement, effet) pour ce rang.
    let mut by_equip: Vec<(u32, Placed)> = Vec::new();
    for (k, v) in &sec.entries {
        if !data_cols.contains(k.as_str()) {
            continue;
        }
        let f: Vec<&str> = v.split(':').collect();
        if f.first().and_then(|r| r.trim().parse::<u32>().ok()) != Some(rank) {
            continue;
        }
        let Some(e) = f.get(1).and_then(|e| e.trim().parse::<u32>().ok()) else { continue };
        if let Some(p) = f.get(3..).and_then(placed_from) {
            by_equip.push((e, p));
        }
    }
    let chosen = if by_equip.iter().any(|(e, _)| *e == equip) { equip } else { by_equip.iter().map(|(e, _)| *e).min().unwrap_or(0) };
    by_equip.into_iter().filter(|(e, _)| *e == chosen).map(|(_, p)| p).collect()
}

/// Effets d'un modèle de personnage (`charactertemplateeventeffect.ini`) dont la condition porte `flag`.
pub fn event_effects(assets: &Assets, char_tid: i32, flag: u32) -> Vec<Placed> {
    let Ok(t) = assets.table("ini", "charactertemplateeventeffect.ini") else { return Vec::new() };
    let (Some(header), Some(sec)) = (t.section(""), t.section(&char_tid.to_string())) else { return Vec::new() };
    let name_of = |k: &str| header.get(k).unwrap_or("");
    let mut conds: HashMap<String, u32> = HashMap::new();
    for (k, v) in &sec.entries {
        if name_of(k).starts_with("AGCMEVENTEFFECT_CONDITION_FLAGS") {
            if let Some((i, f)) = v.split_once(':') {
                conds.insert(i.trim().to_string(), f.split(':').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0));
            }
        }
    }
    sec.entries
        .iter()
        .filter(|(k, _)| name_of(k).starts_with("AGCMEVENTEFFECT_EFFECT_DATA"))
        .filter_map(|(_, v)| {
            let f: Vec<&str> = v.split(':').collect();
            let c = conds.get(f.first()?.trim())?;
            (c & flag != 0).then(|| placed_from(&f[1..])).flatten()
        })
        .collect()
}

/// Valeur d'une animation à l'instant `t` (ms depuis le début de la base).
fn anim_time(a: &EffAnim, t: u32) -> u32 {
    match (a.life, a.loop_opt) {
        (0, _) => t,
        (life, 2) => t % life,
        (life, _) => t.min(life),
    }
}

fn lerp_keys<T: Copy>(keys: &[(u32, T)], t: u32, mix: impl Fn(T, T, f32) -> T) -> Option<T> {
    let first = keys.first()?;
    if t <= first.0 {
        return Some(first.1);
    }
    for w in keys.windows(2) {
        let ((t0, a), (t1, b)) = (w[0], w[1]);
        if t <= t1 {
            let f = if t1 > t0 { (t - t0) as f32 / (t1 - t0) as f32 } else { 1.0 };
            return Some(mix(a, b, f));
        }
    }
    keys.last().map(|k| k.1)
}

/// État animé d'une base.
struct State {
    color: [f32; 4],
    scale: Vec3,
    uv: [f32; 4],
    offset: Vec3,
    /// Rotations cumulées (axe, degrés).
    spins: Vec<(Vec3, f32)>,
}

fn base_state(base: &EffBase, t: u32) -> State {
    let mut s = State { color: [1.0; 4], scale: Vec3::ONE, uv: [0.0, 0.0, 1.0, 1.0], offset: Vec3::ZERO, spins: Vec::new() };
    for a in &base.anims {
        let ta = anim_time(a, t);
        match (&a.keys, a.kind) {
            (AnimKeys::Color(k), _) => {
                if let Some(c) = lerp_keys(k, ta, |a, b, f| std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * f).round() as u8)) {
                    s.color = c.map(|v| v as f32 / 255.0);
                }
            }
            (AnimKeys::Vec3(k), anim_kind::SCALE) => {
                if let Some(v) = lerp_keys(k, ta, |a, b, f| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)) {
                    s.scale = Vec3::from(v);
                }
            }
            (AnimKeys::Vec3(k), anim_kind::LINEAR) => {
                if let Some(v) = lerp_keys(k, ta, |a, b, f| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)) {
                    s.offset = Vec3::from(v);
                }
            }
            // Image par paliers : la dernière clé atteinte.
            (AnimKeys::TuTv(k), _) => {
                if let Some(v) = k.iter().rev().find(|(kt, _)| *kt <= ta).or(k.first()) {
                    s.uv = v.1;
                }
            }
            // Vitesse angulaire (degrés par seconde), cumulée.
            (AnimKeys::Rotation { axis, keys }, _) => {
                if let Some(speed) = lerp_keys(keys, ta, |a, b, f| a + (b - a) * f) {
                    let axis = Vec3::from(*axis);
                    if axis.length_squared() > 1e-6 {
                        s.spins.push((axis.normalize(), speed * t as f32 / 1000.0));
                    }
                }
            }
            _ => {}
        }
    }
    s
}

/// Matrice locale d'une base : position, rotation de la base (x, puis y, puis z, en degrés),
/// rotations animées, échelle.
fn base_matrix(base: &EffBase, st: &State) -> Mat4 {
    let [rx, ry, rz] = base.rotation;
    let mut m = Mat4::from_translation(Vec3::from(base.position) + st.offset)
        * Mat4::from_rotation_z(rz.to_radians())
        * Mat4::from_rotation_y(ry.to_radians())
        * Mat4::from_rotation_x(rx.to_radians());
    for (axis, deg) in &st.spins {
        m *= Mat4::from_axis_angle(*axis, deg.to_radians());
    }
    m * Mat4::from_scale(st.scale)
}

/// Durée d'un jeu : la sienne, sinon la fin de la dernière base bornée.
fn set_life(set: &EffectSetData) -> u32 {
    if set.life > 0 {
        return set.life;
    }
    set.bases.iter().filter(|b| b.life > 0).map(|b| b.delay + b.life).max().unwrap_or(DEFAULT_LIFE_MS).min(20_000)
}

fn quad(out: &mut Vec<Vertex>, corners: [Vec3; 4], uv: [f32; 4], color: [f32; 4]) {
    // Sommets de FUN_1408d2e20 : (−,+) (u0,v0), (−,−) (u0,v1), (+,+) (u1,v0), (+,−) (u1,v1).
    let uvs = [[uv[0], uv[1]], [uv[0], uv[3]], [uv[2], uv[1]], [uv[2], uv[3]]];
    for i in [0usize, 1, 2, 2, 1, 3] {
        out.push(Vertex { position: corners[i].to_array(), normal: [0.0, 1.0, 0.0], uv: uvs[i], color });
    }
}

impl EffectsGlue {
    fn set(&mut self, assets: &Assets, eid: u32) -> Option<Arc<EffectSetData>> {
        self.sets.entry(eid).or_insert_with(|| assets.effect_sets().decode(eid).map(Arc::new)).clone()
    }

    fn clump(&mut self, assets: &Assets, file: &str) -> Option<Arc<Clump>> {
        self.clumps
            .entry(file.to_ascii_lowercase())
            .or_insert_with(|| match assets.clump("effect/clump", file) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::debug!("effet : modèle {file} : {e:#}");
                    None
                }
            })
            .clone()
    }
}

impl App {
    fn effects_class_on(&self, class: Class) -> bool {
        let (_, combat, ui, skill) = self.options.video.effect_levels();
        match class {
            Class::Ui => ui > 0,
            Class::Combat => combat > 0,
            Class::Skill => skill > 0,
        }
    }

    /// Position et orientation d'un personnage en vue.
    fn effects_anchor_of(&self, cid: i32) -> Option<(Vec3, f32)> {
        self.player.as_ref().filter(|p| p.cid == cid).or_else(|| self.entities.iter().find(|e| e.cid == cid)).map(|e| (e.position, e.turn_y))
    }

    fn effects_tid_of(&self, cid: i32) -> Option<i32> {
        self.player.as_ref().filter(|p| p.cid == cid).or_else(|| self.entities.iter().find(|e| e.cid == cid)).map(|e| e.tid)
    }

    /// Joue les effets `placed` sur le personnage `cid`.
    fn effects_play_on(&mut self, cid: i32, placed: &[Placed], class: Class) {
        if !self.effects_class_on(class) {
            return;
        }
        let Some(at) = self.effects_anchor_of(cid) else { return };
        for p in placed {
            self.effects_spawn(*p, Anchor::Character(cid), at);
        }
    }

    fn effects_spawn(&mut self, p: Placed, anchor: Anchor, at: (Vec3, f32)) {
        let assets = self.assets.clone();
        let Some(set) = self.effects.set(&assets, p.eid) else {
            log::debug!("effet {} : jeu absent ou non décodé", p.eid);
            return;
        };
        log::debug!("effet {} « {} » : {} base(s)", p.eid, set.name, set.bases.len());
        if self.effects.active.len() >= MAX_ACTIVE {
            self.effects.active.remove(0);
        }
        let life = set_life(&set);
        self.effects.active.push(Instance { sounded: vec![false; set.bases.len()], set, born: Instant::now(), anchor, last: at, offset: p.offset, scale: p.scale, life });
    }

    /// Effets d'un ensemble nommé d'`ee_common_char.txt` (`LV_UP`, `LEARN_SK`…).
    fn effects_common(&mut self, cid: i32, name: &str, class: Class) {
        let assets = self.assets.clone();
        let list = self.effects.common.get_or_insert_with(|| load_common(&assets)).get(name).cloned().unwrap_or_default();
        self.effects_play_on(cid, &list, class);
    }

    fn effects_event(&mut self, cid: i32, flag: u32) {
        let Some(tid) = self.effects_tid_of(cid) else { return };
        let assets = self.assets.clone();
        let list = self.effects.event_cache.entry((tid, flag)).or_insert_with(|| event_effects(&assets, tid, flag)).clone();
        self.effects_play_on(cid, &list, Class::Combat);
    }

    /// Relevé des événements qui déclenchent un effet (avant leur traitement).
    pub(super) fn effects_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            WorldEvent::Connected | WorldEvent::Disconnected(_) => self.effects.active.clear(),
            WorldEvent::LevelUp { cid } => self.effects_common(*cid, "LV_UP", Class::Ui),
            WorldEvent::Skill(SkillEvent::Learned { result, .. }) if *result == learn_result::SUCCESS => self.effects_common(me, "LEARN_SK", Class::Ui),
            WorldEvent::Skill(SkillEvent::Cast { caster, tid, .. }) => {
                let other_player = *caster != me && self.entities.iter().any(|e| e.cid == *caster && e.is_player);
                if other_player && self.options.video.hide_other_effects {
                    return;
                }
                let assets = self.assets.clone();
                let Some(char_tid) = self.effects_tid_of(*caster) else { return };
                let equipment = self.player.as_ref().filter(|p| p.cid == *caster).or_else(|| self.entities.iter().find(|e| e.cid == *caster)).map(|e| e.equipment.clone()).unwrap_or_default();
                let equip = crate::animation::weapon_anim_equip(crate::scene::equipped_weapon_type(&assets, &equipment));
                let list = self.effects.skill_cache.entry((char_tid, *tid, equip)).or_insert_with(|| skill_effects(&assets, char_tid, *tid, equip)).clone();
                self.effects_play_on(*caster, &list, Class::Skill);
            }
            WorldEvent::CharacterAction { target_cid, result, .. } => {
                if matches!(result, Some(archrust_netclient::combat::outcome::SUCCESS | archrust_netclient::combat::outcome::CRITICAL)) {
                    self.effects_event(*target_cid, COND_STRUCK);
                }
            }
            WorldEvent::CharacterUpdated { cid, action_status: Some(s), .. } if *s == action_status::DEAD => self.effects_event(*cid, COND_DEAD),
            _ => {}
        }
    }

    /// `/effet <EID> [cid]` : joue un jeu d'effets (essais).
    pub(super) fn effects_slash(&mut self, arg: &str) -> String {
        let mut it = arg.split_whitespace();
        let Some(eid) = it.next().and_then(|v| v.parse::<u32>().ok()) else { return "Usage : /effet <EID> [cid]".into() };
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let cid = it.next().and_then(|v| v.parse().ok()).unwrap_or(me);
        let Some(at) = self.effects_anchor_of(cid) else { return format!("Personnage {cid} hors de vue.") };
        let assets = self.assets.clone();
        match self.effects.set(&assets, eid) {
            Some(set) => {
                let msg = format!("Effet {eid} « {} » : {} base(s), {} ms.", set.name, set.bases.len(), set_life(&set));
                self.effects_spawn(Placed { eid, offset: Vec3::ZERO, scale: 1.0 }, Anchor::Character(cid), at);
                msg
            }
            None => format!("Effet {eid} introuvable."),
        }
    }

    /// Chaque image : sons des bases atteintes, triangles des effets pour le moteur de rendu.
    pub(super) fn effects_frame(&mut self) {
        if self.options.video.effect == 0 {
            self.effects.active.clear();
        }
        let now = Instant::now();
        let view = self.camera.view();
        // Axes de la caméra dans le monde (lignes de la vue).
        let right = Vec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x);
        let up = Vec3::new(view.x_axis.y, view.y_axis.y, view.z_axis.y);
        let mut sounds: Vec<String> = Vec::new();
        let mut batches: HashMap<(Option<String>, EffectBlend), Vec<Vertex>> = HashMap::new();
        let assets = self.assets.clone();
        let mut active = std::mem::take(&mut self.effects.active);
        active.retain(|i| (now - i.born).as_millis() as u32 <= i.life);
        for inst in active.iter_mut() {
            let Anchor::Character(cid) = inst.anchor;
            if let Some(at) = self.effects_anchor_of(cid) {
                inst.last = at;
            }
            let (pos, turn) = inst.last;
            let root = Mat4::from_translation(pos) * Mat4::from_rotation_y(turn.to_radians()) * Mat4::from_translation(inst.offset) * Mat4::from_scale(Vec3::splat(inst.scale));
            let t = (now - inst.born).as_millis() as u32;
            let set = inst.set.clone();
            for (bi, base) in set.bases.iter().enumerate() {
                if t < base.delay || (base.life > 0 && t >= base.delay + base.life) {
                    continue;
                }
                let tb = t - base.delay;
                if let Some((_, file, _, _, _)) = &base.sound {
                    if !inst.sounded[bi] && !file.is_empty() {
                        inst.sounded[bi] = true;
                        sounds.push(file.clone());
                    }
                    continue;
                }
                let blend = EffectBlend::from_data(base.blend.unwrap_or(0));
                let st = base_state(base, tb);
                if st.color[3] <= 0.0 {
                    continue;
                }
                let texture = usize::try_from(base.texture).ok().and_then(|i| set.textures.get(i)).map(|t| t.1.clone()).filter(|n| !n.is_empty());
                let m = root * base_matrix(base, &st);
                match base.kind {
                    base_kind::BOARD | base_kind::TERRAIN_BOARD => {
                        let out = batches.entry((texture, blend)).or_default();
                        let h = BOARD_HALF;
                        if base.kind == base_kind::TERRAIN_BOARD {
                            // Couchée sur le sol, à 2 unités au-dessus pour éviter le scintillement.
                            let flat = m * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                            let c = [Vec3::new(-h, h, 0.0), Vec3::new(-h, -h, 0.0), Vec3::new(h, h, 0.0), Vec3::new(h, -h, 0.0)].map(|v| flat.transform_point3(v) + Vec3::Y * 2.0);
                            quad(out, c, st.uv, st.color);
                        } else if base.flags & 1 != 0 {
                            // Face à la caméra : centre et taille de la planche, rotation dans le plan de l'écran.
                            let center = m.transform_point3(Vec3::ZERO);
                            let size = h * st.scale.x.abs() * inst.scale;
                            let spin: f32 = base.rotation[2] + st.spins.iter().filter(|(a, _)| a.z.abs() > 0.5).map(|(a, d)| d * a.z.signum()).sum::<f32>();
                            let (s, c) = spin.to_radians().sin_cos();
                            let (r, u) = (right * c + up * s, up * c - right * s);
                            let corners = [center + (-r + u) * size, center + (-r - u) * size, center + (r + u) * size, center + (r - u) * size];
                            quad(out, corners, st.uv, st.color);
                        } else {
                            let c = [Vec3::new(-h, h, 0.0), Vec3::new(-h, -h, 0.0), Vec3::new(h, h, 0.0), Vec3::new(h, -h, 0.0)].map(|v| m.transform_point3(v));
                            quad(out, c, st.uv, st.color);
                        }
                    }
                    base_kind::OBJECT => {
                        let Some((file, _)) = &base.clump else { continue };
                        let Some(clump) = self.effects.clump(&assets, file) else { continue };
                        let world = clump.world_matrices();
                        for atomic in &clump.atomics {
                            let Some(g) = clump.geometries.get(atomic.geometry as usize) else { continue };
                            let mm = m * world.get(atomic.frame as usize).copied().unwrap_or(Mat4::IDENTITY);
                            let uv0 = g.uv_sets.first();
                            // Triangles (matériau, sommets) : liste du modèle, sinon ses maillages binaires
                            // (bandes déroulées comme `scene.rs`).
                            let mut tris: Vec<(usize, [usize; 3])> = g.triangles.iter().map(|t| (t.material as usize, t.v.map(|v| v as usize))).collect();
                            if tris.is_empty() {
                                for bm in &g.meshes {
                                    let idx: Vec<usize> = bm.indices.iter().map(|&i| i as usize).collect();
                                    if g.tristrip {
                                        for (k, w) in idx.windows(3).enumerate() {
                                            if w[0] != w[1] && w[1] != w[2] && w[0] != w[2] {
                                                tris.push((bm.material as usize, if k % 2 == 0 { [w[0], w[1], w[2]] } else { [w[1], w[0], w[2]] }));
                                            }
                                        }
                                    } else {
                                        tris.extend(idx.chunks_exact(3).map(|c| (bm.material as usize, [c[0], c[1], c[2]])));
                                    }
                                }
                            }
                            for (material, verts) in tris {
                                let tex = g.materials.get(material).and_then(|mat| mat.texture.as_ref()).map(|t| t.name.clone()).filter(|n| !n.is_empty()).or_else(|| texture.clone());
                                let out = batches.entry((tex, blend)).or_default();
                                for vi in verts {
                                    let Some(p) = g.vertices.get(vi) else { continue };
                                    out.push(Vertex {
                                        position: mm.transform_point3(*p).to_array(),
                                        normal: [0.0, 1.0, 0.0],
                                        uv: uv0.and_then(|u| u.get(vi)).copied().unwrap_or([0.0, 0.0]),
                                        color: st.color,
                                    });
                                }
                            }
                        }
                    }
                    // Particules, traînées, lumières, cadres mobiles : pas encore dessinés.
                    _ => {}
                }
            }
        }
        self.effects.active = active;
        for s in sounds {
            self.audio.play_effect(&s);
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        let mut draws = Vec::with_capacity(batches.len());
        for ((texture, blend), vertices) in batches {
            if let Some(t) = &texture {
                crate::scene::ensure_texture(&mut gpu.renderer, &assets, t);
            }
            draws.push(EffectDraw { texture, blend, vertices });
        }
        // Mélange alpha d'abord, puis les additions et soustractions.
        draws.sort_by_key(|d| d.blend as u8);
        gpu.renderer.effects = draws;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_data_fields_and_anim_time() {
        // `0:150:0:1` après l'EID (données de compétence `0:1:0:5001:0:150:0:1:0:0`).
        assert_eq!(placed_from(&["5001", "0", "150", "0", "1", "0", "0"]), Some(Placed { eid: 5001, offset: Vec3::new(0.0, 150.0, 0.0), scale: 1.0 }));
        assert_eq!(placed_from(&["3478", "4", "0", "-4", "0.7", "1", "500"]).unwrap().scale, 0.7);
        assert_eq!(placed_from(&["0"]), None);
        let a = EffAnim { kind: anim_kind::SCALE, flags: 0, life: 1000, loop_opt: 1, keys: AnimKeys::Vec3(vec![(0, [1.0; 3]), (1000, [3.0; 3])]) };
        assert_eq!(anim_time(&a, 1500), 1000);
        let looped = EffAnim { loop_opt: 2, ..a.clone() };
        assert_eq!(anim_time(&looped, 1500), 500);
        let base = EffBase { anims: vec![a], ..Default::default() };
        assert_eq!(base_state(&base, 500).scale, Vec3::splat(2.0));
        // Couleur RwRGBA et rotation cumulée (165 °/s pendant 2 s).
        let b = EffBase {
            anims: vec![
                EffAnim { kind: anim_kind::COLOR, flags: 0, life: 1000, loop_opt: 1, keys: AnimKeys::Color(vec![(0, [255, 255, 255, 255]), (1000, [255, 255, 255, 0])]) },
                EffAnim { kind: anim_kind::ROTATION, flags: 0, life: 0, loop_opt: 1, keys: AnimKeys::Rotation { axis: [0.0, 1.0, 0.0], keys: vec![(0, 165.0)] } },
            ],
            ..Default::default()
        };
        let s = base_state(&b, 2000);
        assert_eq!(s.color[3], 0.0);
        assert_eq!(s.spins, vec![(Vec3::Y, 330.0)]);
        assert_eq!(set_life(&EffectSetData { life: 0, ..Default::default() }), DEFAULT_LIFE_MS);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : effets de « Dual Slash » du chevalier (TID 96,
    /// épée à une main) = jeu 5001 ; montée de niveau = jeux 3610 et 3611 ; coup sur le chevalier.
    #[test]
    fn client_tables_give_skill_and_common_effects() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        let skill = assets.table("ini", "SKILLTEMPLATE.ini").unwrap();
        let name_col = column(&skill, "Name").unwrap();
        let dual = skill.sections.iter().find(|s| s.get(&name_col) == Some("Dual Slash")).and_then(|s| s.name.parse::<i32>().ok()).expect("Dual Slash");
        let fx = skill_effects(&assets, 96, dual, 1);
        assert!(fx.iter().any(|p| p.eid == 5001 && p.offset.y == 150.0), "{fx:?}");
        let common = load_common(&assets);
        assert_eq!(common["LV_UP"].iter().map(|p| p.eid).collect::<Vec<_>>(), vec![3610, 3611]);
        assert_eq!(common["LEARN_SK"][0].eid, 3614);
        assert!(event_effects(&assets, 96, COND_STRUCK).iter().any(|p| p.eid == 3551));
        // Ressources de la montée de niveau : modèle d'effet et texture de planche du pack d'effets.
        let clump = assets.clump("effect/clump", "K0004300.ECL").expect("K0004300.ECL");
        assert!(clump.geometries.iter().any(|g| !g.triangles.is_empty() || !g.meshes.is_empty()));
        assert!(assets.texture_found("S0000102"));
    }
}
