//! Maison des ventes entre joueurs (AgpmAuction, 0x0B ; PNJ « <Item Trader> », fonction 28).
//!
//! Relevé dans Archonia (constructeur `FUN_140268eb0`, réception `FUN_140269a70`, envois `0x140722800` à
//! `0x140722d60`) : achat immédiat à prix fixe (or et / ou CC) d'une pile entière.
//!
//! ```text
//! op 9  (1 CID, 2 EVENT_BASE du PNJ)                         ouvrir la planche (C → S) → op 10 (même EVENT_BASE)
//! op 12 (2 SELECT2 {TID, docID, sens, 0})                     recherche (C → S) ; réponse : SELECT2 + total + ROWSET
//! op 7  (2 BUY {docID, quantité, TID, prix or, 0})            achat → op 7 (3 résultat : 5, 7, 12)
//! op 4  (2 SELL {IID, quantité, or, CC, —, parchemin})        mise en vente → op 4 (6 / 14 : limite) puis op 1
//! op 5  (2 CANCEL {salesID})                                  annulation → op 5 (7 : sac plein) puis op 2
//! op 6  (2 CONFIRM {salesID})                                 encaissement → op 6 (gains) puis op 2
//! op 1 / 2 / 3 (2 SALES)                                      vente ajoutée / retirée / statut (1 vendu)
//! op 8                                                        avis (sans sous-paquet)
//! ```
//!
//! Le rowset de la réponse donne une ligne par vente, chaque colonne étant une chaîne terminée par 0 placée à
//! l'un des décalages (U32) du bloc 7 : 0 docID, 1 vendeur, 3 prix or, 4 prix CC, 6 TID, 7 quantité (`0x1405045d0`).

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction d'événement des marchands d'objets (`npc.ini` : `HaveAuctionEvent=1`).
pub const FUNCTION: i32 = 28;
/// Curseur de la première requête d'un chargement complet (`0x140509770`).
pub const START_DOC_ID: u64 = 9_999_999_999_999;
/// Lignes au plus par réponse (et par page de la planche).
pub const ROWS_PER_PAGE: usize = 15;
/// Requêtes au plus pour un chargement complet (`0x1405045d0`).
pub const MAX_REQUESTS: u32 = 200;
/// Plafonds et minimum des prix de `UI_AuctionCreateSale` (`0x14073b580`).
pub const MAX_GOLD: i64 = 100_000_000_000;
pub const MAX_CC: i64 = 1_000_000;
pub const MIN_GOLD: i64 = 100_000;
/// Frais de mise en vente, en pourcentage du prix or et du prix CC.
pub const FEE_PERCENT: i64 = 3;
/// « Marvel Scroll of Release » (usage 35) : libère un objet lié pour la vente.
pub const MARVEL_SCROLL_TID: i32 = 11256;
/// Ventes au plus par personnage (100 cases de `UI_SalesBox2`).
pub const MAX_SALES: usize = 100;

/// Opérations d'AgpmAuction (champ 0).
pub mod op {
    pub const ADD_SALES: i8 = 1;
    pub const REMOVE_SALES: i8 = 2;
    pub const UPDATE_SALES: i8 = 3;
    pub const SELL: i8 = 4;
    pub const CANCEL: i8 = 5;
    pub const CONFIRM: i8 = 6;
    pub const BUY: i8 = 7;
    pub const NOTIFY: i8 = 8;
    pub const EVENT_REQUEST: i8 = 9;
    pub const EVENT_GRANT: i8 = 10;
    pub const LOGIN: i8 = 11;
    pub const SELECT2: i8 = 12;
}

/// Statut d'une vente (champ 7 de SALES).
pub mod status {
    pub const ON_SALE: i16 = 0;
    pub const SOLD: i16 = 1;
}

/// Codes de résultat (champ 3) traités par le client.
pub mod result {
    pub const SUCCESS: i32 = 0;
    /// Achat : quantité impossible (sysstr 274).
    pub const BAD_QUANTITY: i32 = 5;
    /// Vente : plus de place (279) ; achat (275) et annulation (280) : sac plein.
    pub const LIMIT: i32 = 6;
    pub const INVENTORY_FULL: i32 = 7;
    /// Achat : vente disparue (276).
    pub const NOT_AVAILABLE: i32 = 12;
    pub const LIMIT2: i32 = 14;
}

