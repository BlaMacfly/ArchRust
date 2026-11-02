//! PNJ côté moteur : dialogue, boutique, banque, quêtes. Protocole : `archrust_netclient::npc` ;
//! fenêtres d'Archonia : `hud_npc.rs`.
//!
//! * Clic droit sur un PNJ (ou clic gauche sur le PNJ déjà ciblé) : le joueur s'en approche et le
//!   dialogue s'ouvre (`npcdialog.txt`), avec un bouton par fonction : boutique, banque, quêtes.
//! * Boutique : clic droit sur un objet pour l'acheter (Maj : quantité) ; vendre un objet du sac en
//!   le lâchant sur la boutique ou d'un clic droit, après accord.
//! * Banque : glisser entre le sac et la banque, ou clic droit ; boutons « Déposer » / « Retirer »
//!   pour l'argent ; « Agrandir » (Archonia `Item_RequestBuyBankSlot`) ouvre `UI_BuyBank` : prix de
//!   `bankslotprice.txt` selon les pages déjà achetées (-1 : pas en vente contre de l'or), or porté
//!   vérifié avant l'envoi (`Item_BuyBankSlot`, sinon `Bank_NotEnoughBuyCost`), AGPMITEM op 22 ;
//!   réponse : champ 23 du personnage (onglets ouverts) et message système 30 ou 29, affiché avec
//!   `sysstr.txt` 718 ou 717 (« L'entrepot a ete agrandi avec succes. » / « … ne peut plus etre
//!   agrandi. » : même sens que les codes d'ArchCore, correspondance non retrouvée dans l'exe).
//! * Quêtes : celles du groupe du PNJ (`questgroup.ini`), texte de `questtemplate.ini` (le client
//!   français n'en décrit qu'une soixantaine) ; journal par `L` ou `/quetes`.
//! * Commandes : `/parler [nom]`, `/quetes`, `/prendrequete TID`, `/abandonner TID`.
//! * Achats (boutique des Shrines, EP8) : comme Archonia (`NPCT_BuyItemDirect`, FUN_1407b9b90), tout achat passe
//!   par `UI_SHRINEITEM_BUY_CONFIRM` (voir `hud_social.rs`) : quantité n = celle de l'objet de boutique (1 si
//!   nulle) ou celle choisie (Maj + clic droit), prix de l'itemdatatable.txt du client en facteur PRICE — sous-type
//!   5 charisme (`NPC Charisma Price`, col. 29), 6 Shrine Coins (`NPC Shrine Coin`, col. 35) — et or du serveur
//!   avec la taxe de la région ; restes = charisme du personnage (facteur de statut 4), pièces portées (objets
//!   d'`ItemType` 2 et de `Type` 6, piles de l'inventaire additionnées), or porté. « Acheter » vérifie le charisme
//!   et les pièces avant l'envoi (mêmes boîtes que les refus 10 et 11 du serveur : `NPCTMSG_NotEnoughCharisma`,
//!   `NPCTMSG_NotEnoughShrineCoin`) ; Archonia n'en contrôle aucun et laisse répondre le serveur. Les monnaies
//!   d'Archonia (Archonium Coin, Siege Token, Bloodmark : col. 30, 33, 34, codes 12-14) ne sont pas reprises.
//!   L'infobulle d'un objet de boutique reprend « Prix d'achat : » et ses prix non nuls (FUN_1405479c0).

use super::App;
use crate::assets::Assets;
use crate::hud::{BankView, BuyBankView, DialogView, HudState, ItemUi, NpcUi, PortalUi, QuestEntry, QuestState, QuestView, ShopEntry, ShopView};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use crate::ui::UiState;
use archrust_data::ini::CodePage;
use archrust_netclient::npc::{function, trade_result, NpcCommand, NpcEvent, NpcFunction, QuestProgress, STATUS_BANK};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::{Command, WorldEvent};
use glam::Vec3;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Portée du dialogue d'un PNJ dans ArchLord : 600 unités au sol (même portée pour la boutique et la
/// banque, à confirmer dans Archonia) ; au-delà, le personnage marche jusqu'à 80 % de cette
/// portée (480 unités) puis le dialogue s'ouvre. ArchRust demandait le dialogue dès 1200 unités.
const TALK_RANGE: f32 = 600.0;
/// Point d'arrivée de l'approche : 80 % de la portée.
const APPROACH_RANGE: f32 = TALK_RANGE * 0.8;
/// Au-delà, les fenêtres du PNJ se ferment.
const CLOSE_RANGE: f32 = 2200.0;
/// Banque : 4 pages de 7 lignes × 4 colonnes.
const BANK_ROWS: i16 = 7;
const BANK_COLS: i16 = 4;

/// Texte d'une quête (`questtemplate.ini`).
#[derive(Debug, Clone, Default)]
struct QuestText {
    name: String,
    /// `Area`, ligne sous le nom.
    area: String,
    /// `Dialogue_Start`, sous « Informations de quete ».
    start: String,
    /// Objectifs : `Complete_SummaryN` et sa sorte `CountFactorN` (0 texte, 1 niveau, 2 argent, 4 et 5
    /// monstres A et B…).
    summary: Vec<(String, i32)>,
    monsters: Vec<(String, i32)>,
    /// `Complete_Level`, `Complete_Money`.
    complete_level: i32,
    complete_money: i64,
    money: i64,
    exp: i64,
    /// Objets donnés (`Gift_ItemA` à `D`) et objets au choix (`SelectItem1` à `8`, le joueur en prend un).
    gifts: Vec<QuestItem>,
    choices: Vec<QuestItem>,
}

/// Objet d'une quête : colonnes `…_Count`, `…_Tid` (« TID;a;b »), `…_Upgrade_Slot`, `…_Upgrade_Intensify`
/// (champs +8, +0xc, +0x10, +0x14 de l'élément de 0x20 octets du modèle, lecture FUN_1401e1900).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct QuestItem {
    tid: i32,
    count: i32,
    slot: i32,
    intensify: i32,
    /// Deuxième nombre de la colonne `…_Tid`.
    extra: i32,
}

impl QuestItem {
    /// Balise d'objet du texte de quête (FUN_140657bf0 ; 0x140d5a028) : nombre, TID, emplacements,
    /// renforcement, `extra`, puis 1 si l'objet se choisit (récompense au choix).
    fn tag(&self, selectable: bool) -> String {
        format!("<ITEM::{}::{}::{}::{}::{}::{}>", self.count, self.tid, self.slot, self.intensify, self.extra, i32::from(selectable))
    }
}

/// Données des PNJ lues dans le client.
#[derive(Default)]
struct NpcData {
    /// Répliques par modèle de dialogue (`npcdialog.txt`).
    dialogs: HashMap<i32, Vec<String>>,
    /// Quêtes proposées et reçues par groupe (`questgroup.ini`).
    groups: HashMap<i32, (Vec<i32>, Vec<i32>)>,
    quests: HashMap<i32, QuestText>,
    /// Prix des pages de banque en plus (`bankslotprice.txt`, indice = pages déjà achetées).
    bank_prices: Vec<i64>,
}

struct Shop {
    npc: i32,
    items: Vec<ShopEntry>,
    loading: bool,
    first_row: i16,
}

#[derive(Default)]
pub struct NpcGlue {
    data: Option<NpcData>,
    /// Lignes de `sysstr.txt` déjà lues (textes des quêtes, redessinés à chaque image).
    sys_cache: HashMap<u32, String>,
    /// Boîte d'information (`Action=4:0`) : quête reçue, terminée, journal plein.
    info: Option<String>,
    /// Abandon en attente de `QuestCancelDlg_Open` (TID).
    abandon: Option<i32>,
    /// Messages des événements d'interface (`[EventMap]`).
    events: Option<HashMap<String, String>>,
    /// Fonctions de chaque PNJ vu.
    functions: HashMap<i32, Vec<NpcFunction>>,
    /// PNJ vers lequel le joueur marche pour lui parler.
    approach: Option<(i32, Instant)>,
    dialog: Option<i32>,
    /// Réplique affichée (change à chaque ouverture).
    line: usize,
    shop: Option<Shop>,
    bank: Option<i32>,
    bank_page: i16,
    /// PNJ dont la fenêtre des quêtes est ouverte.
    quest_npc: Option<i32>,
    pub show_journal: bool,
    selected_quest: Option<i32>,
    /// Récompense au choix cliquée : rang parmi les objets choisissables du texte, à partir de 0
    /// (AgcUIQuestText +0x38, -1 sans choix ; clic LAB_14088c840).
    reward_choice: Option<usize>,
    journal: BTreeMap<i32, QuestProgress>,
    done: HashSet<u32>,
    /// Fenêtres de saisie : achat (IID, quantité), vente (IID), argent (dépôt, montant).
    buy: Option<(i32, i32)>,
    sell: Option<i32>,
    money: Option<(bool, i64)>,
    /// Fenêtre d'achat d'une page de banque (`UI_BuyBank`) ouverte.
    buy_bank: bool,
    /// Marchand d'objets choisi dans le dialogue : demande de la maison des ventes (voir `app_auction.rs`).
    pub auction_request: Option<i32>,
    /// Achat en attente de `UI_SHRINEITEM_BUY_CONFIRM` : IID de l'objet de boutique et quantité.
    confirm_buy: Option<(i32, i32)>,
    /// Prix en charisme et en Shrine Coins, pièces (itemdatatable.txt), lus une fois.
    shrine: Option<ShrinePrices>,
}

