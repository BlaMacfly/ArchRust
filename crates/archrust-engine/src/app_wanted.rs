//! Avis de recherche côté moteur. Protocole : `archrust_netclient::wanted` ; fenêtres d'Archonia :
//! `hud_wanted.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * l'op 6 inscrit le nom du tueur (attache du personnage) et active « Wanted » de `UI_Resurrection`
//!   (`WC_ACTIVE_ARRANGE`) ; le nom reste jusqu'à une mise à prix réussie (code 0, `FUN_140692ba0`) ;
//! * « Wanted » → `UI_CriminalArrange1` (`WC_ARRANGE`) ; confirmer (`FUN_1406919e0`) : or porté + banque ≥ prime de
//!   base selon le niveau (`FUN_14028b7a0`), sinon `WC_NOT_ENOUGH_BASE` → `UI_CriminalArrange2` (`WC_ARRANGE2`,
//!   « Total Gold », prime pré-remplie) ; confirmer (`FUN_140691b00`) : `WC_UNDERBASE`, `WC_NOT_ENOUGH_MONEY` →
//!   `UI_CriminalArrange3` (`WC_ARRANGE3`) ; confirmer (`FUN_140691c80`) : mêmes contrôles puis op 2 ; annuler :
//!   `WC_CANCELED` ;
//! * résultat (op 2) : `WC_SUCCEEDED` (nom, prime saisie), `WC_NOT_ENOUGH_BASE`, `WC_NOT_ENOUGH_MONEY`,
//!   `WC_ALREADY_ARRANGED`, en message système ;
//! * PNJ de fonction 32 : bouton « Bounties » (sysstr 336) → op 0 ; accord (op 1) : `UI_WantedCriminal`, page 0 ;
//!   10 pages de 10 lignes (op 3, op 4 jusqu'à la page -100), une page affichée n'est redemandée que par
//!   « Refresh » ; fermeture au-delà de 149 unités (`FUN_140693100`) ; colonne « Class » d'après le TID
//!   (`FUN_1406925d0`) ;
//! * op 7 : icône `WantedAlarm` dans la grille système, op 5 demandé toutes les 30 s tant que recherché, infobulle
//!   `WC_RANK_TOOLTIP1/2` ; op 8 : `WC_KILL` (genre 4), `WC_DEAD` (genre 5).

use super::chat_glue::printf;
use super::App;
use crate::assets::Assets;
use crate::hud::{ArrangeView, HudState, WantedBoardView, WantedRow, WantedUi};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::wanted::{base_bounty, notify, result, Criminal, WantedCommand, WantedEvent, PAGES, RANK_PERIOD_SECS, ROWS};
use archrust_netclient::{Command, WorldEvent};
use egui::Color32;
use glam::Vec3;
use std::time::{Duration, Instant};

/// Déplacement au-delà duquel le tableau se ferme (`FUN_140693100`).
const CLOSE_MOVE: f32 = 149.0;

/// Clé d'uimessage de la colonne « Class » (`FUN_1406925d0`) ; autre TID : colonne vide.
pub fn class_key(tid: i32) -> Option<&'static str> {
    Some(match tid {
        96 => "Knight",
        1 => "Archer",
        6 => "Mage",
        4 => "Berserker",
        8 => "Hunter",
        3 => "Sorcerer",
        9 => "Elementalist",
        460 => "Ranger",
        _ => return None,
    })
}

struct Board {
    at: Vec3,
    page: i32,
    rows: [Option<Criminal>; ROWS as usize],
}

#[derive(Default)]
pub struct WantedGlue {
    /// Tueur que le joueur peut mettre à prix (attache `+0x50`).
    killer: Option<String>,
    /// Étape de la mise à prix ouverte (1 à 3) et prime retenue (`ui+0x1778`).
    step: Option<u8>,
    bounty: i64,
    board: Option<Board>,
    /// Le joueur est recherché (`+0xc1c`), son rang et sa prime cumulée (op 5).
    wanted: bool,
    rank: Option<(i32, i64)>,
    next_rank: Option<Instant>,
    /// Lignes de l'infobulle de `WantedAlarm` (systemgridtooltip.xml), lues au premier besoin.
    default_tip: Option<Vec<(String, Color32)>>,
}

fn send(net: Option<&NetLink>, cmd: WantedCommand) {
    if let Some(net) = net {
        net.send(Command::Wanted(cmd));
    }
}

