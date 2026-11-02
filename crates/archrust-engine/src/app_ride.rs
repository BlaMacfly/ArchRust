//! Montures côté moteur. Protocole : `archrust_netclient::ride` ; monter = porter l'objet de
//! monture (case Ride `CONTROL_3711` de `Character_Status`, partie 14), descendre = le ranger ; la
//! lance va dans la case `CONTROL_3409` (partie 15). Le client n'envoie rien d'autre.
//!
//! Comportement relevé dans Archonia (AgcmRide, AgcmCharacter ; sha256 e9c7609b…) :
//! * op 2 : monture = `SubType` de l'objet monture (FUN_14061d400), l'objet doit être connu ; op 6 :
//!   monture donnée par son TID ; op 4 : la monture est retirée (FUN_14061d530, FUN_140750d50) ; un
//!   autre joueur marqué monté (bit 1 du champ 18 / champ 15 de la vue) dont l'objet de genre 0x20
//!   arrive reçoit sa monture (FUN_1407553e0) ;
//! * la monture est un personnage local (CID ≥ 1 000 000, FUN_140750530) : modèle et animations de son
//!   TID (`charactertemplateclient.ini`, `charactertemplateanimation.ini`), animée comme un monstre
//!   (attente / course) ;
//! * cavalier : animations « à cheval » (FUN_140754290, FUN_140756060, voir `worn::anim_equip`) selon
//!   `ANIMATION_TYPE2` du gabarit : guerrier (1) 7 / lance 8 ; archer et mage (2, 3) 4 / 5 ; Archlord (5)
//!   1 / 1 ; à la montée les armes des mains sont détachées et la lance attachée en main droite,
//!   l'inverse à la descente (filtre au tracé, `worn::Worn::visible`) ;
//! * vitesse : facteur MovementFast, sans le plafond de 60 000 à cheval (FUN_14017a850) ;
//! * refus du client à cheval : compétences sans `auto_attack` (`sysstr` 320, FUN_14069a0c0), production
//!   (`sysstr` 527, FUN_140657410) ; à cheval ou métamorphosé : potion de métamorphose (usage 7, `sysstr`
//!   346), potions d'usage 12 de sous-type 1 (`sysstr` 347) et 2 (`sysstr` 348), FUN_14053d0b0 ;
//! * refus du serveur : message système code 24 → `CannotRideWhileCombat` (uimessage).
//!
//! **Attache du cavalier** (`FUN_140750660`, observé) : nœud de selle = ID HAnim 20 de la monture ;
//! `monde_monture = T(pos) × Ry(turnY) × Rx(turnX) × S(SCALE)` (le cadre du clump de la monture est
//! remplacé : son cadre 0, tourné de 64° pour le cheval `a0000237.ecl`, ne compte pas) ;
//! `monde_cavalier = monde_monture × nœud20(pose de la monture) × Ry(90°) × Rz(90°)`, avec `× S(1/s)` si
//! l'échelle `s` de la selle sort de [0,5 ; 2] (voir [`rider_on_saddle`]). La monture rejoue chaque
//! type d'animation lancé sur le cavalier (`FUN_1404be4f0`, `Animator::follow`). `RIDER_HEIGHT` (270
//! pour 567) ne sert qu'à hausser le panneau du nom (voir `app_boards.rs`).
//! Montures glTF (patchs `[[models]]`, `.glb` du client comme 5748 à 5777) : peau et animations du glTF
//! jouées comme celles des montures d'origine (voir [`GltfMount`]) ; noms des nœuds lus comme Archonia
//! (FUN_140769280, table DAT_140d70270 : `RideNode`, `rideNode`, `Mount` → ID HAnim 20, `Bone_<n>` → n),
//! le cavalier posé sur le nœud 20 comme sur une selle d'origine ; sans ce nœud, à 60 % de `HEIGHT`.

use super::App;
use crate::animation::{AnimType, Animator};
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi};
use crate::render::{DrawItem, Mesh};
use crate::scene::{entity_meshes, models_for_tid, Entity, Head};
use crate::ui::UiState;
use archrust_netclient::ride::{RideEvent, LOCAL_CID_BASE};
use archrust_netclient::world::{EquipmentView, ItemRecord};
use archrust_netclient::WorldEvent;
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Genre (`Kind`) des montures et partie de leur objet.
const KIND_RIDE: i64 = 32;
const PART_RIDE: i64 = 14;
/// Nœud HAnim de la selle (`FUN_140750660`).
const NODE_SADDLE: u32 = 20;
const STATUS_EQUIP: i8 = 3;
/// Message système du serveur : « en combat, impossible de monter ».
const CODE_CANNOT_RIDE_IN_COMBAT: i32 = 24;
/// Attribut des compétences de production dans `SKILL_SPEC.txt`.
const ATTRIBUTE_PRODUCT: i64 = 8;

