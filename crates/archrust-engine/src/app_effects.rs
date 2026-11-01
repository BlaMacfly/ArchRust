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
//! (image par paliers, FUN_1408f45b0), spline (`effect/animation/*.ESP`, position seulement).
//! Mélanges : `sky_render::EffectBlend`. Systèmes de particules (types 1 et 2) : `effect_particles.rs`.
//! Traînées (type 3) : `effect_particles.rs`.
//!
//! Dépendances (FUN_1408386d0) : une base enfant est posée sur le repère animé de sa base parente
//! (`RwFrameAddChild`), ou sur un nœud HAnim du modèle de l'objet parent ; l'échelle animée du parent
//! passe aux enfants. Une traînée enfant (710 des 713) prend le repère du contrôleur parent avec ses
//! hauteurs h1/h2 ; une traînée indépendante d'un jeu sans drapeau 0x400 prend le repère du jeu avec
//! 70 / −20 codés en dur (aucun cas) ; avec 0x400, bords sur les nœuds HAnim 702 (haut) et 701 (bas)
//! du porteur, lus en monde chaque image, et jeu non joué si un nœud manque (FUN_140714b60). La
//! hiérarchie interrogée est celle du personnage (`AgcdCharacter`+0x80, passée par FUN_140570410 en
//! +0x20 des paramètres d'événement puis recopiée par FUN_140570c20), pas celle de l'arme portée.
//! Les jeux concernés (2627, 2960, 6082) visent des modèles sans ces nœuds (seuls les modèles
//! d'armes `d*.ecl` en ont) : ils ne sont pas joués, comme dans Archonia.
//!
//! Lumières (type 5, FUN_140833eb0, FUN_14083cb10) : lumière ponctuelle au repère de la base, rayon
//! 300 (× échelle.x de l'animation 8), couleur de l'animation de couleur / 255 (blanc sinon) ; elle
//! éclaire terrain, objets et personnages (`render::PointLight`, `shader.wgsl`, `terrain.wgsl`) avec
//! l'atténuation `1 − d/R` (choix d'ArchRust : celle du pilote RenderWare est inconnue), au plus
//! [`MAX_EFFECT_LIGHTS`], les plus proches de la caméra. Repère libre (drapeaux 0x28/0x40) non traité
//! (aucun cas : la seule lumière, EID 51, a les drapeaux 0x4).
//!
//! Cadres mobiles (type 7) : **rien n'est rendu, fidèlement**. Archonia insère bien un repère au-dessus
//! du porteur ou de la caméra (drapeau de jeu 0x40), mais sa mise à jour (FUN_14083cbc0) ne traite que
//! `MFrmType == 1` (spline) et tue le contrôleur sinon ; les 165 cadres des données sont de type 0
//! (secousse) et la fonction de secousse FUN_1409038c0 n'est appelée nulle part. Le format est décodé
//! (`effset::MovingFrameParams`) et documenté dans `docs/FORMATS.md`.
//!
//! Options GFX (`OP_Effect`, `OP_TurnOffAllEffects`) : niveau 0 = aucun effet ; l'interface (montée de
//! niveau, apprentissage) demande `EffectUI` ≥ 1 ; les compétences des autres joueurs se masquent.

#[path = "effect_particles.rs"]
mod particles;

use super::App;
use crate::assets::Assets;
use crate::render::{EffectDraw, Vertex};
use crate::sky_render::EffectBlend;
use archrust_data::effset::{anim_kind, base_kind, set_flag, AnimKeys, Attach, EffAnim, EffBase, EffectSetData, Spline, TailParams, LIGHT_RADIUS, TAIL_DEFAULT_BOTTOM, TAIL_DEFAULT_TOP, TAIL_NODE_BOTTOM, TAIL_NODE_TOP};
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
/// Au plus autant d'effets joués à la fois (les plus anciens partent, permanents en dernier).
const MAX_ACTIVE: usize = 128;
/// Drapeaux de condition d'`AgcmEventEffect` (filtre FUN_1405748b0 : condition de l'événement ET
/// condition de la donnée) : 0x1 apparition du personnage ou de l'objet (FUN_14056a0e0, effets
/// permanents), 0x40 attaque (personnage et objets équipés, FUN_140568e80), 0x100 coup reçu
/// (FUN_140571d80, appelée par FUN_140573960 qui journalise « Attacker/Target TID ») ; 0x200 (mort)
/// reste déduit des noms des jeux.
const COND_SPAWN: u32 = 0x1;
const COND_ATTACK: u32 = 0x40;
const COND_STRUCK: u32 = 0x100;
const COND_DEAD: u32 = 0x200;

/// Effet placé : jeu, décalage dans le repère du personnage (ou de son nœud), échelle, nœud
/// d'attache et délai de départ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub eid: u32,
    pub offset: Vec3,
    pub scale: f32,
    /// Nœud HAnim du modèle (`NODE_ID` d'`ee_common_char.txt`, 7ᵉ champ `%d` des données
    /// `AGCMEVENTEFFECT_EFFECT_DATA`, FUN_140575aa0) ; 0 = repère du personnage. Les valeurs
    /// négatives des données (−1, −2, −4) ne sont pas relevées : repère du personnage.
    pub node: i32,
    /// Délai de départ en ms (`GAP`, 8ᵉ champ).
    pub delay: u32,
}