/// Infobulle « Wanted Criminal » de `systemgridtooltip.xml` (nom et lignes).
fn default_tooltip(assets: &Assets) -> Vec<(String, Color32)> {
    let xml = super::npc_glue::client_text(assets, "systemgridtooltip.xml");
    let mut out = vec![("Wanted Criminal".to_string(), Color32::WHITE)];
    if let Some(start) = xml.find("<Name>Wanted Criminal</Name>") {
        let block = &xml[start..];
        let block = &block[..block.find("</Tooltip>").unwrap_or(block.len())];
        for part in block.split("<Line").skip(1) {
            let Some(text) = part.split_once('>').and_then(|(_, t)| t.split("</Line>").next()) else { continue };
            out.push((text.trim().to_string(), Color32::WHITE));
        }
    }
    out
}

/// Message d'Archonia en message système et dans la discussion (`FUN_1402e3c10`).
fn system(hud: &mut HudState, ui: &mut UiState, text: &str) {
    if text.is_empty() {
        return;
    }
    hud.push_system(text);
    ui.push_chat_in(0x10, text.to_string());
}

/// Fenêtres des avis (dans la passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut WantedGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    let held = hud.stats.money.unwrap_or(0) + hud.stats.bank_money.unwrap_or(0);
    let base = base_bounty(hud.stats.level.unwrap_or(1));
    let msg = |hud: &mut HudState, k: &str| crate::hud::ui_message(hud, assets, k);
    if let (Some(step), Some(name)) = (glue.step, glue.killer.clone()) {
        let key = match step {
            1 => "WC_ARRANGE",
            2 => "WC_ARRANGE2",
            _ => "WC_ARRANGE3",
        };
        let text = printf(&msg(hud, key), &[&name]);
        let answer = crate::hud::draw_arrange(ctx, hud, assets, ArrangeView { step, text: &text, held, bounty: &mut glue.bounty });
        match answer {
            Some(true) => {
                let refuse = if base > held {
                    Some(printf(&msg(hud, "WC_NOT_ENOUGH_BASE"), &[&base.to_string()]))
                } else if step > 1 && glue.bounty < base {
                    Some(printf(&msg(hud, "WC_UNDERBASE"), &[&base.to_string()]))
                } else if step > 1 && glue.bounty > held {
                    Some(msg(hud, "WC_NOT_ENOUGH_MONEY"))
                } else {
                    None
                };
                match (refuse, step) {
                    (Some(t), _) => {
                        system(hud, ui, &t);
                        glue.step = None;
                    }
                    (None, 1) => {
                        glue.bounty = base;
                        glue.step = Some(2);
                    }
                    (None, 2) => glue.step = Some(3),
                    (None, _) => {
                        log::info!("avis de recherche : {name} mis à prix pour {} or", glue.bounty);
                        send(net, WantedCommand::Arrange { name, bounty: glue.bounty });
                        glue.step = None;
                    }
                }
            }
            Some(false) => {
                let t = msg(hud, "WC_CANCELED");
                system(hud, ui, &t);
                glue.bounty = 0;
                glue.step = None;
            }
            None => {}
        }
    }
    if let Some(b) = glue.board.as_ref() {
        let rows: Vec<WantedRow> = b
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.as_ref().map(|c| (i, c)))
            .map(|(i, c)| WantedRow {
                rank: (b.page * ROWS + i as i32 + 1).to_string(),
                name: c.name.clone(),
                bounty: c.amount.to_string(),
                level: c.level.to_string(),
                class: class_key(c.tid).map(|k| msg(hud, k)).unwrap_or_default(),
                online: c.online,
            })
            .collect();
        let mut out = Vec::new();
        crate::hud::draw_wanted_board(ctx, hud, assets, &WantedBoardView { rows: &rows, page: b.page }, &mut out);
        for r in out {
            let Some(b) = glue.board.as_mut() else { break };
            let page = match r {
                WantedUi::Page(p) if p != b.page => p,
                WantedUi::Page(_) => continue,
                WantedUi::Left => (b.page - 1).max(0),
                WantedUi::Right => (b.page + 1).min(PAGES - 1),
                WantedUi::Refresh => b.page,
                WantedUi::Close => {
                    glue.board = None;
                    break;
                }
            };
            if page != b.page || r == WantedUi::Refresh {
                b.page = page;
                b.rows = Default::default();
                send(net, WantedCommand::Board(page));
            }
        }
    }
    if glue.wanted {
        let tip = match glue.rank {
            Some((rank, total)) => vec![
                (printf(&msg(hud, "WC_RANK_TOOLTIP1"), &[&rank.to_string()]), Color32::WHITE),
                (printf(&msg(hud, "WC_RANK_TOOLTIP2"), &[&total.to_string()]), Color32::WHITE),
            ],
            None => glue.default_tip.get_or_insert_with(|| default_tooltip(assets)).clone(),
        };
        crate::hud::draw_wanted_alarm(ctx, hud, assets, &tip);
    }
}