/// Colonnes de la boutique des Shrines d'itemdatatable.txt.
#[derive(Debug, Default)]
pub(super) struct ShrinePrices {
    /// Prix unitaires par TID : (charisme, Shrine Coins), l'un au moins non nul.
    pub prices: HashMap<i32, (i64, i64)>,
    /// TID des Shrine Coins : `ItemType` 2 (Other), `Type` 6 (53000 « Arena Coin »).
    pub coins: HashSet<i32>,
}

/// `ItemType` / `Type` des Shrine Coins (Archonia FUN_1401c4900).
const COIN_ITEM_TYPE: i64 = 2;
const COIN_TYPE: i64 = 6;
/// Statut AgpmItem de l'inventaire (piles comptées pour les pièces).
const STATUS_INVENTORY: i8 = 2;
/// Cases du sac : 4 sacs de 4 × 4.
const BAG_CELLS: usize = 64;

/// Lit les colonnes `NPC Charisma Price`, `NPC Shrine Coin`, `ItemType` et `Type` d'itemdatatable.txt.
pub(super) fn parse_shrine_prices(text: &str) -> ShrinePrices {
    let mut out = ShrinePrices::default();
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return out };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (tid, item_type, kind, charisma, coin) = (col("TID"), col("ItemType"), col("Type"), col("NPC Charisma Price"), col("NPC Shrine Coin"));
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let num = |c: Option<usize>| c.and_then(|i| cells.get(i)).and_then(|v| v.trim().split(';').next()?.trim().parse::<f64>().ok()).unwrap_or(0.0) as i64;
        let t = num(tid) as i32;
        if t <= 0 {
            continue;
        }
        let (c, p) = (num(charisma).max(0), num(coin).max(0));
        if c > 0 || p > 0 {
            out.prices.insert(t, (c, p));
        }
        if num(item_type) == COIN_ITEM_TYPE && num(kind) == COIN_TYPE {
            out.coins.insert(t);
        }
    }
    out
}

/// Shrine Coins portées : quantités des piles de l'inventaire.
pub(super) fn shrine_coins(items: &HashMap<i32, ItemRecord>, coins: &HashSet<i32>) -> i64 {
    items
        .values()
        .filter(|i| i.status == Some(STATUS_INVENTORY) && i.template_id.is_some_and(|t| coins.contains(&t)))
        .map(|i| i64::from(i.count.unwrap_or(1).max(1)))
        .sum()
}

/// Contenu d'`UI_SHRINEITEM_BUY_CONFIRM` (FUN_14067dc80 … FUN_14067e3b0) : prix × n ; restes = porté − prix × n,
/// l'or avec la taxe dans le prix (`taxed_gold`, prix unitaire taxé) mais pas dans le reste.
#[allow(clippy::too_many_arguments)]
pub(super) fn shrine_buy_view(name: String, tid: i32, n: i64, unit_gold: i64, taxed_gold: i64, unit: (i64, i64), charisma: i64, coins: i64, money: i64) -> crate::hud::ShrineBuyView {
    let n = n.max(1);
    crate::hud::ShrineBuyView {
        name,
        tid,
        count: n,
        charisma_price: unit.0 * n,
        charisma_rest: charisma - unit.0 * n,
        coin_price: unit.1 * n,
        coin_rest: coins - unit.1 * n,
        gold_price: taxed_gold * n,
        gold_rest: money - unit_gold * n,
    }
}

/// Ce que les fenêtres dessinent, préparé avant la passe egui.
#[derive(Default)]
pub struct NpcViews {
    dialog: Option<DialogView>,
    shop_name: String,
    bank_items: Vec<(ItemRecord, String)>,
    quests: Option<(bool, String, Vec<QuestEntry>, String)>,
}

/// Texte d'un fichier du client, version française d'abord (`ini_fr/`), déchiffré.
pub(super) fn client_text(assets: &Assets, file: &str) -> String {
    crate::assets::text_dirs().iter().map(|d| client_text_in(assets, d, file)).find(|t| !t.is_empty()).unwrap_or_default()
}

/// Texte d'un fichier d'un dossier de tables (`ini`, `ini_fr`…), déchiffré s'il le faut.
pub(super) fn client_text_in(assets: &Assets, dir: &str, file: &str) -> String {
    let Ok(raw) = assets.read_file(&format!("{dir}/{file}")) else { return String::new() };
    let cp = CodePage::for_dir(dir);
    let plain = raw.iter().take(256).filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t').count() * 20 < raw.len().min(256);
    if plain {
        archrust_data::ini::decode_text(&raw, cp)
    } else {
        archrust_data::ini::decrypt_ini_with(&raw, cp)
    }
}

/// Prix des pages de banque en plus : `bankslotprice.txt` (`Slot<TAB>Price`, la page 1 achetée en
/// premier).
fn bank_slot_prices(text: &str) -> Vec<i64> {
    let mut prices = Vec::new();
    for line in text.lines().skip(1) {
        let mut cols = line.split('\t');
        if let (Some(Ok(slot)), Some(Ok(price))) = (cols.next().map(|s| s.trim().parse::<usize>()), cols.next().map(|s| s.trim().parse::<i64>())) {
            if slot >= 1 {
                prices.resize(prices.len().max(slot), -1);
                prices[slot - 1] = price;
            }
        }
    }
    prices
}

/// Texte d'une ligne `id=texte` de `sysstr.txt`.
pub(super) fn sysstr(assets: &Assets, id: u32) -> Option<String> {
    let text = client_text(assets, "sysstr.txt");
    let key = format!("{id}=");
    text.lines().find_map(|l| l.strip_prefix(&key)).map(|t| t.trim().to_string())
}

/// Lignes d'un fichier tabulé ; un champ entre guillemets peut contenir des retours à la ligne.
fn tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            '\t' if !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Enlève les codes de couleur d'Archonia (`<HFFFA7272>`) et les crochets de nom (`<[…]>`).
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let color = tail.len() >= 11 && tail.as_bytes()[1] == b'H' && tail[2..10].chars().all(|c| c.is_ascii_hexdigit()) && tail.as_bytes()[10] == b'>';
        if color {
            rest = &tail[11..];
        } else if let Some(t) = tail.strip_prefix("<[") {
            rest = t;
        } else if let (true, Some(open)) = (tail.starts_with("<POS:"), tail.find('[')) {
            // Lien de position « <POS::carte::x::z::[Nom]> » : le nom seul.
            rest = &tail[open + 1..];
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out.replace("]>", "").replace("\\n", "\n").trim().trim_matches('"').trim().to_string()
}

