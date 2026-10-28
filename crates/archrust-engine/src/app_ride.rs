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
//! * cavalier : animations « à cheval » (FUN_140756060) selon `ANIMATION_TYPE2` du gabarit et le `Type`
//!   de l'arme montée : guerrier (1) 7 / lance 8 / étendard 9 ; archer et mage (2, 3) 4 / 5 / 7 ;
//!   Archlord (5) 1 / 1 / 2 ; autres classes : animations ordinaires ;
//! * vitesse : facteur MovementFast, sans le plafond de 60 000 à cheval (FUN_14017a850) ;
//! * refus du client à cheval : compétences sans `auto_attack` (`sysstr` 320, FUN_14069a0c0), production
//!   (`sysstr` 527, FUN_140657410) ; à cheval ou métamorphosé : potion de métamorphose (usage 7, `sysstr`
//!   346), potions d'usage 12 de sous-type 1 (`sysstr` 347) et 2 (`sysstr` 348), FUN_14053d0b0 ;
//! * refus du serveur : message système code 24 → `CannotRideWhileCombat` (uimessage).
//!
//! **Déduit, à vérifier en jeu** : la position du cavalier. Archonia accroche son cadre à celui de la
//! monture (`ac+0x60`, calcul non relevé) ; ici le cavalier est posé sur la translation de la première
//! piste animée de la monture (bassin : 137 unités pour le cheval 567), son animation « à cheval »
//! plaçant son propre bassin 34,5 unités au-dessus de son origine (`a0001040`). `RIDER_HEIGHT`
//! (270 pour 567) ne sert qu'à hausser le panneau du nom (voir `app_boards.rs`).
//! Les montures récentes au modèle glTF (`IDLE`, `RUN`…, nœud `RideNode`, ex. TID 5808) ne sont pas
//! affichées : le pack `character` n'a pas de `.ecl` pour elles et ArchRust ne lit pas encore les glTF
//! animés du client.

use super::App;
use crate::animation::Animator;
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

/// Genre (`Kind`) des montures et partie des armes montées.
const KIND_RIDE: i64 = 32;
const PART_RIDE: i64 = 14;
const PART_LANCER: i64 = 15;
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
    /// TID de personnage → `ANIMATION_TYPE2`.
    type2: HashMap<i32, i64>,
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

/// Ensemble d'animations « à cheval » du cavalier (FUN_140756060), `None` pour une classe sans
/// animation à cheval (0, 4 et les autres : animations ordinaires).
pub(super) fn ride_anim_equip(type2: i64, weapon_type: Option<i64>) -> Option<u32> {
    let set = match (type2, weapon_type) {
        (1, Some(17)) => 8,
        (1, Some(20)) => 9,
        (1, _) => 7,
        (2 | 3, Some(17)) => 5,
        (2 | 3, Some(20)) => 7,
        (2 | 3, _) => 4,
        (5, Some(20)) => 2,
        (5, _) => 1,
        _ => return None,
    };
    Some(set)
}

/// Monture affichée : maillages, animation et bassin courant (où s'assoit le cavalier).
struct MountModel {
    meshes: Vec<(Arc<Mesh>, Mat4)>,
    animator: Option<Animator>,
    saddle: Vec3,
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
}

impl RideGlue {
    fn data(&mut self, assets: &Assets) -> &RideData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Le personnage `cid` est sur une monture.
    pub fn riding(&self, cid: i32) -> bool {
        self.mounts.contains_key(&cid)
    }

