//! Maison des ventes côté moteur. Protocole : `archrust_netclient::auction` (AgpmAuction 0x0B) ; fenêtres
//! d'Archonia : `hud_auction.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIAuction, AgcmUIAuctionSale, AgcmUIItem, sha256 e9c7609b…) :
//! * marchand d'objets (fonction 28) : bouton « Auction House » (`sysstr` 332) du dialogue ; refus au palier 3
//!   de meurtrier (`CharacterPenalty_CantAuction`), sinon op 9 ; l'op 10 ouvre `UI_Auction`, retient la
//!   position et remet la recherche à zéro ; la planche se ferme à plus de 149 unités de cette position ;
//! * catégories (`FUN_1405071b0`) : colonnes `FirstCategory` / `SecondCategory` d'itemdatatable.txt, objets
//!   dont le modèle existe ; une liste par niveau (catégories, sous-catégories, objets) ; recherche par nom
//!   (`FUN_140507a30`) : complétion sur les objets classés puis saut au niveau des objets ;
//! * objet choisi : chargement complet (`0x140509770`, `0x1405045d0`) : docID 9 999 999 999 999, puis tant
//!   qu'une réponse a 15 lignes et moins de 200 requêtes, docID de la dernière ligne, une requête par seconde ;
//!   tri et pages de 15 lignes côté client, flèches limitées à une fois par 2 s ;
//! * achat (`FUN_1405055c0`, `0x140506b40`) : refus de sa propre vente (268), or insuffisant
//!   (`AC_NotEnoughMoney`), confirmation `AC_BuyMessageBox`, op 7 sans prix CC ; réponses 454, 274, 275, 276
//!   et nouvelle recherche 1 s après ;
//! * bouton « Sell » de l'inventaire : `UI_SalesBox2` et message 456 ; objet lâché (`0x1407c0e80`) : statut
//!   2, 13, 15 ou 47, catégories non nulles (sinon 359), pile entière ; `UI_AuctionCreateSale` (`0x14073b580`) :
//!   prix bridés (or ≤ 1e11, CC ≤ 1e6), or de 1 à 99 999 refusé (922), or seul confirmé (921), récapitulatif
//!   728 (frais 3 %), objet d'événement (842), lié définitivement (843), lié sans parchemin Marvel (841), or ou
//!   CC insuffisant pour les frais (847, 848), puis (`0x1407c1040`) 362 ou op 4 ;
//! * boîte de ventes (`0x1407c0aa0`) : « Cancel » (op 5) en vente, « Confirm » (op 6) vendue ; réponses 451
//!   (279 aux codes 6 et 14), 452 (280 au code 7), 453 ; vente conclue (op 3, statut 1) : 455.
//!
//! Écarts : `AC_BackToParent` n'est relié à aucun contrôle dans l'ini : retour au niveau supérieur par le bouton
//! « − » de la première ligne ou un clic droit sur la liste ; pas d'échange privé dans ArchRust (refus
//! `CannotAuctionInPrivateTrade` sans objet) ; les drapeaux de liaison viennent du champ 19 d'AGPMITEM.

use super::App;
use crate::assets::Assets;
use crate::hud::{AuctionSort, AuctionUi, AuctionView, BoardLine, CreateSaleUi, CreateSaleView, HudState, ItemUi, SalesBoxUi, SalesBoxView, SalesLine, TreeLine, AUCTION_BOARD_ROWS, AUCTION_TREE_ROWS, SALES_BOX_ROWS};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use crate::ui::UiState;
use archrust_netclient::auction::{
    fee, item_flag, result, status, AuctionCommand, AuctionEvent, BoardRow, Sale, MARVEL_SCROLL_TID, MAX_CC, MAX_GOLD, MAX_REQUESTS, MAX_SALES, MIN_GOLD, ROWS_PER_PAGE, START_DOC_ID,
};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use glam::Vec3;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// Distance au-delà de laquelle `UI_Auction` se ferme (`0x140506c90`).
const CLOSE_RANGE: f32 = 149.0;
/// Délai entre deux requêtes de recherche (AgcmUICooldown) et entre deux flèches de page.
const SEARCH_COOLDOWN: Duration = Duration::from_millis(1000);
const ARROW_COOLDOWN: Duration = Duration::from_millis(2000);
/// Statuts d'objet acceptés dans la boîte de ventes : sac, sous-inventaire, inventaires étendus.
const SELLABLE_STATUS: [i8; 4] = [2, 13, 15, 47];
/// Type d'usage des parchemins Marvel (colonne `Type` d'itemdatatable.txt).
const MARVEL_TYPE: i64 = 35;

/// Colonnes utiles d'itemdatatable.txt.
#[derive(Debug, Clone, Default)]
struct ItemInfo {
    first: i32,
    second: i32,
    stack: bool,
    event: bool,
    usage: i64,
}

#[derive(Default)]
struct AuctionData {
    items: HashMap<i32, ItemInfo>,
    /// Catégories (identifiant, nom, sous-catégories), par identifiant.
    firsts: Vec<(i32, String, Vec<i32>)>,
    /// Sous-catégories : nom et objets classés (TID, nom), dans l'ordre du fichier.
    seconds: HashMap<i32, (String, Vec<(i32, String)>)>,
    sysstr: HashMap<u32, String>,
    events: HashMap<String, String>,
    messages: HashMap<String, String>,
}