impl App {
    /// Nouvelle connexion : plus d'avis en cours.
    pub(super) fn wanted_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Connected) {
            let tip = self.wanted.default_tip.take();
            self.wanted = WantedGlue { default_tip: tip, ..Default::default() };
        }
    }

    pub(super) fn on_wanted_event(&mut self, ev: WantedEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let msg = |app: &mut App, k: &str| crate::hud::ui_message(&mut app.hud, &assets, k);
        match ev {
            WantedEvent::Granted { cid } if cid == me || me == 0 => {
                let at = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
                self.wanted.board = Some(Board { at, page: 0, rows: Default::default() });
                send(self.net.as_ref(), WantedCommand::Board(0));
            }
            WantedEvent::Granted { .. } => {}
            WantedEvent::Arranged { result: code, name, amount } => {
                let text = match code {
                    result::SUCCEEDED => {
                        let bounty = if self.wanted.bounty > 0 { self.wanted.bounty } else { amount };
                        self.wanted.killer = None;
                        self.wanted.bounty = 0;
                        printf(&msg(self, "WC_SUCCEEDED"), &[&name, &bounty.to_string()])
                    }
                    result::NOT_ENOUGH_BASE => {
                        let base = base_bounty(self.hud.stats.level.unwrap_or(1));
                        printf(&msg(self, "WC_NOT_ENOUGH_BASE"), &[&base.to_string()])
                    }
                    result::NOT_ENOUGH_MONEY => msg(self, "WC_NOT_ENOUGH_MONEY"),
                    result::ALREADY_ARRANGED => msg(self, "WC_ALREADY_ARRANGED"),
                    _ => return,
                };
                system(&mut self.hud, &mut self.ui, &text);
            }
            WantedEvent::Row { page, rank, criminal } => {
                let Some(b) = self.wanted.board.as_mut() else { return };
                let index = rank - page * ROWS - 1;
                if page == b.page && (0..ROWS).contains(&index) {
                    b.rows[index as usize] = Some(criminal);
                }
            }
            WantedEvent::PageEnd => {}
            WantedEvent::Rank { rank, total } => {
                if self.wanted.wanted {
                    self.wanted.rank = Some((rank, total));
                }
            }
            WantedEvent::Killer { victim, name } if victim == me => {
                log::info!("avis de recherche : tueur « {name} »");
                self.wanted.killer = Some(name).filter(|n| !n.is_empty());
            }
            WantedEvent::Killer { .. } => {}
            WantedEvent::Wanted { cid, wanted } if cid == me => {
                self.wanted.wanted = wanted;
                if wanted {
                    self.wanted.next_rank = Some(Instant::now());
                } else {
                    self.wanted.rank = None;
                    self.wanted.next_rank = None;
                }
            }
            WantedEvent::Wanted { .. } => {}
            WantedEvent::Notify { cid, kind, name, amount } if cid == me => {
                let key = match kind {
                    notify::KILL => "WC_KILL",
                    notify::DEAD => "WC_DEAD",
                    _ => return,
                };
                let text = printf(&msg(self, key), &[&name, &amount.to_string()]);
                system(&mut self.hud, &mut self.ui, &text);
            }
            WantedEvent::Notify { .. } => {}
        }
    }

    /// À chaque image : bouton « Wanted » de la fenêtre de mort, tableau fermé en s'éloignant, rang toutes les 30 s.
    pub(super) fn wanted_frame(&mut self) {
        self.combat.wanted_button = Some(self.wanted.killer.is_some());
        if std::mem::take(&mut self.combat.wanted_clicked) && self.wanted.killer.is_some() {
            // `WC_ARRANGE_MENU` → `WC_OPEN_ARRANGE1`.
            self.wanted.step = Some(1);
        }
        if let (Some(me), Some(b)) = (self.player.as_ref().map(|p| p.position), self.wanted.board.as_ref()) {
            if Vec3::new(me.x - b.at.x, 0.0, me.z - b.at.z).length() > CLOSE_MOVE {
                self.wanted.board = None;
            }
        }
        if self.wanted.wanted && self.wanted.next_rank.is_some_and(|t| Instant::now() >= t) {
            self.wanted.next_rank = Some(Instant::now() + Duration::from_secs(RANK_PERIOD_SECS));
            send(self.net.as_ref(), WantedCommand::MyRank);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colonne_classe() {
        assert_eq!(class_key(96), Some("Knight"));
        assert_eq!(class_key(460), Some("Ranger"));
        assert_eq!(class_key(377), None);
    }

    #[test]
    fn messages_d_archonia() {
        assert_eq!(
            printf("You have put a bounty on (%s) for a reward of (%I64d) Gold.", &["Brute", "75000"]),
            "You have put a bounty on (Brute) for a reward of (75000) Gold."
        );
    }
}
