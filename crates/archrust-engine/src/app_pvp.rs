//! Combat entre joueurs côté client (AgpmPvP 0x4A, AgcmPvP d'Archonia).
//!
//! * Messages système de l'op 18 (`FUN_1406acbf0`) : textes `PvP_Msg_*` d'uimessage.txt, avec le nom (champ 8) ou
//!   l'entier (champ 3) du paquet.
//! * Entrée dans une zone sûre, libre ou de combat : le client les annonce lui-même au changement de région
//!   (bits 2-3 du `Type` de RegionTemplate.ini), le serveur ne les envoie pas.
//! * Ennemis déclarés (op 2, 4, 6, 8) : on les attaque sans Ctrl ; un autre joueur ne s'attaque qu'avec Ctrl (attaque
//!   forcée, champ 6 de l'action). Un membre du groupe n'entre pas dans la liste (`FUN_1402d6780`).
//! * Guildes ennemies (op 10, 12, 14, 16) : hors zone sûre, tout membre d'une guilde ennemie, ou d'une guilde
//!   hostile (AgpmGuild), est un ennemi (EstEnnemi, `FUN_1402d8b90`) ; la sûreté de l'autre joueur n'est pas
//!   connue du client ArchRust : celle de la région du joueur est prise pour les deux. Archonia ne change pas la
//!   couleur du nom d'un ennemi (`FUN_140827ca0` : premier attaquant et meurtrier seulement) ; ArchRust non plus.
//! * Statut de premier attaquant (champ 12 du personnage) et points de meurtrier (facteur de statut 11) : couleur
//!   du nom (`IDColor_FirstAttacker`, `IDColor_MurdererLevel1-3` de color.ini, `FUN_140827ca0`) et, pour le joueur,
//!   « MurdererGetPoint », « BecomeMurdererLv1-3 » et « FreeMurdererLv1-3 » (`FUN_1404fc5e0`).
use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, RemissionUi, RemissionView};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::pvp::{op, PvpEvent};
use archrust_netclient::remission::{RemissionCommand, RemissionEvent, LETTERS, LETTER_TIDS};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use archrust_netclient::world::{CharacterStats, WorldEvent};
use std::collections::{HashMap, HashSet};

/// Paliers de points de meurtrier.
const MURDERER_LEVELS: [i32; 3] = [40, 60, 100];

fn murderer_level(points: i32) -> usize {
    MURDERER_LEVELS.iter().filter(|l| points >= **l).count()
}

/// Textes d'un message système (code de l'op 18), dans l'ordre d'affichage.
fn message_keys(code: i64) -> &'static [&'static str] {
    match code {
        // Entrée dans une zone dangereuse (Battle Square, `FUN_1406acbf0` cas 2) : `_2` / `_3` (pas de perte d'objet
        // à la mort en PK) quand le drapeau de perte d'AgpmConfig est nul, ce qu'ArchRust suppose.
        2 => &["PvP_Msg_EnterCombatArea", "PvP_Msg_EnterCombatArea_2", "PvP_Msg_EnterCombatArea_3", "PvP_Msg_EnterCombatArea_4", "PvP_Msg_EnterCombatArea_5"],
        3 => &["PvP_Msg_NowPvPStatus"],
        4 => &["PvP_Msg_NonePvPStatus"],
        5 => &["PvP_Msg_Dead"],
        6 => &["PvP_Msg_DeadGuildMember"],
        7 => &["PvP_Msg_DeadPartyMember"],
        8 => &["PvP_Msg_ItemDrop"],
        9 => &["PvP_Msg_StartInvincible"],
        10 => &["PvP_Msg_EndInvincible"],
        11 => &["PvP_Msg_CannotUseTeleport"],
        12 => &["PvP_Msg_KillPlayer"],
        13 => &["PvP_Msg_MoveBattleSqaure"],
        14 => &["PvP_Msg_LeaveGuildOrParty", "PvP_Msg_LeaveGuildOrParty_2"],
        15 => &["PvP_Msg_CannotInviteGuild"],
        16 => &["PvP_Msg_CannotInviteParty"],
        17 => &["PvP_Msg_TargetInvincible"],
        18 => &["PvP_Msg_NotEnoughLevel"],
        19 => &["PvP_Msg_CannotInviteMember"],
        20 => &["PvP_Msg_CannotAttackFriend"],
        21 => &["PvP_Msg_CannotAttackOther"],
        22 => &["Skill_Cannot_Apply_Effect"],
        23 => &["PvP_Msg_ItemAntiDrop"],
        24 => &["CharismaGetPoint"],
        25 => &["CannotPvPByLevel"],
        26 => &["DisableNormalAttack"],
        27 => &["DisableSkillCast"],
        _ => &[],
    }
}