impl AuctionData {
    fn sys(&self, id: u32) -> String {
        self.sysstr.get(&id).cloned().unwrap_or_default()
    }

    fn info(&self, tid: Option<i32>) -> ItemInfo {
        tid.and_then(|t| self.items.get(&t).cloned()).unwrap_or_default()
    }

    /// Objet classé dont le nom commence par `text` (casse ignorée) : TID et nom complet.
    fn complete(&self, text: &str) -> Option<(i32, String)> {
        let t = text.trim().to_lowercase();
        if t.is_empty() {
            return None;
        }
        let mut names: Vec<&(i32, String)> = self.seconds.values().flat_map(|(_, l)| l.iter()).collect();
        names.sort_by(|a, b| a.1.cmp(&b.1));
        names.iter().find(|(_, n)| n.to_lowercase() == t).or_else(|| names.iter().find(|(_, n)| n.to_lowercase().starts_with(&t))).map(|x| (**x).clone())
    }
}

fn load_data(assets: &Assets) -> AuctionData {
    let mut d = AuctionData::default();
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    let mut first_names: BTreeMap<i32, (String, Vec<i32>)> = BTreeMap::new();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, f1, f1n, f2, f2n) = (col("TID"), col("FirstCategory"), col("FirstCategoryName"), col("SecondCategory"), col("SecondCategoryName"));
        let (stack, event, usage) = (col("Stack"), col("EventItem"), col("Type"));
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let get = |c: Option<usize>| c.and_then(|i| cells.get(i)).map(|v| v.trim()).unwrap_or("");
            let num = |c: Option<usize>| get(c).parse::<f64>().map(|v| v as i64).unwrap_or(0);
            let Ok(t) = get(tid).parse::<i32>() else { continue };
            let info = ItemInfo { first: num(f1) as i32, second: num(f2) as i32, stack: num(stack) > 1, event: num(event) != 0, usage: num(usage) };
            if info.first != 0 && info.second != 0 {
                if let Some(name) = item_name_for_tid(assets, t) {
                    let f = first_names.entry(info.first).or_insert_with(|| (get(f1n).to_string(), Vec::new()));
                    if !f.1.contains(&info.second) {
                        f.1.push(info.second);
                    }
                    d.seconds.entry(info.second).or_insert_with(|| (get(f2n).to_string(), Vec::new())).1.push((t, name));
                }
            }
            d.items.insert(t, info);
        }
    }
    d.firsts = first_names.into_iter().map(|(id, (name, mut seconds))| {
        seconds.sort_unstable();
        (id, name, seconds)
    }).collect();
    let sys = super::npc_glue::client_text(assets, "sysstr.txt");
    d.sysstr = sys.lines().filter_map(|l| l.split_once('=')).filter_map(|(k, v)| Some((k.trim().parse().ok()?, v.trim().to_string()))).collect();
    d.events = super::portal_glue::ui_events(assets);
    d.messages = super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    log::info!("maison des ventes : {} catégorie(s), {} objet(s) classé(s)", d.firsts.len(), d.seconds.values().map(|s| s.1.len()).sum::<usize>());
    d
}

