//! Quêtes données ou reprises par un objet du monde. Protocole : `archrust_netclient::npc`
//! (`ObjectQuestWindow`, `ObjectQuestAccept`, `ObjectQuestComplete`) ; fenêtre d'origine `QuestAccept_UI`
//! (`hud_npc.rs`), état de la fenêtre dans `app_npc.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) et dans le client de Webzen (20009faa…) :
//! * un objet du décor porte l'événement de quête par `Function=27` et `QuestGroupID=<groupe>` dans son
//!   `obj*.ini` (le groupe vaut d'ordinaire le numéro de l'objet) ; ses quêtes sont celles du groupe de
//!   `questgroup.ini`, comme pour un PNJ ;
//! * clic : AGPMEVENTQUEST op 0, EVENT_BASE { 1, numéro de l'objet, -, 27 }, champ 3 = plus grand rayon
//!   des sphères englobantes de l'objet + 50 (FUN_14069f260 ; Webzen 0x6dde80) ; l'accord ouvre
//!   `QuestAccept_UI` avec les quêtes du groupe (FUN_140656590 → FUN_140659ad0) ;
//! * repère de minicarte d'un objet (FUN_14065c620 → FUN_1404f5e80) : type 1 si une quête est proposée,
//!   2 si une quête du journal s'y rend ; aucun repère sinon (le type 0 n'existe que pour les PNJ).
//!
//! * ramassage (FUN_140657410, Webzen FUN_006b2da0), avant la fenêtre : la première entrée « TID-rang » de la
//!   5ᵉ colonne de `questgroup.ini` dont la quête est au journal décide ; objet `ObjectItem<rang>_TID` détenu au
//!   nombre demandé → sysstr 444, sinon barre d'`UI_Cooldown` de 5 s (titre sysstr 331), personnage arrêté, sans
//!   annulation ; à son terme AGPMQUEST op 11 (FUN_140654900) ; sur une monture : sysstr 527 ; une barre en cours :
//!   rien. Sysstr 444 et 527 n'empêchent pas la fenêtre de s'ouvrir.
//!
//! * marques de quête (FUN_14065c620) : effets communs 5 / 6 / 7 d'`ee_common_char.txt` au-dessus des PNJ et des
//!   objets (voir `App::quest_marks_frame`) ;
//! * animation du ramassage (FUN_14065cc70) : compétence « 연금술 » (Alchemy), voir `App::quest_gather_animation`.

use super::App;
use crate::assets::Assets;
use crate::hud::HudState;
use crate::net::NetLink;
use crate::objects::{tile_function_objects, Placement};
use crate::world::{sector_of, tile_of};
use archrust_netclient::npc::{function, NpcCommand};
use archrust_netclient::Command;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Rayon de l'objet ajouté au champ 3 de la demande (`fVar4 + 50.0`).
const REACH_MARGIN: f32 = 50.0;
/// Barre du ramassage (`FUN_140694d50(…, 5000, sysstr 331, …)`).
const GATHER_BAR: Duration = Duration::from_millis(5000);
const SYS_GATHER_TITLE: u32 = 331;
const SYS_ALREADY_HELD: u32 = 444;
const SYS_GET_OFF_MOUNT: u32 = 527;

/// Ramassage en cours : objet du monde, quête, rang, début de la barre et son titre.
#[derive(Debug, Clone)]
pub struct GatherBar {
    object: u32,
    tid: i32,
    rank: usize,
    since: Instant,
    title: String,
}

/// Objet du monde qui porte l'événement de quête.
#[derive(Debug, Clone)]
pub struct QuestObject {
    pub object: u32,
    /// `QuestGroupID` de l'événement (groupe de questgroup.ini).
    pub group: i32,
    pub placement: Placement,
}

#[derive(Default)]
pub struct QuestObjectGlue {
    /// Tuiles déjà lues.
    tiles: HashSet<u32>,
    pub objects: Vec<QuestObject>,
    /// Barre du ramassage (`+0x1884` d'AgcmUIQuest2).
    pub bar: Option<GatherBar>,
    /// Dernière mise à jour des marques de quête.
    marks_at: Option<Instant>,
}

/// Les marques de quête sont recalculées à ce rythme (Archonia les repose à chaque changement du journal).
const MARKS_EVERY: Duration = Duration::from_millis(250);

/// Clé d'effet d'un objet du monde (négative, hors des clés des portails, qui restent sous 2^27).
fn object_key(object: u32) -> i32 {
    -((0x4000_0000 | (object & 0x3fff_ffff)) as i32)
}