/// Textes annoncés en entrant dans une zone sûre (0), libre (1) ou de combat (2).
fn zone_keys(safety: i64) -> &'static [&'static str] {
    match safety {
        0 => &["PvP_Msg_EnterSafeArea"],
        1 => &["PvP_Msg_EnterFreeArea", "PvP_Msg_EnterFreeArea_2", "PvP_Msg_EnterFreeArea_3", "PvP_Msg_EnterFreeArea_4"],
        2 => &[
            "PvP_Msg_EnterCombatArea",
            "PvP_Msg_EnterCombatArea_2",
            "PvP_Msg_EnterCombatArea_3",
            "PvP_Msg_EnterCombatArea_4",
            "PvP_Msg_EnterCombatArea_5",
            "PvP_Msg_EnterCombatArea_6",
        ],
        _ => &[],
    }
}

/// Remplace les `%s`, `%d` et `%I64d` dans l'ordre.
pub(super) fn fill(text: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut rest = text;
    let mut args = args.iter();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let spec = ["I64d", "s", "d"].into_iter().find(|s| tail.starts_with(s));
        match spec {
            Some(s) => {
                out.push_str(args.next().map(String::as_str).unwrap_or(""));
                rest = &tail[s.len()..];
            }
            None => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out + rest
}

/// Sûreté d'une région (bits 2-3 du champ `Type` de RegionTemplate.ini).
fn region_safety(assets: &Assets, region: u32) -> Option<i64> {
    let table = assets.table("ini", "RegionTemplate.ini").ok()?;
    // Pas de section d'en-tête : chaque région (`[n]`) porte sa clé `Type`.
    let ty = table.section(&region.to_string())?.get("Type")?.trim().parse::<i64>().ok()?;
    Some((ty >> 2) & 3)
}

/// Couleur `0xRRGGBB` d'une clé de color.ini, quelle que soit sa section.
fn ini_color(assets: &Assets, key: &str) -> Option<u32> {
    let table = assets.table("ini", "color.ini").ok()?;
    table.sections.iter().find_map(|s| s.get(key)).and_then(|v| u32::from_str_radix(v.trim(), 16).ok())
}

#[derive(Default)]
pub struct PvpGlue {
    messages: Option<HashMap<String, String>>,
    /// Ennemis déclarés du joueur (op 2 / 4 / 6 / 8).
    pub enemies: HashSet<i32>,
    /// Guildes ennemies du joueur (op 10 / 12 / 14 / 16), en minuscules, et leur mode PvP.
    pub enemy_guilds: HashMap<String, i64>,
    pub wins: i64,
    pub losses: i64,
    /// Statut de premier attaquant et points de meurtrier connus, par CID.
    marks: HashMap<i32, (bool, i32)>,
    /// Sûreté de la dernière région annoncée.
    safety: Option<i64>,
    /// Couleurs des noms (format des polices `0xAABBGGRR`) : premier attaquant, paliers 1 à 3.
    colors: Option<[u32; 4]>,
    /// Fenêtre de rémission (`UI_Remission`) ouverte, confirmation en cours.
    remission_open: bool,
    remission_confirm: bool,
}

impl PvpGlue {
    fn text(&mut self, assets: &Assets, key: &str) -> String {
        let messages = self.messages.get_or_insert_with(|| {
            super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
        });
        messages.get(key).cloned().unwrap_or_default()
    }

    /// Palier de meurtrier connu d'un personnage (0 à 3), pour les refus d'autres systèmes (maison des ventes).
    pub(super) fn murderer_level_of(&self, cid: i32) -> usize {
        murderer_level(self.marks.get(&cid).map_or(0, |m| m.1))
    }
}

/// Points de meurtrier retirés par une rémission, comme le client d'Archonia l'annonce (`REMIT_MSG_RM_PO`).
const REMISSION_POINTS: i64 = 2;

/// Fenêtre de rémission (après l'accord du pardonneur) : « Use » demande confirmation (`REMIT_MSG_CNF_IND`) puis
/// envoie la rémission (op 2, type 4).
#[allow(clippy::too_many_arguments)]
pub fn remission_panel(ctx: &egui::Context, glue: &mut PvpGlue, items: &std::collections::HashMap<i32, ItemRecord>, me: i32, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, _ui: &mut UiState) {
    if !glue.remission_open {
        return;
    }
    let letters: i64 = items
        .values()
        .filter(|i| i.status == Some(2) && i.template_id.is_some_and(|t| LETTER_TIDS.contains(&t)))
        .map(|i| i.count.unwrap_or(1).max(1) as i64)
        .sum();
    let points = glue.marks.get(&me).map_or(0, |m| m.1);
    let view = RemissionView {
        desc: glue.text(assets, "REMIT_MSG_DESC"),
        current: fill(&glue.text(assets, "REMIT_MSG_CUR_PO"), &[points.to_string()]),
        removable: fill(&glue.text(assets, "REMIT_MSG_RM_PO"), &[REMISSION_POINTS.to_string()]),
        letters: fill(&glue.text(assets, "REMIT_MSG_IND_CNT"), &[letters.to_string()]),
        need: fill(&glue.text(assets, "REMIT_MSG_NEED_INDUL"), &[LETTERS.to_string()]),
    };
    let mut out = Vec::new();
    crate::hud::draw_remission(ctx, hud, assets, &view, &mut out);
    for a in out {
        match a {
            RemissionUi::Use => glue.remission_confirm = true,
            RemissionUi::Close => {
                glue.remission_open = false;
                glue.remission_confirm = false;
            }
        }
    }
    if glue.remission_confirm {
        let text = fill(&glue.text(assets, "REMIT_MSG_CNF_IND"), &[REMISSION_POINTS.to_string(), LETTERS.to_string()]);
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "remission-confirm", &text) {
            glue.remission_confirm = false;
            if let (true, Some(net)) = (yes, net) {
                net.send(Command::Remission(RemissionCommand::Remit));
            }
        }
    }
}