/// Remplace dans l'ordre `%s`, `%d`, `%lld`, `%I64d`, `%llu` d'un texte d'Archonia.
fn fill(text: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut rest = text;
    let mut args = args.iter();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match ["I64d", "lld", "llu", "s", "d"].into_iter().find(|s| tail.starts_with(s)) {
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
    out.push_str(rest);
    out
}

/// Nombre avec séparateur de milliers.
fn group(v: i64) -> String {
    let digits = v.abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Planche ouverte : PNJ et position du joueur à l'accord.
struct Board {
    /// PNJ de l'accord (gardé pour le journal et les AddOns).
    #[allow(dead_code)]
    npc: i32,
    at: Option<Vec3>,
}

/// Chargement complet des ventes d'un TID.
#[derive(Debug, Clone, Default)]
struct Loading {
    tid: i32,
    requests: u32,
    /// Prochaine requête (docID du curseur) et son heure.
    next: Option<(u64, Instant)>,
    last_sent: Option<Instant>,
}

/// Mise en vente en cours (`UI_AuctionCreateSale`).
#[derive(Debug, Clone, Default)]
struct CreateSale {
    iid: i32,
    tid: i32,
    count: i32,
    gold: String,
    cc: String,
    marvel: Option<i32>,
}

/// Confirmation en attente.
#[derive(Debug, Clone, PartialEq)]
enum Confirm {
    Buy(BoardRow),
    /// Vente en or seul (921), puis récapitulatif (728).
    GoldOnly,
    Price,
}

#[derive(Default)]
pub struct AuctionGlue {
    data: Option<AuctionData>,
    board: Option<Board>,
    /// Niveau de la liste (0 catégories, 1 sous-catégories, 2 objets) et parents.
    level: u8,
    first_id: i32,
    second_id: i32,
    tree_first: usize,
    /// Objet choisi (TID) et objet mis en évidence par la recherche.
    tid: Option<i32>,
    found: Option<i32>,
    search: String,
    rows: Vec<BoardRow>,
    loading: Option<Loading>,
    sort: Option<(AuctionSort, bool)>,
    page: usize,
    last_arrow: Option<Instant>,
    /// Nouvelle recherche après un achat.
    research_at: Option<Instant>,
    /// Ventes du joueur (salesID), boîte de ventes ouverte, défilement.
    pub sales: BTreeMap<i32, Sale>,
    pub show_sales: bool,
    sales_first: usize,
    create: Option<CreateSale>,
    confirm: Option<Confirm>,
    /// Boîte d'information en attente (`OK`).
    info: Vec<String>,
    /// Drapeaux d'état des objets du joueur (AGPMITEM champ 19).
    pub item_flags: HashMap<i32, u32>,
}

impl AuctionGlue {
    fn data(&mut self, assets: &Assets) -> &AuctionData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Ferme les fenêtres ouvertes (Échap) ; vrai s'il y en avait.
    pub fn close_windows(&mut self) -> bool {
        let open = self.board.is_some() || self.show_sales || self.create.is_some();
        self.board = None;
        self.show_sales = false;
        self.create = None;
        self.confirm = None;
        open
    }

    /// Lignes de la liste au niveau affiché : libellé, ouvrable, identifiant (catégorie ou TID).
    fn tree_lines(&mut self, assets: &Assets) -> Vec<(String, bool, i32)> {
        let (level, first_id, second_id) = (self.level, self.first_id, self.second_id);
        let d = self.data(assets);
        match level {
            0 => d.firsts.iter().map(|(id, n, _)| (n.clone(), true, *id)).collect(),
            1 => d
                .firsts
                .iter()
                .find(|(id, _, _)| *id == first_id)
                .map(|(_, _, s)| s.iter().map(|id| (d.seconds.get(id).map(|x| x.0.clone()).unwrap_or_default(), true, *id)).collect())
                .unwrap_or_default(),
            _ => d.seconds.get(&second_id).map(|(_, l)| l.iter().map(|(t, n)| (n.clone(), false, *t)).collect()).unwrap_or_default(),
        }
    }

    /// Lignes triées de la planche.
    fn sort_rows(&mut self, assets: &Assets) {
        let Some((key, ascending)) = self.sort else { return };
        let names: HashMap<i32, String> = self.rows.iter().map(|r| r.tid).collect::<std::collections::HashSet<_>>().into_iter().map(|t| (t, item_name_for_tid(assets, t).unwrap_or_default())).collect();
        let each = |p: i64, n: i32| p as f64 / n.max(1) as f64;
        self.rows.sort_by(|a, b| {
            let o = match key {
                AuctionSort::Name => names.get(&a.tid).cmp(&names.get(&b.tid)),
                AuctionSort::Gold => a.gold.cmp(&b.gold),
                AuctionSort::GoldPerUnit => each(a.gold, a.count).total_cmp(&each(b.gold, b.count)),
                AuctionSort::Cc => a.cc.cmp(&b.cc),
                AuctionSort::CcPerUnit => each(a.cc, a.count).total_cmp(&each(b.cc, b.count)),
                AuctionSort::Count => a.count.cmp(&b.count),
                AuctionSort::Seller => a.seller.to_lowercase().cmp(&b.seller.to_lowercase()),
            };
            if ascending {
                o
            } else {
                o.reverse()
            }
        });
    }

    /// Objet choisi : chargement complet de ses ventes.
    fn select_item(&mut self, tid: i32, net: Option<&NetLink>) {
        self.tid = Some(tid);
        self.found = None;
        self.rows.clear();
        self.page = 0;
        let now = Instant::now();
        // Une requête par seconde au plus (`0x140508350`).
        let at = self.loading.as_ref().and_then(|l| l.last_sent).map_or(now, |t| (t + SEARCH_COOLDOWN).max(now));
        self.loading = Some(Loading { tid, requests: 0, next: Some((START_DOC_ID, at)), last_sent: self.loading.as_ref().and_then(|l| l.last_sent) });
        self.tick(net);
    }

    /// Envoie la requête prévue si son heure est venue.
    fn tick(&mut self, net: Option<&NetLink>) {
        let now = Instant::now();
        if self.research_at.is_some_and(|t| now >= t) {
            self.research_at = None;
            if let Some(tid) = self.tid {
                self.select_item(tid, net);
                return;
            }
        }
        let Some(l) = self.loading.as_mut() else { return };
        let Some((doc_id, at)) = l.next else { return };
        if now < at {
            return;
        }
        l.next = None;
        l.requests += 1;
        l.last_sent = Some(now);
        if let Some(net) = net {
            net.send(Command::Auction(AuctionCommand::Search { tid: l.tid, doc_id, direction: 0 }));
        }
    }

    /// Réponse d'une recherche : lignes ajoutées, requête suivante tant que la page est pleine.
    fn on_results(&mut self, tid: i32, rows: Vec<BoardRow>, assets: &Assets) {
        let Some(l) = self.loading.as_mut().filter(|l| l.tid == tid) else { return };
        let full = rows.len() >= ROWS_PER_PAGE;
        let last = rows.last().map(|r| r.doc_id);
        for r in rows {
            if !self.rows.iter().any(|x| x.doc_id == r.doc_id) {
                self.rows.push(r);
            }
        }
        match last {
            Some(doc) if full && l.requests < MAX_REQUESTS => {
                let at = l.last_sent.map_or_else(Instant::now, |t| t + SEARCH_COOLDOWN);
                l.next = Some((doc, at));
            }
            _ => self.sort_rows(assets),
        }
    }

    fn pages(&self) -> usize {
        self.rows.len().div_ceil(AUCTION_BOARD_ROWS)
    }

    /// Vente reçue (op 1) : 100 au plus.
    pub fn add_sale(&mut self, sale: Sale) -> bool {
        if self.sales.len() >= MAX_SALES && !self.sales.contains_key(&sale.sales_id) {
            return false;
        }
        self.sales.insert(sale.sales_id, sale);
        true
    }
}

/// Contrôles de `UI_AuctionCreateSale` (`0x14073b580`) jusqu'aux confirmations : prix bridés et minimum.
/// `Err(id)` : message `sysstr` ; `Ok((or, CC))` sinon.
fn price_checks(gold: &str, cc: &str) -> Result<(i64, i64), u32> {
    let gold = gold.trim().parse::<i64>().unwrap_or(if gold.trim().is_empty() { 0 } else { MAX_GOLD }).clamp(0, MAX_GOLD);
    let cc = cc.trim().parse::<i64>().unwrap_or(if cc.trim().is_empty() { 0 } else { MAX_CC }).clamp(0, MAX_CC);
    if (1..MIN_GOLD).contains(&gold) {
        return Err(922);
    }
    Ok((gold, cc))
}

/// Contrôles après le récapitulatif : objet d'événement (842), lié définitivement (843), lié sans parchemin
/// (841) ; `Ok(())` si l'objet peut partir.
fn binding_checks(event: bool, flags: u32, marvel: bool) -> Result<(), u32> {
    if event {
        return Err(842);
    }
    if flags & item_flag::BOUND_FOREVER != 0 {
        return Err(843);
    }
    if flags & item_flag::BOUND_MASK != 0 && !marvel {
        return Err(841);
    }
    Ok(())
}

/// Dernier contrôle (`0x1407c1040`) : prix hors bornes ou nuls (362).
fn final_check(gold: i64, cc: i64) -> bool {
    !(gold > MAX_GOLD || cc > MAX_CC || (gold == 0 && cc == 0))
}

/// Fenêtres de la maison des ventes, demandes de l'inventaire et confirmations.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ctx: &egui::Context,
    glue: &mut AuctionGlue,
    item_requests: &mut Vec<ItemUi>,
    items: &HashMap<i32, ItemRecord>,
    me: &str,
    player_pos: Option<Vec3>,
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
    // Bouton « Sell » de l'inventaire.
    if item_requests.iter().any(|r| *r == ItemUi::SalesBox) {
        item_requests.retain(|r| *r != ItemUi::SalesBox);
        glue.show_sales = true;
        let t = glue.data(assets).sys(456);
        glue.info.push(t);
    }
    // Trop loin du marchand : la planche se ferme.
    if let (Some(b), Some(p)) = (glue.board.as_ref(), player_pos) {
        if b.at.is_some_and(|a| a.distance(p) > CLOSE_RANGE) {
            glue.board = None;
            glue.confirm = glue.confirm.take().filter(|c| !matches!(c, Confirm::Buy(_)));
        }
    }
    glue.tick(net);
    if glue.board.is_some() {
        board_window(ctx, glue, me, hud, assets, net);
    }
    if glue.show_sales {
        sales_window(ctx, glue, items, hud, assets, net);
    }
    if glue.create.is_some() {
        create_window(ctx, glue, items, hud, assets, net, ui);
    }
    // Confirmations et informations.
    if let Some(c) = glue.confirm.clone() {
        let prices = glue.create.as_ref().and_then(|s| price_checks(&s.gold, &s.cc).ok()).unwrap_or((0, 0));
        let d = glue.data(assets);
        let text = match &c {
            Confirm::Buy(_) => d.events.get("AC_BuyMessageBox").cloned().unwrap_or_default(),
            Confirm::GoldOnly => d.sys(921),
            Confirm::Price => {
                let (gold, cc) = prices;
                fill(&d.sys(728), &[gold.to_string(), cc.to_string(), fee(gold).to_string(), fee(cc).to_string()])
            }
        };
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("auction-confirm", format!("{c:?}")), &text) {
            glue.confirm = None;
            if yes {
                match c {
                    Confirm::Buy(row) => {
                        if let Some(net) = net {
                            net.send(Command::Auction(AuctionCommand::Buy { doc_id: row.doc_id, count: row.count as i16, tid: row.tid, gold: row.gold }));
                        }
                    }
                    Confirm::GoldOnly => glue.confirm = Some(Confirm::Price),
                    Confirm::Price => finish_sale(glue, items, hud, assets, net),
                }
            }
        }
    } else if let Some(text) = glue.info.first().cloned() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info.remove(0);
        }
    }
}