/// Colonnes utiles d'itemdatatable.txt.
#[derive(Debug, Clone, Copy, Default)]
struct ItemRow {
    item_type: i64,
    kind: i64,
    part: i64,
    /// `Type` : type d'arme (genre 2) ou d'usage (consommable).
    kind_type: i64,
    sub_type: i64,
}

#[derive(Default)]
struct RideData {
    items: HashMap<i32, ItemRow>,
    /// TID de compétence → (`auto_attack`, production).
    skills: HashMap<i32, (bool, bool)>,
    sysstr: HashMap<u32, String>,
    /// TID de monture → (`HEIGHT`, `RIDER_HEIGHT`).
    heights: HashMap<i32, (f32, f32)>,
}

impl RideData {
    fn item(&self, tid: Option<i32>) -> ItemRow {
        tid.and_then(|t| self.items.get(&t).copied()).unwrap_or_default()
    }

    fn sys(&self, id: u32) -> String {
        self.sysstr.get(&id).cloned().unwrap_or_default()
    }
}

/// Colonne nommée d'une table positionnelle (`[]` = en-tête) pour la section `tid`.
fn column(assets: &Assets, file: &str, tid: i32, name: &str) -> Option<f32> {
    let table = assets.table("ini", file).ok()?;
    let key = table.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(name)).map(|(k, _)| k.clone())?;
    table.section(&tid.to_string())?.get(&key)?.trim().parse().ok()
}

fn load_data(assets: &Assets) -> RideData {
    let mut d = RideData::default();
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, item_type, kind, part, kind_type, sub_type) = (col("TID"), col("ItemType"), col("Kind"), col("Part"), col("Type"), col("SubType"));
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let num = |c: Option<usize>| c.and_then(|i| cells.get(i)).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
            let t = num(tid) as i32;
            if t <= 0 {
                continue;
            }
            d.items.insert(t, ItemRow { item_type: num(item_type), kind: num(kind), part: num(part), kind_type: num(kind_type), sub_type: num(sub_type) });
        }
    }
    // SKILL_SPEC.txt (par nom) : `auto_attack` rempli, attribut de production ; noms → TID par
    // SKILLTEMPLATE.ini.
    let mut by_name: HashMap<String, Vec<i32>> = HashMap::new();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            if let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) {
                by_name.entry(name.trim().to_string()).or_default().push(tid);
            }
        }
    }
    let spec = super::npc_glue::client_text_in(assets, "ini", "SKILL_SPEC.txt");
    let mut rows = spec.lines().map(|l| l.split('\t').map(str::trim).collect::<Vec<_>>());
    if let Some(header) = rows.next() {
        let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
        let (auto, attr) = (col("auto_attack"), col("attribute"));
        for row in rows {
            let cell = |c: Option<usize>| c.and_then(|i| row.get(i)).copied().unwrap_or("");
            let auto_attack = !cell(auto).is_empty() && cell(auto) != "0";
            let product = cell(attr).parse::<i64>().ok() == Some(ATTRIBUTE_PRODUCT);
            for tid in row.first().and_then(|n| by_name.get(*n)).into_iter().flatten() {
                d.skills.insert(*tid, (auto_attack, product));
            }
        }
    }
    for id in [320, 346, 347, 348, 527] {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    log::info!("montures : {} objet(s) de monture, {} compétence(s) décrites", d.items.values().filter(|i| i.kind == KIND_RIDE).count(), d.skills.len());
    d
}

/// Cavalier posé sur la selle (`FUN_140750660`) : `selle` = monde du nœud 20 de la monture (matrice de
/// la monture comprise) ; cadre de correction `Ry(90°) × Rz(90°)`, et `S(1/s)` si l'échelle `s` de la
/// selle est hors de [0,5 ; 2] (et > 1e-4).
pub(super) fn rider_on_saddle(saddle: Mat4) -> Mat4 {
    let s = saddle.x_axis.truncate().length();
    let mut revision = Mat4::from_rotation_y(90f32.to_radians()) * Mat4::from_rotation_z(90f32.to_radians());
    if s > 1e-4 && !(0.5..=2.0).contains(&s) {
        revision *= Mat4::from_scale(Vec3::splat(1.0 / s));
    }
    saddle * revision
}

