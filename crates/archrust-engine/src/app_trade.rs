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
//! * op 25 : `Option_Trade_Off_User` dans la discussion ;
//! * contrôles de `PT_MoveItem` (`FUN_1407bf0f0`) : objet du sac, du sac du familier, de l'inventaire Cash (statuts 2,
//!   13, 15, 51) ; grille Ex : usage 35 (Marvel Scroll of Release) seulement, pas deux du même TID → genre 2 ; grille
//!   ordinaire : objet lié (drapeaux d'état `BOUND_MASK`, AGPMITEM champ 19) → genre 1, refusé par
//!   `Item_TradeFailForBoundItem` s'il y a déjà un objet lié sur la grille ou aucun parchemin sur la grille Ex, et
//!   sans message s'il est lié définitivement (`FUN_1401c2b30`, D) ; objet Chantra déjà entamé (mini Cash : en cours
//!   d'utilisation, en pause ou champ 3 ≥ 1, D pour `+0x508`) → `Item_TradeFailForUsingCashItem` ; objets de la
//!   grille Ex : AGPMITEM statut 14, qu'on ne reprend pas (comme la référence, qui ne les rend qu'à l'annulation).
//!
//! Écarts : ArchCore met le demandeur au champ 1 de l'op 3 (Archonia le lit au champ 2) : le champ 1 est pris quand
//! le champ 2 est le joueur lui-même. L'op 18 reçue remet « Lock » à l'état relâché (ArchCore déverrouille aussi le
//! partenaire ; Archonia laisserait le bouton enfoncé). Le `%s` d'`Option_Trade_Off_User` (laissé sans argument
//! par Archonia) reçoit le nom de la cible. Les refus d'objet lié, utilisé ou non échangeable sont laissés au
//! serveur ; ni pièces Chantra, ni sons ; le statut d'action de la cible (+0x6ae) n'est pas testé. `NotTradeThisItem`
//! (objet non échangeable, `FUN_1401a93d0`) n'a pas de texte dans uimessage : refus laissé au serveur.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi, TradeSlot, TradeUi, TradeView};
use crate::net::NetLink;
use crate::ui::UiState;
use super::consumables_glue::ItemRow;
use archrust_netclient::auction::item_flag;
use archrust_netclient::pet_bag::{in_use, CashState};
use archrust_netclient::trade::{grid_kind, TradeCommand, TradeEvent, RESULT_DONE, STATUS_TRADE_GRID, STATUS_TRADE_OPTION_GRID};
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
/// Statuts d'où un objet peut être proposé (`FUN_1407bf0f0` : 2, et le masque 0x800000000a000 : 13, 15, 51).
const OFFER_STATUS: [i8; 4] = [2, 13, 15, 51];
/// Usage du Marvel Scroll of Release (`PRIVATE_TRADE_OPTION`, 0x23).
const USE_PRIVATE_TRADE_OPTION: i64 = 35;

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
    /// Grilles Ex (statut 14) : les siens et ceux de l'autre.
    mine_ex: BTreeMap<i32, ItemRecord>,
    theirs_ex: BTreeMap<i32, ItemRecord>,
    /// Usage et type Cash des objets (itemdatatable.txt).
    rows: Option<HashMap<i32, ItemRow>>,
    /// Boîte d'information (`Item_TradeFailFor*`, `Action=4:0`).
    info: Option<String>,
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
        self.mine_ex.clear();
        self.theirs_ex.clear();
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
        self.mine_ex.clear();
        self.theirs_ex.clear();
        (self.my_gold, self.peer_gold, self.my_cash, self.peer_cash) = (0, 0, 0, 0);
    }

    fn rows(&mut self, assets: &Assets) -> &HashMap<i32, ItemRow> {
        self.rows.get_or_insert_with(|| super::consumables_glue::parse_items(&super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt")))
    }

    /// Objet AGPMITEM vu par l'échange ; vrai s'il ne concerne pas l'inventaire (objet de l'autre). Les siens
    /// passés en statut 6 quittent l'inventaire par le traitement ordinaire (statut inconnu de l'inventaire).
    pub fn take_item(&mut self, item: &ItemRecord, me: i32) -> bool {
        if item.operation == 1 {
            self.mine.remove(&item.id);
            let ex = self.mine_ex.remove(&item.id).is_some() | self.theirs_ex.remove(&item.id).is_some();
            return self.theirs.remove(&item.id).is_some() || ex;
        }
        // Grille Ex (statut 14) : jamais dans l'inventaire.
        if item.status == Some(STATUS_TRADE_OPTION_GRID) {
            let theirs = item.owner_cid.is_some_and(|o| o != me);
            let list = if theirs { &mut self.theirs_ex } else { &mut self.mine_ex };
            list.entry(item.id).or_insert_with(|| item.clone()).count = item.count.or(Some(1));
            return true;
        }
        if item.status.is_some() && (self.mine_ex.remove(&item.id).is_some() || self.theirs_ex.remove(&item.id).is_some()) {
            // Parchemin rendu à l'inventaire Cash (échange annulé).
            return false;
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

    /// Grille Ex : case = rang d'arrivée (la référence pose le parchemin en 1, 1, 1).
    fn ex_slots(list: &BTreeMap<i32, ItemRecord>, assets: &Assets) -> Vec<TradeSlot> {
        let mut out = Self::slots(list, assets);
        for (i, s) in out.iter_mut().enumerate() {
            s.grid = [0, i as i16, 0];
        }
        out
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

/// Décision de `PT_MoveItem` (`FUN_1407bf0f0`) pour un objet lâché sur sa grille d'échange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferCheck {
    /// Op 9 avec ce genre de grille (champ 8).
    Send(i32),
    /// Refus sans message.
    Silent,
    /// Refus avec la boîte de cet événement.
    Event(&'static str),
}

/// Ce qu'il faut savoir d'un objet proposé.
#[derive(Debug, Clone, Copy, Default)]
pub struct OfferItem {
    pub status: i8,
    pub tid: i32,
    pub row: ItemRow,
    /// Drapeaux d'état (AGPMITEM champ 19).
    pub flags: u32,
    /// Objet Chantra entamé (mini Cash).
    pub cash_used: bool,
}

/// Contrôles de `FUN_1407bf0f0` : `ex` = lâché sur la grille Ex ; `grid_bound` = un objet lié est déjà sur sa
/// grille ; `ex_tids` = TID des parchemins de sa grille Ex.
pub fn offer_check(item: OfferItem, ex: bool, grid_bound: bool, ex_tids: &[i32]) -> OfferCheck {
    if !OFFER_STATUS.contains(&item.status) {
        return OfferCheck::Silent;
    }
    let bound = item.flags & item_flag::BOUND_MASK != 0;
    let kind = if ex {
        if item.row.item_type != 1 || item.row.use_type != USE_PRIVATE_TRADE_OPTION || ex_tids.contains(&item.tid) {
            return OfferCheck::Silent;
        }
        grid_kind::EX
    } else if bound {
        if grid_bound || ex_tids.is_empty() {
            return OfferCheck::Event("Item_TradeFailForBoundItem");
        }
        grid_kind::BOUND
    } else {
        grid_kind::NORMAL
    };
    if item.row.cash && item.cash_used {
        return OfferCheck::Event("Item_TradeFailForUsingCashItem");
    }
    if !ex && bound && item.flags & item_flag::BOUND_FOREVER != 0 {
        return OfferCheck::Silent;
    }
    OfferCheck::Send(kind)
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
/// `flags` : drapeaux d'état des objets (AGPMITEM champ 19, `app_auction.rs`) ; `cash` : mini Cash (`app_petbag.rs`).
pub fn panel(
    ctx: &egui::Context,
    glue: &mut TradeGlue,
    f: &TradeFrame,
    chosen: Option<(i32, String)>,
    items: &HashMap<i32, ItemRecord>,
    (flags, cash): (&HashMap<i32, u32>, &HashMap<i32, CashState>),
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
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
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
    let Some((_, peer)) = glue.peer.clone() else { return };
    let (mine, theirs) = (TradeGlue::slots(&glue.mine, assets), TradeGlue::slots(&glue.theirs, assets));
    let (mine_ex, theirs_ex) = (TradeGlue::ex_slots(&glue.mine_ex, assets), TradeGlue::ex_slots(&glue.theirs_ex, assets));
    let view = TradeView {
        me: &f.me,
        peer: &peer,
        mine: &mine,
        theirs: &theirs,
        mine_ex: &mine_ex,
        theirs_ex: &theirs_ex,
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
            TradeUi::Offer { drag, cell } | TradeUi::OfferEx { drag, cell } => {
                let ex = matches!(a, TradeUi::OfferEx { .. });
                let Some(item) = items.get(&drag.iid).filter(|_| !drag.worn && !drag.bank) else { continue };
                if glue.mine.contains_key(&drag.iid) || glue.mine_ex.contains_key(&drag.iid) {
                    continue;
                }
                let tid = item.template_id.unwrap_or(drag.tid);
                let row = glue.rows(assets).get(&tid).copied().unwrap_or_default();
                let used = cash.get(&item.id).is_some_and(|c| matches!(c.in_use, Some(in_use::YES | in_use::PAUSED)) || c.field3.unwrap_or(0) >= 1);
                let offer = OfferItem { status: item.status.unwrap_or(-1), tid, row, flags: flags.get(&item.id).copied().unwrap_or(0), cash_used: used };
                let grid_bound = glue.mine.keys().any(|iid| flags.get(iid).is_some_and(|f| f & item_flag::BOUND_MASK != 0));
                let ex_tids: Vec<i32> = glue.mine_ex.values().filter_map(|i| i.template_id).collect();
                match offer_check(offer, ex, grid_bound, &ex_tids) {
                    // Case d'origine (l'inventaire Cash n'en a pas : 0, 0, 0).
                    OfferCheck::Send(kind) => send(TradeCommand::Add { iid: drag.iid, from: item.grid.unwrap_or([0, 0, 0]), to: cell, kind }),
                    OfferCheck::Silent => {}
                    OfferCheck::Event(key) => glue.info = Some(glue.event(assets, key)).filter(|t| !t.is_empty()),
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
    fn grille_ex_et_objets_lies() {
        let marvel = ItemRow { item_type: 1, use_type: USE_PRIVATE_TRADE_OPTION, cash: true, ..Default::default() };
        let sword = ItemRow::default();
        let scroll = OfferItem { status: 51, tid: 11256, row: marvel, ..Default::default() };
        // Parchemin sur la grille Ex : genre 2, un seul de chaque TID.
        assert_eq!(offer_check(scroll, true, false, &[]), OfferCheck::Send(grid_kind::EX));
        assert_eq!(offer_check(scroll, true, false, &[11256]), OfferCheck::Silent);
        assert_eq!(offer_check(OfferItem { tid: 14560, ..scroll }, true, false, &[11256]), OfferCheck::Send(grid_kind::EX));
        // Autre objet sur la grille Ex : rien.
        assert_eq!(offer_check(OfferItem { status: 2, tid: 190, row: sword, ..Default::default() }, true, false, &[]), OfferCheck::Silent);
        // Objet lié : genre 1 avec un parchemin posé, un seul par échange.
        let bound = OfferItem { status: 2, tid: 190, row: sword, flags: 1, cash_used: false };
        assert_eq!(offer_check(bound, false, false, &[]), OfferCheck::Event("Item_TradeFailForBoundItem"));
        assert_eq!(offer_check(bound, false, false, &[11256]), OfferCheck::Send(grid_kind::BOUND));
        assert_eq!(offer_check(bound, false, true, &[11256]), OfferCheck::Event("Item_TradeFailForBoundItem"));
        assert_eq!(offer_check(OfferItem { flags: 1 | item_flag::BOUND_FOREVER, ..bound }, false, false, &[11256]), OfferCheck::Silent);
        // Objet ordinaire ; Chantra entamé ; statut d'origine.
        let plain = OfferItem { flags: 0, ..bound };
        assert_eq!(offer_check(plain, false, true, &[]), OfferCheck::Send(grid_kind::NORMAL));
        assert_eq!(offer_check(OfferItem { status: 51, row: ItemRow { cash: true, ..sword }, cash_used: true, ..plain }, false, false, &[]), OfferCheck::Event("Item_TradeFailForUsingCashItem"));
        assert_eq!(offer_check(OfferItem { status: 3, ..plain }, false, false, &[]), OfferCheck::Silent, "porté");
        assert_eq!(offer_check(OfferItem { status: 13, ..plain }, false, false, &[]), OfferCheck::Send(0));
    }

    #[test]
    fn parchemin_de_la_grille_ex() {
        let mut g = TradeGlue::default();
        // Retiré de l'inventaire Cash (op 1), puis renvoyé au statut 14 : jamais dans l'inventaire.
        assert!(!g.take_item(&item(70, 1, None, Some(1)), 1));
        assert!(g.take_item(&item(70, 0, Some(STATUS_TRADE_OPTION_GRID), Some(1)), 1));
        assert!(g.mine_ex.contains_key(&70));
        assert!(g.take_item(&item(71, 0, Some(STATUS_TRADE_OPTION_GRID), Some(2)), 1));
        assert!(g.theirs_ex.contains_key(&71));
        // Échange fait : consommé (op 1) ; annulé : rendu à l'inventaire Cash.
        assert!(g.take_item(&item(71, 1, None, Some(2)), 1));
        assert!(!g.take_item(&item(70, 0, Some(51), Some(1)), 1));
        assert!(g.mine_ex.is_empty() && g.theirs_ex.is_empty());
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