/// Lit `EID:x:y:z:échelle[:nœud:délai]` (une donnée d'effet après ses indices).
fn placed_from(fields: &[&str]) -> Option<Placed> {
    let num = |i: usize| fields.get(i).and_then(|v| v.trim().parse::<f32>().ok());
    let int = |i: usize| fields.get(i).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    let eid = fields.first()?.trim().parse::<u32>().ok().filter(|e| *e > 0)?;
    Some(Placed {
        eid,
        offset: Vec3::new(num(1).unwrap_or(0.0), num(2).unwrap_or(0.0), num(3).unwrap_or(0.0)),
        scale: num(4).filter(|s| *s > 0.0).unwrap_or(1.0),
        node: int(5) as i32,
        delay: int(6).clamp(0, 60_000) as u32,
    })
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
    /// Nœud HAnim d'attache (0 : repère du personnage) et délai de départ (ms).
    node: i32,
    delay: u32,
    /// Effet d'apparition (condition 0x1) : joué tant que le personnage est en vue.
    permanent: bool,
    /// Émetteurs des bases de particules (types 1 et 2), par indice de base.
    emitters: Vec<Option<particles::Emitter>>,
    /// Traînées (type 3), par indice de base.
    trails: Vec<Option<particles::Trail>>,
    /// Rattachement de chaque base (dépendances du jeu, `EffectSetData::attachments`).
    attach: Vec<Option<Attach>>,
}

/// Au plus autant de lumières d'effet appliquées (les plus proches de la caméra) : les shaders
/// d'Archonia acceptent `LightPosition1…7` en plus de la lumière principale (FUN_1408633b0, déduit).
pub const MAX_EFFECT_LIGHTS: usize = 8;

/// Lumière d'effet (type 5) d'une image : position monde, rayon, couleur (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectLight {
    pub position: Vec3,
    pub radius: f32,
    pub color: [f32; 3],
}

/// Lumière d'une base de type 5 à l'instant `tb` dans le repère `frame` (FUN_140833eb0,
/// FUN_14083cb10) : rayon 300, ou `échelle.x × 300` avec une animation d'échelle (FUN_1408f44e0) ;
/// couleur de l'animation de couleur / 255, blanc sinon (FUN_140834b80). Seules les lumières
/// ponctuelles existent dans les données (`LightType` 0) : les projecteurs (1, 2) sont traités en
/// point. `None` si le rayon est nul.
fn base_light(base: &EffBase, tb: u32, frame: Mat4) -> Option<EffectLight> {
    base.light.as_ref()?;
    let st = base_state(base, tb);
    let has_scale = base.anims.iter().any(|a| a.kind == anim_kind::SCALE);
    let radius = if has_scale { st.scale.x * LIGHT_RADIUS } else { LIGHT_RADIUS };
    (radius > 0.0).then(|| EffectLight { position: frame.w_axis.truncate(), radius, color: [st.color[0], st.color[1], st.color[2]] })
}

#[derive(Default)]
pub struct EffectsGlue {
    active: Vec<Instance>,
    sets: HashMap<u32, Option<Arc<EffectSetData>>>,
    clumps: HashMap<String, Option<Arc<Clump>>>,
    /// Splines des animations 6 (`effect/animation/*.ESP`).
    splines: HashMap<String, Option<Arc<Spline>>>,
    /// Graine des émetteurs.
    seed: u32,
    common: Option<HashMap<String, Vec<Placed>>>,
    skill_cache: HashMap<(i32, i32, u32), Vec<Placed>>,
    event_cache: HashMap<(i32, u32), Vec<Placed>>,
    /// Personnages apparus dont les effets d'apparition restent à lancer (l'entité n'existe qu'après
    /// le traitement de l'événement).
    pending_spawn: Vec<i32>,
    /// Les effets lancés en ce moment sont des effets d'apparition.
    spawning: bool,
}

