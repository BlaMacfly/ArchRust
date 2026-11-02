//! Échange entre joueurs côté moteur. Protocole : `archrust_netclient::trade` (AgpmPrivateTrade 0x07) ;
//! fenêtres d'Archonia : `hud_trade.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIItem et AgcmPrivateTrade, sha256 e9c7609b…) :
//! * entrée « Trade » du menu du joueur ciblé (`IsActivePrivateTradeMemu` `FUN_14053e080`) : pas sur soi, palier de
//!   meurtrier des deux sous 2 (`FUN_1401793e0`), métamorphosé seulement l'ArchLord ([`App::morph_allows_private_trade`]) ;
//! * demande (`PT_RequestTrade` `FUN_14053e3b0`) : refusée au palier de meurtrier 1 du demandeur (`FUN_14017c5d0`,
//!   `CharacterPenalty_CantPT`) et maison des ventes ouverte (`CannotAuctionInPrivateTrade`) ; op 1 puis
//!   `UI_PrivateTradeRequest` (sysstr 520) ; « Cancel » : op 5 ;
//! * demande reçue (op 3, `FUN_1407bfe60`) : `UI_PrivateTradeYesNo` (sysstr 519) au nom du personnage du **champ 2** ;
//!   « Yes » (`FUN_1407bee30`) : op 4, op 5 si le joueur est meurtrier, rien si la maison des ventes est ouverte ;
//!   « No » : op 5 (jamais l'op 25) ;
//! * début (op 6, `FUN_1407bfc70`) : demande fermée, `UI_PrivateTrade` et l'inventaire ouverts, grilles vidées,
//!   « Lock » actif et « Trade » inactif ; fin (op 5 / op 0 résultat 8, `FUN_1407bfaa0`) : fenêtres de l'échange et
//!   inventaire fermés, montants remis à zéro, `PTMSG_TradeCancel` / `PTMSG_TradeComplete` ;
//! * objet du sac lâché sur sa grille (`PT_MoveItem` `FUN_1407bf0f0`) : op 9 avec la case d'origine et la case visée ;
//!   objet de la grille lâché dans le sac : op 10 (`FUN_140662a00`) ; objets de la grille : AGPMITEM statut 6 ;
//! * « Lock » à bascule (op 16 / 18, ferme `UI_PrivateTrade_Money`) ; « Trade » (op 21) se désactive ; l'autre :
//!   ops 17 / 19 (`PT_ClientLock` / `Unlock`), 20 (`PT_ActiveReadyToExchange` : « Lock » inactif, « Trade » actif),
//!   22 (`PT_ClientReadyToExchange`) ;
//! * or (`PT_UpdateMoney` `FUN_1407bf970`) : envoyé seulement s'il est positif, borné à l'or porté ; ops 23 / 24 ;
//! * op 25 : `Option_Trade_Off_User` dans la discussion.
//!
//! Écarts : ArchCore met le demandeur au champ 1 de l'op 3 (Archonia le lit au champ 2) : le champ 1 est pris quand
//! le champ 2 est le joueur lui-même. L'op 18 reçue remet « Lock » à l'état relâché (ArchCore déverrouille aussi le
//! partenaire ; Archonia laisserait le bouton enfoncé). Le `%s` d'`Option_Trade_Off_User` (laissé sans argument
//! par Archonia) reçoit le nom de la cible. Les refus d'objet lié, utilisé ou non échangeable sont laissés au
//! serveur ; ni grilles Ex, ni pièces Chantra, ni sons ; le statut d'action de la cible (+0x6ae) n'est pas testé.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi, TradeSlot, TradeUi, TradeView};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::trade::{TradeCommand, TradeEvent, RESULT_DONE, STATUS_TRADE_GRID};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use std::collections::{BTreeMap, HashMap};

/// Palier de meurtrier qui cache l'entrée « Trade » (60 points, `FUN_14053e080` : `0x3b < palier`).
const MURDERER_HIDES_MENU: usize = 2;
/// Palier de meurtrier qui refuse la demande et l'acceptation (pénalité 0, 40 points, `FUN_14017c5d0`).
const MURDERER_REFUSES: usize = 1;
/// Statuts de départ de `CONTROL_PTC_LOCK` et `CONTROL_PTC_ReadyToExchange` (`DefaultIDs=-1:0`, `-1:2`).
const PEER_LOCK_START: usize = 0;
const PEER_READY_START: usize = 2;

