//! Métamorphose (potions d'usage 7, compétences « transform_target ») et évolution du Dragon Scion côté moteur.
//! Protocole : `archrust_netclient::morph` ; icônes : `hud_wanted.rs` (`draw_system_grid_icon`).
//!
//! Comportement relevé dans Archonia (AgpmCharacter, AgcmCharacter, AgcmUICharacter ; sha256 e9c7609b…) :
//! * op 11 (`FUN_140173f10` → `FUN_1401698d0`) : forme au gabarit inconnu ignorée ; même TID : rien (l'état
//!   métamorphosé reste celui que donne le TID d'origine sauvé) ; sinon TID, visage et coiffure d'origine sauvés
//!   **une fois** (`+0x608…+0x610`), modèle, animations et objets portés rechargés ;
//! * op 12 (`FUN_140169550`) : seulement métamorphosé ; TID, visage et coiffure d'origine rétablis puis oubliés ;
//! * op 26 / 27 : refusées pour un TID nul ou inconnu, une évolution déjà faite (ou absente pour 27) ou le même
//!   TID ; race et classe du nouveau gabarit recopiées dans les facteurs (`FUN_14019e640`, type 2) ; objets portés
//!   par `ItemEvolutionTable.txt` (voir `scene::evolution_item`) ;
//! * champ 18 de l'op 0 : bit 0 métamorphosé, bit 2 évolué (pas de TID d'origine connu dans ce cas) ;
//! * icônes de `UI_SystemMessageGrid` pour soi, pas pour l'ArchLord (`FUN_1404fdda0`, `FUN_1404fdbe0`) :
//!   `Cancel_Transform.png` (clic → op 13, `FUN_1404b71e0`) et `UI_MainGrid_BtnRestoreEvolution.png` (clic →
//!   AgpmSkill op 7 action 4 sur la compétence d'évolution dont l'effet est actif, `FUN_1404b6ee0`) ; infobulle
//!   `sysstr` 665 ; retirer l'icône de métamorphose remet le compte à rebours à zéro ;
//! * compte à rebours (`FUN_1405006b0`, `FUN_1404fd3e0`) : à l'op 4 d'une potion d'usage 7, `Transform_Remain_Notice`
//!   (durée en minutes), puis chaque minute tant que le joueur est métamorphosé ou évolué, `Transform_Restore_Notice`
//!   à l'échéance ;
//! * AgpmItem op 27 : messages `UseItemFailedBySiegewar`, `UseItemCannotTransformCreature`,
//!   `UseItemCannotTransformPotion` (code 3 : fenêtre au sens inconnu, ignoré) ; ops 19 / 20 : recharge commune des
//!   potions de métamorphose (10 min), affichée sur la barre de raccourcis ;
//! * refus du client métamorphosé, sauf l'ArchLord (`FUN_1402944c0`) : compétences (`FUN_14069a0c0`, sans
//!   message), dialogue des PNJ (`FUN_140574e80`, sans message), passeports (`Item_TryUseTelScrollOnTransform`,
//!   `FUN_14053d0b0`) ; coiffeur `CustomizeNeedRestoreTransform` métamorphosé **ou évolué** (`app_customize.rs`) ;
//!   DragonScion d'une autre classe que Scion : objet d'oubli des compétences (usage 27) refusé
//!   (`IsEvolutionNotUseInitSkillItem`). Potions 346 à 348 : voir `app_ride.rs` (inventaire) et
//!   [`refused_item`] (barre de raccourcis).
//!
//! Écarts : le rappel 0x35 de l'op 13 reçue et le champ `+0xb38` qui bloque l'op 26 ne sont pas reproduits ; les
//! actions sociales n'existent pas encore dans ArchRust ; l'infobulle des icônes n'a que le texte 665.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi};
use crate::scene::{entity_meshes_with_equipment, Head};
use crate::ui::UiState;
use archrust_netclient::morph::{use_result, MorphCommand, MorphEvent};
use archrust_netclient::skills::SkillCommand;
use archrust_netclient::world::{factor_part, CharacterStats, ItemRecord};
use archrust_netclient::{Command, WorldEvent};
use egui::Color32;
use glam::Mat4;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Usages (`Type` d'itemdatatable, objets d'`ItemType` 1) relevés par `FUN_14053d0b0`.
const USE_TELEPORT_SCROLL: i64 = 4;
const USE_TRANSFORM: i64 = 7;
const USE_SPEED_POTION: i64 = 12;
const USE_SKILL_INIT: i64 = 27;
/// `transform_target` de SKILL_SPEC des compétences d'évolution (bits `0x8000000001000`, `FUN_1402fef60`).
const TRANSFORM_TARGET_EVOLUTION: i64 = 5;
/// Intervalle des avis du compte à rebours.
const NOTICE_EVERY: Duration = Duration::from_secs(60);
const TEXTURE_TRANSFORM: &str = "Cancel_Transform.png";
const TEXTURE_EVOLUTION: &str = "UI_MainGrid_BtnRestoreEvolution.png";