/// Matrice de la monture : celle du cavalier (`T × Ry`, la monture prend sa position et ses angles) ×
/// `S(SCALE)` du gabarit.
pub(super) fn mount_world(base: Mat4, scale: f32) -> Mat4 {
    base * Mat4::from_scale(Vec3::splat(scale))
}

/// Monture affichée : maillages (cadre 0 à l'identité), animation, échelle et selle.
struct MountModel {
    meshes: Vec<(Arc<Mesh>, Mat4)>,
    animator: Option<Animator>,
    /// `SCALE` de `charactertemplateclient.ini` (1 sans valeur).
    scale: f32,
    /// Nœud 20 en pose de liaison (sans animateur) ; `None` sans squelette (glTF).
    saddle_bind: Option<Mat4>,
    /// `HEIGHT` : repli du cavalier sans squelette.
    height: f32,
    /// Monture glTF (`.glb` du client ou patch `[[models]]`) : squelette, clips et nœud de selle.
    gltf: Option<GltfMount>,
}

/// Monture glTF animée. Les variantes de `charactertemplateanimation.ini` nomment ses animations
/// (`[5748]` : `0:0:0:IDLE`, `0:0:1:ROAR`, `2:0:0:RUN`…, comme les `.glb` du client) ; une variante
/// `fichier.ean` (patch remplaçant un `.ecl` d'origine) cherche l'animation nommée `fichier` (convention
/// d'ArchRust). Même suivi que les montures d'origine ([`Animator::follow`]) ; peau déformée sur le
/// CPU, au plus 30 fois par seconde. Nœud de selle : ID HAnim 20 d'après son nom
/// ([`crate::gltf_model::hanim_id`] : `RideNode`, `Saddle_20`…).
struct GltfMount {
    model: Arc<crate::assets::PatchedModel>,
    /// (instance, maillage GPU propre, matrice de repos `S(échelle) × nœud`).
    parts: Vec<(usize, Arc<Mesh>, Mat4)>,
    set: crate::animation::AnimationSet,
    anim: Option<usize>,
    kind: u32,
    followed: u32,
    time: f32,
    since_update: f32,
    seed: u32,
    /// Pose courante (repère du glTF, sans l'échelle du manifeste).
    globals: Vec<Mat4>,
    ride_node: Option<usize>,
    /// Repère du squelette ([`crate::gltf_model::GltfModel::skeleton_space`]) : celui de la monture.
    space: Mat4,
}

impl GltfMount {
    fn new(renderer: &mut crate::render::Renderer, assets: &Assets, tid: i32, model: Arc<crate::assets::PatchedModel>, seed: u32) -> GltfMount {
        let parts = crate::gltf_model::upload_instances(renderer, &model.model, &model.texture_prefix(), model.scale);
        let globals = model.model.node_matrices(None, 0.0);
        let ride_node = model.model.hanim_node(NODE_SADDLE);
        let space = model.model.skeleton_space();
        let g = GltfMount { set: crate::animation::AnimationSet::for_tid(assets, tid, 0, false), parts, anim: None, kind: u32::MAX, followed: u32::MAX, time: 0.0, since_update: f32::MAX, seed: seed.max(1), globals, ride_node, space, model };
        // Peau au repos dans le repère de la scène (règle glTF), même sans animation jouée.
        g.write_skins(renderer);
        g
    }

    fn write_skins(&self, renderer: &crate::render::Renderer) {
        for (index, mesh, _) in &self.parts {
            let Some(inst) = self.model.model.instances.get(*index) else { continue };
            if let Some(v) = inst.skin.and_then(|skin| self.model.model.skinned_vertices(inst.mesh, skin, &self.globals)) {
                renderer.write_vertices(mesh, &v);
            }
        }
    }

    /// Repère de la monture → repère glTF : échelle du manifeste × repère du squelette.
    fn root(&self) -> Mat4 {
        Mat4::from_scale(Vec3::splat(if self.model.scale > 0.0 { self.model.scale } else { 1.0 })) * self.space
    }