impl App {
    /// Paquet AgpmRemission : accord du pardonneur, résultat de la rémission.
    pub(super) fn on_remission(&mut self, ev: RemissionEvent) {
        let assets = self.assets.clone();
        match ev {
            RemissionEvent::Granted => self.pvp.remission_open = true,
            RemissionEvent::Result { kind: 4, result: 1 } => {
                let line = fill(&self.pvp.text(&assets, "REMIT_MSG_SUCCESS"), &[REMISSION_POINTS.to_string()]);
                self.ui.push_chat(line);
            }
            RemissionEvent::Result { kind: 1, .. } => {
                let line = self.pvp.text(&assets, "REMIT_MSG_NOT_TP");
                self.ui.push_chat(line);
            }
            RemissionEvent::Result { result: 7, .. } => {
                let line = fill(&self.pvp.text(&assets, "REMIT_MSG_NEED_INDUL"), &[LETTERS.to_string()]);
                self.ui.push_chat(line);
            }
            RemissionEvent::Result { .. } => {}
        }
    }

    /// Relevé du statut criminel et des points de meurtrier dans les paquets personnage.
    pub(super) fn pvp_observe(&mut self, ev: &WorldEvent) {
        let (cid, stats): (i32, Option<&CharacterStats>) = match ev {
            WorldEvent::CharacterUpdated { cid, stats, .. } => (*cid, stats.as_ref()),
            WorldEvent::SelfCharacter { cid, stats, .. } => (*cid, stats.as_ref()),
            WorldEvent::CharacterSeen(c) => (c.cid, c.stats.as_ref()),
            WorldEvent::Disconnected(_) | WorldEvent::Connected => {
                self.pvp.enemies.clear();
                self.pvp.enemy_guilds.clear();
                self.pvp.marks.clear();
                self.pvp.safety = None;
                return;
            }
            _ => return,
        };
        let Some(s) = stats.filter(|s| s.criminal.is_some() || s.murderer.is_some()) else { return };
        let known = self.pvp.marks.get(&cid).copied();
        let old = known.unwrap_or((false, 0));
        let new = (s.criminal.map_or(old.0, |c| c != 0), s.murderer.unwrap_or(old.1));
        self.pvp.marks.insert(cid, new);
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        // Points déjà connus (pas l'entrée en jeu) qui changent pour le joueur.
        if cid != me || known.is_none() || new.1 == old.1 {
            return;
        }
        let assets = self.assets.clone();
        let (before, after) = (murderer_level(old.1), murderer_level(new.1));
        if new.1 > old.1 {
            let line = fill(&self.pvp.text(&assets, "MurdererGetPoint"), &[(new.1 - old.1).to_string()]);
            self.ui.push_chat(line);
        }
        if after > before {
            let line = self.pvp.text(&assets, &format!("BecomeMurdererLv{after}"));
            self.ui.push_chat(line);
        } else if after < before {
            let line = self.pvp.text(&assets, &format!("FreeMurdererLv{before}"));
            self.ui.push_chat(line);
        }
    }