fn load_data(assets: &Assets) -> NpcData {
    let mut data = NpcData::default();
    // npcdialog.txt : ID, nom, …, réplique (colonne 5) ; lignes de suite sans ID ; « - » sépare.
    // Lu ligne à ligne : le fichier contient des guillemets orphelins en début de ligne (suite d'une
    // réplique de « méchant »), qui décaleraient une lecture avec guillemets.
    let mut current: Option<i32> = None;
    let text = client_text(assets, "npcdialog.txt");
    for row in text.lines().skip(1).map(|l| l.split('\t').map(str::to_string).collect::<Vec<_>>()) {
        let first = row.first().map(|s| s.trim()).unwrap_or("");
        if first == "-" {
            current = None;
            continue;
        }
        if let Ok(id) = first.parse::<i32>() {
            current = Some(id);
        }
        let (Some(id), Some(line)) = (current, row.get(5).map(|s| clean(s)).filter(|s| !s.is_empty() && s != "-")) else { continue };
        data.dialogs.entry(id).or_default().push(line);
    }
    // questgroup.ini : ID, nom, quêtes proposées « a:b: », quêtes reçues.
    let list = |s: Option<&String>| s.map(|s| s.split(':').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default();
    for row in tsv(&client_text(assets, "questgroup.ini")) {
        if let Some(id) = row.first().and_then(|s| s.trim().parse::<i32>().ok()) {
            data.groups.insert(id, (list(row.get(2)), list(row.get(3))));
        }
    }
    // questtemplate.ini : colonnes nommées par l'en-tête.
    let rows = tsv(&client_text(assets, "questtemplate.ini"));
    if let Some((header, body)) = rows.split_first() {
        let col = |name: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
        for row in body {
            let get = |name: &str| col(name).and_then(|i| row.get(i)).map(|s| clean(s)).filter(|s| !s.is_empty());
            let num = |name: &str| get(name).and_then(|s| s.parse::<i64>().ok());
            let Some(tid) = num("Tid") else { continue };
            let mut q = QuestText {
                name: get("QuestName").unwrap_or_default(),
                area: get("Area").unwrap_or_default(),
                start: get("Dialogue_Start").unwrap_or_default(),
                complete_level: num("Complete_Level").unwrap_or(0) as i32,
                complete_money: num("Complete_Money").unwrap_or(0),
                money: num("Gift_Money").unwrap_or(0),
                exp: num("Gift_Exp").unwrap_or(0),
                ..Default::default()
            };
            q.summary = (1..=5).filter_map(|i| get(&format!("Complete_Summary{i}")).map(|s| (s, num(&format!("CountFactor{i}")).unwrap_or(0) as i32))).collect();
            for m in ["A", "B"] {
                if let (Some(name), Some(n)) = (get(&format!("Complete_Monster{m}_Name")), num(&format!("Complete_Monster{m}_Count"))) {
                    q.monsters.push((name, n as i32));
                }
            }
            // « TID;a;b » (FUN_1401e1900 : « %d;%d;%d »).
            let item = |prefix: &str| {
                let raw = get(&format!("{prefix}_Tid"))?;
                let mut parts = raw.split(';').map(|p| p.trim().parse::<i32>().unwrap_or(0));
                let tid = parts.next().filter(|t| *t > 0)?;
                let n = |k: &str| num(&format!("{prefix}_{k}")).unwrap_or(0) as i32;
                Some(QuestItem { tid, count: n("Count"), slot: n("Upgrade_Slot"), intensify: n("Upgrade_Intensify"), extra: parts.next().unwrap_or(0) })
            };
            q.gifts = ["A", "B", "C", "D"].iter().filter_map(|g| item(&format!("Gift_Item{g}"))).collect();
            q.choices = (1..=8).filter_map(|i| item(&format!("SelectItem{i}"))).collect();
            data.quests.insert(tid as i32, q);
        }
    }
    log::info!(
        "PNJ : {} dialogue(s), {} groupe(s) de quêtes, {} quête(s) décrite(s) par le client",
        data.dialogs.len(),
        data.groups.len(),
        data.quests.len()
    );
    data.bank_prices = bank_slot_prices(&client_text(assets, "bankslotprice.txt"));
    data
}

impl NpcGlue {
    /// Échap : ferme les fenêtres des PNJ (dialogue, boutique, banque, quêtes, journal, saisies) ;
    /// rend `true` si l'une était ouverte.
    pub(super) fn close_windows(&mut self) -> bool {
        let open = self.dialog.is_some()
            || self.shop.is_some()
            || self.bank.is_some()
            || self.quest_npc.is_some()
            || self.show_journal
            || self.buy.is_some()
            || self.sell.is_some()
            || self.money.is_some()
            || self.buy_bank;
        self.dialog = None;
        self.shop = None;
        self.bank = None;
        self.quest_npc = None;
        self.show_journal = false;
        self.buy = None;
        self.sell = None;
        self.money = None;
        self.buy_bank = false;
        self.confirm_buy = None;
        open
    }

    fn shrine(&mut self, assets: &Assets) -> &ShrinePrices {
        self.shrine.get_or_insert_with(|| parse_shrine_prices(&client_text_in(assets, "ini", "itemdatatable.txt")))
    }

    fn data(&mut self, assets: &Assets) -> &NpcData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    fn quest_name(&mut self, assets: &Assets, tid: i32) -> String {
        self.data(assets).quests.get(&tid).map(|q| q.name.clone()).unwrap_or_default()
    }

    fn state(&self, tid: i32) -> QuestState {
        match self.journal.get(&tid) {
            Some(p) if p.done => QuestState::Ready,
            Some(_) => QuestState::InProgress,
            None if self.done.contains(&(tid as u32)) => QuestState::Done,
            None => QuestState::Available,
        }
    }

    /// Texte d'une quête comme `QuestMainText_Display2` (FUN_140658ae0) : nom entre guillemets et `Area`,
    /// « Objectifs de quete » (sysstr 442) et les objectifs (FUN_140658620), « Recompense de quete » (443),
    /// argent (« %d %s », sysstr 10), expérience (« %s %d », sysstr 5), objets, puis « Informations de
    /// quete » (441) et `Dialogue_Start`. Codes de couleur d'Archonia (`<H…>`) et balises d'objets
    /// (`<ITEM::…>`, voir [`QuestItem::tag`]) gardés. Écarts : la ligne « %s (%d) » (+0x1c88 du modèle, non
    /// identifié) n'est pas reprise ; les objectifs d'objets (sortes 3 et 11) sans le compte du sac.
    /// Ligne de `sysstr.txt`, gardée après la première lecture.
    fn sys(&mut self, assets: &Assets, id: u32) -> String {
        self.sys_cache.entry(id).or_insert_with(|| sysstr(assets, id).unwrap_or_default()).clone()
    }

    fn quest_text(&mut self, assets: &Assets, tid: i32, level: i32, money: i64) -> String {
        let progress = self.journal.get(&tid).copied();
        let Some(q) = self.data(assets).quests.get(&tid).cloned() else { return String::new() };
        let texts: HashMap<u32, String> = [442, 443, 441, 10, 5].into_iter().map(|id| (id, self.sys(assets, id))).collect();
        let s = |id: u32| texts.get(&id).cloned().unwrap_or_default();
        const WHITE: &str = "<HFFFFFFFF>";
        const HEAD: &str = "<HFF8DCAE0>";
        const DONE: &str = "<HFF5EFF00>";
        let mut t = format!("\n<HFFB6FF38>\"{}\"{WHITE}\n{}\n\n", q.name, q.area);
        t.push_str(&format!("{HEAD}{}{WHITE}\n", s(442)));
        for (line, kind) in &q.summary {
            let state = |met: bool| if met { DONE } else { WHITE };
            match kind {
                1 => t.push_str(&format!("{}{line}\n{WHITE}", state(level >= q.complete_level))),
                2 => t.push_str(&format!("{}{line}\n{WHITE}", state(money >= q.complete_money))),
                4 | 5 => {
                    let i = (*kind - 4) as usize;
                    let need = q.monsters.get(i).map_or(0, |m| m.1);
                    let have = progress.map_or(0, |p| if i == 0 { p.monster_a } else { p.monster_b });
                    t.push_str(&format!("{}{line}({have}/{need})\n{WHITE}", state(have >= need)));
                }
                _ => t.push_str(&format!("{line}\n")),
            }
        }
        t.push_str(&format!("\n{HEAD}{}{WHITE}\n", s(443)));
        if q.money != 0 {
            t.push_str(&format!("{} {}\n", q.money, s(10)));
        }
        if q.exp != 0 {
            t.push_str(&format!("{} {}\n", s(5), q.exp));
        }
        // Objets donnés puis, s'il y en a, « Selectionnez … » (sysstr 837, « </n></n><H%x>%s<H%x></n> ») et
        // les objets au choix : balises `<ITEM::…>` dessinées en icônes (AgcUIQuestText, FUN_14088cd40).
        for g in &q.gifts {
            t.push_str(&g.tag(false));
        }
        if !q.choices.is_empty() {
            t.push_str(&format!("\n\n{HEAD}{}{WHITE}\n", self.sys(assets, 837)));
            for c in &q.choices {
                t.push_str(&c.tag(true));
            }
        }
        t.push_str(&format!("\n{HEAD}{}{WHITE}\n{}\n", s(441), q.start));
        t
    }

    fn function_data(&self, npc: i32, f: i32) -> Option<i32> {
        self.functions.get(&npc)?.iter().find(|x| x.function == f)?.data
    }
}

/// Première case libre d'une banque (pages ouvertes) en commençant par la page affichée.
fn free_bank_cell(items: &HashMap<i32, ItemRecord>, first_page: i16, pages: i16) -> Option<[i16; 3]> {
    let taken: Vec<[i16; 3]> = items.values().filter(|i| i.status == Some(STATUS_BANK)).filter_map(|i| i.grid).collect();
    (0..pages)
        .map(|p| (first_page + p) % pages)
        .flat_map(|p| (0..BANK_ROWS).flat_map(move |r| (0..BANK_COLS).map(move |c| [p, r, c])))
        .find(|g| !taken.contains(g))
}

/// Ouvre une fonction d'un PNJ (bouton du dialogue ou commande) : boutique, banque, quêtes.
fn start_function(glue: &mut NpcGlue, npc: i32, f: i32, net: Option<&NetLink>, ui: &mut UiState) {
    match f {
        function::TRADE => {
            glue.shop = Some(Shop { npc, items: Vec::new(), loading: true, first_row: 0 });
            ui.show_inventory = true;
            glue.dialog = None;
            send(net, NpcCommand::ShopList(npc));
        }
        function::BANK => {
            glue.dialog = None;
            send(net, NpcCommand::BankOpen(npc));
        }
        // Coiffeur : AGPMEVENTCHARCUSTOMIZE op 0, l'accord ouvre `UI_Customizing` (voir `app_customize.rs`).
        archrust_netclient::customize::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Customize(archrust_netclient::customize::CustomizeCommand::Request(npc)));
            }
        }
        // Tableau des avis : AGPMWANTEDCRIMINAL op 0, l'accord ouvre `UI_WantedCriminal` (voir `app_wanted.rs`).
        archrust_netclient::wanted::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Wanted(archrust_netclient::wanted::WantedCommand::Request(npc)));
            }
        }
        archrust_netclient::remission::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Remission(archrust_netclient::remission::RemissionCommand::Request(npc)));
            }
        }
        // Marchand d'objets : AGPMAUCTION op 9, l'accord (op 10) ouvre `UI_Auction` (voir `app_auction.rs`).
        archrust_netclient::auction::FUNCTION => {
            glue.dialog = None;
            glue.auction_request = Some(npc);
        }
        // PNJ de production : AGPMEVENTPRODUCT op 0, l'accord ouvre `UI_PDSkillBuy` (voir `app_product.rs`).
        archrust_netclient::product::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Product(archrust_netclient::product::ProductCommand::Open(npc)));
            }
        }
        function::QUEST => {
            glue.dialog = None;
            send(net, NpcCommand::QuestWindow(npc));
        }
        function::SKILL_MASTER => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Skill(archrust_netclient::skills::SkillCommand::MasterOpen(npc)));
            }
        }
        // Gestionnaire de guilde : AGPMEVENTGUILD op 0, l'accord ouvre `Guild_Main` (voir `app_guild.rs`).
        archrust_netclient::guild::FUNCTION_GUILD => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Guild(archrust_netclient::guild::GuildCommand::TalkToManager(npc)));
            }
        }
        // Entrepôt de guilde : AGPMEVENTGUILD op 2, l'accord ouvre `UI_GuildWarehouse` (voir `app_guild.rs`).
        archrust_netclient::guild::FUNCTION_GUILD_WAREHOUSE => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Guild(archrust_netclient::guild::GuildCommand::WarehouseRequest(npc)));
            }
        }
        // Forgeron : AGPMEVENTITEMCONVERT op 0, l'accord ouvre `UI_ConvertPhysics` (voir `app_convert.rs`).
        archrust_netclient::convert::FUNCTION_ITEM_CONVERT => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Convert(archrust_netclient::convert::ConvertCommand::Open(npc)));
            }
        }
        // Réparation : AGPMEVENTITEMREPAIR op 3, l'accord ouvre `UI_Repair` (voir `app_repair.rs`).
        archrust_netclient::repair::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Repair(archrust_netclient::repair::RepairCommand::Open(npc)));
            }
        }
        // Siege Commander : AGPMEVENTSIEGEWARNPC op 1, l'accord ouvre `UI_SiegeWarMain` (voir `app_siege.rs`).
        archrust_netclient::siege::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Siege(archrust_netclient::siege::SiegeCommand::NpcRequest(npc)));
            }
        }
        // Intendant « Grand Advisor » : AGPMTAX op 0 (`FUN_1406341e0`), l'accord ouvre `UI_TaxSteward` (voir
        // `app_tax.rs`).
        archrust_netclient::tax::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Tax(archrust_netclient::tax::TaxCommand::Request(npc)));
            }
        }
        // Événement ArchLord : AGPMARCHLORD op 7 seule (`FUN_14061b230`, voir `app_archlord.rs`).
        archrust_netclient::archlord::FUNCTION => {
            glue.dialog = None;
            if let Some(net) = net {
                net.send(Command::Archlord(archrust_netclient::archlord::ArchlordCommand::EventRequest));
            }
        }
        _ => {}
    }
}