/// Après le récapitulatif : liaison, frais, dernier contrôle, op 4.
fn finish_sale(glue: &mut AuctionGlue, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let Some(s) = glue.create.clone() else { return };
    let Ok((gold, cc)) = price_checks(&s.gold, &s.cc) else { return };
    let info = glue.data(assets).info(Some(s.tid));
    let flags = glue.item_flags.get(&s.iid).copied().unwrap_or(0);
    if let Err(id) = binding_checks(info.event, flags, s.marvel.is_some()) {
        let t = glue.data(assets).sys(id);
        glue.info.push(t);
        return;
    }
    // Frais : messages système (`FUN_140515040`).
    if hud.stats.money.unwrap_or(0) < fee(gold) {
        let t = glue.data(assets).sys(847);
        hud.push_system(&t);
        return;
    }
    if hud.stats.cash.unwrap_or(0) < fee(cc) {
        let t = glue.data(assets).sys(848);
        hud.push_system(&t);
        return;
    }
    glue.create = None;
    if !final_check(gold, cc) {
        let t = glue.data(assets).sys(362);
        glue.info.push(t);
        return;
    }
    let count = items.get(&s.iid).and_then(|i| i.count).unwrap_or(s.count).max(1);
    if let Some(net) = net {
        net.send(Command::Auction(AuctionCommand::Sell { iid: s.iid, count: count as i16, gold, cc, marvel: s.marvel.unwrap_or(0) }));
    }
}