/// État d'un personnage (`AgpdCharacter` : `+0x600` métamorphosé, `+0x604` évolué, `+0x608…+0x610` origine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct MorphState {
    pub transformed: bool,
    pub evolved: bool,
    /// TID, visage et coiffure d'avant la première métamorphose (op 11).
    pub origin: Option<(i32, Head)>,
}

impl MorphState {
    /// Op 11 : nouveau TID à poser, ou rien. `known` : gabarit de la forme connu du client.
    pub fn transform(&mut self, current: i32, head: Head, tid: i32, known: bool) -> Option<i32> {
        if !known {
            return None;
        }
        if tid == current {
            // FUN_1401698d0 échoue : +0x600 = (+0x608 ≠ 0).
            self.transformed = self.origin.is_some();
            return None;
        }
        if self.origin.is_none() {
            self.origin = Some((current, head));
        }
        self.transformed = true;
        Some(tid)
    }

    /// Op 12 : TID et tête d'origine à rétablir, ou rien (l'état métamorphosé est levé dans les deux cas).
    pub fn restore(&mut self, current: i32) -> Option<(i32, Head)> {
        if !self.transformed {
            return None;
        }
        self.transformed = false;
        match self.origin {
            Some((tid, head)) if tid != 0 && tid != current => {
                self.origin = None;
                Some((tid, head))
            }
            _ => None,
        }
    }

    /// Op 26 : TID évolué à poser, ou rien.
    pub fn evolve(&mut self, current: i32, tid: i32, known: bool) -> Option<i32> {
        if tid == 0 || !known || self.evolved || tid == current {
            return None;
        }
        self.evolved = true;
        Some(tid)
    }

    /// Op 27 : TID à rétablir, ou rien.
    pub fn restore_evolution(&mut self, current: i32, tid: i32, known: bool) -> Option<i32> {
        if tid == 0 || !known || !self.evolved || tid == current {
            return None;
        }
        self.evolved = false;
        Some(tid)
    }
}

/// Avis du compte à rebours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notice {
    /// `Transform_Remain_Notice` : minutes restantes.
    Remain(u64),
    /// `Transform_Restore_Notice`.
    Restore,
}

/// Compte à rebours d'AgcmUICharacter (`+0x26b8` fin, `+0x26c0` restant, `+0x26c8` prochain avis).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Countdown {
    end: Instant,
    remain_ms: i64,
    next: Instant,
}

impl Countdown {
    /// `FUN_1405006b0` : départ, avec le premier avis.
    fn start(now: Instant, duration_ms: u64) -> (Self, Notice) {
        let c = Countdown { end: now + Duration::from_millis(duration_ms), remain_ms: duration_ms as i64, next: now + NOTICE_EVERY };
        (c, Notice::Remain(duration_ms / 60_000))
    }

    /// `FUN_1404fd3e0` : avis de cette image ; `None` dans le second membre : compte à rebours fini.
    fn tick(mut self, now: Instant, active: bool) -> (Option<Self>, Option<Notice>) {
        if !active {
            return (Some(self), None);
        }
        if now >= self.end {
            return (None, Some(Notice::Restore));
        }
        if self.next <= now {
            self.remain_ms -= 60_000;
            self.next = now + NOTICE_EVERY;
            return (Some(self), Some(Notice::Remain(self.remain_ms.max(0) as u64 / 60_000)));
        }
        (Some(self), None)
    }
}

/// Colonnes utiles d'itemdatatable.txt : `ItemType`, `Type`, `SubType`, `Polymorph Dur`.
#[derive(Debug, Clone, Copy, Default)]
struct ItemRow {
    item_type: i64,
    use_type: i64,
    sub_type: i64,
    duration_ms: u64,
}

