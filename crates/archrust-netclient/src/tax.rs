//! Taxes des régions (AgpmTax 0x53, drapeaux 2, constructeur d'Archonia `FUN_14026c110`, réception
//! `FUN_14026c7b0`) ; intendants « Grand Advisor » (fonction de PNJ 34).
//!
//! Champs : op, CID du personnage concerné, résultat, mini « château » (ops 2 et 3) ou EVENT_BASE (ops 0 et 1),
//! 4 mini « région ». Château : nom (CHAR[128]), recette totale, date du dernier versement, date de la dernière
//! modification des taux (time_t). Région : nom (CHAR[33], **traduit** par le glossaire du joueur), recette, taux.
//! * op 0 (client) : demande d'ouverture chez l'intendant (EVENT_BASE de la fonction 34) ;
//! * op 1 (serveur) : accord → `UI_TaxSteward` si le joueur est le maître du château où il se trouve ;
//! * op 2 : changement des taux (client : château {nom}, régions {nom, taux}) ; réponse : résultat
//!   ([`result`]), château et régions mis à jour ;
//! * op 3 : consultation (client : château {nom}) ; réponse : château et ≤ 4 régions complets ;
//! * op 4 (serveur, à tous) : taux d'une région (champ 4), ce qui sert au calcul des prix.

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction d'événement « Taxe » (intendant).
pub const FUNCTION: i32 = 34;
/// Régions d'un château.
pub const REGIONS: usize = 4;
/// Taux maximal permis par la fenêtre (`DAT_140e0beb4`).
pub const MAX_RATIO: i32 = 50;
/// Tribut versé à l'ArchLord, en % des recettes des régions (`FUN_14026e250`).
pub const ARCHLORD_TRIBUTE: i64 = 20;

pub mod op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const MODIFY: i8 = 2;
    pub const VIEW: i8 = 3;
    pub const REGION_RATIO: i8 = 4;
}

/// Résultats de l'op 2 (`FUN_140634430`).
pub mod result {
    pub const SUCCESS: i32 = 0;
    pub const NOT_OWNER: i32 = 1;
    pub const NOT_TIME: i32 = 2;
    pub const OUT_OF_RANGE: i32 = 3;

    /// Clé d'`uimessage.txt` d'un résultat (`TAX_MODIFY_F_NOT_OWNER` manque à Archonia : rien d'affiché).
    pub fn message(code: i32) -> Option<&'static str> {
        Some(match code {
            SUCCESS => "TAX_MODIFY_SUCCESS",
            NOT_OWNER => "TAX_MODIFY_F_NOT_OWNER",
            NOT_TIME => "TAX_MODIFY_F_NOT_TIME",
            OUT_OF_RANGE => "TAX_MODIFY_F_EX_RANGE",
            _ => return None,
        })
    }
}

/// Prix taxé : `prix + (prix × taux) / 100`, taxe nulle si le taux n'est pas positif (division entière tronquée,
/// formule d'une trentaine de fonctions d'Archonia).
pub fn taxed(price: i64, ratio: i32) -> i64 {
    price + tax_of(price, ratio)
}

/// Taxe seule.
pub fn tax_of(price: i64, ratio: i32) -> i64 {
    if ratio > 0 {
        price * ratio as i64 / 100
    } else {
        0
    }
}

/// Château (mini TAX_CASTLE) ; les champs absents restent `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CastleTax {
    pub name: String,
    pub income: Option<i64>,
    /// Date du dernier versement (time_t, 0 : jamais).
    pub transferred: Option<u64>,
    /// Date de la dernière modification des taux (time_t).
    pub modified: Option<u64>,
}