/// `ee_common_char.txt` : NAME → effets (EID, OFFSET_X/Y/Z, SCALE, NODE_ID, GAP).
fn load_common(assets: &Assets) -> HashMap<String, Vec<Placed>> {
    let text = super::npc_glue::client_text_in(assets, "ini", "ee_common_char.txt");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (Some(name), Some(eid)) = (col("NAME"), col("EID")) else { return HashMap::new() };
    let (x, y, z, s) = (col("OFFSET_X"), col("OFFSET_Y"), col("OFFSET_Z"), col("SCALE"));
    let (node, gap) = (col("NODE_ID"), col("GAP"));
    let mut out: HashMap<String, Vec<Placed>> = HashMap::new();
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let get = |i: Option<usize>| i.and_then(|i| c.get(i)).copied().unwrap_or("");
        if let Some(p) = placed_from(&[get(Some(eid)), get(x), get(y), get(z), get(s), get(node), get(gap)]) {
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

/// Temps d'une animation à l'instant `t` (ms depuis le début de la base), selon son mode de boucle
/// (FUN_1408ec780) : 0 une fois (bornée à sa durée), 1 sans boucle (temps tel quel), 2 en boucle,
/// 3 aller-retour.
fn anim_time(a: &EffAnim, t: u32) -> u32 {
    loop_time(a.loop_opt, t, a.life)
}

fn loop_time(opt: u32, t: u32, life: u32) -> u32 {
    match opt {
        0 => t.min(life),
        2 if life == 0 => 0,
        2 => t % life,
        3 if life == 0 || t <= life => t,
        3 => {
            let (n, r) = (t / life, t % life);
            if n % 2 == 0 { r } else { (n + 1) * life - t }
        }
        _ => t,
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

/// Rotation propre d'une base (degrés), relevée dans FUN_1408f83d0 : le premier angle tourne autour
/// de Y, le deuxième autour de X, le troisième autour de Z ; ordre (RenderWare, vecteurs lignes)
/// Z puis Y puis X, et X puis Y puis Z pour les objets (type 4).
fn base_rotation(base: &EffBase) -> Mat4 {
    let [a, b, c] = base.rotation;
    let (ry, rx, rz) = (Mat4::from_rotation_y(a.to_radians()), Mat4::from_rotation_x(b.to_radians()), Mat4::from_rotation_z(c.to_radians()));
    if base.kind == base_kind::OBJECT {
        rz * ry * rx
    } else {
        rx * ry * rz
    }
}

/// Matrice locale d'une base : position, rotation de la base, rotations animées, échelle.
fn base_matrix(base: &EffBase, st: &State) -> Mat4 {
    let mut m = Mat4::from_translation(Vec3::from(base.position) + st.offset) * base_rotation(base);
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

    fn spline(&mut self, assets: &Assets, file: &str) -> Option<Arc<Spline>> {
        self.splines
            .entry(file.to_ascii_lowercase())
            .or_insert_with(|| match assets.read_packed("effect/animation", file) {
                Ok(b) => Spline::parse(&b).map(Arc::new),
                Err(e) => {
                    log::debug!("effet : spline {file} : {e:#}");
                    None
                }
            })
            .clone()
    }

    /// Déplacement de la base par ses animations spline (type 6) à l'instant `tb` : position sur la
    /// spline au temps de boucle rapporté à la durée (FUN_1408f4210, FUN_1408f0f70). L'orientation
    /// le long de la courbe n'est pas appliquée.
    fn spline_offset(&mut self, assets: &Assets, base: &EffBase, tb: u32) -> Vec3 {
        let mut out = Vec3::ZERO;
        for a in base.anims.iter().filter(|a| a.kind == anim_kind::RP_SPLINE && a.life > 0) {
            let AnimKeys::File(file) = &a.keys else { continue };
            if let Some(s) = self.spline(assets, file) {
                out += Vec3::from(s.position(anim_time(a, tb) as f32 / a.life as f32));
            }
        }
        out
    }

    /// Matrice (espace du modèle) du nœud HAnim `node` du modèle d'un objet d'effet, dans la pose
    /// du fichier (les animations RtAnim des objets ne sont pas jouées).
    fn clump_node(&mut self, assets: &Assets, base: &EffBase, node: u32) -> Option<Mat4> {
        let clump = self.clump(assets, &base.clump.as_ref()?.0)?;
        let i = clump.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == node))?;
        clump.world_matrices().get(i).copied()
    }

    /// Repère (monde) sur lequel la base `i` est posée : celui de sa base parente (et du nœud HAnim
    /// désigné de son modèle), sinon `root`, le repère du jeu (FUN_1408386d0 : `RwFrameAddChild`).
    fn attach_frame(&mut self, assets: &Assets, inst: &Instance, root: Mat4, t: u32, i: usize, frames: &mut [Option<Mat4>]) -> Mat4 {
        let Some(a) = inst.attach.get(i).copied().flatten() else { return root };
        let parent = self.base_frame(assets, inst, root, t, a.parent, frames);
        match a.node {
            // Nœud absent du modèle : Archonia arrête le jeu ; ici repère de l'objet (repli).
            Some(node) => parent * self.clump_node(assets, &inst.set.bases[a.parent], node).unwrap_or(Mat4::IDENTITY),
            None => parent,
        }
    }

    /// Repère animé (monde) de la base `i` à l'instant `t` du jeu : repère d'attache × position,
    /// rotation, rotations animées, déplacements (linéaire, spline) et échelle de la base. L'échelle
    /// (animation 8) entre dans le repère et passe aux enfants (`RwFrameScale` de FUN_1408f44e0),
    /// sauf pour les particules (elle porte sur les particules), les traînées et les lumières (elle
    /// règle le rayon). Une base pas encore partie garde son repère de départ.
    fn base_frame(&mut self, assets: &Assets, inst: &Instance, root: Mat4, t: u32, i: usize, frames: &mut [Option<Mat4>]) -> Mat4 {
        if let Some(m) = frames[i] {
            return m;
        }
        let parent = self.attach_frame(assets, inst, root, t, i, frames);
        let base = &inst.set.bases[i];
        let tb = t.saturating_sub(base.delay);
        let mut st = base_state(base, tb);
        st.offset += self.spline_offset(assets, base, tb);
        if base.particles.is_some() || matches!(base.kind, base_kind::TAIL | base_kind::LIGHT) {
            st.scale = Vec3::ONE;
        }
        let m = parent * base_matrix(base, &st);
        frames[i] = Some(m);
        m
    }
}

/// Les [`MAX_EFFECT_LIGHTS`] lumières les plus proches de `eye`, pour le moteur de rendu.
fn nearest_lights(mut lights: Vec<EffectLight>, eye: Vec3) -> Vec<crate::render::PointLight> {
    lights.sort_by(|a, b| a.position.distance_squared(eye).total_cmp(&b.position.distance_squared(eye)));
    lights.into_iter().take(MAX_EFFECT_LIGHTS).map(|l| crate::render::PointLight { position: l.position, radius: l.radius, color: l.color }).collect()
}

/// Position et orientation (sans échelle) d'une matrice monde.
fn frame_of(m: Mat4) -> particles::Frame {
    let (_, rotation, position) = m.to_scale_rotation_translation();
    particles::Frame { position, rotation }
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

    /// Matrice (espace du modèle) du nœud HAnim `node` du personnage `cid`, dans sa pose animée.
    fn effects_node_matrix(&self, cid: i32, node: u32) -> Option<Mat4> {
        let animator = if self.player.as_ref().is_some_and(|p| p.cid == cid) { self.player_animator.as_ref() } else { self.animators.get(&cid) };
        animator?.1.as_ref()?.node_matrix(node)
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
            let p = self.effects_resolve_node(cid, *p);
            self.effects_spawn(p, Anchor::Character(cid), at);
        }
    }

    /// Hauteur du modèle (`HEIGHT` de `charactertemplateclient.ini`).
    fn effects_height_of(&self, cid: i32) -> f32 {
        let Some(tid) = self.effects_tid_of(cid) else { return 0.0 };
        let Ok(t) = self.assets.table("ini", "charactertemplateclient.ini") else { return 0.0 };
        column(&t, "HEIGHT").and_then(|c| t.section(&tid.to_string())?.get(&c)?.trim().parse().ok()).unwrap_or(0.0)
    }

    /// Nœuds spéciaux (négatifs) des données d'effet, comme FUN_140573480 : −1 au pied du personnage
    /// sans le suivre, −2 au-dessus de sa tête (hauteur + 50, × échelle) en le suivant, −4 à sa
    /// hauteur sans le suivre, −5 décalage tourné par sa direction sans le suivre ; −3 (nœud tiré au
    /// hasard dans le squelette) est ramené au repère du personnage. Le décalage d'un effet qui ne
    /// suit pas est pris dans les axes du monde (sauf −5).
    fn effects_resolve_node(&self, cid: i32, mut p: Placed) -> Placed {
        match p.node {
            -2 => p.offset.y = (self.effects_height_of(cid) + 50.0) * p.scale,
            -4 => p.offset.y += self.effects_height_of(cid),
            -3 => p.node = 0,
            _ => {}
        }
        p
    }

    /// Lance un jeu ; `false` s'il n'est pas joué (absent, dépendances invalides, nœuds manquants).
    fn effects_spawn(&mut self, p: Placed, anchor: Anchor, at: (Vec3, f32)) -> bool {
        let assets = self.assets.clone();
        let Some(set) = self.effects.set(&assets, p.eid) else {
            log::debug!("effet {} : jeu absent ou non décodé", p.eid);
            return false;
        };
        log::debug!("effet {} « {} » : {} base(s)", p.eid, set.name, set.bases.len());
        // Dépendances impossibles (FUN_1408386d0) : le jeu est arrêté.
        let Some(attach) = set.attachments() else {
            log::debug!("effet {} : dépendances invalides, jeu non joué", p.eid);
            return false;
        };
        // Traînée à deux nœuds (drapeau 0x400) : sans les nœuds 701 et 702 dans la hiérarchie du
        // porteur, le jeu entier n'est pas joué (FUN_140714b60 → FUN_1408d9df0).
        let Anchor::Character(cid) = anchor;
        if set.flags & set_flag::TWO_NODE_TAIL != 0 && [TAIL_NODE_TOP, TAIL_NODE_BOTTOM].iter().any(|&n| self.effects_node_matrix(cid, n).is_none()) {
            log::debug!("effet {} : nœuds 701/702 absents du porteur {cid}, jeu non joué", p.eid);
            return false;
        }
        if self.effects.active.len() >= MAX_ACTIVE {
            let i = self.effects.active.iter().position(|i| !i.permanent).unwrap_or(0);
            self.effects.active.remove(i);
        }
        let life = set_life(&set);
        let mut emitters = Vec::with_capacity(set.bases.len());
        for b in &set.bases {
            self.effects.seed = self.effects.seed.wrapping_add(1);
            emitters.push(b.particles.is_some().then(|| particles::Emitter::new(self.effects.seed)));
        }
        let trails = set.bases.iter().map(|b| b.tail.is_some().then(particles::Trail::default)).collect();
        self.effects.active.push(Instance { sounded: vec![false; set.bases.len()], set, born: Instant::now(), anchor, last: at, offset: p.offset, scale: p.scale, life, node: p.node, delay: p.delay, permanent: self.effects.spawning, emitters, trails, attach });
        true
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

    /// Effets d'apparition (condition 0x1) d'un personnage entré dans la vue : remplacent les siens.
    fn effects_spawn_permanent(&mut self, cid: i32) {
        self.effects.active.retain(|i| !(i.permanent && matches!(i.anchor, Anchor::Character(c) if c == cid)));
        self.effects.spawning = true;
        self.effects_event(cid, COND_SPAWN);
        self.effects.spawning = false;
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
            WorldEvent::CharacterSeen(s) => self.effects.pending_spawn.push(s.cid),
            WorldEvent::CharacterRemoved { cid, .. } => self.effects.active.retain(|i| !(i.permanent && matches!(i.anchor, Anchor::Character(c) if c == *cid))),
            WorldEvent::CharacterAction { cid, target_cid, result, .. } => {
                self.effects_event(*cid, COND_ATTACK);
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
                if self.effects_spawn(Placed { eid, offset: Vec3::ZERO, scale: 1.0, node: 0, delay: 0 }, Anchor::Character(cid), at) {
                    format!("Effet {eid} « {} » : {} base(s), {} ms.", set.name, set.bases.len(), set_life(&set))
                } else {
                    format!("Effet {eid} « {} » non joué (dépendances ou nœuds 701/702 du porteur manquants).", set.name)
                }
            }
            None => format!("Effet {eid} introuvable."),
        }
    }

    /// Chaque image : sons des bases atteintes, triangles des effets pour le moteur de rendu.
    pub(super) fn effects_frame(&mut self) {
        if self.options.video.effect == 0 {
            self.effects.active.clear();
        }
        for cid in std::mem::take(&mut self.effects.pending_spawn) {
            if self.effects_anchor_of(cid).is_some() {
                self.effects_spawn_permanent(cid);
            }
        }
        let now = Instant::now();
        let view = self.camera.view();
        // Axes de la caméra dans le monde (lignes de la vue).
        let right = Vec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x);
        let up = Vec3::new(view.x_axis.y, view.y_axis.y, view.z_axis.y);
        let cam = particles::CameraAxes { right, up };
        let mut sounds: Vec<String> = Vec::new();
        let mut batches: HashMap<(Option<String>, EffectBlend), Vec<Vertex>> = HashMap::new();
        let mut lights: Vec<EffectLight> = Vec::new();
        let assets = self.assets.clone();
        let mut active = std::mem::take(&mut self.effects.active);
        active.retain(|i| i.permanent || (now - i.born).as_millis() as u32 <= i.delay + i.life);
        for inst in active.iter_mut() {
            // Délai de départ (`GAP`) : rien n'est joué avant.
            let Some(t) = ((now - inst.born).as_millis() as u32).checked_sub(inst.delay) else { continue };
            let Anchor::Character(cid) = inst.anchor;
            // Nœuds 0, positifs et −2 : l'effet suit le personnage (drapeau 1 de FUN_140573480) ;
            // −1, −4, −5 : posé à sa position du départ.
            let follows = inst.node >= 0 || inst.node == -2;
            if follows {
                if let Some(at) = self.effects_anchor_of(cid) {
                    inst.last = at;
                }
            }
            let (pos, turn) = inst.last;
            // Nœud d'attache : repère de l'os dans la pose courante (repli : repère du personnage).
            let node = u32::try_from(inst.node).ok().filter(|n| *n > 0).and_then(|n| self.effects_node_matrix(cid, n)).unwrap_or(Mat4::IDENTITY);
            let turn = Mat4::from_rotation_y(turn.to_radians());
            let root = if follows || inst.node == -5 {
                Mat4::from_translation(pos) * turn * node * Mat4::from_translation(inst.offset) * Mat4::from_scale(Vec3::splat(inst.scale))
            } else {
                Mat4::from_translation(pos + inst.offset) * turn * Mat4::from_scale(Vec3::splat(inst.scale))
            };
            let set = inst.set.clone();
            // Repères animés des bases, dépendances comprises (une base enfant suit le repère de sa
            // parente), et repère d'attache des traînées enfants (repère du contrôleur parent).
            let n = set.bases.len();
            let mut frames: Vec<Option<Mat4>> = vec![None; n];
            let mut tail_frames: Vec<Option<Mat4>> = vec![None; n];
            for bi in 0..n {
                self.effects.base_frame(&assets, inst, root, t, bi, &mut frames);
                if set.bases[bi].tail.is_some() && inst.attach.get(bi).is_some_and(|a| a.is_some()) {
                    tail_frames[bi] = Some(self.effects.attach_frame(&assets, inst, root, t, bi, &mut frames));
                }
            }
            // Traînée à deux nœuds (drapeau 0x400) : bord haut sur le nœud 702, bas sur le 701, lus en
            // monde à chaque image sur le porteur (sa position courante, même pour un effet posé).
            let two_node = set.flags & set_flag::TWO_NODE_TAIL != 0;
            let node_edges = two_node
                .then(|| {
                    let (p, y) = self.effects_anchor_of(cid).unwrap_or(inst.last);
                    let m = Mat4::from_translation(p) * Mat4::from_rotation_y(y.to_radians());
                    Some(((m * self.effects_node_matrix(cid, TAIL_NODE_TOP)?).w_axis.truncate(), (m * self.effects_node_matrix(cid, TAIL_NODE_BOTTOM)?).w_axis.truncate()))
                })
                .flatten();
            for (bi, base) in set.bases.iter().enumerate() {
                // Fin d'une base (FUN_14083ff40) : seule une base sans boucle (mode 0) s'arrête à la fin
                // de sa durée ; les autres vivent jusqu'à la fin du jeu.
                if t < base.delay || (base.loop_opt == 0 && base.life > 0 && t > base.delay + base.life) {
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
                let m = frames[bi].unwrap_or(root);
                let texture = usize::try_from(base.texture).ok().and_then(|i| set.textures.get(i)).map(|t| t.1.clone()).filter(|n| !n.is_empty());
                if let Some(Some(emitter)) = inst.emitters.get_mut(bi) {
                    // Particules : les animations 0, 1, 8 et 9 portent sur les particules, les autres
                    // déplacent l'émetteur (repère sans échelle, voir `base_frame`).
                    let frame = frame_of(m);
                    emitter.emit(base, t, &frame, &cam);
                    let out = batches.entry((texture, blend)).or_default();
                    for s in emitter.sprites(base, t, &frame, &cam, inst.scale) {
                        quad(out, s.corners, s.uv, s.color);
                    }
                    continue;
                }
                if let (Some(p), Some(Some(trail))) = (&base.tail, inst.trails.get_mut(bi)) {
                    // Traînée (FUN_14083d8b0) : couleur des animations de la base ; bords selon le chemin
                    // de création (FUN_1408386d0, FUN_140714b60).
                    let color = st.color;
                    let edges = if let Some(pm) = tail_frames[bi] {
                        // Enfant d'une dépendance (710 des 713 traînées) : repère de la base parente,
                        // h1/h2 des données.
                        particles::Trail::edges(base, p, &frame_of(pm), inst.scale)
                    } else if two_node {
                        let Some(e) = node_edges else { continue };
                        e
                    } else {
                        // Indépendante, jeu sans 0x400 : repère du jeu, hauteurs 70 / −20 codées en dur
                        // (aucun cas dans les données).
                        let q = TailParams { top: TAIL_DEFAULT_TOP, bottom: TAIL_DEFAULT_BOTTOM, ..p.clone() };
                        particles::Trail::edges(base, &q, &frame_of(root), inst.scale)
                    };
                    trail.update(p, t, edges);
                    let out = batches.entry((texture, blend)).or_default();
                    for q in trail.ribbon(edges) {
                        for i in [0usize, 1, 2, 2, 1, 3] {
                            let (pos, uv, alpha) = q[i];
                            out.push(Vertex { position: pos.to_array(), normal: [0.0, 1.0, 0.0], uv, color: [color[0], color[1], color[2], color[3] * alpha] });
                        }
                    }
                    continue;
                }
                if base.kind == base_kind::LIGHT {
                    // Lumière : ajoutée à l'éclairage du terrain, des objets et des personnages.
                    lights.extend(base_light(base, tb, m));
                    continue;
                }
                if st.color[3] <= 0.0 {
                    continue;
                }
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
                    // Cadres mobiles (type 7) : jamais joués par Archonia (voir l'en-tête du module).
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
        let eye = view.inverse().w_axis.truncate();
        gpu.renderer.point_lights = nearest_lights(lights, eye);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_data_fields_and_anim_time() {
        // `0:150:0:1` après l'EID (données de compétence `0:1:0:5001:0:150:0:1:0:0`).
        assert_eq!(placed_from(&["5001", "0", "150", "0", "1", "0", "0"]), Some(Placed { eid: 5001, offset: Vec3::new(0.0, 150.0, 0.0), scale: 1.0, node: 0, delay: 0 }));
        let p = placed_from(&["3478", "4", "0", "-4", "0.7", "101", "500"]).unwrap();
        assert_eq!((p.scale, p.node, p.delay), (0.7, 101, 500));
        assert_eq!(placed_from(&["0"]), None);
        let a = EffAnim { kind: anim_kind::SCALE, flags: 0, life: 1000, loop_opt: 1, keys: AnimKeys::Vec3(vec![(0, [1.0; 3]), (1000, [3.0; 3])]) };
        // Modes de boucle (FUN_1408ec780) : 1 temps tel quel, 0 borné, 2 répété, 3 aller-retour.
        assert_eq!(anim_time(&a, 1500), 1500);
        assert_eq!(anim_time(&EffAnim { loop_opt: 0, ..a.clone() }, 1500), 1000);
        let looped = EffAnim { loop_opt: 2, ..a.clone() };
        assert_eq!(anim_time(&looped, 1500), 500);
        assert_eq!(loop_time(3, 1500, 1000), 500);
        assert_eq!(loop_time(3, 2300, 1000), 300);
        assert_eq!(loop_time(3, 800, 1000), 800);
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
        // Effets d'apparition (0x1) et d'attaque (0x40) : présents pour une partie des modèles.
        let t = assets.table("ini", "charactertemplateeventeffect.ini").unwrap();
        let tids: Vec<i32> = t.sections.iter().filter_map(|s| s.name.parse().ok()).take(400).collect();
        assert!(tids.iter().any(|&tid| !event_effects(&assets, tid, COND_SPAWN).is_empty()));
        assert!(tids.iter().any(|&tid| !event_effects(&assets, tid, COND_ATTACK).is_empty()));
        // Ressources de la montée de niveau : modèle d'effet et texture de planche du pack d'effets.
        let clump = assets.clump("effect/clump", "K0004300.ECL").expect("K0004300.ECL");
        assert!(clump.geometries.iter().any(|g| !g.triangles.is_empty() || !g.meshes.is_empty()));
        assert!(assets.texture_found("S0000102"));
        // Spline de l'émetteur des particules de la montée de niveau (jeu 3611, animation 6).
        let spline = Spline::parse(&assets.read_packed("effect/animation", "G00e1b00.ESP").expect("G00e1b00.ESP")).expect("spline");
        assert_eq!((spline.points.len(), spline.closed), (5, false));
        let (start, end) = (spline.position(0.0), spline.position(1.0));
        assert!(start[1] < 80.0 && end[1] > 200.0, "{start:?} → {end:?}");
    }

    /// Lumière de l'EID 51 : rayon 2 × 300 → 0 sur 75 ms, couleur de l'animation / 255, au repère.
    #[test]
    fn light_radius_color_and_nearest() {
        use archrust_data::effset::LightParams;
        let base = EffBase {
            kind: base_kind::LIGHT,
            life: 75,
            loop_opt: 2,
            light: Some(LightParams { light_type: 0, pos: [0.0; 3], rot: [0.0; 3], cone_angle: 30.0, surface: [-1.0; 3], material: [0xff; 4] }),
            anims: vec![
                EffAnim { kind: anim_kind::SCALE, flags: 0, life: 75, loop_opt: 1, keys: AnimKeys::Vec3(vec![(0, [2.0; 3]), (75, [0.0; 3])]) },
                EffAnim { kind: anim_kind::COLOR, flags: 0, life: 0, loop_opt: 1, keys: AnimKeys::Color(vec![(0, [0x9a, 0x92, 0xff, 0xcc])]) },
            ],
            ..Default::default()
        };
        let frame = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let l = base_light(&base, 0, frame).unwrap();
        assert_eq!((l.position, l.radius), (Vec3::new(10.0, 20.0, 30.0), 600.0));
        assert!((l.color[0] - 0x9a as f32 / 255.0).abs() < 1e-6 && l.color[2] == 1.0);
        assert!((base_light(&base, 37, frame).unwrap().radius - 300.0 * 2.0 * 38.0 / 75.0).abs() < 1.0);
        assert!(base_light(&base, 75, frame).is_none(), "rayon nul");
        // Sans animation : rayon 300, blanc.
        let plain = EffBase { anims: Vec::new(), ..base.clone() };
        assert_eq!(base_light(&plain, 0, frame), Some(EffectLight { position: frame.w_axis.truncate(), radius: 300.0, color: [1.0; 3] }));
        // Au plus huit, les plus proches de la caméra.
        let many: Vec<EffectLight> = (0..12).rev().map(|i| EffectLight { position: Vec3::X * i as f32 * 100.0, radius: 300.0, color: [1.0; 3] }).collect();
        let near = nearest_lights(many, Vec3::ZERO);
        assert_eq!(near.len(), MAX_EFFECT_LIGHTS);
        assert_eq!((near[0].position.x, near[7].position.x), (0.0, 700.0));
    }

    /// Repères : une base enfant suit le repère animé (échelle comprise) de sa parente.
    #[test]
    fn dependent_base_follows_parent_frame() {
        let parent = EffBase {
            kind: base_kind::OBJECT,
            position: [0.0, 100.0, 0.0],
            anims: vec![EffAnim { kind: anim_kind::ROTATION, flags: 0, life: 0, loop_opt: 1, keys: AnimKeys::Rotation { axis: [0.0, 0.0, 1.0], keys: vec![(0, 90.0)] } }],
            ..Default::default()
        };
        let child = EffBase { kind: base_kind::BOARD, position: [10.0, 0.0, 0.0], ..Default::default() };
        let set = Arc::new(EffectSetData { deps: vec![archrust_data::effset::EffDep { parent: 0, node: None, child: 1, flags: 0 }], bases: vec![parent, child], ..Default::default() });
        let attach = set.attachments().unwrap();
        let inst = Instance { set, born: Instant::now(), anchor: Anchor::Character(0), last: (Vec3::ZERO, 0.0), offset: Vec3::ZERO, scale: 1.0, sounded: vec![false; 2], life: 1000, node: 0, delay: 0, permanent: false, emitters: vec![None, None], trails: vec![None, None], attach };
        let assets = Assets::new(None, &[]);
        let mut glue = EffectsGlue::default();
        let mut frames = vec![None; 2];
        let root = Mat4::from_translation(Vec3::new(1000.0, 0.0, 0.0));
        // 90 °/s pendant 1 s autour de Z : l'axe X de l'objet devient Y ; l'enfant (10, 0, 0) suit.
        let m = glue.base_frame(&assets, &inst, root, 1000, 1, &mut frames);
        assert!((m.w_axis.truncate() - Vec3::new(1000.0, 110.0, 0.0)).length() < 1e-3, "{}", m.w_axis);
        // Repère d'attache de l'enfant = repère de l'objet.
        let pm = glue.attach_frame(&assets, &inst, root, 1000, 1, &mut frames);
        assert!((pm.w_axis.truncate() - Vec3::new(1000.0, 100.0, 0.0)).length() < 1e-3);
    }

    /// Avec les données du client : traînées dépendantes des jeux 127 (catapulte, sur des objets,
    /// ±700 / ±500) et 231 (sur une planche, −400 / 200) posées sur le repère de leur parente avec
    /// les hauteurs des données ; les 710 traînées dépendantes du client ont un repère d'attache.
    #[test]
    fn dependent_trails_ride_parent_frames() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        let mut glue = EffectsGlue::default();
        let root = Mat4::from_translation(Vec3::new(5000.0, 0.0, 5000.0));
        let mut checked = 0;
        for eid in [127u32, 231] {
            let set = glue.set(&assets, eid).expect("jeu");
            let attach = set.attachments().unwrap();
            let n = set.bases.len();
            let inst = Instance { set: set.clone(), born: Instant::now(), anchor: Anchor::Character(0), last: (Vec3::ZERO, 0.0), offset: Vec3::ZERO, scale: 1.0, sounded: vec![false; n], life: 1000, node: 0, delay: 0, permanent: false, emitters: vec![None; n], trails: vec![None; n], attach: attach.clone() };
            for (i, b) in set.bases.iter().enumerate() {
                let (Some(p), Some(a)) = (&b.tail, attach[i]) else { continue };
                let mut frames = vec![None; n];
                let t = b.delay + 300;
                let parent = glue.base_frame(&assets, &inst, root, t, a.parent, &mut frames);
                let pm = glue.attach_frame(&assets, &inst, root, t, i, &mut frames);
                assert_eq!(pm, parent, "jeu {eid} base {i} : repère du parent");
                let f = frame_of(pm);
                let (top, bottom) = particles::Trail::edges(b, p, &f, 1.0);
                assert!(((top - bottom).length() - (p.top - p.bottom).abs()).abs() < 1e-2, "jeu {eid} base {i}");
                assert!((top - f.position).length() - p.top.abs() < 1e-2);
                checked += 1;
            }
        }
        assert!(checked >= 3, "{checked}");
        let sets = assets.effect_sets();
        let dependent = sets.iter().filter_map(|s| sets.decode(s.id)).map(|s| {
            let at = s.attachments().unwrap();
            s.bases.iter().enumerate().filter(|(i, b)| b.kind == base_kind::TAIL && at[*i].is_some()).count()
        });
        assert_eq!(dependent.sum::<usize>(), 710);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : les modèles qui jouent les traînées à deux
    /// nœuds (2627 Evelikos 2103, 2960 Sky Dragon Summoner 1939 / 2111, 6082 archer 1) n'ont **pas**
    /// les nœuds HAnim 701 / 702 dans leur squelette : ces nœuds sont ceux des modèles d'objets
    /// (armes `d*.ecl`, ex. `d0000002.ecl`) et de trois créatures (`a00006bb`, `a00006bc`, `a00006c4`).
    /// Archonia cherche ces nœuds dans la hiérarchie du personnage (`AgcdCharacter`+0x80,
    /// FUN_140570410 → FUN_140570c20 → FUN_140714b60), pas dans l'arme : ces trois jeux ne sont pas joués.
    #[test]
    fn two_node_tail_models_lack_nodes_701_702() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        let ids = |f: &str| -> std::collections::BTreeSet<u32> {
            assets.clump("character", f).map(|c| c.frames.iter().filter_map(|f| f.hanim.as_ref().map(|h| h.node_id)).collect()).unwrap_or_default()
        };
        for tid in [2103, 1939, 2111, 1] {
            let files = crate::scene::models_for_tid(&assets, tid);
            assert!(!files.is_empty(), "TID {tid} sans modèle");
            for f in &files {
                let n = ids(f);
                assert!(!n.is_empty() && !n.contains(&TAIL_NODE_TOP) && !n.contains(&TAIL_NODE_BOTTOM), "TID {tid} {f} : {n:?}");
            }
        }
        for f in ["d0000002.ecl", "a00006bb.ecl"] {
            let n = ids(f);
            assert!(n.contains(&TAIL_NODE_TOP) && n.contains(&TAIL_NODE_BOTTOM), "{f} : {n:?}");
        }
    }
}