#[derive(Default)]
pub struct TradeGlue {
    /// Demande envoyée : CID et nom de la cible (`UI_PrivateTradeRequest`).
    request: Option<(i32, String)>,
    /// Demande reçue : CID du demandeur (`UI_PrivateTradeYesNo`, montrée quand il est en vue).
    confirm: Option<i32>,
    /// Échange ouvert avec ce joueur (CID, nom).
    peer: Option<(i32, String)>,
    /// Objets de la grille d'échange : les siens et ceux de l'autre (IID → objet, statut 6).
    mine: BTreeMap<i32, ItemRecord>,
    theirs: BTreeMap<i32, ItemRecord>,
    my_gold: i64,
    peer_gold: i64,
    my_cash: i64,
    peer_cash: i64,
    locked: bool,
    lock_enabled: bool,
    ready_enabled: bool,
    ready_pressed: bool,
    peer_lock: usize,
    peer_ready: usize,
    /// `UI_PrivateTrade_Money` ouverte, montant saisi.
    money: Option<i64>,
    messages: Option<HashMap<String, String>>,
    events: Option<HashMap<String, String>>,
}

/// Demandeur d'une op 3 : le champ 2 (Archonia), sauf s'il désigne le joueur lui-même (ArchCore l'y met).
fn requester(cid: i32, other: i32, me: i32) -> i32 {
    if other != 0 && other != me {
        other
    } else {
        cid
    }
}

/// `IsActivePrivateTradeMemu` (`FUN_14053e080`) : paliers de meurtrier de soi et de la cible, règle de
/// métamorphose (`morph_ok`).
fn menu_active(me: i32, target: i32, levels: (usize, usize), morph_ok: bool) -> bool {
    me != 0 && target != me && levels.0 < MURDERER_HIDES_MENU && levels.1 < MURDERER_HIDES_MENU && morph_ok
}

impl TradeGlue {
    fn text(&mut self, assets: &Assets, key: &str) -> String {
        let messages = self.messages.get_or_insert_with(|| {
            super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
        });
        messages.get(key).cloned().unwrap_or_default()
    }

    fn event(&mut self, assets: &Assets, key: &str) -> String {
        self.events.get_or_insert_with(|| super::portal_glue::ui_events(assets)).get(key).cloned().unwrap_or_default()
    }

    /// Op 6 : fenêtre ouverte, grilles et montants vidés, « Lock » actif, « Trade » inactif (`PT_OpenUIEvent`).
    fn start(&mut self, peer: (i32, String)) {
        self.request = None;
        self.confirm = None;
        self.peer = Some(peer);
        self.theirs.clear();
        (self.my_gold, self.peer_gold, self.my_cash, self.peer_cash) = (0, 0, 0, 0);
        (self.locked, self.lock_enabled, self.ready_enabled, self.ready_pressed) = (false, true, false, false);
        (self.peer_lock, self.peer_ready) = (PEER_LOCK_START, PEER_READY_START);
        self.money = None;
    }

    /// Fin de l'échange (`FUN_1407bfaa0`) : fenêtres fermées, montants remis à zéro.
    fn close(&mut self) {
        self.request = None;
        self.confirm = None;
        self.peer = None;
        self.money = None;
        self.theirs.clear();
        (self.my_gold, self.peer_gold, self.my_cash, self.peer_cash) = (0, 0, 0, 0);
    }

    /// Objet AGPMITEM vu par l'échange ; vrai s'il ne concerne pas l'inventaire (objet de l'autre). Les siens
    /// passés en statut 6 quittent l'inventaire par le traitement ordinaire (statut inconnu de l'inventaire).
    pub fn take_item(&mut self, item: &ItemRecord, me: i32) -> bool {
        if item.operation == 1 {
            self.mine.remove(&item.id);
            return self.theirs.remove(&item.id).is_some();
        }
        if item.status == Some(STATUS_TRADE_GRID) {
            let theirs = item.owner_cid.is_some_and(|o| o != me) && !self.mine.contains_key(&item.id);
            let list = if theirs { &mut self.theirs } else { &mut self.mine };
            let current = list.entry(item.id).or_insert_with(|| item.clone());
            current.status = item.status;
            current.template_id = item.template_id.or(current.template_id);
            current.owner_cid = item.owner_cid.or(current.owner_cid);
            current.count = item.count.or(current.count);
            current.grid = item.grid.or(current.grid);
            return theirs;
        }
        // Objet repris ou rendu à l'annulation (statut 2…) : il sort de la grille.
        if item.status.is_some() {
            self.mine.remove(&item.id);
        }
        false
    }