/// Frais de mise en vente d'un prix (division entière, `0x14073b580`).
pub fn fee(price: i64) -> i64 {
    price * FEE_PERCENT / 100
}

/// Vente du joueur (mini SALES).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sale {
    pub sales_id: i32,
    pub doc_id: u64,
    pub item_seq: u64,
    pub iid: i32,
    pub gold: i64,
    pub cc: i64,
    pub count: i16,
    pub status: i16,
    pub date: String,
    pub tid: i32,
}

/// Ligne de la planche (rowset d'une recherche).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardRow {
    pub doc_id: u64,
    pub seller: String,
    pub gold: i64,
    pub cc: i64,
    pub tid: i32,
    pub count: i32,
}

/// Demandes de la maison des ventes.
#[derive(Debug, Clone, PartialEq)]
pub enum AuctionCommand {
    /// Ouvrir la planche auprès du marchand `npc` (op 9).
    Open(i32),
    /// Ventes du TID après le curseur `doc_id` (op 12) ; sens 0 : docID décroissants sous le curseur.
    Search { tid: i32, doc_id: u64, direction: i16 },
    /// Acheter toute la ligne `doc_id` (op 7) ; le client n'envoie pas le prix CC.
    Buy { doc_id: u64, count: i16, tid: i32, gold: i64 },
    /// Mettre en vente une pile entière (op 4) ; `marvel` : IID du parchemin (0 sans).
    Sell { iid: i32, count: i16, gold: i64, cc: i64, marvel: i32 },
    /// Annuler une vente (op 5).
    Cancel(i32),
    /// Encaisser une vente conclue (op 6).
    Confirm(i32),
}

/// Réponses du serveur.
#[derive(Debug, Clone, PartialEq)]
pub enum AuctionEvent {
    /// Planche accordée (op 10) par le PNJ `npc`.
    Granted { cid: i32, npc: i32 },
    SaleAdded { cid: i32, sale: Sale },
    SaleRemoved { cid: i32, sales_id: i32 },
    SaleStatus { cid: i32, sales_id: i32, status: i16 },
    /// Réponse à la mise en vente (op 4).
    Sold { cid: i32, result: i32, iid: i32, count: i16, gold: i64, cc: i64 },
    /// Réponse à l'annulation (op 5).
    Cancelled { cid: i32, result: i32, sales_id: i32, tid: i32, count: i16 },
    /// Réponse à l'encaissement (op 6).
    Confirmed { cid: i32, result: i32, sales_id: i32, tid: i32, count: i16, gold: i64, cc: i64 },
    /// Réponse à l'achat (op 7).
    Bought { cid: i32, result: i32, doc_id: u64, count: i16, tid: i32, gold: i64, cc: i64 },
    Notify { cid: i32 },
    /// Réponse à une recherche (op 12).
    Results { cid: i32, tid: i32, doc_id: u64, direction: i16, total: i32, rows: Vec<BoardRow> },
    /// Drapeaux d'état d'un objet (AGPMITEM champ 19) : liaison, lu à part de l'objet.
    ItemFlags { iid: i32, flags: u32 },
}

/// Bits des drapeaux d'état d'un objet (`FUN_1401c27b0`, `FUN_1401c2960`).
pub mod item_flag {
    /// Lié (à l'équipement, au ramassage, à l'usage) : vente avec un parchemin Marvel seulement (sysstr 841).
    pub const BOUND_MASK: u32 = 0x7;
    /// Lié définitivement : jamais en vente (843).
    pub const BOUND_FOREVER: u32 = 1 << 30;
}