    fn roll(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Animation glTF de la variante tirée pour le type `kind`.
    fn pick(&mut self, kind: u32) -> Option<usize> {
        let roll = self.roll();
        let file = self.set.pick(kind, roll)?.file.clone();
        self.model.model.animation(&file)
    }

    /// Comme [`Animator::follow`] : le type lancé sur le cavalier l'est sur la monture, sauf si elle joue
    /// déjà ce type d'attente, de marche ou de course ; sans animation pour lui, elle garde la sienne. Les
    /// types ponctuels (attaque, coup, mort) se figent sur leur dernière pose.
    fn follow(&mut self, renderer: &crate::render::Renderer, dt: f32, kind: u32, max_hz: f32) {
        let looping = |k: u32| k <= AnimType::Run as u32;
        if kind != self.followed {
            self.followed = kind;
            if !(looping(kind) && self.kind == kind) {
                if let Some(i) = self.pick(kind) {
                    (self.anim, self.kind, self.time, self.since_update) = (Some(i), kind, 0.0, f32::MAX);
                }
            }
        }
        let Some(duration) = self.anim.and_then(|i| self.model.model.animations.get(i)).map(|a| a.duration) else { return };
        self.time += dt;
        if !looping(self.kind) {
            self.time = self.time.min((duration - 1e-3).max(0.0));
        } else if self.time >= duration && duration > 0.0 {
            self.time -= duration;
            // Fin d'un cycle d'attente : nouveau tirage parmi les variantes (IDLE / ROAR).
            if self.kind == AnimType::Wait as u32 {
                if let Some(i) = self.pick(self.kind) {
                    self.anim = Some(i);
                }
            }
        }
        self.since_update += dt;
        if self.since_update < 1.0 / max_hz {
            return;
        }
        self.since_update = 0.0;
        self.globals = self.model.model.node_matrices(self.anim, self.time);
        self.write_skins(renderer);
    }

    /// Maillages et matrices (repère de la monture) : peau déjà dans le repère de la scène glTF, autres
    /// nœuds suivant leur matrice animée.
    fn meshes(&self) -> Vec<(Arc<Mesh>, Mat4)> {
        let root = self.root();
        self.parts
            .iter()
            .map(|(index, mesh, rest)| {
                let m = match self.model.model.instances.get(*index) {
                    Some(i) if i.skin.is_some() => root,
                    Some(i) => self.globals.get(i.node_index).map_or(*rest, |g| root * *g),
                    None => *rest,
                };
                (mesh.clone(), m)
            })
            .collect()
    }

    /// Nœud de selle (repère de la monture), dans la pose courante.
    fn saddle(&self) -> Option<Mat4> {
        Some(self.root() * *self.globals.get(self.ride_node?)?)
    }
}

/// Monture d'un personnage.
struct Mount {
    /// TID du modèle de monture (`SubType` de l'objet, ou op 6).
    tid: i32,
    model: Option<MountModel>,
    /// Modèle introuvable : signalé une fois.
    failed: bool,
}

#[derive(Default)]
pub struct RideGlue {
    data: Option<RideData>,
    mounts: HashMap<i32, Mount>,
    /// Personnages marqués montés (bit 1 de l'octet d'état).
    flagged: HashSet<i32>,
    /// Personnages métamorphosés (bit 0).
    transformed: HashSet<i32>,
    /// CID du joueur, pour les demandes de l'interface.
    me: i32,
    /// Le joueur est l'ArchLord (`AgpmArchlord`, voir `app_archlord.rs`) : potions d'usage 12 permises.
    pub archlord: bool,
}

impl RideGlue {
    fn data(&mut self, assets: &Assets) -> &RideData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Le personnage `cid` est sur une monture.
    pub fn riding(&self, cid: i32) -> bool {
        self.mounts.contains_key(&cid)
    }

    /// Matrice du cavalier `cid` (`base` : sa position et son orientation) : posé sur la selle de sa
    /// monture (voir [`rider_on_saddle`]).
    pub fn rider_matrix(&self, cid: i32, base: Mat4) -> Mat4 {
        let Some(m) = self.mounts.get(&cid).and_then(|m| m.model.as_ref()) else { return base };
        let world = mount_world(base, m.scale);
        let saddle = m.animator.as_ref().and_then(|a| a.node_matrix(NODE_SADDLE)).or(m.saddle_bind).or_else(|| m.gltf.as_ref().and_then(|g| g.saddle()));
        match saddle {
            Some(saddle) => rider_on_saddle(world * saddle),
            None => base * Mat4::from_translation(Vec3::new(0.0, m.height * 0.6, 0.0)),
        }
    }

    /// Maillages des montures des personnages `chars` à tracer (dans la distance `CharView`, le
    /// joueur toujours).
    pub fn draw_items<'a>(&self, chars: impl Iterator<Item = &'a Entity>, char_distance: Option<f32>, eye: Vec3) -> Vec<DrawItem> {
        let mut out = Vec::new();
        for e in chars {
            let Some(m) = self.mounts.get(&e.cid).and_then(|m| m.model.as_ref()) else { continue };
            if !e.is_player && char_distance.is_some_and(|d| e.position.distance(eye) > d) {
                continue;
            }
            let world = mount_world(e.model_matrix(), m.scale);
            for (mesh, local) in &m.meshes {
                out.push(DrawItem { mesh: mesh.clone(), model: world * *local, tint: [1.0; 4], character: true });
            }
        }
        out
    }