fn board_window(ctx: &egui::Context, glue: &mut AuctionGlue, me: &str, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let lines = glue.tree_lines(assets);
    glue.tree_first = glue.tree_first.min(lines.len().saturating_sub(AUCTION_TREE_ROWS));
    let tree: Vec<TreeLine> = lines.iter().map(|(label, branch, id)| TreeLine { label: label.clone(), branch: *branch, selected: !*branch && (glue.tid == Some(*id) || glue.found == Some(*id)) }).collect();
    let pages = glue.pages();
    glue.page = glue.page.min(pages.saturating_sub(1));
    let start = glue.page * AUCTION_BOARD_ROWS;
    let page_rows: Vec<BoardRow> = glue.rows.iter().skip(start).take(AUCTION_BOARD_ROWS).cloned().collect();
    let board: Vec<BoardLine> = page_rows
        .iter()
        .map(|r| BoardLine {
            name: item_name_for_tid(assets, r.tid).unwrap_or_default(),
            gold: group(r.gold),
            gold_each: group(r.gold / r.count.max(1) as i64),
            cc: group(r.cc),
            cc_each: group(r.cc / r.count.max(1) as i64),
            count: r.count.to_string(),
            seller: r.seller.clone(),
        })
        .collect();
    let (first_id, second_id) = (glue.first_id, glue.second_id);
    let title = match glue.tid {
        Some(t) => item_name_for_tid(assets, t).unwrap_or_default(),
        None => match glue.level {
            1 => glue.data(assets).firsts.iter().find(|f| f.0 == first_id).map(|f| f.1.clone()).unwrap_or_default(),
            2 => glue.data(assets).seconds.get(&second_id).map(|s| s.0.clone()).unwrap_or_default(),
            _ => String::new(),
        },
    };
    let mut out = Vec::new();
    let mut search = std::mem::take(&mut glue.search);
    let view = AuctionView { title: &title, item_tid: glue.tid, tree: &tree, tree_first: glue.tree_first, level: glue.level, board: &board, search: &mut search, page: glue.page, pages };
    crate::hud::draw_auction(ctx, hud, assets, view, &mut out);
    glue.search = search;
    for a in out {
        match a {
            AuctionUi::Close => glue.board = None,
            AuctionUi::Sort(key) => {
                // Même colonne : sens inversé ; autre colonne : croissant (`0x140508b70`).
                let asc = match glue.sort {
                    Some((k, asc)) if k == key => !asc,
                    _ => true,
                };
                glue.sort = Some((key, asc));
                if glue.loading.as_ref().is_none_or(|l| l.next.is_none()) {
                    glue.sort_rows(assets);
                }
            }
            AuctionUi::Tree(k) => {
                let Some((_, _, id)) = lines.get(k).cloned() else { continue };
                match glue.level {
                    0 => {
                        glue.level = 1;
                        glue.first_id = id;
                        glue.tree_first = 0;
                    }
                    1 => {
                        glue.level = 2;
                        glue.second_id = id;
                        glue.tree_first = 0;
                    }
                    _ => glue.select_item(id, net),
                }
            }
            AuctionUi::TreeBack => {
                glue.level = glue.level.saturating_sub(1);
                glue.tree_first = 0;
            }
            AuctionUi::TreeScroll(step) => glue.tree_first = (glue.tree_first as i32 + step).clamp(0, lines.len().saturating_sub(AUCTION_TREE_ROWS) as i32) as usize,
            AuctionUi::Search => {
                let text = glue.search.clone();
                let found = glue.data(assets).complete(&text);
                match found {
                    Some((tid, name)) => {
                        glue.search = name;
                        let info = glue.data(assets).info(Some(tid));
                        glue.level = 2;
                        glue.first_id = info.first;
                        glue.second_id = info.second;
                        glue.found = Some(tid);
                        let pos = glue.tree_lines(assets).iter().position(|l| l.2 == tid).unwrap_or(0);
                        glue.tree_first = pos.saturating_sub(AUCTION_TREE_ROWS / 2);
                    }
                    None => {
                        let t = glue.data(assets).sys(282);
                        hud.push_system(&t);
                    }
                }
            }
            AuctionUi::Buy(k) => {
                let Some(row) = page_rows.get(k).cloned() else { continue };
                if row.seller == me {
                    let t = glue.data(assets).sys(268);
                    glue.info.push(t);
                } else if row.gold > hud.stats.money.unwrap_or(0) {
                    let t = glue.data(assets).events.get("AC_NotEnoughMoney").cloned().unwrap_or_default();
                    glue.info.push(t);
                } else {
                    glue.confirm = Some(Confirm::Buy(row));
                }
            }
            AuctionUi::Prev | AuctionUi::Next | AuctionUi::Page(_) => {
                if let AuctionUi::Page(p) = a {
                    glue.page = p;
                    continue;
                }
                let now = Instant::now();
                if glue.last_arrow.is_some_and(|t| now < t + ARROW_COOLDOWN) {
                    continue;
                }
                glue.last_arrow = Some(now);
                glue.page = if a == AuctionUi::Prev { glue.page.saturating_sub(1) } else { (glue.page + 1).min(pages.saturating_sub(1)) };
            }
        }
    }
}