#[derive(Default)]
struct MorphData {
    items: HashMap<i32, ItemRow>,
    /// Compétences d'évolution (SKILL_SPEC `transform_target` 5).
    evolution_skills: HashSet<i32>,
    sysstr: HashMap<u32, String>,
}

impl MorphData {
    fn item(&self, tid: Option<i32>) -> ItemRow {
        tid.and_then(|t| self.items.get(&t).copied()).unwrap_or_default()
    }
}

fn load_data(assets: &Assets) -> MorphData {
    let mut d = MorphData::default();
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, item_type, use_type, sub_type, dur) = (col("TID"), col("ItemType"), col("Type"), col("SubType"), col("Polymorph Dur"));
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let num = |c: Option<usize>| c.and_then(|i| cells.get(i)).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
            let t = num(tid) as i32;
            if t > 0 {
                d.items.insert(t, ItemRow { item_type: num(item_type), use_type: num(use_type), sub_type: num(sub_type), duration_ms: num(dur).max(0) as u64 });
            }
        }
    }
    // SKILL_SPEC.txt (par nom) → TID par SKILLTEMPLATE.ini.
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
        let transform = header.iter().position(|h| h.eq_ignore_ascii_case("transform_target"));
        for row in rows {
            let value = transform.and_then(|i| row.get(i)).and_then(|v| v.parse::<i64>().ok());
            if value == Some(TRANSFORM_TARGET_EVOLUTION) {
                d.evolution_skills.extend(row.first().and_then(|n| by_name.get(*n)).into_iter().flatten());
            }
        }
    }
    for id in [346, 347, 348, 665] {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    log::info!("métamorphose : {} potion(s), {} compétence(s) d'évolution", d.items.values().filter(|i| i.item_type == 1 && i.use_type == USE_TRANSFORM).count(), d.evolution_skills.len());
    d
}

/// Objet refusé par le client (`FUN_14053d0b0`) : clé d'uimessage (`Err`) ou numéro de `sysstr` (`Ok`).
/// `riding_cases` : potions 346 à 348, que l'inventaire laisse à `app_ride.rs`.
fn refused_item(row: ItemRow, transformed: bool, archlord: bool, race_class: Option<(i32, i32)>, riding_cases: bool) -> Option<Result<u32, &'static str>> {
    if row.item_type != 1 {
        return None;
    }
    match row.use_type {
        USE_TRANSFORM if riding_cases && transformed => Some(Ok(346)),
        USE_SPEED_POTION if riding_cases && transformed && !archlord => match row.sub_type {
            1 => Some(Ok(347)),
            2 => Some(Ok(348)),
            _ => None,
        },
        USE_TELEPORT_SCROLL if transformed && !archlord => Some(Err("Item_TryUseTelScrollOnTransform")),
        USE_SKILL_INIT if matches!(race_class, Some((4, class)) if class != 3) => Some(Err("IsEvolutionNotUseInitSkillItem")),
        _ => None,
    }
}

/// Clics sur les icônes, traités par `App::morph_frame`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    Transform,
    Evolution,
}

#[derive(Default)]
pub struct MorphGlue {
    data: Option<MorphData>,
    states: HashMap<i32, MorphState>,
    /// CID du joueur.
    me: i32,
    /// Le joueur est l'ArchLord (voir `app_archlord.rs`), relevé à chaque image.
    archlord: bool,
    /// TID de la compétence d'évolution dont l'effet (AgpmSkill op 4) est actif sur le joueur.
    evolution_effect: Option<i32>,
    countdown: Option<Countdown>,
    /// Fin de la recharge commune des potions de métamorphose (AgpmItem op 19) et sa durée.
    reuse_end: Option<Instant>,
    clicks: Vec<Click>,
}

impl MorphGlue {
    fn data(&mut self, assets: &Assets) -> &MorphData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    pub fn transformed(&self, cid: i32) -> bool {
        self.states.get(&cid).is_some_and(|s| s.transformed)
    }

    pub fn evolved(&self, cid: i32) -> bool {
        self.states.get(&cid).is_some_and(|s| s.evolved)
    }
}