    fn slots(list: &BTreeMap<i32, ItemRecord>, assets: &Assets) -> Vec<TradeSlot> {
        list.values()
            .filter_map(|i| {
                let tid = i.template_id?;
                let name = crate::scene::item_name_for_tid(assets, tid).unwrap_or_default();
                Some(TradeSlot { iid: i.id, tid, count: i.count.unwrap_or(1).max(1), grid: i.grid.unwrap_or([0, 0, 0]), name })
            })
            .collect()
    }
}

/// Objet de sa grille d'échange lâché dans le sac : op 10 (case d'échange, case du sac, statut 2).
pub fn item_requests(glue: &TradeGlue, requests: &mut Vec<ItemUi>, net: Option<&NetLink>) {
    requests.retain(|r| {
        let ItemUi::DropOnBag { drag, grid } = *r else { return true };
        let Some(item) = glue.mine.get(&drag.iid) else { return true };
        if let Some(net) = net {
            net.send(Command::Trade(TradeCommand::Remove { iid: item.id, status: 2, from: item.grid.unwrap_or([0, 0, 0]), to: grid }));
        }
        false
    });
}

/// État lu avant l'image (le moteur est emprunté pendant le dessin) : joueur, entrée « Trade », refus.
pub struct TradeFrame {
    pub me: String,
    /// Or porté (borne de `UI_PrivateTrade_Money`).
    pub carried: i64,
    /// Entrée « Trade » du menu du joueur ciblé (`FUN_14053e080`).
    pub menu: bool,
    /// Palier de meurtrier 1 ou plus : demande et acceptation refusées (`FUN_14017c5d0`).
    pub refused: bool,
    /// `UI_Auction` ouverte (`CannotAuctionInPrivateTrade`).
    pub auction_open: bool,
    /// Nom du demandeur d'une demande reçue, s'il est en vue.
    pub requester: Option<String>,
}

/// « Trade » choisi dans le menu du joueur ciblé (`PT_RequestTrade`).
fn request(glue: &mut TradeGlue, f: &TradeFrame, target: i32, name: String, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    if f.refused {
        ui.push_chat(glue.text(assets, "CharacterPenalty_CantPT"));
        return;
    }
    if f.auction_open {
        let line = glue.text(assets, "CannotAuctionInPrivateTrade");
        if !line.is_empty() {
            ui.push_chat(line);
        }
        return;
    }
    if let Some(net) = net {
        net.send(Command::Trade(TradeCommand::Request(target)));
    }
    glue.request = Some((target, name));
}