fn sales_window(ctx: &egui::Context, glue: &mut AuctionGlue, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let sales: Vec<Sale> = glue.sales.values().cloned().collect();
    glue.sales_first = glue.sales_first.min(sales.len().saturating_sub(SALES_BOX_ROWS));
    let (cancel, confirm) = (glue.data(assets).sys(366), glue.data(assets).sys(365));
    let rows: Vec<SalesLine> = sales
        .iter()
        .map(|s| SalesLine {
            tid: s.tid,
            count: s.count as i32,
            name: item_name_for_tid(assets, s.tid).unwrap_or_default(),
            gold: group(s.gold),
            cc: group(s.cc),
            button: if s.status == status::SOLD { confirm.clone() } else { cancel.clone() },
        })
        .collect();
    let on = sales.iter().filter(|s| s.status == status::ON_SALE).count();
    let on_sale = format!("{}: {on}", glue.data(assets).sys(363));
    let sold = format!("{}: {}", glue.data(assets).sys(364), sales.len() - on);
    let mut out = Vec::new();
    crate::hud::draw_sales_box(ctx, hud, assets, &SalesBoxView { rows: &rows, first: glue.sales_first, on_sale: &on_sale, sold: &sold }, &mut out);
    for a in out {
        match a {
            SalesBoxUi::Close => glue.show_sales = false,
            SalesBoxUi::Scroll(step) => glue.sales_first = (glue.sales_first as i32 + step).clamp(0, sales.len().saturating_sub(SALES_BOX_ROWS) as i32) as usize,
            SalesBoxUi::Button(k) => {
                let (Some(s), Some(net)) = (sales.get(k), net) else { continue };
                let cmd = if s.status == status::SOLD { AuctionCommand::Confirm(s.sales_id) } else { AuctionCommand::Cancel(s.sales_id) };
                net.send(Command::Auction(cmd));
            }
            SalesBoxUi::Drop(drag) => {
                // `AC_MoveToSalesBox` : rien tant qu'une mise en vente est ouverte.
                if glue.create.is_some() || drag.worn || drag.bank {
                    continue;
                }
                let Some(item) = items.get(&drag.iid).filter(|i| i.status.is_some_and(|st| SELLABLE_STATUS.contains(&st))) else { continue };
                let info = glue.data(assets).info(item.template_id);
                if info.first == 0 || info.second == 0 {
                    let t = glue.data(assets).sys(359);
                    glue.info.push(t);
                    continue;
                }
                let count = if info.stack { item.count.unwrap_or(1).max(1) } else { 1 };
                glue.create = Some(CreateSale { iid: item.id, tid: item.template_id.unwrap_or(drag.tid), count, ..Default::default() });
            }
        }
    }
}

fn create_window(ctx: &egui::Context, glue: &mut AuctionGlue, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    let (fee_gold_fmt, fee_cc_fmt) = (glue.data(assets).sys(845), glue.data(assets).sys(846));
    let Some(s) = glue.create.as_mut() else { return };
    let gold = s.gold.trim().parse::<i64>().unwrap_or(0).clamp(0, MAX_GOLD);
    let cc = s.cc.trim().parse::<i64>().unwrap_or(0).clamp(0, MAX_CC);
    let fee_gold = fill(&fee_gold_fmt, &[group(fee(gold))]);
    let fee_cc = fill(&fee_cc_fmt, &[group(fee(cc))]);
    let marvel_tid = s.marvel.and_then(|iid| items.get(&iid)).and_then(|i| i.template_id);
    let mut out = Vec::new();
    crate::hud::draw_create_sale(ctx, hud, assets, CreateSaleView { gold: &mut s.gold, cc: &mut s.cc, fee_gold: &fee_gold, fee_cc: &fee_cc, marvel: marvel_tid }, &mut out);
    for a in out {
        match a {
            CreateSaleUi::Cancel => glue.create = None,
            CreateSaleUi::RemoveMarvel => {
                if let Some(s) = glue.create.as_mut() {
                    s.marvel = None;
                }
            }
            CreateSaleUi::Marvel(drag) => {
                // Parchemin du sac d'usage 35 ; la version liée (événement) est refusée (`NotTradeThisItem`).
                let ok = !drag.worn && !drag.bank && items.get(&drag.iid).is_some_and(|i| {
                    let info = glue.data(assets).info(i.template_id);
                    i.status == Some(2) && (i.template_id == Some(MARVEL_SCROLL_TID) || info.usage == MARVEL_TYPE && !info.event) && glue.item_flags.get(&i.id).copied().unwrap_or(0) & item_flag::BOUND_MASK == 0
                });
                if let (true, Some(s)) = (ok, glue.create.as_mut()) {
                    s.marvel = Some(drag.iid);
                }
            }
            CreateSaleUi::Confirm => {
                let Some(s) = glue.create.as_mut() else { continue };
                match price_checks(&s.gold, &s.cc) {
                    Err(id) => {
                        let t = glue.data(assets).sys(id);
                        glue.info.push(t);
                    }
                    Ok((gold, cc)) => {
                        // Valeurs bridées réécrites dans les champs, comme Archonia.
                        s.gold = if gold > 0 { gold.to_string() } else { String::new() };
                        s.cc = if cc > 0 { cc.to_string() } else { String::new() };
                        glue.confirm = Some(if gold > 0 && cc == 0 { Confirm::GoldOnly } else { Confirm::Price });
                    }
                }
            }
        }
    }
    let _ = (net, ui);
}