    fn mount(&mut self, assets: &Assets, cid: i32, tid: i32) {
        if self.mounts.get(&cid).is_some_and(|m| m.tid == tid) {
            return;
        }
        log::info!("monture : personnage {cid} sur le modèle {tid} (CID local {})", LOCAL_CID_BASE + cid.rem_euclid(1_000_000));
        let _ = self.data(assets);
        self.mounts.insert(cid, Mount { tid, model: None, failed: false });
    }

    /// Monture d'après l'objet porté en partie 14 (`SubType`).
    fn mount_from_item(&mut self, assets: &Assets, cid: i32, item_tid: Option<i32>) -> bool {
        let row = self.data(assets).item(item_tid);
        if (row.kind != KIND_RIDE && row.part != PART_RIDE) || row.sub_type <= 0 {
            return false;
        }
        self.mount(assets, cid, row.sub_type as i32);
        true
    }
}

/// Objet consommable refusé à cheval ou métamorphosé (FUN_14053d0b0) : texte `sysstr`. Les potions d'usage 12
/// sont permises à l'ArchLord (`FUN_1402944c0`, nom comparé à celui d'AgpmArchlord).
fn refused_item(d: &RideData, tid: Option<i32>, archlord: bool) -> Option<u32> {
    let row = d.item(tid);
    match (row.item_type, row.kind_type, row.sub_type) {
        (1, 7, _) => Some(346),
        (1, 12, _) if archlord => None,
        (1, 12, 1) => Some(347),
        (1, 12, 2) => Some(348),
        _ => None,
    }
}

/// Clic droit sur une potion à cheval ou métamorphosé : refusé avec le message d'Archonia. Avant
/// `app_items::panel`.
pub fn item_requests(glue: &mut RideGlue, items: &HashMap<i32, ItemRecord>, requests: &mut Vec<ItemUi>, hud: &mut HudState, assets: &Assets, _ui: &mut UiState) {
    let me = glue.me;
    if !glue.riding(me) && !glue.transformed.contains(&me) {
        return;
    }
    let archlord = glue.archlord;
    let d = glue.data(assets);
    requests.retain(|r| {
        let ItemUi::Activate { iid, worn: false } = *r else { return true };
        let Some(id) = items.get(&iid).and_then(|i| refused_item(d, i.template_id, archlord)) else { return true };
        let text = d.sys(id);
        hud.push_system(&text);
        false
    });
}

impl App {
    /// Paquets qui touchent aux montures sans leur être propres : nouvelle connexion, métamorphose,
    /// refus du serveur, personnage sorti de la vue.
    pub(super) fn ride_observe(&mut self, ev: &WorldEvent) {
        match ev {
            WorldEvent::Connected => {
                self.ride.mounts.clear();
                self.ride.flagged.clear();
                self.ride.transformed.clear();
            }
            WorldEvent::Transformed { cid, transformed } => {
                if *transformed {
                    self.ride.transformed.insert(*cid);
                } else {
                    self.ride.transformed.remove(cid);
                }
            }
            WorldEvent::SystemCode(CODE_CANNOT_RIDE_IN_COMBAT) => {
                let text = crate::hud::ui_message(&mut self.hud, &self.assets, "CannotRideWhileCombat");
                self.hud.push_system(&text);
            }
            WorldEvent::CharacterRemoved { cid, .. } if self.player.as_ref().is_none_or(|p| p.cid != *cid) => {
                self.ride.mounts.remove(cid);
                self.ride.flagged.remove(cid);
            }
            _ => {}
        }
    }

    /// Objet monture `iid` d'un personnage (le joueur : son inventaire ; un autre : son équipement vu).
    fn ride_item_tid(&self, cid: i32, iid: i32) -> Option<i32> {
        if self.player.as_ref().is_some_and(|p| p.cid == cid) {
            return self.items.get(&iid).and_then(|i| i.template_id);
        }
        self.entities.iter().find(|e| e.cid == cid)?.equipment.iter().find(|e| e.item_id == iid).map(|e| e.template_id)
    }

    /// Objets portés d'un personnage (TID).
    fn ride_worn(&self, cid: i32) -> Vec<i32> {
        if self.player.as_ref().is_some_and(|p| p.cid == cid) {
            return self.items.values().filter(|i| i.status == Some(STATUS_EQUIP)).filter_map(|i| i.template_id).collect();
        }
        self.entities.iter().find(|e| e.cid == cid).map(|e| e.equipment.iter().map(|q: &EquipmentView| q.template_id).collect()).unwrap_or_default()
    }