/// Région (mini TAX_REGION).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionTax {
    pub name: String,
    pub income: Option<i64>,
    pub ratio: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaxCommand {
    /// Ouvrir le dialogue de l'intendant `npc` (op 0).
    Request(i32),
    /// Changer les taux des régions du château (op 2).
    Modify { castle: String, regions: Vec<(String, i32)> },
    /// Lire les taxes du château (op 3).
    View(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaxEvent {
    /// Demande reçue (op 0, usage serveur).
    Request { cid: i32 },
    /// Accord d'ouverture (op 1) : intendant de l'événement.
    Granted { cid: i32, npc: Option<i32> },
    /// Réponse au changement des taux (op 2).
    Modified { cid: i32, result: i32, castle: Option<CastleTax>, regions: Vec<RegionTax> },
    /// Taxes du château (op 3).
    Castle { cid: i32, result: i32, castle: Option<CastleTax>, regions: Vec<RegionTax> },
    /// Taux d'une région (op 4).
    Region { cid: i32, region: RegionTax },
}

/// Texte d'un champ (latin-1, comme `Value::as_text`).
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

pub fn castle_value(c: &CastleTax) -> Value {
    let m = mini::TAX_CASTLE;
    let mut pairs = vec![(0, Value::Str(latin1(&c.name)))];
    if let Some(v) = c.income {
        pairs.push((1, Value::I64(v)));
    }
    if let Some(v) = c.transferred {
        pairs.push((2, Value::U64(v)));
    }
    if let Some(v) = c.modified {
        pairs.push((3, Value::U64(v)));
    }
    Value::Packet(build_mini(m, &sparse(m, &pairs)).unwrap_or_default())
}

pub fn region_value(r: &RegionTax) -> Value {
    let m = mini::TAX_REGION;
    let mut pairs = vec![(0, Value::Str(latin1(&r.name)))];
    if let Some(v) = r.income {
        pairs.push((1, Value::I64(v)));
    }
    if let Some(v) = r.ratio {
        pairs.push((2, Value::I32(v)));
    }
    Value::Packet(build_mini(m, &sparse(m, &pairs)).unwrap_or_default())
}

/// Paquet d'une commande (`me` : CID du joueur).
pub fn command_packet(cmd: &TaxCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::TAX;
    let castle = |name: &str| castle_value(&CastleTax { name: name.to_string(), ..Default::default() });
    let values = match cmd {
        TaxCommand::Request(npc) => sparse(s, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(me)), (3, crate::npc::event_base(*npc, FUNCTION))]),
        TaxCommand::Modify { castle: c, regions } => {
            let mut pairs = vec![(0, Value::I8(op::MODIFY)), (1, Value::I32(me)), (3, castle(c))];
            for (i, (name, ratio)) in regions.iter().take(REGIONS).enumerate() {
                pairs.push((4 + i, region_value(&RegionTax { name: name.clone(), income: None, ratio: Some(*ratio) })));
            }
            sparse(s, &pairs)
        }
        TaxCommand::View(c) => sparse(s, &[(0, Value::I8(op::VIEW)), (1, Value::I32(me)), (3, castle(c))]),
    };
    (s, values)
}

fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn bytes(v: &[Option<Value>], i: usize) -> Option<&[u8]> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_bytes)
}

pub fn decode_castle(b: &[u8]) -> Option<CastleTax> {
    let (_, v) = decode_mini(mini::TAX_CASTLE, b).ok()?;
    Some(CastleTax {
        name: v.first().and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default(),
        income: int(&v, 1),
        transferred: int(&v, 2).map(|x| x as u64),
        modified: int(&v, 3).map(|x| x as u64),
    })
}