impl App {
    /// Paquet AgpmAuction (rappels d'Archonia : le CID doit être celui du joueur).
    pub(super) fn on_auction_event(&mut self, ev: AuctionEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let pos = self.player.as_ref().map(|p| p.position);
        let glue = &mut self.auction;
        let name = |tid: i32| item_name_for_tid(&assets, tid).unwrap_or_default();
        match ev {
            AuctionEvent::ItemFlags { iid, flags } => {
                glue.item_flags.insert(iid, flags);
            }
            AuctionEvent::Granted { cid, npc } if cid == me => {
                glue.board = Some(Board { npc, at: pos });
                glue.level = 0;
                glue.tree_first = 0;
                glue.tid = None;
                glue.found = None;
                glue.rows.clear();
                glue.loading = None;
                glue.page = 0;
            }
            AuctionEvent::SaleAdded { cid, sale } if cid == me => {
                glue.add_sale(sale);
            }
            AuctionEvent::SaleRemoved { cid, sales_id } if cid == me => {
                glue.sales.remove(&sales_id);
            }
            AuctionEvent::SaleStatus { cid, sales_id, status: st } if cid == me => {
                if let Some(s) = glue.sales.get_mut(&sales_id) {
                    let was = s.status;
                    s.status = st;
                    if st == status::SOLD && was != status::SOLD {
                        let s = s.clone();
                        let line = fill(&glue.data(&assets).sys(455), &[name(s.tid), s.count.to_string(), s.gold.to_string(), s.cc.to_string()]);
                        self.ui.push_chat(line);
                    }
                }
            }
            AuctionEvent::Sold { cid, result: code, iid, count, gold, cc } if cid == me => {
                let line = match code {
                    result::SUCCESS => {
                        let tid = self.items.get(&iid).and_then(|i| i.template_id).unwrap_or(0);
                        fill(&glue.data(&assets).sys(451), &[name(tid), count.to_string(), gold.to_string(), cc.to_string()])
                    }
                    result::LIMIT | result::LIMIT2 => glue.data(&assets).sys(279),
                    _ => return,
                };
                self.ui.push_chat(line);
            }
            AuctionEvent::Cancelled { cid, result: code, sales_id, tid, count } if cid == me => {
                let tid = if tid != 0 { tid } else { glue.sales.get(&sales_id).map_or(0, |s| s.tid) };
                let line = match code {
                    result::SUCCESS => fill(&glue.data(&assets).sys(452), &[name(tid), count.to_string()]),
                    result::INVENTORY_FULL => glue.data(&assets).sys(280),
                    _ => return,
                };
                self.ui.push_chat(line);
            }
            AuctionEvent::Confirmed { cid, result: result::SUCCESS, sales_id, tid, count, gold, cc } if cid == me => {
                let tid = if tid != 0 { tid } else { glue.sales.get(&sales_id).map_or(0, |s| s.tid) };
                let line = fill(&glue.data(&assets).sys(453), &[name(tid), count.to_string(), gold.to_string(), cc.to_string()]);
                self.ui.push_chat(line);
            }
            AuctionEvent::Bought { cid, result: code, count, tid, gold, cc, .. } if cid == me => {
                let line = match code {
                    result::SUCCESS => fill(&glue.data(&assets).sys(454), &[name(tid), count.to_string(), gold.to_string(), cc.to_string()]),
                    result::BAD_QUANTITY => glue.data(&assets).sys(274),
                    result::INVENTORY_FULL => glue.data(&assets).sys(275),
                    result::NOT_AVAILABLE => glue.data(&assets).sys(276),
                    _ => String::new(),
                };
                // Nouvelle recherche une seconde après la réponse (`0x140508350`).
                glue.research_at = Some(Instant::now() + SEARCH_COOLDOWN);
                if !line.is_empty() {
                    self.ui.push_chat(line);
                }
            }
            AuctionEvent::Results { cid, tid, rows, .. } if cid == me => {
                let page_was_empty = rows.is_empty();
                glue.on_results(tid, rows, &assets);
                if page_was_empty && glue.page != 0 {
                    let t = glue.data(&assets).sys(737);
                    glue.info.push(t);
                }
            }
            _ => {}
        }
    }

    /// Bouton du marchand d'objets : refus au palier 3 de meurtrier, sinon op 9.
    pub(super) fn auction_request(&mut self) {
        let Some(npc) = self.npc.auction_request.take() else { return };
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if self.pvp.murderer_level_of(me) >= 3 {
            let assets = self.assets.clone();
            let t = self.auction.data(&assets).messages.get("CharacterPenalty_CantAuction").cloned().unwrap_or_default();
            self.hud.push_system(&t);
            return;
        }
        if let Some(net) = self.net.as_ref() {
            net.send(Command::Auction(AuctionCommand::Open(npc)));
        }
    }

    /// Connexion au monde perdue : ventes oubliées (le serveur les renvoie à l'entrée en jeu).
    pub(super) fn auction_reset(&mut self) {
        let data = self.auction.data.take();
        self.auction = AuctionGlue { data, ..Default::default() };
    }