/// Fenêtres de l'échange ; `chosen` : « Trade » choisi dans le menu du joueur ciblé.
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut TradeGlue, f: &TradeFrame, chosen: Option<(i32, String)>, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    if let Some((target, name)) = chosen {
        request(glue, f, target, name, assets, net, ui);
    }
    let send = |cmd: TradeCommand| {
        if let Some(net) = net {
            net.send(Command::Trade(cmd));
        }
    };
    // Demande envoyée.
    if let Some((_, name)) = glue.request.clone() {
        let text = super::npc_glue::sysstr(assets, 520).unwrap_or_default().replacen("%s", &name, 1);
        if crate::hud::draw_trade_request(ctx, hud, assets, &text) {
            send(TradeCommand::Cancel);
            glue.request = None;
        }
    }
    // Demande reçue : montrée quand le demandeur est en vue (Archonia la garde en attente sinon).
    if let (Some(_), Some(name)) = (glue.confirm, f.requester.as_ref()) {
        let text = super::npc_glue::sysstr(assets, 519).unwrap_or_default().replacen("%s", name, 1);
        if let Some(yes) = crate::hud::draw_trade_yes_no(ctx, hud, assets, &text) {
            if yes && !f.refused && f.auction_open {
                // Maison des ventes ouverte : message, la boîte reste.
                let line = glue.text(assets, "CannotAuctionInPrivateTrade");
                if !line.is_empty() {
                    ui.push_chat(line);
                }
            } else {
                send(if yes && !f.refused { TradeCommand::Confirm } else { TradeCommand::Cancel });
                glue.confirm = None;
            }
        }
    }
    let Some((_, peer)) = glue.peer.clone() else { return };
    let (mine, theirs) = (TradeGlue::slots(&glue.mine, assets), TradeGlue::slots(&glue.theirs, assets));
    let view = TradeView {
        me: &f.me,
        peer: &peer,
        mine: &mine,
        theirs: &theirs,
        my_gold: glue.my_gold,
        peer_gold: glue.peer_gold,
        my_cash: glue.my_cash,
        peer_cash: glue.peer_cash,
        locked: glue.locked,
        lock_enabled: glue.lock_enabled,
        ready_enabled: glue.ready_enabled,
        ready_pressed: glue.ready_pressed,
        peer_lock: glue.peer_lock,
        peer_ready: glue.peer_ready,
    };
    let mut out = Vec::new();
    crate::hud::draw_private_trade(ctx, hud, assets, &view, &mut out);
    for a in out {
        match a {
            TradeUi::Lock => {
                send(TradeCommand::Lock);
                glue.locked = true;
                glue.money = None;
            }
            TradeUi::Unlock => {
                send(TradeCommand::Unlock);
                glue.locked = false;
            }
            TradeUi::Ready => {
                send(TradeCommand::Ready);
                glue.ready_enabled = false;
                glue.ready_pressed = true;
            }
            TradeUi::Close => {
                // `Action=1:2:0:1:UI_PrivateTrade` puis `PT_Cancel` ; l'op 5 du serveur ferme le reste.
                send(TradeCommand::Cancel);
                glue.peer = None;
                glue.money = None;
                ui.show_inventory = false;
            }
            TradeUi::Gold => glue.money = Some(0),
            TradeUi::Offer { drag, cell } => {
                // Objet du sac seulement (statut 2 chez ArchRust ; 13, 15 et 51 aussi chez Archonia).
                let from = items.get(&drag.iid).filter(|i| !drag.worn && !drag.bank && i.status == Some(2)).and_then(|i| i.grid);
                if let (Some(from), false) = (from, glue.mine.contains_key(&drag.iid)) {
                    send(TradeCommand::Add { iid: drag.iid, from, to: cell, kind: 0 });
                }
            }
        }
    }
    if let Some(mut amount) = glue.money {
        match crate::hud::draw_trade_money(ctx, hud, assets, &mut amount, f.carried) {
            Some(true) => {
                if amount > 0 {
                    send(TradeCommand::Money(amount.min(f.carried)));
                }
                glue.money = None;
            }
            Some(false) => glue.money = None,
            None => glue.money = Some(amount),
        }
    }
}

impl App {
    /// État de l'échange lu avant l'image ; `target` : joueur ciblé.
    pub(super) fn trade_frame(&self, target: Option<i32>) -> TradeFrame {
        let (me_cid, me) = self.player.as_ref().map_or((0, String::new()), |p| (p.cid, p.name.clone()));
        let my_level = self.pvp.murderer_level_of(me_cid);
        let menu = target.is_some_and(|t| menu_active(me_cid, t, (my_level, self.pvp.murderer_level_of(t)), self.morph_allows_private_trade(t)));
        let requester = self.trade.confirm.and_then(|cid| self.entities.iter().find(|e| e.cid == cid)).map(|e| e.name.clone());
        TradeFrame {
            me,
            carried: self.hud.stats.money.unwrap_or(0),
            menu,
            refused: my_level >= MURDERER_REFUSES,
            auction_open: self.auction.board_open(),
            requester,
        }
    }