    /// Annonces AgpmRide et octet d'état.
    pub(super) fn on_ride_event(&mut self, ev: RideEvent) {
        let assets = self.assets.clone();
        match ev {
            RideEvent::Mounted { cid, iid } => {
                self.ride.flagged.insert(cid);
                let tid = self.ride_item_tid(cid, iid);
                if !self.ride.mount_from_item(&assets, cid, tid) {
                    log::warn!("monture : objet {iid} du personnage {cid} inconnu ou sans monture (TID {tid:?})");
                }
            }
            RideEvent::MountedTid { cid, tid } => {
                self.ride.flagged.insert(cid);
                self.ride.mount(&assets, cid, tid);
            }
            RideEvent::Dismounted { cid } => {
                self.ride.flagged.remove(&cid);
                if self.ride.mounts.remove(&cid).is_some() {
                    log::info!("monture : personnage {cid} descendu");
                }
            }
            RideEvent::State { cid, riding: true } => {
                self.ride.flagged.insert(cid);
            }
            // Le drapeau seul ne retire pas la monture : c'est l'op 4 qui la détache.
            RideEvent::State { cid, riding: false } => {
                self.ride.flagged.remove(&cid);
            }
        }
    }

    /// À chaque image, après la simulation : montures des personnages marqués qui portent un objet
    /// de monture, vitesse sans plafond, modèles chargés, animations et bassin.
    pub(super) fn ride_frame(&mut self, dt: f32) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        self.ride.me = me;
        let alive: HashSet<i32> = self.entities.iter().map(|e| e.cid).chain(std::iter::once(me)).collect();
        self.ride.mounts.retain(|cid, _| alive.contains(cid));
        // Personnage marqué monté dont l'objet de genre 0x20 est connu (FUN_1407553e0).
        let pending: Vec<i32> = self.ride.flagged.iter().copied().filter(|c| !self.ride.mounts.contains_key(c) && alive.contains(c)).collect();
        for cid in pending {
            let worn = self.ride_worn(cid);
            for tid in worn {
                if self.ride.mount_from_item(&assets, cid, Some(tid)) {
                    break;
                }
            }
        }
        if let Some(p) = self.player.as_mut() {
            p.speeds.riding = self.ride.mounts.contains_key(&p.cid);
        }
        for e in self.entities.iter_mut() {
            e.speeds.riding = self.ride.mounts.contains_key(&e.cid);
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        // Type d'animation de chaque cavalier (FUN_1404be4f0) ; sans animateur, course ou attente.
        let mut kinds: HashMap<i32, u32> = self.entities.iter().chain(self.player.as_ref()).map(|e| (e.cid, if Self::is_moving(e) { AnimType::Run } else { AnimType::Wait } as u32)).collect();
        for (cid, (_, a)) in &self.animators {
            if let Some(a) = a {
                kinds.insert(*cid, a.current_type());
            }
        }
        if let (Some(p), Some((_, Some(a)))) = (self.player.as_ref(), self.player_animator.as_ref()) {
            kinds.insert(p.cid, a.current_type());
        }
        for (cid, m) in self.ride.mounts.iter_mut() {
            if m.model.is_none() && !m.failed {
                let files = models_for_tid(&assets, m.tid);
                let loadable = !files.is_empty() && files.iter().all(|f| assets.clump("character", f).is_ok() || assets.patched_model(&format!("character/{f}")).is_some());
                if !loadable {
                    m.failed = true;
                    log::warn!("monture TID {} : modèle introuvable dans le pack character ({files:?})", m.tid);
                    continue;
                }
                let fallback = gpu.marker.clone();
                let seed = ((LOCAL_CID_BASE + cid.rem_euclid(1_000_000)) as u32).wrapping_mul(0x9e37_79b9) | 1;
                // Monture glTF (un seul modèle, glTF) : maillages propres, animations et selle du glTF.
                let gltf_model = match files.as_slice() {
                    [f] => assets.patched_model(&format!("character/{f}")).and_then(|r| r.ok()),
                    _ => None,
                };
                let gltf = gltf_model.map(|pm| GltfMount::new(&mut gpu.renderer, &assets, m.tid, pm, seed));
                let meshes = match &gltf {
                    Some(g) => g.meshes(),
                    None => entity_meshes(&mut gpu.renderer, &assets, m.tid, Head::default(), &fallback).unwrap_or_default(),
                };
                // La monture n'est pas un joueur : ensemble 0.
                let animator = Animator::new(&assets, m.tid, &meshes, seed, 0, false);
                let height = column(&assets, "charactertemplateclient.ini", m.tid, "HEIGHT").unwrap_or(200.0);
                let scale = column(&assets, "charactertemplateclient.ini", m.tid, "SCALE").filter(|s| *s > 0.0).unwrap_or(1.0);
                let saddle_bind = animator.as_ref().and_then(|a| a.node_matrix(NODE_SADDLE));
                log::info!("monture TID {} : {} maillage(s), animée : {}, selle : {}, glTF : {}", m.tid, meshes.len(), animator.is_some(), saddle_bind.is_some(), gltf.as_ref().map(|g| format!("{} animation(s), selle {}", g.model.model.animations.len(), g.ride_node.is_some())).unwrap_or_else(|| "non".into()));
                m.model = Some(MountModel { meshes, animator, scale, saddle_bind, height, gltf });
            }
            let Some(model) = m.model.as_mut() else { continue };
            let kind = kinds.get(cid).copied().unwrap_or(AnimType::Wait as u32);
            if let Some(a) = model.animator.as_mut() {
                a.follow(&assets, &gpu.renderer, dt, kind, 30.0);
            }
            if let Some(g) = model.gltf.as_mut() {
                g.follow(&gpu.renderer, dt, kind, 30.0);
                model.meshes = g.meshes();
            }
        }
    }