    /// Matrice du cavalier `cid` (`base` : sa position et son orientation) : posé sur le bassin de
    /// sa monture.
    pub fn rider_matrix(&self, cid: i32, base: Mat4) -> Mat4 {
        match self.mounts.get(&cid).and_then(|m| m.model.as_ref()) {
            Some(m) => base * Mat4::from_translation(m.saddle),
            None => base,
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
            for (mesh, local) in &m.meshes {
                out.push(DrawItem { mesh: mesh.clone(), model: e.model_matrix() * *local, tint: [1.0; 4], character: true });
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

/// Objet consommable refusé à cheval ou métamorphosé (FUN_14053d0b0) : texte `sysstr`.
fn refused_item(d: &RideData, tid: Option<i32>) -> Option<u32> {
    let row = d.item(tid);
    match (row.item_type, row.kind_type, row.sub_type) {
        (1, 7, _) => Some(346),
        (1, 12, 1) => Some(347),
        (1, 12, 2) => Some(348),
        _ => None,
    }
}

/// Clic droit sur une potion à cheval ou métamorphosé : refusé avec le message d'Archonia. Avant
/// `app_items::panel`.
pub fn item_requests(glue: &mut RideGlue, items: &HashMap<i32, ItemRecord>, requests: &mut Vec<ItemUi>, hud: &mut HudState, assets: &Assets, ui: &mut UiState) {
    let me = glue.me;
    if !glue.riding(me) && !glue.transformed.contains(&me) {
        return;
    }
    let d = glue.data(assets);
    requests.retain(|r| {
        let ItemUi::Activate { iid, worn: false } = *r else { return true };
        let Some(id) = items.get(&iid).and_then(|i| refused_item(d, i.template_id)) else { return true };
        let text = d.sys(id);
        hud.push_system(&text);
        ui.push_chat(format!("[monture] {text}"));
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
                self.ui.push_chat(format!("[monture] {text}"));
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
        let moving: HashMap<i32, bool> = self.entities.iter().chain(self.player.as_ref()).map(|e| (e.cid, Self::is_moving(e))).collect();
        for (cid, m) in self.ride.mounts.iter_mut() {
            if m.model.is_none() && !m.failed {
                let files = models_for_tid(&assets, m.tid);
                let loadable = !files.is_empty() && files.iter().all(|f| assets.clump("character", f).is_ok());
                if !loadable {
                    m.failed = true;
                    log::warn!("monture TID {} : modèle introuvable dans le pack character ({files:?}) ; monture glTF non prise en charge", m.tid);
                    continue;
                }
                let fallback = gpu.marker.clone();
                let meshes = entity_meshes(&mut gpu.renderer, &assets, m.tid, Head::default(), &fallback).unwrap_or_default();
                let seed = ((LOCAL_CID_BASE + cid.rem_euclid(1_000_000)) as u32).wrapping_mul(0x9e37_79b9) | 1;
                let animator = Animator::new(&assets, m.tid, &meshes, seed, 0);
                let height = column(&assets, "charactertemplateclient.ini", m.tid, "HEIGHT").unwrap_or(200.0);
                log::info!("monture TID {} : {} maillage(s), animée : {}", m.tid, meshes.len(), animator.is_some());
                m.model = Some(MountModel { meshes, animator, saddle: Vec3::new(0.0, height * 0.6, 0.0) });
            }
            let Some(model) = m.model.as_mut() else { continue };
            if let Some(a) = model.animator.as_mut() {
                // La monture court quand son cavalier se déplace (pas d'animation de marche).
                a.update(&assets, &gpu.renderer, dt, moving.get(cid).copied().unwrap_or(false), true, 30.0);
                if let Some(t) = a.track_translation(1) {
                    model.saddle = t;
                }
            }
        }
    }


    /// Ensemble d'animations du cavalier `cid` (modèle `tid`) s'il est monté.
    pub(super) fn ride_anim_equip(&mut self, cid: i32, tid: i32) -> Option<u32> {
        if !self.ride.riding(cid) {
            return None;
        }
        let assets = self.assets.clone();
        let worn = self.ride_worn(cid);
        let d = self.ride.data(&assets);
        let lance = worn.iter().map(|t| d.item(Some(*t))).find(|r| r.part == PART_LANCER && r.kind == 2).map(|r| r.kind_type);
        let cached = d.type2.get(&tid).copied();
        let type2 = cached.unwrap_or_else(|| column(&assets, "charactertemplateanimation.ini", tid, "ANIMATION_TYPE2").unwrap_or(0.0) as i64);
        if let Some(d) = self.ride.data.as_mut() {
            d.type2.insert(tid, type2);
        }
        ride_anim_equip(type2, lance)
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
        self.ui.push_chat(format!("[monture] {text}"));
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

    #[test]
    fn animations_a_cheval_comme_archonia() {
        // FUN_140756060 : classe d'animation × arme montée (17 lance, 20 étendard).
        assert_eq!(ride_anim_equip(1, None), Some(7));
        assert_eq!(ride_anim_equip(1, Some(17)), Some(8));
        assert_eq!(ride_anim_equip(1, Some(20)), Some(9));
        assert_eq!(ride_anim_equip(2, None), Some(4));
        assert_eq!(ride_anim_equip(3, Some(17)), Some(5));
        assert_eq!(ride_anim_equip(2, Some(20)), Some(7));
        assert_eq!(ride_anim_equip(5, Some(17)), Some(1));
        assert_eq!(ride_anim_equip(5, Some(20)), Some(2));
        assert_eq!(ride_anim_equip(0, Some(17)), None);
        assert_eq!(ride_anim_equip(4, None), None);
    }

    #[test]
    fn potions_refusees_a_cheval() {
        let mut d = RideData::default();
        d.items.insert(1, ItemRow { item_type: 1, kind_type: 7, ..Default::default() });
        d.items.insert(596, ItemRow { item_type: 1, kind_type: 12, sub_type: 1, ..Default::default() });
        d.items.insert(3, ItemRow { item_type: 1, kind_type: 12, sub_type: 2, ..Default::default() });
        d.items.insert(4, ItemRow { item_type: 1, kind_type: 0, ..Default::default() });
        assert_eq!(refused_item(&d, Some(1)), Some(346));
        assert_eq!(refused_item(&d, Some(596)), Some(347));
        assert_eq!(refused_item(&d, Some(3)), Some(348));
        assert_eq!(refused_item(&d, Some(4)), None);
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
        let set = crate::animation::AnimationSet::for_tid(&assets, 1, 4);
        assert!(set.by_type.get(&0).is_some_and(|v| v.iter().any(|x| x.file == "a0001040.ean")));
        assert!(d.skills.values().any(|(auto, _)| *auto), "Normal Attack : auto_attack");
    }
}
