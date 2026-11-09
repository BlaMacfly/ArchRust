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
//! Non repris : animation du ramassage (compétence au nom coréen), effets 5 / 6 / 7 au-dessus de l'objet (ArchRust
//! ne les dessine pour aucune source).

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
        let title = self.npc_sysstr(SYS_GATHER_TITLE);
        log::info!("quêtes : barre du ramassage sur l'objet {object} (quête {tid}, rang {rank})");
        self.quest_objects.bar = Some(GatherBar { object, tid, rank, since: Instant::now(), title });
        true
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