/// Infobulle des deux icônes : `sysstr` 665 « Cancel Metamorphosis ».
fn tooltip(glue: &mut MorphGlue, assets: &Assets) -> Vec<(String, Color32)> {
    let text = glue.data(assets).sysstr.get(&665).cloned().filter(|t| !t.is_empty()).unwrap_or_else(|| "Cancel Metamorphosis".into());
    vec![(text, Color32::WHITE)]
}

/// Icônes d'annulation dans `UI_SystemMessageGrid`, après l'avis de recherche (`wanted_shown`) s'il est affiché.
pub fn panel(ctx: &egui::Context, glue: &mut MorphGlue, wanted_shown: bool, hud: &mut HudState, assets: &Assets) {
    if glue.me == 0 || glue.archlord {
        return;
    }
    let mut index = wanted_shown as usize;
    let tip = tooltip(glue, assets);
    for (on, texture, click) in [(glue.transformed(glue.me), TEXTURE_TRANSFORM, Click::Transform), (glue.evolved(glue.me), TEXTURE_EVOLUTION, Click::Evolution)] {
        if !on {
            continue;
        }
        if crate::hud::draw_system_grid_icon(ctx, hud, assets, index, texture, &tip) {
            glue.clicks.push(click);
        }
        index += 1;
    }
}

/// Race et classe du joueur dans ses facteurs (`FACTOR_TYPE` 0 et 2).
fn race_class(hud: &HudState) -> Option<(i32, i32)> {
    let f = &hud.stats.factors;
    Some((*f.get(&(factor_part::TYPE, 0))?, *f.get(&(factor_part::TYPE, 2))?))
}

/// Clic droit sur un objet du sac refusé métamorphosé ou évolué (passeport, objet d'oubli des compétences). Avant
/// `app_items::panel`.
pub fn item_requests(glue: &mut MorphGlue, items: &HashMap<i32, ItemRecord>, requests: &mut Vec<ItemUi>, hud: &mut HudState, assets: &Assets, ui: &mut UiState) {
    let (transformed, archlord, race_class) = (glue.transformed(glue.me), glue.archlord, race_class(hud));
    let d = glue.data(assets);
    requests.retain(|r| {
        let ItemUi::Activate { iid, worn: false } = *r else { return true };
        let row = d.item(items.get(&iid).and_then(|i| i.template_id));
        let Some(refusal) = refused_item(row, transformed, archlord, race_class, false) else { return true };
        let text = match refusal {
            Ok(id) => d.sysstr.get(&id).cloned().unwrap_or_default(),
            Err(key) => crate::hud::ui_message(hud, assets, key),
        };
        hud.push_system(&text);
        ui.push_chat(format!("[métamorphose] {text}"));
        false
    });
}

impl App {
    fn morph_me(&self) -> i32 {
        self.player.as_ref().map_or(0, |p| p.cid)
    }


    /// Le gabarit `tid` est connu du client (`CharacterTemplatePublic.ini` ; sans client, tous).
    fn morph_known(&self, tid: i32) -> bool {
        !self.assets.has_client() || self.assets.table("ini", "CharacterTemplatePublic.ini").is_ok_and(|t| t.section(&tid.to_string()).is_some())
    }

    /// TID et tête courants d'un personnage en vue (le joueur compris).
    fn morph_current(&self, cid: i32) -> Option<(i32, Head)> {
        self.player.iter().chain(self.entities.iter()).find(|e| e.cid == cid).map(|e| (e.tid, e.head))
    }

    /// Pose le modèle `tid` (et la tête d'origine) d'un personnage : corps, tête, objets portés rechargés, animations
    /// reconstruites avec les maillages.
    fn morph_set_model(&mut self, cid: i32, tid: i32, head: Option<Head>) {
        if self.player.as_ref().is_some_and(|p| p.cid == cid) {
            let Some(p) = self.player.as_mut() else { return };
            p.tid = tid;
            if let Some(h) = head {
                p.head = h;
            }
            let (tid, head, worn) = (p.tid, p.head, p.equipment.clone());
            let Some(gpu) = self.gpu.as_mut() else { return };
            let fallback = gpu.player_marker.clone();
            let meshes = entity_meshes_with_equipment(&mut gpu.renderer, &self.assets, tid, head, &worn, &fallback).unwrap_or_else(|err| {
                log::warn!("métamorphose du joueur (TID {tid}) : {err:#}");
                vec![(fallback.clone(), Mat4::IDENTITY)]
            });
            if let Some(p) = self.player.as_mut() {
                p.meshes = meshes;
            }
        } else if let Some(index) = self.entities.iter().position(|e| e.cid == cid) {
            self.entities[index].tid = tid;
            if let Some(h) = head {
                self.entities[index].head = h;
            }
            self.reload_entity_meshes(index);
        }
    }