/// Barre du ramassage (`UI_Cooldown`) ; à son terme, AGPMQUEST op 11.
pub(super) fn panel(ctx: &egui::Context, glue: &mut QuestObjectGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let Some(bar) = glue.bar.clone() else { return };
    let f = bar.since.elapsed().as_secs_f32() / GATHER_BAR.as_secs_f32();
    crate::hud::draw_cooldown(ctx, hud, assets, f.min(1.0), &bar.title);
    if bar.since.elapsed() < GATHER_BAR {
        ctx.request_repaint();
        return;
    }
    glue.bar = None;
    log::info!("quêtes : ramassage sur l'objet {} (quête {}, rang {})", bar.object, bar.tid, bar.rank);
    if let Some(net) = net {
        net.send(Command::Npc(NpcCommand::QuestGather { object: bar.object, tid: bar.tid, rank: bar.rank as i32 }));
    }
}

/// Nom de la compétence jouée au ramassage (client de Webzen : « 연금술 ») et son nom dans les tables traduites.
const GATHER_SKILL_NAMES: [&str; 2] = ["연금술", "Alchemy"];

/// TID de la compétence du ramassage : nom de `SKILLTEMPLATE.ini` (colonne `Name`).
fn gather_skill_tid(assets: &Assets) -> Option<i32> {
    let table = assets.table("ini", "SKILLTEMPLATE.ini").ok()?;
    let col = table.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone())?;
    GATHER_SKILL_NAMES.iter().find_map(|n| table.sections.iter().find(|s| s.get(&col).is_some_and(|v| v.trim() == *n))?.name.trim().parse().ok())
}

/// `QuestGroupID` de l'événement de quête (après `Function=27`) de chaque objet d'un `obj*.ini`.
fn quest_groups(assets: &Assets, tile: u32) -> HashMap<u32, i32> {
    let Ok(table) = assets.table("ini", &format!("obj{tile:05}.ini")) else { return HashMap::new() };
    let mut out = HashMap::new();
    for s in &table.sections {
        let Ok(id) = s.name.trim().parse::<u32>() else { continue };
        let mut quest = false;
        for (k, v) in &s.entries {
            if k.eq_ignore_ascii_case("Function") {
                quest = v.trim() == function::QUEST.to_string();
            } else if quest && k.eq_ignore_ascii_case("QuestGroupID") {
                if let Some(g) = v.trim().parse::<i32>().ok().filter(|g| *g > 0) {
                    out.insert(id, g);
                }
            }
        }
    }
    out
}

/// Objets à quêtes d'une tuile (un objet de groupe 0 n'en est pas un).
fn tile_quest_objects(assets: &Assets, tile: u32) -> Vec<QuestObject> {
    let groups = quest_groups(assets, tile);
    tile_function_objects(assets, tile, function::QUEST)
        .into_iter()
        .filter_map(|(object, placement)| Some(QuestObject { object, group: *groups.get(&object)?, placement }))
        .collect()
}