fn mini_packet(s: &'static Schema, pairs: &[(usize, Value)]) -> Value {
    Value::Packet(build_mini(s, &sparse(s, pairs)).unwrap_or_default())
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &AuctionCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::AUCTION;
    let (op, detail) = match cmd {
        AuctionCommand::Open(npc) => (op::EVENT_REQUEST, crate::npc::event_base(*npc, FUNCTION)),
        AuctionCommand::Search { tid, doc_id, direction } => (
            op::SELECT2,
            mini_packet(mini::AUCTION_SELECT2, &[(0, Value::I32(*tid)), (1, Value::U64(*doc_id)), (2, Value::I16(*direction)), (3, Value::I32(0))]),
        ),
        AuctionCommand::Buy { doc_id, count, tid, gold } => (
            op::BUY,
            mini_packet(mini::AUCTION_BUY, &[(0, Value::I64(*doc_id as i64)), (1, Value::I16(*count)), (2, Value::I32(*tid)), (3, Value::I64(*gold)), (4, Value::I64(0))]),
        ),
        AuctionCommand::Sell { iid, count, gold, cc, marvel } => (
            op::SELL,
            mini_packet(mini::AUCTION_SELL, &[(0, Value::I32(*iid)), (1, Value::I16(*count)), (2, Value::I64(*gold)), (3, Value::I64(*cc)), (5, Value::I32(*marvel))]),
        ),
        AuctionCommand::Cancel(id) => (op::CANCEL, mini_packet(mini::AUCTION_CANCEL, &[(0, Value::I32(*id))])),
        AuctionCommand::Confirm(id) => (op::CONFIRM, mini_packet(mini::AUCTION_CONFIRM, &[(0, Value::I32(*id))])),
    };
    (s, sparse(s, &[(0, Value::I8(op)), (1, Value::I32(me)), (2, detail)]))
}

fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn u64_of(v: &[Option<Value>], i: usize) -> u64 {
    match v.get(i).and_then(Option::as_ref) {
        Some(Value::U64(x)) => *x,
        Some(other) => other.as_i64().unwrap_or(0) as u64,
        None => 0,
    }
}

/// Vente d'un mini SALES.
pub fn decode_sale(bytes: &[u8]) -> Option<Sale> {
    let (_, v) = decode_mini(mini::AUCTION_SALES, bytes).ok()?;
    Some(Sale {
        sales_id: int(&v, 0)? as i32,
        doc_id: u64_of(&v, 1),
        item_seq: u64_of(&v, 2),
        iid: int(&v, 3).unwrap_or(0) as i32,
        gold: int(&v, 4).unwrap_or(0),
        cc: int(&v, 5).unwrap_or(0),
        count: int(&v, 6).unwrap_or(1) as i16,
        status: int(&v, 7).unwrap_or(0) as i16,
        date: v.get(8).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default(),
        tid: int(&v, 9).unwrap_or(0) as i32,
    })
}

/// Mini SALES (envoi du serveur, essais).
pub fn encode_sale(s: &Sale) -> Vec<u8> {
    let m = mini::AUCTION_SALES;
    build_mini(
        m,
        &sparse(
            m,
            &[
                (0, Value::I32(s.sales_id)),
                (1, Value::U64(s.doc_id)),
                (2, Value::U64(s.item_seq)),
                (3, Value::I32(s.iid)),
                (4, Value::I64(s.gold)),
                (5, Value::I64(s.cc)),
                (6, Value::I16(s.count)),
                (7, Value::I16(s.status)),
                (8, Value::str(s.date.as_bytes())),
                (9, Value::I32(s.tid)),
            ],
        ),
    )
    .unwrap_or_default()
}

/// Colonnes d'un rowset : une ligne par entrée, chaînes terminées par 0 (`0x14026b9b0`).
pub fn decode_rowset(bytes: &[u8]) -> Vec<Vec<String>> {
    let Ok((_, v)) = decode_mini(mini::AUCTION_ROWSET, bytes) else { return Vec::new() };
    let rows = int(&v, 3).unwrap_or(0).max(0) as usize;
    let row_size = int(&v, 5).unwrap_or(0).max(0) as usize;
    let block = |i: usize| v.get(i).and_then(Option::as_ref).and_then(Value::as_bytes).unwrap_or(&[]);
    let (buffer, offsets) = (block(6), block(7));
    let offsets: Vec<usize> = offsets.chunks_exact(4).take(32).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize).collect();
    let mut out = Vec::new();
    for r in 0..rows {
        let Some(row) = buffer.get(r * row_size..(r + 1) * row_size) else { break };
        let cols = offsets
            .iter()
            .map(|&o| {
                let cell = row.get(o..).unwrap_or(&[]);
                let end = cell.iter().position(|&b| b == 0).unwrap_or(cell.len());
                cell[..end].iter().map(|&b| b as char).collect::<String>()
            })
            .collect();
        out.push(cols);
    }
    out
}