    /// Paquet AgpmPvP.
    pub(super) fn on_pvp(&mut self, ev: PvpEvent) {
        let assets = self.assets.clone();
        match ev.op {
            op::INFO => {
                self.pvp.wins = ev.win;
                self.pvp.losses = ev.lose;
                self.hud.stats.pvp = Some((ev.win, ev.lose));
            }
            // FUN_1402d6780 : refusé pour un membre du groupe du joueur.
            op::ADD_ENEMY if !self.party.state.contains(ev.target as i32) => {
                self.pvp.enemies.insert(ev.target as i32);
            }
            op::ADD_ENEMY => {}
            op::REMOVE_ENEMY => {
                self.pvp.enemies.remove(&(ev.target as i32));
            }
            op::INIT_ENEMY => self.pvp.enemies.clear(),
            // Ennemi déjà connu : mode et instant actualisés (le client n'en montre rien).
            op::UPDATE_ENEMY => log::debug!("ennemi {} actualisé (mode {:?})", ev.target, ev.mode),
            op::ADD_ENEMY_GUILD | op::UPDATE_ENEMY_GUILD if !ev.text1.is_empty() => {
                // Op 16 (`FUN_1402d78e0`) : guilde déjà connue seulement.
                let key = ev.text1.to_lowercase();
                if ev.op == op::ADD_ENEMY_GUILD || self.pvp.enemy_guilds.contains_key(&key) {
                    self.pvp.enemy_guilds.insert(key, ev.mode.unwrap_or(-1));
                }
            }
            op::REMOVE_ENEMY_GUILD => {
                self.pvp.enemy_guilds.remove(&ev.text1.to_lowercase());
            }
            op::INIT_ENEMY_GUILD => self.pvp.enemy_guilds.clear(),
            op::SYSTEM_MESSAGE => {
                let code = ev.cid;
                let args: Vec<String> = match code {
                    // Nom du modèle d'objet perdu ou protégé.
                    8 | 23 => vec![crate::scene::item_name_for_tid(&assets, ev.win as i32).unwrap_or_default()],
                    // Niveau exigé : 10 en dur dans Archonia.
                    18 => vec!["10".into()],
                    24 | 25 => vec![ev.win.to_string()],
                    // Cible sélectionnée (pas un champ du paquet).
                    17 => vec![self.addons.target_cid().and_then(|c| self.entities.iter().find(|e| e.cid == c)).map(|e| e.name.clone()).unwrap_or_default()],
                    _ => vec![ev.text1.clone(), ev.text2.clone()],
                };
                for key in message_keys(code) {
                    let text = self.pvp.text(&assets, key);
                    if !text.is_empty() {
                        self.ui.push_chat(fill(&text, &args));
                    }
                }
            }
            _ => {}
        }
    }