impl App {
    /// Objets à quêtes des tuiles autour du joueur.
    pub(super) fn quest_objects_frame(&mut self) {
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return };
        let (sx, sy) = sector_of(me.x, me.z);
        for dy in [-16, 0, 16] {
            for dx in [-16, 0, 16] {
                let tile = tile_of(sx + dx, sy + dy);
                if self.quest_objects.tiles.insert(tile) {
                    let found = tile_quest_objects(&self.assets, tile);
                    if !found.is_empty() {
                        log::info!("quêtes : {} objet(s) à quêtes dans obj{tile:05}.ini", found.len());
                    }
                    self.quest_objects.objects.extend(found);
                }
            }
        }
        self.quest_marks_frame();
    }

    /// Marques de quête au-dessus des PNJ vus et des objets à quêtes à portée de tracé (FUN_14065c620, voir
    /// `App::effects_quest_mark` et `App::quest_group_mark`) ; retirées hors de vue.
    pub(super) fn quest_marks_frame(&mut self) {
        if self.quest_objects.marks_at.is_some_and(|t| t.elapsed() < MARKS_EVERY) {
            return;
        }
        self.quest_objects.marks_at = Some(Instant::now());
        let me = self.player.as_ref().map(|p| p.position);
        let mut keep: HashSet<i32> = HashSet::new();
        if let Some(me) = me.filter(|_| self.options.video.effect_levels() != (0, 0, 0, 0)) {
            for (cid, group) in self.quest_npcs_in_view() {
                let mark = self.quest_group_mark(group);
                self.effects_quest_mark(cid, mark, None);
                keep.insert(cid);
            }
            let reach = self.options.video.object_distance();
            let near: Vec<(u32, i32, Placement)> = self.quest_objects.objects.iter().filter(|o| o.placement.position.distance(me) <= reach).map(|o| (o.object, o.group, o.placement.clone())).collect();
            for (object, group, placement) in near {
                let mark = self.quest_group_mark(group);
                // Haut de la sphère englobante du modèle (sinon 150 au-dessus du pied, comme la visée au clic).
                let top = self.objects.placed_bounds(&placement).map_or(150.0, |(c, r)| c.y + r - placement.position.y);
                self.effects_quest_mark(object_key(object), mark, Some((placement.position, top)));
                keep.insert(object_key(object));
            }
        }
        for key in self.effects_quest_mark_keys() {
            if !keep.contains(&key) {
                self.effects_quest_mark(key, None, None);
            }
        }
    }

    /// Accord de la fenêtre d'un objet (FUN_140657410) : lance la barre du ramassage s'il y a lieu et rend `true`
    /// (la fenêtre ne s'ouvre pas) ; messages sysstr 527 (monture) et 444 (objet déjà détenu), puis `false`.
    pub(super) fn quest_gather_start(&mut self, object: u32, group: i32) -> bool {
        if self.quest_objects.bar.is_some() {
            return false;
        }
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let Some(entry) = self.quest_gather_entry(group) else { return false };
        if self.ride.riding(me) {
            let text = self.npc_sysstr(SYS_GET_OFF_MOUNT);
            self.ui.push_chat_in(0x10, text);
            return false;
        }
        let Ok((tid, rank)) = entry else {
            let text = self.npc_sysstr(SYS_ALREADY_HELD);
            self.ui.push_chat_in(0x10, text);
            return false;
        };
        // Personnage arrêté pendant la barre (FUN_140169c60).
        if self.walking {
            self.walking = false;
            if let (Some(net), Some(p)) = (&self.net, &mut self.player) {
                p.destination = p.position;
                net.send(Command::Stop { position: p.position });
            }
        }
        if let Some((_, at)) = self.quest_object(object) {
            self.quest_gather_animation(at);
        }
        let title = self.npc_sysstr(SYS_GATHER_TITLE);
        log::info!("quêtes : barre du ramassage sur l'objet {object} (quête {tid}, rang {rank})");
        self.quest_objects.bar = Some(GatherBar { object, tid, rank, since: Instant::now(), title });
        true
    }

    /// Animation du ramassage (FUN_14065cc70, appelée au départ de la barre ; Webzen FUN_006b0560) : le personnage est
    /// tourné vers l'objet (FUN_14016bf60 / FUN_14016a070) et joue l'animation de lancement de la compétence nommée
    /// « 연금술 » (Alchemy, chaîne EUC-KR `bf ac b1 dd bc fa` du client de Webzen, 0x8d10c0), rang de la compétence
    /// dans les `UsableSkillTName` de son modèle, ensemble d'animation courant, sinon 0 (FUN_14069c2c0, `+0x60 +
    /// ensemble × 8`, type −11 par FUN_1404be8a0) ; ses actions sont bloquées pendant la durée du clip (FUN_14017acd0,
    /// type 4). Dans Archonia le nom (0x140d5a0d8) a été abîmé en UTF-8 : la recherche par nom (FUN_1402f4a60) échoue
    /// et le client sort avant de tourner le personnage ; ArchRust suit l'intention du client de Webzen.
    fn quest_gather_animation(&mut self, at: Vec3) {
        let assets = self.assets.clone();
        let Some(skill_tid) = gather_skill_tid(&assets) else { return };
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let equip = self.anim_equip_of(me).max(0) as u32;
        let Some(p) = self.player.as_mut() else { return };
        if let Some(turn) = crate::scene::turn_towards(p.position, at) {
            p.turn_y = turn;
        }
        let tid = p.tid;
        let Some(rank) = super::effects_glue::skill_rank(&assets, tid, skill_tid) else { return };
        let Some(file) = crate::animation::skill_file(&assets, tid, rank, equip) else { return };
        let played = self.player_animator.as_mut().and_then(|(_, a)| a.as_mut()).and_then(|a| a.play_skill(&assets, &file, 0.0));
        log::info!("quêtes : animation du ramassage {file} (compétence {skill_tid}, rang {rank}, ensemble {equip}) : {played:?} s");
        if let Some(seconds) = played {
            self.skills_lock_until(Instant::now() + Duration::from_secs_f32(seconds));
        }
    }

    /// Objet à quêtes connu : (groupe, position).
    pub(super) fn quest_object(&self, object: u32) -> Option<(i32, Vec3)> {
        self.quest_objects.objects.iter().find(|o| o.object == object).map(|o| (o.group, o.placement.position))
    }

    /// Demande la fenêtre des quêtes d'un objet (champ 3 : rayon de son modèle + 50).
    fn quest_object_request(&mut self, object: u32) {
        let Some(o) = self.quest_objects.objects.iter().find(|o| o.object == object) else { return };
        let radius = self.objects.placed_bounds(&o.placement).map_or(0.0, |(_, r)| r);
        let reach = (radius + REACH_MARGIN) as i32;
        log::info!("quêtes : demande à l'objet {object} (groupe {}, portée {reach})", o.group);
        if let Some(net) = &self.net {
            net.send(Command::Npc(NpcCommand::ObjectQuestWindow { object, reach }));
        }
    }

    /// Clic gauche sur un objet à quêtes : demande au serveur. Rend `true` si un objet est visé.
    pub(super) fn quest_object_left_click(&mut self) -> bool {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else { return false };
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let ndc = Vec3::new((cx as f32 / w) * 2.0 - 1.0, 1.0 - (cy as f32 / h) * 2.0, 0.0);
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let near = inv.project_point3(ndc);
        let dir = (inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0)) - near).normalize_or_zero();
        // Sphère englobante du modèle, sinon 150 unités autour du point posé (comme les sphères d'alchimie).
        let hit = self
            .quest_objects
            .objects
            .iter()
            .filter_map(|o| {
                let p = &o.placement;
                let (center, radius) = self.objects.placed_bounds(p).unwrap_or((p.position + Vec3::new(0.0, 100.0, 0.0), 150.0));
                let t = (center - near).dot(dir);
                (t > 0.0 && (near + dir * t).distance(center) <= radius).then_some((o.object, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        let Some(object) = hit else { return false };
        self.quest_object_request(object);
        true
    }

    /// Repères de minicarte des objets à quêtes à portée `reach` (même mesure que les PNJ).
    pub(super) fn quest_object_minimap_icons(&mut self, me: Vec3, reach: f32) -> Vec<crate::hud::MinimapIcon> {
        let near: Vec<(i32, Vec3)> =
            self.quest_objects.objects.iter().filter(|o| (o.placement.position.x - me.x).abs() < reach && (o.placement.position.z - me.z).abs() < reach).map(|o| (o.group, o.placement.position)).collect();
        near.into_iter().filter_map(|(group, pos)| self.quest_group_minimap_kind(group).filter(|k| *k > 0).map(|kind| crate::hud::MinimapIcon { kind, pos })).collect()
    }

    /// `/queteobjet` : fenêtre des quêtes de l'objet à quêtes le plus proche, comme un clic (essais sans souris).
    pub(super) fn quest_object_slash(&mut self, cmd: &str) -> bool {
        if !matches!(cmd, "/queteobjet" | "/questobject") {
            return false;
        }
        let me = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        match self.quest_objects.objects.iter().map(|o| (o.object, o.placement.position.distance(me))).min_by(|a, b| a.1.total_cmp(&b.1)) {
            Some((object, d)) => {
                self.ui.push_chat(format!("[quêtes] objet {object} à {d:.0} unités : demande envoyée."));
                self.quest_object_request(object);
            }
            None => self.ui.push_chat("[quêtes] Aucun objet à quêtes dans les tuiles proches."),
        }
        true
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Clés des objets : négatives et hors de la plage des portails (`app_portal.rs` : −(id 24 bits × 8 + n + 1)).
    #[test]
    fn object_keys_do_not_meet_portal_keys() {
        for object in [1u32, 165_609_488, 0x3fff_ffff] {
            let k = object_key(object);
            assert!(k < -(0x00ff_ffff * 8 + 8), "{object} → {k}");
        }
        assert_ne!(object_key(165_609_488), object_key(165_609_489));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : compétence du ramassage et jeux des marques de quête.
    #[test]
    fn client_data_gather_skill_and_quest_marks() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        assert_eq!(gather_skill_tid(&assets), Some(285));
        for (eid, name) in [(3619u32, "Quest_Take"), (3620, "Quest_Response"), (3632, "Quest_Completed")] {
            let set = assets.effect_sets().decode(eid).unwrap_or_else(|| panic!("jeu {eid}"));
            assert_eq!(set.name, name);
            assert!(set.bases.iter().any(|b| b.clump.is_some()), "{eid} : modèle de la marque");
        }
    }
}