/// Rowset de lignes de colonnes (envoi du serveur, essais) : colonnes de largeur fixe, chaînes terminées par 0.
pub fn encode_rowset(rows: &[Vec<String>]) -> Vec<u8> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols).map(|c| rows.iter().map(|r| r.get(c).map_or(0, String::len)).max().unwrap_or(0) + 1).collect();
    let mut offsets = Vec::new();
    let mut at = 0usize;
    for w in &widths {
        offsets.extend_from_slice(&(at as u32).to_le_bytes());
        at += w;
    }
    let row_size = at;
    let mut buffer = vec![0u8; row_size * rows.len()];
    for (r, row) in rows.iter().enumerate() {
        let mut o = r * row_size;
        for (c, w) in widths.iter().enumerate() {
            if let Some(cell) = row.get(c) {
                buffer[o..o + cell.len()].copy_from_slice(cell.as_bytes());
            }
            o += w;
        }
    }
    let m = mini::AUCTION_ROWSET;
    build_mini(
        m,
        &sparse(
            m,
            &[
                (0, Value::I32(0)),
                (3, Value::U32(rows.len() as u32)),
                (4, Value::U32(cols as u32)),
                (5, Value::U32(row_size as u32)),
                (6, Value::Block(buffer)),
                (7, Value::Block(offsets)),
            ],
        ),
    )
    .unwrap_or_default()
}

/// Ligne de la planche d'après ses colonnes (atoi / _atoi64 : 0 si illisible).
pub fn board_row(cols: &[String]) -> BoardRow {
    let num = |i: usize| cols.get(i).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    BoardRow {
        doc_id: num(0).max(0) as u64,
        seller: cols.get(1).cloned().unwrap_or_default(),
        gold: num(3),
        cc: num(4),
        tid: num(6) as i32,
        count: num(7).max(1) as i32,
    }
}

/// Paquet AgpmAuction traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<AuctionEvent> {
    if ptype != packet_type::AUCTION {
        return None;
    }
    let cid = int(values, 1).unwrap_or(0) as i32;
    let result = int(values, 3).unwrap_or(0) as i32;
    let detail = values.get(2).and_then(Option::as_ref).and_then(Value::as_bytes);
    let sub = |s: &'static Schema| detail.and_then(|b| decode_mini(s, b).ok()).map(|(_, v)| v).unwrap_or_default();
    Some(match int(values, 0)? as i8 {
        op::ADD_SALES => AuctionEvent::SaleAdded { cid, sale: decode_sale(detail?)? },
        op::REMOVE_SALES => AuctionEvent::SaleRemoved { cid, sales_id: int(&sub(mini::AUCTION_SALES), 0)? as i32 },
        op::UPDATE_SALES => {
            let v = sub(mini::AUCTION_SALES);
            AuctionEvent::SaleStatus { cid, sales_id: int(&v, 0)? as i32, status: int(&v, 7).unwrap_or(0) as i16 }
        }
        op::SELL => {
            let v = sub(mini::AUCTION_SELL);
            AuctionEvent::Sold { cid, result, iid: int(&v, 0).unwrap_or(0) as i32, count: int(&v, 1).unwrap_or(1) as i16, gold: int(&v, 2).unwrap_or(0), cc: int(&v, 3).unwrap_or(0) }
        }
        op::CANCEL => {
            let v = sub(mini::AUCTION_CANCEL);
            AuctionEvent::Cancelled { cid, result, sales_id: int(&v, 0).unwrap_or(0) as i32, tid: int(&v, 1).unwrap_or(0) as i32, count: int(&v, 2).unwrap_or(1) as i16 }
        }
        op::CONFIRM => {
            let v = sub(mini::AUCTION_CONFIRM);
            AuctionEvent::Confirmed {
                cid,
                result,
                sales_id: int(&v, 0).unwrap_or(0) as i32,
                tid: int(&v, 1).unwrap_or(0) as i32,
                count: int(&v, 2).unwrap_or(1) as i16,
                gold: int(&v, 3).unwrap_or(0),
                cc: int(&v, 4).unwrap_or(0),
            }
        }
        op::BUY => {
            let v = sub(mini::AUCTION_BUY);
            AuctionEvent::Bought {
                cid,
                result,
                doc_id: int(&v, 0).unwrap_or(0) as u64,
                count: int(&v, 1).unwrap_or(1) as i16,
                tid: int(&v, 2).unwrap_or(0) as i32,
                gold: int(&v, 3).unwrap_or(0),
                cc: int(&v, 4).unwrap_or(0),
            }
        }
        op::NOTIFY => AuctionEvent::Notify { cid },
        op::EVENT_GRANT => {
            let npc = detail.and_then(|b| decode_mini(mini::EVENT_BASE, b).ok()).and_then(|(_, v)| int(&v, 1)).unwrap_or(0) as i32;
            AuctionEvent::Granted { cid, npc }
        }
        op::SELECT2 => {
            let v = sub(mini::AUCTION_SELECT2);
            let rowset = v.get(4).and_then(Option::as_ref).and_then(Value::as_bytes).unwrap_or(&[]);
            let rows = decode_rowset(rowset).iter().map(|c| board_row(c)).collect();
            AuctionEvent::Results {
                cid,
                tid: int(&v, 0).unwrap_or(0) as i32,
                doc_id: u64_of(&v, 1),
                direction: int(&v, 2).unwrap_or(0) as i16,
                total: int(&v, 3).unwrap_or(0) as i32,
                rows,
            }
        }
        _ => return None,
    })
}