    /// `RIDER_HEIGHT` de la monture du personnage, ajouté à la hauteur de son panneau.
    pub(super) fn ride_board_height(&mut self, cid: i32) -> f32 {
        let Some(tid) = self.ride.mounts.get(&cid).map(|m| m.tid) else { return 0.0 };
        let assets = self.assets.clone();
        let d = self.ride.data.get_or_insert_with(|| load_data(&assets));
        d.heights
            .entry(tid)
            .or_insert_with(|| (column(&assets, "charactertemplateclient.ini", tid, "HEIGHT").unwrap_or(0.0), column(&assets, "charactertemplateclient.ini", tid, "RIDER_HEIGHT").unwrap_or(0.0)))
            .1
    }

    /// Avant le lancement d'une compétence à cheval : `auto_attack` seulement (`sysstr` 320), pas de
    /// production (`sysstr` 527). Rend `false` si elle ne doit pas partir.
    pub(super) fn ride_allows_skill(&mut self, tid: i32) -> bool {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if !self.ride.riding(me) {
            return true;
        }
        let assets = self.assets.clone();
        let d = self.ride.data(&assets);
        let (auto_attack, product) = d.skills.get(&tid).copied().unwrap_or((false, false));
        let id = match (auto_attack, product) {
            (true, _) => return true,
            (false, true) => 527,
            (false, false) => 320,
        };
        let text = d.sys(id);
        self.hud.push_system(&text);
        false
    }

