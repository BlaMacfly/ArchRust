//! Boutique Cash : type 0x2E (AgpmCashMall).
//!
//! Dialogue relevé dans Archonia (envoi `FUN_14049dfe0`, `14049e1b0`, `14049e2b0`, `14049e330` ; réception
//! `FUN_140285e90`), identique au client de Webzen pour les ops 1 à 7, et validé avec ArchCore (scénario
//! `sim cashmall`) :
//!
//! ```text
//! op 7 (2 version connue, 3 CID)                     ouverture de UI_CashMall (puis op 1 de l'onglet courant)
//!   → op 2 : 2 version, 6 onglets « nom;produit,produit,…,: »
//! op 1 (3 CID, 4 onglet)                             choix d'un onglet
//!   → op 3 : 2 version, 4 onglet, 7 produits « produit,TID,nombre,prix,genre,description: »
//! op 4 (2 version, 3 CID, 5 produit, 8 : 0)          « Purchase » de UI_CashMallBuy
//!   → op 5 : 1 résultat (1 réussite, 2 Cash, 3 inventaire, 4 liste périmée : l'onglet est redemandé, 5 et 6,
//!            9 et 10 : salles de jeu, achats limités)
//! op 6 (3 CID)                                       revoir le solde (au plus une fois par seconde)
//! op 8 (serveur, Archonia) : offre du jour, non gérée
//! ```
//! Le solde lui-même est le Cash du personnage (champ 10 d'AGPMCHARACTER).

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, Schema};

/// Opérations d'AGPMCASHMALL.
pub mod op {
    pub const REQUEST_PRODUCTS: i8 = 1;
    pub const MALL_LIST: i8 = 2;
    pub const PRODUCT_LIST: i8 = 3;
    pub const BUY: i8 = 4;
    pub const BUY_RESULT: i8 = 5;
    pub const REFRESH_CASH: i8 = 6;
    pub const CHECK_VERSION: i8 = 7;
}

/// Résultats d'un achat (op 5) et leur événement `CashMallBuyResult*` (`FUN_1407191b0`).
pub mod code {
    pub const SUCCESS: i32 = 1;
    pub const NOT_ENOUGH_CASH: i32 = 2;
    pub const INVENTORY_FULL: i32 = 3;
    pub const NEED_NEW_ITEM_LIST: i32 = 4;

    /// Nom de l'événement d'interface d'un résultat (texte dans `UI_1024X768.ini`), `None` hors table.
    pub fn event(code: i32) -> Option<&'static str> {
        Some(match code {
            1 => "CashMallBuyResultSuccess",
            2 => "CashMallBuyResultNotEnoughCash",
            3 => "CashMallBuyResultInventoryFull",
            4 => "CashMallBuyResultNeedNewItemList",
            5 => "CashMallBuyResultPCBangOnly",
            6 => "CashMallBuyResultLimitedCount",
            9 => "CashMallBuyResultNotTPCBang",
            10 => "CashMallBuyResultNotSPCBang",
            _ => return None,
        })
    }
}

/// Demandes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashMallCommand {
    /// Ouverture : version connue (op 7), puis produits de l'onglet (op 1).
    Open { version: u8, tab: i32 },
    /// Produits d'un onglet (op 1).
    Products(i32),
    /// Achat d'un produit (op 4).
    Buy { product: i32, version: u8 },
    /// Solde (op 6).
    RefreshCash,
}

/// Un onglet : nom et produits, dans l'ordre.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MallTab {
    pub name: String,
    pub products: Vec<i32>,
}

/// Un produit (`%d,%d,%d,%I64d,%d,` puis la description).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MallProduct {
    pub id: i32,
    pub tid: i32,
    pub count: i32,
    pub price: i64,
    /// Genre affiché (0 normal, 1 nouveau, 2 populaire, 3 événement, 4 solde, 5 SPC, 6 TPack).
    pub kind: i32,
    pub description: String,
}

/// Réponses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashMallEvent {
    Tabs { version: u8, tabs: Vec<MallTab> },
    Products { version: u8, tab: i32, products: Vec<MallProduct> },
    BuyResult(i32),
}

/// Onglets d'une op 2 (lecture d'Archonia : nom jusqu'au `;`, nombres terminés par `,`, onglet fini par `,:`).
pub fn parse_tabs(text: &str) -> Vec<MallTab> {
    text.split(":")
        .filter_map(|t| {
            let (name, ids) = t.split_once(';')?;
            Some(MallTab { name: name.chars().take(32).collect(), products: ids.split(',').filter_map(|v| v.trim().parse().ok()).collect() })
        })
        .collect()
}

/// Produits d'une op 3 ; la description va de la dernière virgule aux deux-points.
pub fn parse_products(text: &str) -> Vec<MallProduct> {
    text.split(':')
        .filter_map(|e| {
            let mut it = e.splitn(6, ',');
            let mut n = || it.next().map(str::trim);
            let id = n()?.parse().ok()?;
            let (tid, count, price, kind) = (n()?.parse().ok()?, n()?.parse().ok()?, n()?.parse().ok()?, n()?.parse().ok()?);
            let description = e.rsplit_once(',').map(|(_, d)| d.to_string()).unwrap_or_default();
            Some(MallProduct { id, tid, count, price, kind, description })
        })
        .collect()
}