/// Drapeaux d'état d'un paquet AGPMITEM (op 0 ou 2) qui porte son champ 19.
pub fn item_flags_of(values: &[Option<Value>]) -> Option<AuctionEvent> {
    if !matches!(int(values, 0), Some(0 | 2)) {
        return None;
    }
    Some(AuctionEvent::ItemFlags { iid: int(values, 2)? as i32, flags: int(values, 19)? as u32 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(s: &'static Schema, v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, v).unwrap();
        decode_packet(s, &bytes).unwrap().values
    }

    #[test]
    fn demandes() {
        let (s, v) = command_packet(&AuctionCommand::Open(77), 42);
        assert_eq!(s.id, 0x0b);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 1)), (Some(9), Some(42)));
        let (_, base) = decode_mini(mini::EVENT_BASE, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!((int(&base, 1), int(&base, 3)), (Some(77), Some(28)));

        let (s, v) = command_packet(&AuctionCommand::Search { tid: 4963, doc_id: START_DOC_ID, direction: 0 }, 42);
        let v = roundtrip(s, &v);
        assert_eq!(int(&v, 0), Some(12));
        let (_, m) = decode_mini(mini::AUCTION_SELECT2, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!((int(&m, 0), u64_of(&m, 1), int(&m, 2), int(&m, 3)), (Some(4963), START_DOC_ID, Some(0), Some(0)));

        let (s, v) = command_packet(&AuctionCommand::Buy { doc_id: 123, count: 5, tid: 4963, gold: 200_000 }, 42);
        let v = roundtrip(s, &v);
        let (_, m) = decode_mini(mini::AUCTION_BUY, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!((int(&m, 0), int(&m, 1), int(&m, 2), int(&m, 3), int(&m, 4)), (Some(123), Some(5), Some(4963), Some(200_000), Some(0)));

        let (s, v) = command_packet(&AuctionCommand::Sell { iid: 9, count: 3, gold: 150_000, cc: 7, marvel: 0 }, 42);
        let v = roundtrip(s, &v);
        assert_eq!(int(&v, 0), Some(4));
        let (flags, m) = decode_mini(mini::AUCTION_SELL, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        // Date absente, parchemin à 0 envoyé.
        assert_eq!(flags, 0b10_1111);
        assert_eq!((int(&m, 0), int(&m, 1), int(&m, 2), int(&m, 3), int(&m, 5)), (Some(9), Some(3), Some(150_000), Some(7), Some(0)));

        for (cmd, op) in [(AuctionCommand::Cancel(5), 5), (AuctionCommand::Confirm(5), 6)] {
            let (s, v) = command_packet(&cmd, 42);
            let v = roundtrip(s, &v);
            assert_eq!(int(&v, 0), Some(op));
            let (_, m) = decode_mini(mini::AUCTION_CANCEL, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
            assert_eq!(int(&m, 0), Some(5));
        }
    }

    #[test]
    fn ventes_du_joueur() {
        let s = main::AUCTION;
        let sale = Sale { sales_id: 3, doc_id: 1001, item_seq: 55, iid: 9, gold: 150_000, cc: 2, count: 4, status: 0, date: "2026-10-28 12:00:00".into(), tid: 4963 };
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(1)), (1, Value::I32(42)), (2, Value::Packet(encode_sale(&sale)))]));
        assert_eq!(translate(0x0b, &v), Some(AuctionEvent::SaleAdded { cid: 42, sale: sale.clone() }));
        let only_id = Sale { sales_id: 3, ..Default::default() };
        let m = mini::AUCTION_SALES;
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(2)), (1, Value::I32(42)), (2, Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I32(only_id.sales_id))])).unwrap()))]));
        assert_eq!(translate(0x0b, &v), Some(AuctionEvent::SaleRemoved { cid: 42, sales_id: 3 }));
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(3)), (1, Value::I32(42)), (2, Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I32(3)), (7, Value::I16(1))])).unwrap()))]));
        assert_eq!(translate(0x0b, &v), Some(AuctionEvent::SaleStatus { cid: 42, sales_id: 3, status: 1 }));
        // Réponse de vente refusée (limite).
        let sell = mini::AUCTION_SELL;
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(4)), (1, Value::I32(42)), (2, Value::Packet(build_mini(sell, &sparse(sell, &[(0, Value::I32(9)), (1, Value::I16(4))])).unwrap())), (3, Value::I32(6))]));
        assert_eq!(translate(0x0b, &v), Some(AuctionEvent::Sold { cid: 42, result: 6, iid: 9, count: 4, gold: 0, cc: 0 }));
    }

    #[test]
    fn recherche_et_rowset() {
        let rows = vec![
            vec!["1001".to_string(), "Vendeur".into(), "3".into(), "150000".into(), "2".into(), "9".into(), "4963".into(), "4".into()],
            vec!["998".to_string(), "Autre".into(), "4".into(), "99".into(), "0".into(), "10".into(), "4963".into(), "1".into()],
        ];
        let rowset = encode_rowset(&rows);
        assert_eq!(decode_rowset(&rowset), rows);
        let m = mini::AUCTION_SELECT2;
        let detail = build_mini(m, &sparse(m, &[(0, Value::I32(4963)), (1, Value::U64(START_DOC_ID)), (2, Value::I16(0)), (3, Value::I32(2)), (4, Value::Packet(rowset))])).unwrap();
        let s = main::AUCTION;
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(12)), (1, Value::I32(42)), (2, Value::Packet(detail))]));
        let Some(AuctionEvent::Results { tid, total, rows, .. }) = translate(0x0b, &v) else { panic!() };
        assert_eq!((tid, total, rows.len()), (4963, 2, 2));
        assert_eq!(rows[0], BoardRow { doc_id: 1001, seller: "Vendeur".into(), gold: 150_000, cc: 2, tid: 4963, count: 4 });
        assert_eq!(rows[1].count, 1);
    }

    #[test]
    fn accord_et_frais() {
        let s = main::AUCTION;
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(10)), (1, Value::I32(42)), (2, crate::npc::event_base(77, FUNCTION))]));
        assert_eq!(translate(0x0b, &v), Some(AuctionEvent::Granted { cid: 42, npc: 77 }));
        assert_eq!((fee(100_000), fee(99), fee(0)), (3_000, 2, 0));
        let item = main::ITEM;
        let v = roundtrip(item, &sparse(item, &[(0, Value::U8(0)), (2, Value::I32(9)), (19, Value::I32(1 << 30))]));
        assert_eq!(item_flags_of(&v), Some(AuctionEvent::ItemFlags { iid: 9, flags: 1 << 30 }));
    }
}