/// Prix de la page de banque suivante (Archonia FUN_140314970 : pages achetées < 3 et prix ≥ 0).
fn next_bank_price(glue: &mut NpcGlue, hud: &HudState, assets: &Assets) -> Option<i64> {
    let bought = hud.stats.bank_pages.unwrap_or(0).max(0) as usize;
    glue.data(assets).bank_prices.get(bought).copied().filter(|p| *p >= 0 && bought < 3)
}

/// « Acheter » d'`UI_BuyBank` (`Item_BuyBankSlot`, FUN_1406f6c80) : l'or porté est vérifié avant
/// l'envoi (sinon `Bank_NotEnoughBuyCost`).
fn confirm_buy_bank(glue: &mut NpcGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, _ui: &mut UiState) {
    glue.buy_bank = false;
    let Some(price) = next_bank_price(glue, hud, assets) else { return };
    if hud.stats.money.unwrap_or(0) < price {
        let text = super::portal_glue::ui_event_text(assets, "Bank_NotEnoughBuyCost");
        hud.push_system(&text);
    } else {
        send(net, NpcCommand::BuyBankSlot);
    }
}

/// Infobulle d'un objet de boutique : nom (et quantité), puis « Prix d'achat : » suivi des prix non nuls de la
/// pile, un par ligne (FUN_1405479c0 : « %s : », puis « %lld %s » avec `Money_Name`, `Shrine_Coin`,
/// `Charisma_Point`). `labels` : `Buy_Price`, `Money_Name`, `Shrine_Coin`, `Charisma_Point`.
pub(super) fn shop_tip(name: &str, count: i32, gold: i64, coin: i64, charisma: i64, labels: &[String; 4]) -> Vec<(String, egui::Color32)> {
    let n = i64::from(count.max(1));
    let title = if count > 1 { format!("{name} ({count})") } else { name.to_string() };
    let mut lines = vec![(title, egui::Color32::WHITE)];
    let values: Vec<String> = [(gold, &labels[1]), (coin, &labels[2]), (charisma, &labels[3])]
        .into_iter()
        .filter(|(v, _)| *v != 0)
        .map(|(v, l)| format!("{} {l}", v * n))
        .collect();
    let head = format!("{} : ", labels[0]);
    let indent = " ".repeat(head.chars().count() + 2);
    for (i, v) in values.into_iter().enumerate() {
        lines.push((if i == 0 { format!("{head}{v}") } else { format!("{indent}{v}") }, egui::Color32::WHITE));
    }
    lines
}

fn send(net: Option<&NetLink>, cmd: NpcCommand) {
    if let Some(net) = net {
        net.send(Command::Npc(cmd));
    }
}