    /// Race et classe du gabarit recopiées dans les facteurs du joueur (évolution et retour).
    fn morph_copy_type(&mut self, cid: i32, tid: i32) {
        if cid != self.morph_me() {
            return;
        }
        if let Some((race, class)) = crate::scene::template_race_class(&self.assets, tid) {
            let mut s = CharacterStats::default();
            s.factors.insert((factor_part::TYPE, 0), race);
            s.factors.insert((factor_part::TYPE, 2), class);
            self.hud.merge_stats(&s);
        }
    }

    fn morph_message(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.hud.push_system(text);
            self.ui.push_chat(format!("[métamorphose] {text}"));
        }
    }

    /// Paquets qui touchent à la métamorphose sans lui être propres : connexion, effets de compétence, objet
    /// utilisé (compte à rebours), personnage sorti de la vue.
    pub(super) fn morph_observe(&mut self, ev: &WorldEvent) {
        let me = self.morph_me();
        match ev {
            WorldEvent::Connected => {
                // Changement de personnage (`FUN_1405a7180`) : icônes retirées, tout est oublié.
                let data = self.morph.data.take();
                self.morph = MorphGlue { data, ..Default::default() };
            }
            WorldEvent::CharacterRemoved { cid, .. } if *cid != me => {
                self.morph.states.remove(cid);
            }
            WorldEvent::BuffAdded(b) if b.owner == me => {
                let assets = self.assets.clone();
                if self.morph.data(&assets).evolution_skills.contains(&b.skill_tid) {
                    self.morph.evolution_effect = Some(b.skill_tid);
                }
            }
            WorldEvent::BuffRemoved { owner, skill_tid } if *owner == me && self.morph.evolution_effect == Some(*skill_tid) => {
                self.morph.evolution_effect = None;
            }
            WorldEvent::ItemUsed { tid: Some(tid), user, .. } if user.is_none_or(|u| u == me) => {
                let assets = self.assets.clone();
                let row = self.morph.data(&assets).item(Some(*tid));
                if row.item_type == 1 && row.use_type == USE_TRANSFORM {
                    let (c, notice) = Countdown::start(Instant::now(), row.duration_ms);
                    self.morph.countdown = Some(c);
                    self.morph_notice(notice);
                }
            }
            _ => {}
        }
    }

    fn morph_notice(&mut self, notice: Notice) {
        let text = match notice {
            Notice::Remain(min) => super::chat_glue::printf(&crate::hud::ui_message(&mut self.hud, &self.assets, "Transform_Remain_Notice"), &[min.to_string().as_str()]),
            Notice::Restore => crate::hud::ui_message(&mut self.hud, &self.assets, "Transform_Restore_Notice"),
        };
        self.morph_message(&text);
    }

    /// Annonces de `morph`.
    pub(super) fn on_morph_event(&mut self, ev: MorphEvent) {
        let me = self.morph_me();
        match ev {
            MorphEvent::State { cid, transformed, evolved } => {
                let st = self.morph.states.entry(cid).or_default();
                st.transformed = transformed;
                st.evolved = evolved;
            }
            MorphEvent::Transform { cid, tid } => {
                let Some((current, head)) = self.morph_current(cid) else { return };
                let known = self.morph_known(tid);
                let st = self.morph.states.entry(cid).or_default();
                let new = st.transform(current, head, tid, known);
                let transformed = st.transformed;
                if let Some(tid) = new {
                    log::info!("métamorphose : personnage {cid} en TID {tid} (d'origine {current})");
                    self.morph_set_model(cid, tid, None);
                }
                self.on_world_event(WorldEvent::Transformed { cid, transformed });
            }
            MorphEvent::Restore { cid } => {
                let current = self.morph_current(cid).map_or(0, |c| c.0);
                let Some(st) = self.morph.states.get_mut(&cid) else { return };
                let was = st.transformed;
                if let Some((tid, head)) = st.restore(current) {
                    log::info!("fin de la métamorphose : personnage {cid} rendu au TID {tid}");
                    self.morph_set_model(cid, tid, Some(head));
                }
                if was {
                    // Icône retirée : compte à rebours remis à zéro (FUN_1404fdda0).
                    if cid == me {
                        self.morph.countdown = None;
                    }
                    self.on_world_event(WorldEvent::Transformed { cid, transformed: false });
                }
            }
            MorphEvent::Evolve { cid, tid } | MorphEvent::RestoreEvolution { cid, tid } => {
                let evolve = matches!(ev, MorphEvent::Evolve { .. });
                let Some((current, _)) = self.morph_current(cid) else { return };
                let known = self.morph_known(tid);
                let st = self.morph.states.entry(cid).or_default();
                let new = if evolve { st.evolve(current, tid, known) } else { st.restore_evolution(current, tid, known) };
                if let Some(tid) = new {
                    log::info!("{} : personnage {cid} en TID {tid}", if evolve { "évolution" } else { "fin de l'évolution" });
                    self.morph_set_model(cid, tid, None);
                    self.morph_copy_type(cid, tid);
                }
            }
            MorphEvent::UseItemResult { cid, code } if cid == me || cid == 0 => match use_result::message(code) {
                Some(key) => {
                    let text = crate::hud::ui_message(&mut self.hud, &self.assets, key);
                    self.morph_message(&text);
                }
                None => log::info!("objet : refus {code} du serveur (fenêtre d'Archonia non reproduite)"),
            },
            MorphEvent::ReuseTime { cid, remain_ms } if cid == me || cid == 0 => {
                self.morph.reuse_end = Some(Instant::now() + Duration::from_millis(remain_ms));
            }
            MorphEvent::ReuseReset { cid } if cid == me || cid == 0 => self.morph.reuse_end = None,
            MorphEvent::UseItemResult { .. } | MorphEvent::ReuseTime { .. } | MorphEvent::ReuseReset { .. } => {}
        }
    }

    /// À chaque image : CID et titre d'ArchLord du joueur, clics des icônes, compte à rebours.
    pub(super) fn morph_frame(&mut self) {
        let me = self.morph_me();
        self.morph.me = me;
        self.morph.archlord = self.archlord_is_me();
        for click in std::mem::take(&mut self.morph.clicks) {
            match click {
                Click::Transform if self.morph.transformed(me) => {
                    if let Some(net) = &self.net {
                        net.send(Command::Morph(MorphCommand::CancelTransform));
                    }
                }
                // L'effet de la compétence d'évolution (op 4) doit avoir été reçu, sinon rien n'est envoyé.
                Click::Evolution if self.morph.evolved(me) => {
                    let id = self.morph.evolution_effect.and_then(|tid| self.skills_id_of(tid));
                    match (id, &self.net) {
                        (Some(id), Some(net)) => net.send(Command::Skill(SkillCommand::CancelCast { id })),
                        (None, _) => log::info!("évolution : aucun effet de compétence d'évolution actif, annulation non envoyée"),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = self.morph.countdown {
            let active = self.morph.transformed(me) || self.morph.evolved(me);
            let (next, notice) = c.tick(Instant::now(), active);
            self.morph.countdown = next;
            if let Some(n) = notice {
                self.morph_notice(n);
            }
        }
    }

    /// Lancement d'une compétence permis : pas métamorphosé, sauf l'ArchLord (`FUN_14069a0c0`, refus sans message).
    pub(super) fn morph_allows_skill(&self, tid: i32) -> bool {
        let ok = !self.morph.transformed(self.morph_me()) || self.archlord_is_me();
        if !ok {
            log::info!("compétence {tid} : refusée métamorphosé");
        }
        ok
    }

    /// Compétence d'évolution : lancée sur soi (`target` 100 de SKILL_SPEC).
    pub(super) fn morph_is_evolution_skill(&mut self, tid: i32) -> bool {
        let assets = self.assets.clone();
        self.morph.data(&assets).evolution_skills.contains(&tid)
    }

    /// Dialogue des PNJ permis : pas métamorphosé, sauf l'ArchLord (`FUN_140574e80`, retour sans message).
    pub(super) fn morph_allows_npc(&self) -> bool {
        !self.morph.transformed(self.morph_me()) || self.archlord_is_me()
    }

    /// Joueur métamorphosé ou évolué (coiffeur : `CustomizeNeedRestoreTransform`, `FUN_1406bd4b0`).
    pub(super) fn morph_changed_form(&self) -> bool {
        let me = self.morph_me();
        self.morph.transformed(me) || self.morph.evolved(me)
    }

    /// Objet de la barre de raccourcis refusé (mêmes règles que l'inventaire, potions 346 à 348 comprises) : rend
    /// `true` après avoir affiché le message.
    pub(super) fn morph_refuses_item(&mut self, tid: i32) -> bool {
        let assets = self.assets.clone();
        let me = self.morph_me();
        let (transformed, archlord, race_class) = (self.morph.transformed(me), self.archlord_is_me(), race_class(&self.hud));
        let row = self.morph.data(&assets).item(Some(tid));
        let Some(refusal) = refused_item(row, transformed, archlord, race_class, true) else { return false };
        let text = match refusal {
            Ok(id) => self.morph.data(&assets).sysstr.get(&id).cloned().unwrap_or_default(),
            Err(key) => crate::hud::ui_message(&mut self.hud, &assets, key),
        };
        self.morph_message(&text);
        true
    }

    /// Recharge commune d'une potion de métamorphose `tid` pour la barre : part restante et secondes.
    pub(super) fn morph_item_cooldown(&mut self, tid: i32) -> Option<(f32, u32)> {
        let assets = self.assets.clone();
        let row = self.morph.data(&assets).item(Some(tid));
        if row.item_type != 1 || row.use_type != USE_TRANSFORM {
            return None;
        }
        let left = self.morph.reuse_end?.checked_duration_since(Instant::now())?;
        let total = Duration::from_millis(archrust_netclient::morph::TRANSFORM_REUSE_MS);
        Some(((left.as_secs_f32() / total.as_secs_f32()).min(1.0), left.as_secs_f32().ceil() as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: Head = Head { face: 3, hair: 5 };

    #[test]
    fn metamorphose_comme_archonia() {
        let mut s = MorphState::default();
        // Gabarit inconnu : ignoré.
        assert_eq!(s.transform(96, HEAD, 272, false), None);
        assert!(!s.transformed);
        // Origine sauvée à la première forme seulement.
        assert_eq!(s.transform(96, HEAD, 272, true), Some(272));
        assert_eq!(s.transform(272, Head::default(), 456, true), Some(456));
        assert_eq!(s.origin, Some((96, HEAD)));
        // Même TID : rien, reste métamorphosé (origine connue).
        assert_eq!(s.transform(456, Head::default(), 456, true), None);
        assert!(s.transformed);
        // Fin : TID, visage et coiffure d'origine, puis oubliés.
        assert_eq!(s.restore(456), Some((96, HEAD)));
        assert!(!s.transformed && s.origin.is_none());
        // Pas métamorphosé : l'op 12 ne fait rien.
        assert_eq!(s.restore(96), None);
        // Ajouté métamorphosé par l'op 0 (bit 0) : pas d'origine, l'op 12 lève seulement l'état.
        let mut s = MorphState { transformed: true, ..Default::default() };
        assert_eq!(s.restore(272), None);
        assert!(!s.transformed);
    }

    #[test]
    fn evolution_comme_archonia() {
        let mut s = MorphState::default();
        assert_eq!(s.restore_evolution(1724, 1724, true), None, "non évolué");
        assert_eq!(s.evolve(1724, 0, true), None, "TID nul");
        assert_eq!(s.evolve(1724, 1722, false), None, "gabarit inconnu");
        assert_eq!(s.evolve(1724, 1722, true), Some(1722));
        assert_eq!(s.evolve(1722, 1723, true), None, "déjà évolué : retour au Scion d'abord");
        assert_eq!(s.restore_evolution(1722, 1722, true), None, "même TID");
        assert_eq!(s.restore_evolution(1722, 1724, true), Some(1724));
        assert!(!s.evolved);
    }

    #[test]
    fn compte_a_rebours() {
        let t0 = Instant::now();
        let (c, n) = Countdown::start(t0, 240_000);
        assert_eq!(n, Notice::Remain(4));
        // Rien avant la minute ; pas d'avis si le joueur n'est plus métamorphosé.
        assert_eq!(c.tick(t0 + Duration::from_secs(30), true).1, None);
        assert_eq!(c.tick(t0 + Duration::from_secs(90), false), (Some(c), None));
        let (c2, n) = c.tick(t0 + Duration::from_secs(60), true);
        assert_eq!(n, Some(Notice::Remain(3)));
        let (c3, n) = c2.unwrap().tick(t0 + Duration::from_secs(240), true);
        assert_eq!((c3, n), (None, Some(Notice::Restore)));
        // Durée 0 (objet 3909) : fin aussitôt.
        let (c, n) = Countdown::start(t0, 0);
        assert_eq!(n, Notice::Remain(0));
        assert_eq!(c.tick(t0, true), (None, Some(Notice::Restore)));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : potions, compétences d'évolution, gabarits, table
    /// d'évolution des objets, textures des icônes.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        let spider = d.item(Some(406));
        assert_eq!((spider.item_type, spider.use_type, spider.duration_ms), (1, USE_TRANSFORM, 240_000));
        assert_eq!(d.item(Some(198)).use_type, USE_TELEPORT_SCROLL);
        assert_eq!(d.item(Some(4876)).use_type, USE_SKILL_INIT);
        for tid in [1189, 1192, 1195] {
            assert!(d.evolution_skills.contains(&tid), "compétence d'évolution {tid}");
        }
        assert_eq!(d.evolution_skills.len(), 3);
        assert!(d.sysstr.get(&665).is_some_and(|t| !t.is_empty()), "sysstr 665 (« Cancel Metamorphosis »)");
        assert_eq!(crate::scene::template_race_class(&assets, 1722), Some((4, 1)));
        assert_eq!(crate::scene::template_race_class(&assets, 1724), Some((4, 3)));
        assert_eq!(crate::scene::template_race_class(&assets, 1732), Some((4, 4)));
        let evo = crate::scene::parse_evolution_table(&super::super::npc_glue::client_text_in(&assets, "ini", "itemevolutiontable.txt"));
        assert!(evo.len() > 800, "{} lignes", evo.len());
        assert_eq!((crate::scene::evolution_item(&evo, 1, 7829), crate::scene::evolution_item(&evo, 2, 7829)), (6661, 6686));
        assert_eq!(crate::scene::evolution_item(&evo, 4, 7831), 8276);
        let mut hud = HudState::default();
        for key in ["Transform_Remain_Notice", "Transform_Restore_Notice", "Item_TryUseTelScrollOnTransform", "IsEvolutionNotUseInitSkillItem", "UseItemCannotTransformPotion"] {
            assert!(!crate::hud::ui_message(&mut hud, &assets, key).is_empty(), "{key}");
        }
        for t in [TEXTURE_TRANSFORM, TEXTURE_EVOLUTION] {
            assert!(assets.image("texture/ui", &t.to_ascii_lowercase()).is_ok(), "{t}");
        }
    }

    #[test]
    fn objets_refuses() {
        let row = |use_type, sub_type| ItemRow { item_type: 1, use_type, sub_type, duration_ms: 0 };
        // Passeport métamorphosé, sauf l'ArchLord.
        assert_eq!(refused_item(row(4, 0), true, false, None, false), Some(Err("Item_TryUseTelScrollOnTransform")));
        assert_eq!(refused_item(row(4, 0), true, true, None, false), None);
        assert_eq!(refused_item(row(4, 0), false, false, None, false), None);
        // Objet d'oubli : DragonScion d'une autre classe que Scion.
        assert_eq!(refused_item(row(27, 0), false, false, Some((4, 1)), false), Some(Err("IsEvolutionNotUseInitSkillItem")));
        assert_eq!(refused_item(row(27, 0), false, false, Some((4, 3)), false), None);
        assert_eq!(refused_item(row(27, 0), false, false, Some((1, 1)), false), None);
        // Potions de la barre : 346 même pour l'ArchLord, 347 / 348 sauf l'ArchLord.
        assert_eq!(refused_item(row(7, 0), true, true, None, true), Some(Ok(346)));
        assert_eq!(refused_item(row(7, 0), true, true, None, false), None, "inventaire : voir app_ride.rs");
        assert_eq!(refused_item(row(12, 2), true, false, None, true), Some(Ok(348)));
        assert_eq!(refused_item(row(12, 1), true, true, None, true), None);
    }
}