    /// `/monture` : état des montures en vue (essais).
    pub(super) fn ride_slash(&mut self, cmd: &str) -> bool {
        if !matches!(cmd, "/monture" | "/mount") {
            return false;
        }
        let lines: Vec<String> = self
            .ride
            .mounts
            .iter()
            .map(|(cid, m)| format!("[monture] personnage {cid} : modèle {}{}", m.tid, if m.model.is_some() { "" } else { " (non affiché)" }))
            .collect();
        if lines.is_empty() {
            self.ui.push_chat("[monture] Personne n'est monté en vue.");
        }
        for l in lines {
            self.ui.push_chat(l);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cheval blanc 567 en pose de liaison : sans cadre 0, la selle (nœud 20) est au-dessus du dos et le
    /// cavalier, après `Ry(90°) × Rz(90°)`, se tient droit et regarde vers l'avant du cheval (+Z).
    #[test]
    fn cavalier_sur_la_selle_du_cheval() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let horse = assets.clump("character", "a0000237.ecl").unwrap();
        let world = horse.world_matrices_rooted();
        let frame = horse.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == NODE_SADDLE)).unwrap();
        let rider = rider_on_saddle(mount_world(Mat4::IDENTITY, 1.0) * world[frame]);
        assert!(rider.y_axis.truncate().normalize().y > 0.95, "cavalier droit : {:?}", rider.y_axis);
        assert!(rider.z_axis.truncate().normalize().z > 0.95, "cavalier vers l'avant : {:?}", rider.z_axis);
        let seat = rider.w_axis.truncate();
        assert!(seat.y > 100.0 && seat.y < 180.0 && seat.x.abs() < 10.0, "selle : {seat:?}");
        // Avec le cadre 0 (ancien calcul), la selle était décalée et tournée.
        let old = horse.world_matrices()[frame];
        assert!((old.w_axis.truncate() - seat).length() > 10.0);
        // Selle tournée, échelle hors de [0,5 ; 2] : ramenée à 1.
        let big = rider_on_saddle(Mat4::from_scale(Vec3::splat(3.0)));
        assert!((big.x_axis.truncate().length() - 1.0).abs() < 1e-5);
        assert!((rider_on_saddle(Mat4::from_scale(Vec3::splat(1.5))).x_axis.truncate().length() - 1.5).abs() < 1e-5);
    }

    #[test]
    fn potions_refusees_a_cheval() {
        let mut d = RideData::default();
        d.items.insert(1, ItemRow { item_type: 1, kind_type: 7, ..Default::default() });
        d.items.insert(596, ItemRow { item_type: 1, kind_type: 12, sub_type: 1, ..Default::default() });
        d.items.insert(3, ItemRow { item_type: 1, kind_type: 12, sub_type: 2, ..Default::default() });
        d.items.insert(4, ItemRow { item_type: 1, kind_type: 0, ..Default::default() });
        assert_eq!(refused_item(&d, Some(1), false), Some(346));
        assert_eq!(refused_item(&d, Some(596), false), Some(347));
        assert_eq!(refused_item(&d, Some(3), false), Some(348));
        assert_eq!(refused_item(&d, Some(4), false), None);
        // L'ArchLord : potions d'usage 12 permises, pas la potion de métamorphose.
        assert_eq!((refused_item(&d, Some(1), true), refused_item(&d, Some(596), true), refused_item(&d, Some(3), true)), (Some(346), None, None));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : White Horse (3002) → monture 567, dont le
    /// modèle `a0000237.ecl` et les animations existent ; l'archer humain (TID 1) est de classe 2.
    #[test]
    fn white_horse_avec_le_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        let horse = d.item(Some(3002));
        assert_eq!((horse.kind, horse.part, horse.sub_type), (KIND_RIDE, PART_RIDE, 567));
        assert_eq!(models_for_tid(&assets, 567), vec!["a0000237.ecl".to_string()]);
        assert!(assets.clump("character", "a0000237.ecl").is_ok());
        assert_eq!(column(&assets, "charactertemplateclient.ini", 567, "RIDER_HEIGHT"), Some(270.0));
        assert_eq!(column(&assets, "charactertemplateanimation.ini", 1, "ANIMATION_TYPE2"), Some(2.0));
        let set = crate::animation::AnimationSet::for_tid(&assets, 1, 4, true);
        assert!(set.by_type.get(&0).is_some_and(|v| v.iter().any(|x| x.file == "a0001040.ean")));
        assert!(d.skills.values().any(|(auto, _)| *auto), "Normal Attack : auto_attack");
    }

    /// Avec les données du client : monture glTF 5748 (`a0001674.glb`) — les variantes de
    /// `charactertemplateanimation.ini` nomment ses animations (attente IDLE ou ROAR, course RUN),
    /// le nœud `Saddle_20` porte le cavalier : selle à ≈ 281 unités, cavalier droit.
    #[test]
    fn monture_gltf_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        assert_eq!(models_for_tid(&assets, 5748), vec!["a0001674.ecl".to_string()]);
        let pm = assets.patched_model("character/a0001674.ecl").unwrap().unwrap();
        let set = crate::animation::AnimationSet::for_tid(&assets, 5748, 0, false);
        let name = |kind: u32, roll: f32| set.pick(kind, roll).and_then(|v| pm.model.animation(&v.file)).map(|i| pm.model.animations[i].name.clone());
        assert_eq!(name(AnimType::Wait as u32, 0.0).as_deref(), Some("IDLE"));
        assert_eq!(name(AnimType::Wait as u32, 0.99).as_deref(), Some("ROAR"));
        assert_eq!(name(AnimType::Run as u32, 0.0).as_deref(), Some("RUN"));
        let ride = pm.model.hanim_node(NODE_SADDLE).unwrap();
        let rest = pm.model.node_matrices(None, 0.0);
        let rider = rider_on_saddle(mount_world(Mat4::IDENTITY, 1.0) * pm.model.skeleton_space() * rest[ride]);
        let seat = rider.w_axis.truncate();
        assert!((seat.y - 281.0).abs() < 1.0, "selle : {seat:?}");
        assert!(rider.y_axis.truncate().normalize().y > 0.9, "cavalier droit : {:?}", rider.y_axis);
    }
}