/// Fenêtres des PNJ et leurs demandes, ainsi que les demandes du sac détournées par la boutique
/// (vente) et la banque (dépôt, retrait). À appeler après l'inventaire, avant `app_items::panel`.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ctx: &egui::Context,
    glue: &mut NpcGlue,
    views: &NpcViews,
    item_requests: &mut Vec<ItemUi>,
    items: &HashMap<i32, ItemRecord>,
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
    let mut out: Vec<NpcUi> = Vec::new();
    if let Some(view) = &views.dialog {
        crate::hud::draw_npc_dialog(ctx, hud, assets, view, &mut out);
    }
    if let Some(shop) = glue.shop.as_mut() {
        // Prix d'achat affiché avec la taxe de la région du joueur (« Buy_Price », `FUN_1405479c0`, voir `app_tax.rs`).
        let labels = ["Buy_Price", "Money_Name", "Shrine_Coin", "Charisma_Point"].map(|k| crate::hud::ui_message(hud, assets, k));
        let prices = &glue.shrine.get_or_insert_with(|| parse_shrine_prices(&client_text_in(assets, "ini", "itemdatatable.txt"))).prices;
        let items: Vec<ShopEntry> = shop
            .items
            .iter()
            .map(|e| {
                let price = super::tax_glue::taxed(e.price);
                let (charisma, coin) = prices.get(&e.tid).copied().unwrap_or((0, 0));
                ShopEntry { iid: e.iid, tid: e.tid, price, count: e.count, grid: e.grid, name: e.name.clone(), tip: shop_tip(&e.name, e.count, price, coin, charisma, &labels) }
            })
            .collect();
        let view = ShopView { npc_name: &views.shop_name, items: &items, loading: shop.loading, first_row: &mut shop.first_row };
        crate::hud::draw_shop(ctx, hud, assets, view, &mut out);
    }
    let pages = 1 + hud.stats.bank_pages.unwrap_or(0).clamp(0, 3) as i16;
    let bought = hud.stats.bank_pages.unwrap_or(0).max(0) as usize;
    let price = next_bank_price(glue, hud, assets);
    if glue.bank.is_some() {
        let money = hud.stats.bank_money;
        crate::hud::draw_bank(ctx, hud, assets, BankView { items: &views.bank_items, money, pages, page: &mut glue.bank_page, can_buy: price.is_some() }, &mut out);
    }
    if glue.bank.is_none() || price.is_none() {
        glue.buy_bank = false;
    }
    if let (true, Some(price)) = (glue.buy_bank, price) {
        let messages: HashMap<String, String> = client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
        let msg = |k: &str| messages.get(k).cloned().unwrap_or_default();
        let money = hud.stats.money.unwrap_or(0);
        let view = BuyBankView {
            title: msg("Item_BuyBankSlotTitle").replace("%d", &(bought + 1).to_string()),
            price_label: msg("Item_BuyBankSlotPrice"),
            price,
            money_name: msg("Money_Name"),
            affordable: price <= money,
        };
        let mut answer = Vec::new();
        crate::hud::draw_buy_bank(ctx, hud, assets, &view, &mut answer);
        for a in answer {
            glue.buy_bank = false;
            if a == PortalUi::BuyBank {
                confirm_buy_bank(glue, hud, assets, net, ui);
            }
        }
    }
    if let Some((journal, title, entries, text)) = &views.quests {
        let available = glue.selected_quest.is_some_and(|t| glue.state(t) == QuestState::Available);
        let left_label = glue.sys(assets, if available { 432 } else { 431 });
        let view = QuestView { journal: *journal, title: title.clone(), entries, selected: glue.selected_quest, text: text.clone(), left_label, reward_choice: glue.reward_choice };
        crate::hud::draw_quests(ctx, hud, assets, view, &mut out);
    }
    // Abandon : `QuestCancelDlg_Open` (« {{QuestMainCancel…}} » : sysstr 435 avec le nom, FUN_140655d60).
    if let Some(tid) = glue.abandon {
        let name = glue.quest_name(assets, tid);
        let text = glue.sys(assets, 435).replacen("%s", &name, 1);
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("quest-cancel", tid), &text) {
            glue.abandon = None;
            if yes {
                send(net, NpcCommand::QuestAbandon(tid));
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }

    // Demandes du sac pendant qu'une boutique ou la banque est ouverte.
    item_requests.retain(|r| match *r {
        ItemUi::Activate { iid, worn: false } if glue.shop.is_some() => {
            glue.sell = Some(iid);
            false
        }
        ItemUi::Activate { iid, worn: false } if glue.bank.is_some() => {
            match free_bank_cell(items, glue.bank_page, pages) {
                Some(grid) => send(net, NpcCommand::BankStore { iid, grid }),
                None => log::debug!("banque : aucune case libre sur la page {}", glue.bank_page),
            }
            false
        }
        ItemUi::DropOnBag { drag, grid } if drag.bank => {
            send(net, NpcCommand::BankTake { iid: drag.iid, grid: Some(grid) });
            false
        }
        _ => true,
    });

    for r in out {
        match r {
            NpcUi::CloseDialog => glue.dialog = None,
            NpcUi::Function(f) => {
                if let Some(npc) = glue.dialog {
                    start_function(glue, npc, f, net, ui);
                }
            }
            NpcUi::CloseShop => glue.shop = None,
            NpcUi::Buy { iid, ask_count: true } => glue.buy = Some((iid, 1)),
            // `NPCT_BuyItemDirect` : `Shrine_ItemBuyConfirmOpen_Event`, n = quantité de l'objet de boutique.
            NpcUi::Buy { iid, ask_count: false } => {
                if let Some(e) = glue.shop.as_ref().and_then(|s| s.items.iter().find(|e| e.iid == iid)) {
                    glue.confirm_buy = Some((iid, e.count.max(1)));
                }
            }
            NpcUi::Sell(iid) => glue.sell = Some(iid),
            NpcUi::CloseBank => glue.bank = None,
            NpcUi::BankDrop { drag, grid } => send(net, NpcCommand::BankStore { iid: drag.iid, grid }),
            NpcUi::BankTake(iid) => send(net, NpcCommand::BankTake { iid, grid: None }),
            NpcUi::BankMoney { deposit } => glue.money = Some((deposit, 0)),
            NpcUi::BankBuyRequest => glue.buy_bank = true,
            NpcUi::CloseQuests => {
                glue.quest_npc = None;
                glue.show_journal = false;
            }
            NpcUi::SelectQuest(tid) => {
                if glue.selected_quest != Some(tid) {
                    glue.reward_choice = None;
                }
                glue.selected_quest = Some(tid);
            }
            NpcUi::QuestSelectReward(k) => glue.reward_choice = Some(k),
            // Journal plein (60 quêtes) : `QuestFull_Message` au lieu de la demande (FUN_140655180).
            NpcUi::QuestAccept(tid) if glue.journal.len() >= 60 => {
                log::info!("quête {tid} : journal plein");
                glue.info = Some(super::portal_glue::ui_event_cached(&mut glue.events, assets, "QuestFull_Message")).filter(|t| !t.is_empty());
            }
            NpcUi::QuestAccept(tid) => send(net, NpcCommand::QuestAccept { npc: glue.quest_npc, tid }),
            // FUN_140655330 / FUN_140654970 : place dans le sac pour les objets donnés (sinon
            // `QuestInventoryMainFull_Message`), récompense au choix exigée s'il y en a (sinon uimessage
            // `QuestSelectItem`, notice au centre de l'écran dans Archonia), puis op 2, champ 7 = rang choisi
            // (-1 sans choix).
            NpcUi::QuestComplete(tid) => {
                let q = glue.data(assets).quests.get(&tid).cloned().unwrap_or_default();
                let used = items.values().filter(|i| i.status == Some(STATUS_INVENTORY)).count();
                if BAG_CELLS.saturating_sub(used) < q.gifts.len() {
                    glue.info = Some(super::portal_glue::ui_event_cached(&mut glue.events, assets, "QuestInventoryMainFull_Message")).filter(|t| !t.is_empty());
                } else if !q.choices.is_empty() && glue.reward_choice.is_none_or(|k| k >= q.choices.len()) {
                    let text = crate::hud::ui_message(hud, assets, "QuestSelectItem");
                    hud.push_system(&text);
                } else {
                    let reward = if q.choices.is_empty() { -1 } else { glue.reward_choice.map_or(-1, |k| k as i32) };
                    send(net, NpcCommand::QuestComplete { npc: glue.quest_npc, tid, reward });
                }
            }
            NpcUi::QuestAbandon(tid) => glue.abandon = Some(tid),
        }
    }

    // Quantité à acheter.
    if let Some((iid, mut count)) = glue.buy {
        let entry = glue.shop.as_ref().and_then(|s| s.items.iter().find(|e| e.iid == iid)).map(|e| (e.name.clone(), e.price));
        let _ = &entry;
        let mut n = count as i64;
        let answer = crate::hud::draw_buy_count(ctx, hud, assets, &mut n, 99);
        count = n.clamp(1, 99) as _;
        glue.buy = Some((iid, count));
        match (answer, &glue.shop) {
            (Some(true), Some(_)) => {
                glue.confirm_buy = Some((iid, count));
                glue.buy = None;
            }
            (Some(_), _) | (None, None) => glue.buy = None,
            _ => {}
        }
    }

    // Confirmation de l'achat (`UI_SHRINEITEM_BUY_CONFIRM`).
    if let Some((iid, count)) = glue.confirm_buy {
        let entry = glue.shop.as_ref().and_then(|s| s.items.iter().find(|e| e.iid == iid).map(|e| (s.npc, e.tid, e.name.clone(), e.price)));
        match entry {
            None => glue.confirm_buy = None,
            Some((npc, tid, name, unit_gold)) => {
                let shrine = glue.shrine(assets);
                let unit = shrine.prices.get(&tid).copied().unwrap_or((0, 0));
                let coins = shrine_coins(items, &shrine.coins);
                let charisma = hud.stats.factors.get(&(archrust_netclient::world::factor_part::STATUS, 4)).copied();
                let money = hud.stats.money.unwrap_or(0);
                let view = shrine_buy_view(name, tid, i64::from(count), unit_gold, super::tax_glue::taxed(unit_gold), unit, charisma.map_or(0, i64::from), coins, money);
                let mut answer = Vec::new();
                crate::hud::draw_shrine_buy_confirm(ctx, hud, assets, &view, &mut answer);
                for a in answer {
                    glue.confirm_buy = None;
                    if a != crate::hud::ShrineBuyUi::Buy {
                        continue;
                    }
                    // Charisme inconnu (facteur pas encore reçu) : le serveur tranche.
                    let refused = if charisma.is_some() && view.charisma_rest < 0 {
                        Some("NPCTMSG_NotEnoughCharisma")
                    } else if view.coin_rest < 0 {
                        Some("NPCTMSG_NotEnoughShrineCoin")
                    } else {
                        None
                    };
                    match refused {
                        Some(key) => glue.info = Some(super::portal_glue::ui_event_cached(&mut glue.events, assets, key)).filter(|t| !t.is_empty()),
                        None => send(net, NpcCommand::Buy { npc, iid, count }),
                    }
                }
            }
        }
    }

    // Vente, après accord.
    if let Some(iid) = glue.sell {
        let name = items.get(&iid).and_then(|i| i.template_id).and_then(|t| item_name_for_tid(assets, t));
        // Message de l'[EventMap] d'Archonia (`Action=4:1:-1:`), suivi du nom de l'objet.
        let text = format!("{}___[{}]", super::portal_glue::ui_event_cached(&mut glue.events, assets, "NPCTMSG_ConfirmSell"), name.as_deref().unwrap_or_default());
        let answer = crate::hud::confirm_box(ctx, hud, assets, ("sell", iid), &text);
        match (answer, &glue.shop) {
            (Some(true), Some(s)) => {
                send(net, NpcCommand::Sell { npc: s.npc, iid });
                glue.sell = None;
            }
            (Some(_), _) | (None, None) => glue.sell = None,
            _ => {}
        }
    }

    // Argent de la banque.
    if let Some((deposit, mut amount)) = glue.money {
        let max = if deposit { hud.stats.money } else { hud.stats.bank_money }.unwrap_or(0).max(0);
        let mut n = amount as i64;
        let answer = crate::hud::draw_bank_money(ctx, hud, assets, deposit, &mut n, max as i64);
        amount = n.clamp(0, max as i64) as _;
        glue.money = Some((deposit, amount));
        match answer {
            Some(true) if amount > 0 => {
                send(net, NpcCommand::BankMoney(if deposit { amount } else { -amount }));
                glue.money = None;
            }
            Some(_) => glue.money = None,
            None => {}
        }
    }
}