    /// Paquet AgpmPrivateTrade.
    pub(super) fn on_trade_event(&mut self, ev: TradeEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let name_of = |cid: i32| self.entities.iter().find(|e| e.cid == cid).map(|e| e.name.clone()).unwrap_or_default();
        match ev {
            TradeEvent::RequestConfirm { cid, other } => self.trade.confirm = Some(requester(cid, other, me)),
            TradeEvent::Started { other, .. } => {
                let peer = (other, name_of(other));
                self.trade.start(peer);
                self.ui.show_inventory = true;
            }
            TradeEvent::Cancelled { .. } => {
                self.trade.close();
                self.ui.show_inventory = false;
                let line = self.trade.event(&assets, "PTMSG_TradeCancel");
                self.ui.push_chat(line);
            }
            TradeEvent::Result { code, .. } if code == RESULT_DONE => {
                self.trade.close();
                self.ui.show_inventory = false;
                let line = self.trade.event(&assets, "PTMSG_TradeComplete");
                self.ui.push_chat(line);
            }
            TradeEvent::Result { .. } | TradeEvent::Ready => {}
            TradeEvent::Locked { locked, .. } => self.trade.locked = locked,
            TradeEvent::TargetLocked(locked) => self.trade.peer_lock = if locked { 3 } else { 2 },
            TradeEvent::ActiveReady => {
                self.trade.lock_enabled = false;
                self.trade.ready_enabled = true;
                (self.trade.peer_lock, self.trade.peer_ready) = (3, 3);
            }
            TradeEvent::TargetReady => self.trade.peer_ready = 4,
            TradeEvent::Money { mine, amount } => *if mine { &mut self.trade.my_gold } else { &mut self.trade.peer_gold } = amount,
            TradeEvent::Cash { mine, amount } => *if mine { &mut self.trade.my_cash } else { &mut self.trade.peer_cash } = amount,
            TradeEvent::Refused { .. } => {
                let target = self.trade.request.take().map(|(_, n)| n).unwrap_or_default();
                let line = self.trade.text(&assets, "Option_Trade_Off_User").replacen("%s", &target, 1);
                self.ui.push_chat(line);
            }
        }
    }

    /// Objet AGPMITEM : grille d'échange (voir [`TradeGlue::take_item`]).
    pub(super) fn trade_item(&mut self, item: &ItemRecord) -> bool {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        self.trade.take_item(item, me)
    }

    /// Connexion au monde perdue : l'échange est annulé par le serveur.
    pub(super) fn trade_reset(&mut self) {
        self.trade = TradeGlue::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i32, op: u8, status: Option<i8>, owner: Option<i32>) -> ItemRecord {
        ItemRecord { operation: op, id, template_id: Some(190), owner_cid: owner, count: Some(2), status, grid: Some([0, 1, 2]) }
    }

    #[test]
    fn demandeur_de_l_op_3() {
        // Archonia : champ 2 ; ArchCore : champ 1 (le champ 2 est le joueur lui-même).
        assert_eq!(requester(5, 9, 1), 9);
        assert_eq!(requester(9, 1, 1), 9);
        assert_eq!(requester(9, 0, 1), 9);
    }

    #[test]
    fn entree_du_menu() {
        assert!(menu_active(1, 2, (0, 1), true));
        assert!(!menu_active(1, 1, (0, 0), true), "soi-même");
        assert!(!menu_active(1, 2, (2, 0), true), "meurtrier de palier 2");
        assert!(!menu_active(1, 2, (0, 3), true));
        assert!(!menu_active(1, 2, (0, 0), false), "métamorphosé");
        assert!(!menu_active(0, 2, (0, 0), true));
    }

    #[test]
    fn objets_de_la_grille() {
        let mut g = TradeGlue::default();
        // Le sien posé dans la grille : gardé, l'inventaire le retire lui-même.
        assert!(!g.take_item(&item(7, 2, Some(6), Some(1)), 1));
        assert!(g.mine.contains_key(&7));
        // Celui de l'autre (op 0, propriétaire 2) : pas pour l'inventaire.
        assert!(g.take_item(&item(8, 0, Some(6), Some(2)), 1));
        assert!(g.theirs.contains_key(&8));
        // Repris dans le sac : sorti de la grille, rendu à l'inventaire.
        assert!(!g.take_item(&item(7, 2, Some(2), Some(1)), 1));
        assert!(g.mine.is_empty());
        // Fin de l'échange : l'objet de l'autre retiré (op 1), puis reçu dans son sac.
        assert!(g.take_item(&item(8, 1, None, Some(2)), 1));
        assert!(!g.take_item(&item(8, 0, Some(2), Some(1)), 1));
        assert!(g.theirs.is_empty());
        // Objet sans rapport.
        assert!(!g.take_item(&item(9, 2, Some(2), Some(1)), 1));
    }

    #[test]
    fn debut_et_fin() {
        let mut g = TradeGlue { request: Some((2, "B".into())), my_gold: 5, locked: true, ..Default::default() };
        g.start((2, "B".into()));
        assert!(g.request.is_none() && g.peer.is_some() && g.lock_enabled && !g.ready_enabled && !g.locked);
        assert_eq!((g.my_gold, g.peer_lock, g.peer_ready), (0, PEER_LOCK_START, PEER_READY_START));
        g.peer_gold = 100;
        g.close();
        assert!(g.peer.is_none() && g.peer_gold == 0);
    }
}