/// Paquets d'une demande pour le personnage `me` (l'ouverture en donne deux).
pub fn command_packets(cmd: &CashMallCommand, me: i32) -> Vec<(&'static Schema, Vec<Option<Value>>)> {
    let s = main::CASH_MALL;
    let products = |tab: i32| (s, sparse(s, &[(0, Value::I8(op::REQUEST_PRODUCTS)), (3, Value::I32(me)), (4, Value::I32(tab))]));
    match cmd {
        CashMallCommand::Open { version, tab } => {
            vec![(s, sparse(s, &[(0, Value::I8(op::CHECK_VERSION)), (2, Value::U8(*version)), (3, Value::I32(me))])), products(*tab)]
        }
        CashMallCommand::Products(tab) => vec![products(*tab)],
        CashMallCommand::Buy { product, version } => {
            vec![(s, sparse(s, &[(0, Value::I8(op::BUY)), (2, Value::U8(*version)), (3, Value::I32(me)), (5, Value::I32(*product)), (8, Value::I32(0))]))]
        }
        CashMallCommand::RefreshCash => vec![(s, sparse(s, &[(0, Value::I8(op::REFRESH_CASH)), (3, Value::I32(me))]))],
    }
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn text(values: &[Option<Value>], i: usize) -> String {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_bytes).map(|b| String::from_utf8_lossy(b).trim_end_matches('\0').to_string()).unwrap_or_default()
}

/// Traduit un paquet 0x2E, sinon `None`.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<CashMallEvent> {
    if packet_type != archrust_protocol::schema::packet_type::CASH_MALL {
        return None;
    }
    let version = int(values, 2).unwrap_or(0) as u8;
    match int(values, 0)? as i8 {
        op::MALL_LIST => Some(CashMallEvent::Tabs { version, tabs: parse_tabs(&text(values, 6)) }),
        op::PRODUCT_LIST => Some(CashMallEvent::Products { version, tab: int(values, 4).unwrap_or(0) as i32, products: parse_products(&text(values, 7)) }),
        op::BUY_RESULT => Some(CashMallEvent::BuyResult(int(values, 1).unwrap_or(0) as i32)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn demandes_et_reponses() {
        let open = command_packets(&CashMallCommand::Open { version: 0, tab: 2 }, 7);
        assert_eq!(open.len(), 2);
        let (s, v) = &open[0];
        assert_eq!(s.id, 0x2e);
        let back = decode_packet(main::CASH_MALL, &build_packet(s, v).unwrap()).unwrap();
        assert_eq!((back.op(), int(&back.values, 2), int(&back.values, 3)), (Some(7), Some(0), Some(7)));
        assert_eq!((int(&open[1].1, 0), int(&open[1].1, 4)), (Some(1), Some(2)));
        let buy = command_packets(&CashMallCommand::Buy { product: 12, version: 1 }, 7);
        assert_eq!((int(&buy[0].1, 0), int(&buy[0].1, 2), int(&buy[0].1, 5), int(&buy[0].1, 8)), (Some(4), Some(1), Some(12), Some(0)));

        // Réponses d'ArchCore.
        let tabs = sparse(main::CASH_MALL, &[(0, Value::I8(2)), (2, Value::U8(1)), (6, Value::Block(b"Bundles;1,3,:Consumable;4,:".to_vec()))]);
        let bytes = build_packet(main::CASH_MALL, &tabs).unwrap();
        let back = decode_packet(main::CASH_MALL, &bytes).unwrap();
        assert_eq!(
            translate(0x2e, &back.values),
            Some(CashMallEvent::Tabs { version: 1, tabs: vec![MallTab { name: "Bundles".into(), products: vec![1, 3] }, MallTab { name: "Consumable".into(), products: vec![4] }] })
        );
        let list = sparse(main::CASH_MALL, &[(0, Value::I8(3)), (2, Value::U8(1)), (4, Value::I32(0)), (7, Value::Block(b"1,6007,50000,450,2,Lens Stone+ Bundle (50000):3,7436,50,2300,0,Pass:".to_vec()))]);
        let Some(CashMallEvent::Products { tab, products, .. }) = translate(0x2e, &list) else { panic!() };
        assert_eq!(tab, 0);
        assert_eq!(products[0], MallProduct { id: 1, tid: 6007, count: 50000, price: 450, kind: 2, description: "Lens Stone+ Bundle (50000)".into() });
        assert_eq!((products[1].id, products[1].description.as_str()), (3, "Pass"));
        let done = sparse(main::CASH_MALL, &[(0, Value::I8(5)), (1, Value::I8(3))]);
        assert_eq!(translate(0x2e, &done), Some(CashMallEvent::BuyResult(3)));
        assert_eq!(translate(0x57, &done), None);
        assert_eq!((code::event(4), code::event(7)), (Some("CashMallBuyResultNeedNewItemList"), None));
    }
}