impl App {
    /// Fonctions des PNJ en vue (champ 4 de leur vue).
    pub(super) fn npc_observe(&mut self, ev: &WorldEvent) {
        if let WorldEvent::CharacterSeen(c) = ev {
            if !c.npc_functions.is_empty() {
                self.npc.functions.insert(c.cid, c.npc_functions.clone());
            }
        }
    }

    /// Message système sans texte (0x44) : codes de la banque 29 (au maximum) et 30 (page ajoutée).
    pub(super) fn npc_system_code(&mut self, code: i32) {
        let id = match code {
            29 => 717,
            30 => 718,
            _ => {
                log::debug!("message système {code} sans texte connu");
                return;
            }
        };
        if let Some(text) = sysstr(&self.assets, id) {
            self.hud.push_system(&text);
        }
    }

    /// Fonctions connues d'un PNJ.
    pub(super) fn npc_functions(&self, npc: i32) -> &[NpcFunction] {
        self.npc.functions.get(&npc).map(Vec::as_slice).unwrap_or(&[])
    }

    fn npc_position(&self, cid: i32) -> Option<Vec3> {
        self.entities.iter().find(|e| e.cid == cid).map(|e| e.position)
    }

    fn npc_name(&self, cid: i32) -> String {
        self.entities.iter().find(|e| e.cid == cid).map(|e| e.name.clone()).unwrap_or_else(|| "PNJ".into())
    }

    /// Parler à un PNJ qui a des fonctions : il devient la cible, le joueur s'en approche et le
    /// dialogue est demandé. Rend `false` pour un personnage sans fonctions.
    pub(super) fn npc_talk(&mut self, cid: i32) -> bool {
        if !self.npc.functions.contains_key(&cid) {
            return false;
        }
        self.addons.set_target(Some(cid));
        // Métamorphosé : ni approche ni dialogue, sans message, sauf pour l'ArchLord (voir `app_morph.rs`).
        if !self.morph_allows_npc() {
            return true;
        }
        let (Some(npc), Some(me)) = (self.npc_position(cid), self.player.as_ref().map(|p| p.position)) else { return true };
        let gap = Vec3::new(npc.x - me.x, 0.0, npc.z - me.z);
        if gap.length() <= TALK_RANGE {
            self.npc_request_dialog(cid);
            return true;
        }
        // S'approcher jusqu'à 80 % de la portée (480 unités) du PNJ.
        let dest = npc - gap.normalize_or_zero() * APPROACH_RANGE;
        if let (Some(net), Some(p)) = (&self.net, &mut self.player) {
            net.send(Command::MoveTo { position: p.position, destination: dest });
            p.destination = dest;
            self.walking = true;
        }
        self.npc.approach = Some((cid, Instant::now()));
        true
    }

    /// Demande le dialogue. Le serveur juge la distance sur la position connue au dernier arrêt :
    /// un joueur qui marche s'arrête d'abord là où il est.
    fn npc_request_dialog(&mut self, cid: i32) {
        if self.walking {
            self.walking = false;
            if let (Some(net), Some(p)) = (&self.net, &mut self.player) {
                p.destination = p.position;
                net.send(Command::Stop { position: p.position });
            }
        }
        send(self.net.as_ref(), NpcCommand::Dialog(cid));
    }