pub fn decode_region(b: &[u8]) -> Option<RegionTax> {
    let (_, v) = decode_mini(mini::TAX_REGION, b).ok()?;
    Some(RegionTax { name: v.first().and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default(), income: int(&v, 1), ratio: int(&v, 2).map(|x| x as i32) })
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<TaxEvent> {
    if ptype != packet_type::TAX {
        return None;
    }
    let cid = int(values, 1).unwrap_or(0) as i32;
    let res = int(values, 2).unwrap_or(0) as i32;
    let regions = || (4..4 + REGIONS).filter_map(|i| bytes(values, i).and_then(decode_region)).filter(|r| !r.name.is_empty()).collect::<Vec<_>>();
    let castle = || bytes(values, 3).and_then(decode_castle);
    Some(match int(values, 0)? as i8 {
        op::REQUEST => TaxEvent::Request { cid },
        op::GRANT => {
            let npc = bytes(values, 3).and_then(|b| decode_mini(mini::EVENT_BASE, b).ok()).and_then(|(_, v)| int(&v, 1)).map(|x| x as i32);
            TaxEvent::Granted { cid, npc }
        }
        op::MODIFY => TaxEvent::Modified { cid, result: res, castle: castle(), regions: regions() },
        op::VIEW => TaxEvent::Castle { cid, result: res, castle: castle(), regions: regions() },
        op::REGION_RATIO => TaxEvent::Region { cid, region: bytes(values, 4).and_then(decode_region)? },
        _ => return None,
    })
}

/// Paquet d'un événement (essais et serveur de test).
pub fn event_packet(ev: &TaxEvent) -> Vec<Option<Value>> {
    let s = main::TAX;
    let with_regions = |o: i8, cid: i32, result: i32, castle: &Option<CastleTax>, regions: &[RegionTax]| {
        let mut pairs = vec![(0, Value::I8(o)), (1, Value::I32(cid)), (2, Value::I32(result))];
        if let Some(c) = castle {
            pairs.push((3, castle_value(c)));
        }
        for (i, r) in regions.iter().take(REGIONS).enumerate() {
            pairs.push((4 + i, region_value(r)));
        }
        sparse(s, &pairs)
    };
    match ev {
        TaxEvent::Request { cid } => sparse(s, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(*cid))]),
        TaxEvent::Granted { cid, npc } => {
            let mut pairs = vec![(0, Value::I8(op::GRANT)), (1, Value::I32(*cid))];
            if let Some(n) = npc {
                pairs.push((3, crate::npc::event_base(*n, FUNCTION)));
            }
            sparse(s, &pairs)
        }
        TaxEvent::Modified { cid, result, castle, regions } => with_regions(op::MODIFY, *cid, *result, castle, regions),
        TaxEvent::Castle { cid, result, castle, regions } => with_regions(op::VIEW, *cid, *result, castle, regions),
        TaxEvent::Region { cid, region } => sparse(s, &[(0, Value::I8(op::REGION_RATIO)), (1, Value::I32(*cid)), (4, region_value(region))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round(values: &[Option<Value>]) -> Vec<Option<Value>> {
        let s = main::TAX;
        decode_packet(s, &build_packet(s, values).unwrap()).unwrap().values
    }

    #[test]
    fn formule_de_la_taxe() {
        assert_eq!(taxed(1000, 10), 1100);
        assert_eq!(taxed(199, 5), 208);
        assert_eq!(taxed(99, 1), 99);
        assert_eq!(taxed(1000, 0), 1000);
        assert_eq!(taxed(1000, -5), 1000);
        assert_eq!(tax_of(12_345, 50), 6172);
    }

    #[test]
    fn commandes_du_client() {
        let (s, v) = command_packet(&TaxCommand::Request(0x4000_0010), 7);
        assert_eq!(s.id, 0x53);
        let v = round(&v);
        assert_eq!((int(&v, 0), int(&v, 1)), (Some(0), Some(7)));
        let (_, b) = decode_mini(mini::EVENT_BASE, bytes(&v, 3).unwrap()).unwrap();
        assert_eq!((int(&b, 1), int(&b, 3)), (Some(0x4000_0010), Some(34)));

        let cmd = TaxCommand::Modify { castle: "Inflame".into(), regions: vec![("Château d'Inflame".into(), 10), ("Abords".into(), 50)] };
        let v = round(&command_packet(&cmd, 7).1);
        assert_eq!(int(&v, 0), Some(2));
        assert_eq!(decode_castle(bytes(&v, 3).unwrap()).unwrap(), CastleTax { name: "Inflame".into(), ..Default::default() });
        assert_eq!(decode_region(bytes(&v, 5).unwrap()).unwrap(), RegionTax { name: "Abords".into(), income: None, ratio: Some(50) });
        assert_eq!(decode_region(bytes(&v, 4).unwrap()).unwrap().name, "Château d'Inflame");
        assert!(v[6].is_none() && v[7].is_none());

        let v = round(&command_packet(&TaxCommand::View("Traqua".into()), 7).1);
        assert_eq!((int(&v, 0), decode_castle(bytes(&v, 3).unwrap()).unwrap().name.as_str()), (Some(3), "Traqua"));
    }

    #[test]
    fn paquets_du_serveur() {
        let castle = Some(CastleTax { name: "Inflame".into(), income: Some(5000), transferred: Some(1_761_000_000), modified: Some(1_760_000_000) });
        let regions = vec![RegionTax { name: "A".into(), income: Some(300), ratio: Some(5) }, RegionTax { name: "B".into(), income: Some(0), ratio: Some(0) }];
        let all = [
            TaxEvent::Granted { cid: 7, npc: Some(0x4000_0001) },
            TaxEvent::Modified { cid: 7, result: result::NOT_TIME, castle: castle.clone(), regions: regions.clone() },
            TaxEvent::Castle { cid: 7, result: 0, castle, regions },
            TaxEvent::Region { cid: 9, region: RegionTax { name: "C".into(), income: Some(1), ratio: Some(12) } },
            TaxEvent::Request { cid: 7 },
        ];
        for ev in all {
            assert_eq!(translate(0x53, &round(&event_packet(&ev))), Some(ev));
        }
        assert_eq!(translate(0x55, &round(&event_packet(&TaxEvent::Request { cid: 1 }))), None);
        assert_eq!((result::message(2), result::message(4)), (Some("TAX_MODIFY_F_NOT_TIME"), None));
    }
}