    /// `/ventes` (`/sales`) : boîte de ventes.
    pub(super) fn auction_slash(&mut self, cmd: &str) -> bool {
        if matches!(cmd, "/ventes" | "/sales") {
            self.auction.show_sales = !self.auction.show_sales;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controles_de_mise_en_vente() {
        assert_eq!(price_checks("99999", ""), Err(922));
        assert_eq!(price_checks("1", "5"), Err(922));
        assert_eq!(price_checks("", "5"), Ok((0, 5)));
        assert_eq!(price_checks("100000", ""), Ok((100_000, 0)));
        // Bridés comme Archonia.
        assert_eq!(price_checks("999999999999", "99999999"), Ok((MAX_GOLD, MAX_CC)));
        assert_eq!(binding_checks(true, 0, false), Err(842));
        assert_eq!(binding_checks(false, item_flag::BOUND_FOREVER | 1, true), Err(843));
        assert_eq!(binding_checks(false, 2, false), Err(841));
        assert_eq!(binding_checks(false, 2, true), Ok(()));
        assert_eq!(binding_checks(false, 0, false), Ok(()));
        assert!(!final_check(0, 0));
        assert!(final_check(100_000, 0) && final_check(0, 1));
        assert_eq!(fee(150_000), 4_500);
    }

    #[test]
    fn chargement_par_curseur() {
        let mut g = AuctionGlue::default();
        let assets = Assets::new(None, &[]);
        g.select_item(4963, None);
        let l = g.loading.clone().unwrap();
        assert_eq!((l.requests, l.next), (1, None));
        let page = |start: u64, n: u64| (0..n).map(|i| BoardRow { doc_id: start - i, tid: 4963, count: 1, gold: (i * 10) as i64, ..Default::default() }).collect::<Vec<_>>();
        g.on_results(4963, page(1000, 15), &assets);
        // Page pleine : requête suivante au curseur de la dernière ligne, une seconde après.
        let (doc, at) = g.loading.as_ref().unwrap().next.unwrap();
        assert_eq!(doc, 986);
        assert!(at > Instant::now());
        g.loading.as_mut().unwrap().next = Some((doc, Instant::now()));
        g.tick(None);
        g.sort = Some((AuctionSort::Gold, false));
        g.on_results(4963, page(985, 3), &assets);
        assert_eq!(g.rows.len(), 18);
        assert!(g.loading.as_ref().unwrap().next.is_none());
        // Tri décroissant appliqué à la fin du chargement, deux pages.
        assert_eq!(g.rows[0].gold, 140);
        assert_eq!(g.pages(), 2);
        // Réponse d'un autre objet ignorée.
        g.on_results(1, page(5, 1), &assets);
        assert_eq!(g.rows.len(), 18);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : arbre des catégories, textes, recherche par nom.
    #[test]
    fn categories_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        assert!(d.firsts.iter().any(|(id, n, _)| *id == 75 && n == "Pets"), "{:?}", d.firsts.iter().map(|f| f.0).collect::<Vec<_>>());
        assert!(d.seconds.get(&111).is_some_and(|(n, l)| n == "Shield" && l.iter().any(|(t, _)| *t == 8)));
        let classified: usize = d.seconds.values().map(|s| s.1.len()).sum();
        assert!(classified > 2500, "{classified}");
        assert!(!d.sys(332).is_empty() && !d.sys(728).is_empty());
        assert!(d.events.get("AC_BuyMessageBox").is_some_and(|t| !t.is_empty()));
        let (tid, name) = d.complete("wood shi").unwrap();
        assert_eq!((tid, name.as_str()), (8, "Wood Shield"));
        assert!(d.info(Some(14560)).event && d.info(Some(11256)).usage == MARVEL_TYPE);
        // Fenêtres d'origine et contrôles utilisés par `hud_auction.rs` / `hud_itemset.rs`.
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        for (w, controls) in [
            ("UI_Auction", &["CONTROL_AC_Tree_Data", "CONTROL_4940", "CONTROL_AC_BoardSelectBar", "CONTROL_AC_SearchTID", "CONTROL_AC_Page10", "CONTROL_6622"][..]),
            ("UI_SalesBox2", &["CONTROL_SalesBox2_List", "CONTROL_SalesBox2_OkCancel_Button", "CONTROL_531", "CONTROL_4130"][..]),
            ("UI_AuctionCreateSale", &["CONTROL_PriceGold", "CONTROL_PriceCC", "CONTROL_MarvelScrollAuction", "CONTROL_29441"][..]),
            ("UI_SwitchEditor", &["UI_SwitchEditor_GridA", "UI_SwitchEditor_LstRow9", "UI_SwitchEditor_BtnCaptureB", "UI_SwitchEditor_CmbProfile"][..]),
        ] {
            let win = crate::hud::UiWindow::parse(&ui, None, w).unwrap();
            for c in controls {
                assert!(win.control(c).is_some(), "{w} : {c}");
            }
        }
        // Lignes de la planche : boutons enfants de `CONTROL_AC_BoardBase` (307 + 0, 123 + 0).
        let win = crate::hud::UiWindow::parse(&ui, None, "UI_Auction").unwrap();
        assert_eq!(win.control("CONTROL_AC_ItemNameButton").unwrap().pos, [307.0, 125.0]);
    }

    #[test]
    fn textes() {
        assert_eq!(fill("You have sold %s (%d) for (%lld) Gold and (%lld) CC.", &["Épée".into(), "2".into(), "100".into(), "0".into()]), "You have sold Épée (2) for (100) Gold and (0) CC.");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(group(12), "12");
    }
}