    /// Changement de région du joueur : annonce de la zone sûre, libre ou de combat quand la sûreté change.
    pub(super) fn pvp_region(&mut self, region: u32) {
        let assets = self.assets.clone();
        let Some(safety) = region_safety(&assets, region) else { return };
        if self.pvp.safety == Some(safety) {
            return;
        }
        let first = self.pvp.safety.is_none();
        self.pvp.safety = Some(safety);
        if first && safety == 0 {
            return;
        }
        for key in zone_keys(safety) {
            let text = self.pvp.text(&assets, key);
            if !text.is_empty() {
                self.ui.push_chat(text);
            }
        }
    }

    /// Autre joueur que le clic peut attaquer : ennemi déclaré, ou Ctrl enfoncé (attaque forcée). Le serveur
    /// juge du reste (zones, niveaux, amis).
    pub(super) fn pvp_target(&self, cid: i32) -> bool {
        let player = self.entities.iter().any(|e| e.cid == cid && e.is_player);
        player && (self.pvp_enemy(cid) || self.addon_glue.modifiers.control_key())
    }

    /// EstEnnemi (`FUN_1402d8b90`, sans le combat de guildes, vu par `GuildState::is_enemy`) : ennemi déclaré ;
    /// hors zone sûre, membre d'une guilde ennemie (op 10) ou hostile.
    pub(super) fn pvp_enemy(&self, cid: i32) -> bool {
        if self.pvp.enemies.contains(&cid) {
            return true;
        }
        if self.pvp.safety.is_none_or(|s| s == 0) {
            return false;
        }
        let Some(guild) = self.guild.state.links.get(&cid).filter(|g| !g.is_empty()) else { return false };
        enemy_guild(&self.pvp.enemy_guilds, &self.guild.state.hostiles, guild)
    }

    /// Couleur du nom d'un joueur selon son statut (format `0xAABBGGRR`), sinon `None`.
    pub(super) fn pvp_name_color(&mut self, cid: i32) -> Option<u32> {
        // Combat de Shrine : couleur de race, avant celles du statut (`FUN_140825a20`, voir `app_shrine.rs`).
        if let Some(color) = self.shrine_name_color(cid) {
            return color;
        }
        let (criminal, points) = self.pvp.marks.get(&cid).copied()?;
        let assets = self.assets.clone();
        let colors = *self.pvp.colors.get_or_insert_with(|| {
            let rgb = |key: &str, default: u32| ini_color(&assets, key).unwrap_or(default);
            // color.ini est en 0xRRGGBB ; les polices attendent 0xAABBGGRR.
            let swap = |c: u32| 0xFF00_0000 | ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
            [
                swap(rgb("IDColor_FirstAttacker", 0xFF00FF)),
                swap(rgb("IDColor_MurdererLevel1", 0xFF00FF)),
                swap(rgb("IDColor_MurdererLevel2", 0xA2A200)),
                swap(rgb("IDColor_MurdererLevel3", 0xA244FF)),
            ]
        });
        if criminal {
            return Some(colors[0]);
        }
        match murderer_level(points) {
            0 => None,
            n => Some(colors[n]),
        }
    }
}

/// Guilde dans les guildes ennemies (op 10) ou hostiles (casse ignorée).
fn enemy_guild(enemies: &HashMap<String, i64>, hostiles: &[(String, i64)], guild: &str) -> bool {
    enemies.contains_key(&guild.to_lowercase()) || hostiles.iter().any(|(g, _)| g.eq_ignore_ascii_case(guild))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guildes_ennemies_et_hostiles() {
        let enemies = HashMap::from([("lames".to_string(), 2)]);
        let hostiles = vec![("Corbeaux".to_string(), 0)];
        assert!(enemy_guild(&enemies, &hostiles, "Lames") && enemy_guild(&enemies, &hostiles, "corbeaux"));
        assert!(!enemy_guild(&enemies, &hostiles, "Amis"));
    }

    #[test]
    fn textes_a_trous() {
        assert_eq!(fill("You won the battle against %s.", &["SimB".into()]), "You won the battle against SimB.");
        assert_eq!(fill("You've obtained (%d) Charisma points.", &["42".into()]), "You've obtained (42) Charisma points.");
        assert_eq!(fill("100%", &[]), "100%");
    }

    #[test]
    fn paliers() {
        assert_eq!([0, 39, 40, 60, 100].map(murderer_level), [0, 0, 1, 2, 3]);
    }
}