    /// À chaque image : dialogue demandé à l'arrivée près du PNJ ; fenêtres fermées si le joueur
    /// s'éloigne.
    pub(super) fn npc_frame(&mut self) {
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return };
        let dist = |s: &Self, cid: i32| s.npc_position(cid).map(|n| Vec3::new(n.x - me.x, 0.0, n.z - me.z).length());
        if let Some((cid, since)) = self.npc.approach {
            match dist(self, cid) {
                Some(d) if d <= TALK_RANGE => {
                    self.npc.approach = None;
                    self.npc_request_dialog(cid);
                }
                _ if since.elapsed() > Duration::from_secs(20) || !self.walking => self.npc.approach = None,
                _ => {}
            }
        }
        let far = |s: &Self, cid: Option<i32>| cid.is_some_and(|c| dist(s, c).is_none_or(|d| d > CLOSE_RANGE));
        if far(self, self.npc.dialog) {
            self.npc.dialog = None;
        }
        if far(self, self.npc.shop.as_ref().map(|s| s.npc)) {
            self.npc.shop = None;
        }
        if far(self, self.npc.bank) {
            self.npc.bank = None;
        }
        if far(self, self.npc.quest_npc) {
            self.npc.quest_npc = None;
        }
    }

    /// Réponses des PNJ.
    pub(super) fn on_npc_event(&mut self, ev: NpcEvent) {
        let assets = self.assets.clone();
        match ev {
            NpcEvent::DialogGranted { npc } => {
                self.npc.dialog = Some(npc);
                self.npc.line += 1;
            }
            NpcEvent::ShopItem { npc, iid, tid, price, count, grid } => {
                if let Some(s) = self.npc.shop.as_mut().filter(|s| s.npc == npc) {
                    let name = item_name_for_tid(&assets, tid).unwrap_or_default();
                    s.items.retain(|e| e.iid != iid);
                    s.items.push(ShopEntry { iid, tid, price, count, grid, name, tip: Vec::new() });
                }
            }
            // La fin de liste ne désigne pas toujours le PNJ (champ 0 absent) : boutique ouverte.
            NpcEvent::ShopEnd { npc } => {
                if let Some(s) = self.npc.shop.as_mut().filter(|s| s.npc == npc || npc == 0) {
                    s.loading = false;
                }
            }
            // Inventaire plein : boîte `NPCTMSG_InvenFull` de l'[EventMap] ; Archonia n'affiche rien d'autre
            // (les refus d'or arrivent en message système du serveur).
            NpcEvent::Bought { result } => {
                log::info!("boutique : achat, résultat {result}");
                if result == trade_result::INVENTORY_FULL {
                    let text = super::portal_glue::ui_event_cached(&mut self.npc.events, &assets, "NPCTMSG_InvenFull");
                    self.hud.push_system(&text);
                }
                // Boutique des Shrines : boîtes `Action=4:0` de l'[EventMap] (rappels 10 et 11 d'AgcmEventNPCTrade).
                let key = match result {
                    trade_result::NOT_ENOUGH_CHARISMA => Some("NPCTMSG_NotEnoughCharisma"),
                    trade_result::NOT_ENOUGH_SHRINE_COIN => Some("NPCTMSG_NotEnoughShrineCoin"),
                    _ => None,
                };
                if let Some(key) = key {
                    self.npc.info = Some(super::portal_glue::ui_event_cached(&mut self.npc.events, &assets, key)).filter(|t| !t.is_empty());
                }
            }
            NpcEvent::Sold { result } => log::info!("boutique : vente, résultat {result}"),
            NpcEvent::BankOpened { npc } => {
                self.npc.bank = Some(npc);
                self.ui.show_inventory = true;
            }
            NpcEvent::QuestWindow { npc } => {
                self.npc.quest_npc = Some(npc);
                self.npc.show_journal = false;
                self.npc.selected_quest = None;
                self.npc.reward_choice = None;
            }
            // Boîtes `QuestStart_Message` (sysstr 436, FUN_140656240) et `QuestComplete_Message` (sysstr 437,
            // FUN_140655ce0) avec le nom de la quête ; les refus et l'abandon n'affichent rien.
            NpcEvent::QuestResult { op, tid, success } => {
                let name = self.npc.quest_name(&assets, tid);
                log::info!("quête {tid} : op {op}, réussite {success}");
                let id = match (op, success) {
                    (1, true) => {
                        self.npc.journal.entry(tid).or_insert(QuestProgress { tid, monster_a: 0, monster_b: 0, done: false });
                        Some(436)
                    }
                    (3, true) => {
                        self.npc.journal.remove(&tid);
                        self.npc.done.insert(tid as u32);
                        Some(437)
                    }
                    (7, true) => {
                        self.npc.journal.remove(&tid);
                        None
                    }
                    _ => None,
                };
                if let Some(id) = id {
                    self.npc.info = Some(self.npc.sys(&assets, id).replacen("%s", &name, 1)).filter(|t| !t.is_empty());
                }
            }
            NpcEvent::QuestJournal { progress, update } => {
                let before = self.npc.journal.insert(progress.tid, progress);
                // Progression : Archonia la montre dans le texte de la quête, rien dans la discussion.
                if update && before != Some(progress) {
                    log::debug!("quête {} : {} / {}, remplie {}", progress.tid, progress.monster_a, progress.monster_b, progress.done);
                }
            }
            NpcEvent::QuestsDone(list) => self.npc.done = list.into_iter().collect(),
        }
    }

    /// Données des fenêtres pour cette image.
    pub(super) fn npc_views(&mut self) -> NpcViews {
        let assets = self.assets.clone();
        let mut v = NpcViews::default();
        if let Some(npc) = self.npc.dialog {
            let template = self.npc.function_data(npc, function::DIALOG);
            let line = self.npc.line;
            let text = template
                .and_then(|t| self.npc.data(&assets).dialogs.get(&t).filter(|l| !l.is_empty()).map(|l| l[line % l.len()].clone()))
                .unwrap_or_default();
            // Libellés d'Archonia (FUN_1406ca6a0) : chaîne `sysstr` de la fonction, « Quitter »
            // (`StaticText_Exit`) pour finir.
            let mut buttons = Vec::new();
            for f in self.npc.functions.get(&npc).into_iter().flatten() {
                let id = match f.function {
                    function::TRADE => 324,
                    function::BANK => 326,
                    function::QUEST => 331,
                    function::SKILL_MASTER => 14,
                    archrust_netclient::guild::FUNCTION_GUILD => 328,
                    // Entrepôt de guilde chez les gardiens de banque (FUN_1406ca6a0 : fonction 0x23).
                    archrust_netclient::guild::FUNCTION_GUILD_WAREHOUSE => 611,
                    archrust_netclient::convert::FUNCTION_ITEM_CONVERT => 327,
                    // Réparation (fonction 0x11) : « Repair ».
                    archrust_netclient::repair::FUNCTION => 325,
                    // Pardonneur (fonction 0x1F) : « Pardon ».
                    archrust_netclient::remission::FUNCTION => 335,
                    // Coiffeur (fonction 0x1D) : « Change Appearance » ; tableau des avis (0x20) : « Bounties ».
                    archrust_netclient::customize::FUNCTION => 334,
                    archrust_netclient::wanted::FUNCTION => 336,
                    // Production (fonction 0x18) : « Crafting ».
                    archrust_netclient::product::FUNCTION => 329,
                    // Marchand d'objets (fonction 0x1C) : « Auction House ».
                    archrust_netclient::auction::FUNCTION => 332,
                    // Siege Commander (fonction 0x21) : « Apply for a Siege War ».
                    archrust_netclient::siege::FUNCTION => 529,
                    // Intendant (fonction 0x22) : « Tax » ; événement ArchLord (0x24) : « Archlord ».
                    archrust_netclient::tax::FUNCTION => 609,
                    archrust_netclient::archlord::FUNCTION => 672,
                    _ => continue,
                };
                buttons.push((f.function, sysstr(&assets, id).unwrap_or_default()));
            }
            let exit_label = client_text(&assets, "uimessage.txt").lines().find_map(|l| l.strip_prefix("StaticText_Exit\t")).map(|t| t.trim().to_string()).unwrap_or_default();
            v.dialog = Some(DialogView { npc_name: self.npc_name(npc), text, buttons, exit_label });
        }
        if let Some(s) = &self.npc.shop {
            v.shop_name = self.npc_name(s.npc);
        }
        if self.npc.bank.is_some() {
            v.bank_items = self
                .items
                .values()
                .filter(|i| i.status == Some(STATUS_BANK))
                .map(|i| (i.clone(), i.template_id.and_then(|t| item_name_for_tid(&assets, t)).unwrap_or_default()))
                .collect();
        }
        let journal = self.npc.show_journal && self.npc.quest_npc.is_none();
        if let Some(npc) = self.npc.quest_npc.filter(|_| !journal) {
            let group = self.npc.function_data(npc, function::QUEST);
            let (give, confirm) = group.and_then(|g| self.npc.data(&assets).groups.get(&g).cloned()).unwrap_or_default();
            let mut tids: Vec<i32> = give.iter().copied().filter(|t| !self.npc.done.contains(&(*t as u32))).collect();
            tids.extend(confirm.iter().copied().filter(|t| self.npc.journal.contains_key(t) && !give.contains(t)));
            let entries = self.quest_entries(&tids);
            // Titre : texte fixe de la fenêtre (`CONTROL_16487`), le nom du PNJ reste pour les journaux.
            v.quests = Some((false, self.npc_name(npc), entries, String::new()));
        } else if journal {
            let tids: Vec<i32> = self.npc.journal.keys().copied().collect();
            let entries = self.quest_entries(&tids);
            v.quests = Some((true, String::new(), entries, String::new()));
        }
        if let Some(q) = v.quests.as_mut() {
            if self.npc.selected_quest.is_none_or(|t| !q.2.iter().any(|e| e.tid == t)) {
                self.npc.selected_quest = q.2.first().map(|e| e.tid);
                self.npc.reward_choice = None;
            }
            q.3 = match self.npc.selected_quest {
                Some(t) => self.npc.quest_text(&assets, t, self.hud.stats.level.unwrap_or(0), self.hud.stats.money.unwrap_or(0)),
                None => String::new(),
            };
        }
        v
    }

    fn quest_entries(&mut self, tids: &[i32]) -> Vec<QuestEntry> {
        let assets = self.assets.clone();
        tids.iter().map(|&tid| QuestEntry { tid, name: self.npc.quest_name(&assets, tid), state: self.npc.state(tid) }).collect()
    }

    /// `/parler [nom]`, `/boutique`, `/banque`, `/registraire` (gestionnaire de guilde), `/quetespnj`, `/quetes`, `/prendrequete TID`,
    /// `/abandonner TID`.
    pub(super) fn npc_slash(&mut self, cmd: &str, arg: &str) -> bool {
        match cmd {
            // Banque ouverte : `/agrandir` comme le bouton « Agrandir », `/agrandir oui` comme « Acheter ».
            "/agrandir" | "/expandbank" => {
                if self.npc.bank.is_none() || next_bank_price(&mut self.npc, &self.hud, &self.assets).is_none() {
                    self.ui.push_chat("[banque] Ouvrez la banque d'un gardien ; la page suivante doit être en vente contre de l'or.");
                } else if matches!(arg.trim(), "oui" | "yes") {
                    let assets = self.assets.clone();
                    confirm_buy_bank(&mut self.npc, &mut self.hud, &assets, self.net.as_ref(), &mut self.ui);
                } else {
                    self.npc.buy_bank = true;
                }
            }
            // Fonction du PNJ le plus proche qui la propose, s'il est à portée.
            "/boutique" | "/shop" | "/banque" | "/bank" | "/quetespnj" | "/npcquests" | "/instructeur" | "/trainer" | "/registraire" | "/registrar" | "/forgeron" | "/smith" | "/entrepot" | "/gstorage" => {
                let f = match cmd {
                    "/boutique" | "/shop" => function::TRADE,
                    "/forgeron" | "/smith" => archrust_netclient::convert::FUNCTION_ITEM_CONVERT,
                    "/registraire" | "/registrar" => archrust_netclient::guild::FUNCTION_GUILD,
                    "/entrepot" | "/gstorage" => archrust_netclient::guild::FUNCTION_GUILD_WAREHOUSE,
                    "/banque" | "/bank" => function::BANK,
                    "/instructeur" | "/trainer" => function::SKILL_MASTER,
                    _ => function::QUEST,
                };
                let me = self.player.as_ref().map(|p| p.position).unwrap_or(Vec3::ZERO);
                let near = self
                    .entities
                    .iter()
                    .filter(|e| self.npc.functions.get(&e.cid).is_some_and(|fs| fs.iter().any(|x| x.function == f)))
                    .map(|e| (e.cid, Vec3::new(e.position.x - me.x, 0.0, e.position.z - me.z).length()))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match near {
                    Some((cid, d)) if d <= TALK_RANGE * 1.2 => start_function(&mut self.npc, cid, f, self.net.as_ref(), &mut self.ui),
                    Some((cid, _)) => {
                        self.ui.push_chat(format!("[PNJ] {} est trop loin : approchez-vous (/parler).", self.npc_name(cid)));
                    }
                    None => self.ui.push_chat("[PNJ] Personne ici ne propose cela."),
                }
            }
            "/parler" | "/talk" => {
                let wanted = arg.trim().to_lowercase();
                let me = self.player.as_ref().map(|p| p.position).unwrap_or(Vec3::ZERO);
                let pick = if wanted.is_empty() {
                    self.addons.target_cid().filter(|c| self.npc.functions.contains_key(c))
                } else {
                    self.entities
                        .iter()
                        .filter(|e| self.npc.functions.contains_key(&e.cid) && e.name.to_lowercase().contains(&wanted))
                        .min_by_key(|e| e.position.distance(me) as i64)
                        .map(|e| e.cid)
                };
                match pick {
                    Some(cid) => {
                        self.npc_talk(cid);
                    }
                    None => self.ui.push_chat("[PNJ] Aucun personnage à qui parler (ciblez-le ou donnez son nom)."),
                }
            }
            "/quetes" | "/quêtes" | "/quests" | "/journal" => {
                self.npc.show_journal = !self.npc.show_journal;
                if self.npc.show_journal {
                    self.npc.quest_npc = None;
                }
            }
            "/prendrequete" | "/prendrequête" | "/abandonner" | "/abandon" => match arg.trim().parse::<i32>() {
                Ok(tid) => send(
                    self.net.as_ref(),
                    if cmd.starts_with("/prendre") { NpcCommand::QuestAccept { npc: None, tid } } else { NpcCommand::QuestAbandon(tid) },
                ),
                Err(_) => self.ui.push_chat(format!("[quête] usage : {cmd} TID")),
            },
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balise_des_objets_de_quete() {
        let q = QuestItem { tid: 65461, count: 3, slot: 2, intensify: 5, extra: 1 };
        assert_eq!(q.tag(false), "<ITEM::3::65461::2::5::1::0>");
        assert_eq!(q.tag(true), "<ITEM::3::65461::2::5::1::1>");
    }

    #[test]
    fn lecture_des_tables() {
        let rows = tsv("ID\tnom\n1\t\"un\ndeux\"\tx\n\t\tsuite\n");
        assert_eq!(rows[1], vec!["1".to_string(), "un\ndeux".to_string(), "x".to_string()]);
        assert_eq!(rows[2], vec!["".to_string(), "".to_string(), "suite".to_string()]);
        assert_eq!(clean("Detruisez <HFFFA7272><[Lord of the Vale]><HFFFFFFFF>."), "Detruisez Lord of the Vale.");
        assert_eq!(clean("a <b> c"), "a <b> c");
        assert_eq!(clean("Detruisez 6 <POS::115::-266267::-434221::[Winged Latch Deck]>."), "Detruisez 6 Winged Latch Deck.");
    }

    #[test]
    fn case_libre_de_la_banque() {
        let mut items = HashMap::new();
        items.insert(1, ItemRecord { operation: 0, id: 1, template_id: Some(1), owner_cid: None, count: Some(1), status: Some(STATUS_BANK), grid: Some([0, 0, 0]) });
        assert_eq!(free_bank_cell(&items, 0, 1), Some([0, 0, 1]));
        assert_eq!(free_bank_cell(&items, 1, 2), Some([1, 0, 0]));
    }

    #[test]
    fn boutique_des_shrines() {
        let text = "TID\tItemName\tItemType\tType\tNPC Price\tNPC Charisma Price\tNPC Shrine2 Coin\tNPC Shrine Coin\n\
                    1836\tLetter of Great Pardon\t1\t0\t\t\t\t75\n\
                    50139\tElixir\t1\t0\t\t2000\t\t\n\
                    15213\tKaligo\t0\t0\t\t160000\t80\t\n\
                    53000\tArena Coin\t2\t6\t\t\t\t\n\
                    190\tPotion\t1\t1\t25\t\t\t\n";
        let p = parse_shrine_prices(text);
        assert_eq!(p.prices.get(&1836), Some(&(0, 75)));
        assert_eq!(p.prices.get(&50139), Some(&(2000, 0)));
        // Prix Bloodmark (col. 30, Archonia) ignoré : seul le charisme reste.
        assert_eq!(p.prices.get(&15213), Some(&(160000, 0)));
        assert!(!p.prices.contains_key(&190));
        assert_eq!(p.coins, HashSet::from([53000]));
        let mut items = HashMap::new();
        for (id, count, status) in [(1, 60, STATUS_INVENTORY), (2, 30, STATUS_INVENTORY), (3, 99, STATUS_BANK)] {
            items.insert(id, ItemRecord { operation: 0, id, template_id: Some(53000), owner_cid: None, count: Some(count), status: Some(status), grid: Some([0, 0, id as i16]) });
        }
        assert_eq!(shrine_coins(&items, &p.coins), 90);
        // Deux lettres : 150 pièces pour 90 portées ; or taxé dans le prix, pas dans le reste.
        let v = shrine_buy_view("Lettre".into(), 1836, 2, 100, 110, (0, 75), 500, 90, 1000);
        assert_eq!((v.coin_price, v.coin_rest, v.charisma_price, v.charisma_rest), (150, -60, 0, 500));
        assert_eq!((v.gold_price, v.gold_rest), (220, 800));
        let labels = ["Prix d'achat".to_string(), "Or".to_string(), "Pieces d'arene".to_string(), "Points de charisme".to_string()];
        let tip: Vec<String> = shop_tip("Lettre", 1, 0, 75, 2000, &labels).into_iter().map(|(t, _)| t).collect();
        assert_eq!(tip[0], "Lettre");
        assert_eq!(tip[1], "Prix d'achat : 75 Pieces d'arene");
        assert!(tip[2].ends_with("2000 Points de charisme") && tip[2].starts_with(' '));
    }

    #[test]
    fn prix_des_pages_de_banque() {
        assert_eq!(bank_slot_prices("Slot\tPrice\t\r\n1\t1000000\t# commentaire\r\n2\t-1\t\r\n3\t-1\t\r\n"), vec![1_000_000, -1, -1]);
    }
}
