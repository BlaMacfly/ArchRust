//! Avis de recherche (AgpmWantedCriminal, 0x46 ; fonction de PNJ 32, bouton « Wanted » de la fenêtre de mort).
//!
//! Relevé dans Archonia (sha256 e9c7609b…, constructeur `FUN_14028a080`, réception `FUN_14028a700`) : drapeaux
//! 2 octets ; champ 5 = EVENT_BASE (ops 0 et 1) ou mini « criminel » {nom, montant, niveau, TID, en ligne}.
//!
//! | Op | Sens | Contenu |
//! |---|---|---|
//! | 0 | C→S | CID, EVENT_BASE du PNJ : ouvrir le tableau |
//! | 1 | S→C | accord : `UI_WantedCriminal`, page 0 demandée |
//! | 2 | C→S / S→C | mise à prix {nom du criminel, prime} / résultat au champ 4 (0 réussite, 1 prime de base, 2 or, 3 déjà) |
//! | 3 | C→S | page 0 à 9 du tableau |
//! | 4 | S→C | ligne : page, rang (1-100), criminel ; page -100 : fin de page |
//! | 5 | C→S / S→C | rang de soi, toutes les 30 s si recherché / rang et prime cumulée |
//! | 6 | S→C | CID de la victime, nom du tueur : bouton « Wanted » actif |
//! | 7 | S→C | recherché (champ 4 : 0 ou 1) : icône `WantedAlarm` |
//! | 8 | S→C | genre 4 `WC_KILL` (criminel attrapé, or gagné) ou 5 `WC_DEAD` (tué, expérience perdue) |

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction de PNJ du tableau des avis (chefs de ville).
pub const FUNCTION: i32 = 32;
/// Pages du tableau et lignes par page (100 avis visibles).
pub const PAGES: i32 = 10;
pub const ROWS: i32 = 10;
/// Page de l'op 4 qui clôt une page.
pub const PAGE_END: i32 = -100;
/// Délai entre deux demandes de rang (op 5) tant que le joueur est recherché.
pub const RANK_PERIOD_SECS: u64 = 30;

pub mod op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const ARRANGE: i8 = 2;
    pub const BOARD: i8 = 3;
    pub const BOARD_ROW: i8 = 4;
    pub const RANK: i8 = 5;
    pub const KILLER: i8 = 6;
    pub const WANTED: i8 = 7;
    pub const NOTIFY: i8 = 8;
}

/// Résultats de la mise à prix (op 2, champ 4, `FUN_140692ba0`).
pub mod result {
    pub const SUCCEEDED: i32 = 0;
    pub const NOT_ENOUGH_BASE: i32 = 1;
    pub const NOT_ENOUGH_MONEY: i32 = 2;
    pub const ALREADY_ARRANGED: i32 = 3;
}

/// Genres de notification (op 8, champ 4).
pub mod notify {
    pub const KILL: i32 = 4;
    pub const DEAD: i32 = 5;
}

/// Prime de base selon le niveau de celui qui pose l'avis (`FUN_14028b7a0`, table `0x140f6b030`).
pub fn base_bounty(level: i32) -> i64 {
    match level {
        i32::MIN..=20 => 50_000,
        21..=40 => 100_000,
        41..=60 => 200_000,
        61..=80 => 400_000,
        _ => 1_000_000,
    }
}

/// Mini « criminel » (`AgpdWantedCriminal`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Criminal {
    pub name: String,
    /// Prime cumulée, prime posée, or gagné ou expérience perdue selon l'opération.
    pub amount: i64,
    pub level: i32,
    /// TID du modèle (colonne « Class »).
    pub tid: i32,
    pub online: bool,
}

fn criminal_packet(c: &Criminal) -> Value {
    let m = mini::WANTED_CRIMINAL;
    let mut name = crate::guild::to_cp1252(&c.name);
    name.truncate(32);
    let values = sparse(
        m,
        &[(0, Value::Str(name)), (1, Value::I64(c.amount)), (2, Value::I32(c.level)), (3, Value::I32(c.tid)), (4, Value::I32(c.online as i32))],
    );
    Value::Packet(build_mini(m, &values).unwrap_or_default())
}

fn decode_criminal(v: Option<&Value>) -> Option<Criminal> {
    let (_, m) = decode_mini(mini::WANTED_CRIMINAL, v?.as_bytes()?).ok()?;
    let int = |i: usize| m.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0);
    Some(Criminal { name: crate::chat::cp1252(m.first().and_then(Option::as_ref)), amount: int(1), level: int(2) as i32, tid: int(3) as i32, online: int(4) != 0 })
}

#[derive(Debug, Clone, PartialEq)]
pub enum WantedCommand {
    /// Ouvrir le tableau auprès du PNJ `npc` (op 0).
    Request(i32),
    /// Mettre `name` à prix pour `bounty` or (op 2).
    Arrange { name: String, bounty: i64 },
    /// Page 0 à 9 du tableau (op 3).
    Board(i32),
    /// Rang et prime de soi (op 5).
    MyRank,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WantedEvent {
    /// Accord du PNJ (op 1).
    Granted { cid: i32 },
    /// Résultat de la mise à prix (op 2) ; le nom n'est jamais vide.
    Arranged { result: i32, name: String, amount: i64 },
    /// Ligne du tableau (op 4).
    Row { page: i32, rank: i32, criminal: Criminal },
    /// Fin de la page demandée (op 4, page -100).
    PageEnd,
    /// Rang et prime cumulée de soi (op 5, rang > 0).
    Rank { rank: i32, total: i64 },
    /// Tueur de la victime `victim` (op 6) : le joueur peut le mettre à prix (nom vide : plus rien).
    Killer { victim: i32, name: String },
    /// Recherché ou non (op 7).
    Wanted { cid: i32, wanted: bool },
    /// Notification (op 8) : genre 4 ou 5, nom, montant.
    Notify { cid: i32, kind: i32, name: String, amount: i64 },
}

pub fn command_packet(cmd: &WantedCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::WANTED_CRIMINAL;
    let values = match cmd {
        WantedCommand::Request(npc) => sparse(s, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(me)), (5, crate::npc::event_base(*npc, FUNCTION))]),
        WantedCommand::Arrange { name, bounty } => sparse(
            s,
            &[(0, Value::I8(op::ARRANGE)), (1, Value::I32(me)), (5, criminal_packet(&Criminal { name: name.clone(), amount: *bounty, ..Default::default() }))],
        ),
        WantedCommand::Board(page) => sparse(s, &[(0, Value::I8(op::BOARD)), (1, Value::I32(me)), (2, Value::I32(*page))]),
        WantedCommand::MyRank => sparse(s, &[(0, Value::I8(op::RANK)), (1, Value::I32(me))]),
    };
    (s, values)
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<WantedEvent> {
    if ptype != packet_type::WANTED_CRIMINAL {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0);
    let criminal = || decode_criminal(values.get(5).and_then(Option::as_ref));
    let cid = int(1) as i32;
    Some(match int(0) as i8 {
        op::GRANT => WantedEvent::Granted { cid },
        op::ARRANGE => {
            let c = criminal().filter(|c| !c.name.is_empty())?;
            WantedEvent::Arranged { result: int(4) as i32, name: c.name, amount: c.amount }
        }
        op::BOARD_ROW if int(2) as i32 == PAGE_END => WantedEvent::PageEnd,
        op::BOARD_ROW => WantedEvent::Row { page: int(2) as i32, rank: int(3) as i32, criminal: criminal()? },
        op::RANK if int(3) > 0 => WantedEvent::Rank { rank: int(3) as i32, total: criminal().map_or(0, |c| c.amount) },
        op::KILLER => WantedEvent::Killer { victim: cid, name: criminal().map(|c| c.name).unwrap_or_default() },
        op::WANTED => WantedEvent::Wanted { cid, wanted: int(4) != 0 },
        op::NOTIFY => {
            let c = criminal().filter(|c| !c.name.is_empty())?;
            WantedEvent::Notify { cid, kind: int(4) as i32, name: c.name, amount: c.amount }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round_trip(values: Vec<Option<Value>>) -> Vec<Option<Value>> {
        let s = main::WANTED_CRIMINAL;
        decode_packet(s, &build_packet(s, &values).unwrap()).unwrap().values
    }

    #[test]
    fn prime_de_base() {
        assert_eq!(base_bounty(1), 50_000);
        assert_eq!(base_bounty(20), 50_000);
        assert_eq!(base_bounty(21), 100_000);
        assert_eq!(base_bounty(60), 200_000);
        assert_eq!(base_bounty(80), 400_000);
        assert_eq!(base_bounty(99), 1_000_000);
    }

    #[test]
    fn mise_a_prix() {
        let (s, v) = command_packet(&WantedCommand::Arrange { name: "Brute".into(), bounty: 75_000 }, 7);
        let v = round_trip(v);
        assert_eq!(v[0].as_ref().and_then(Value::as_i64), Some(2));
        let c = decode_criminal(v[5].as_ref()).unwrap();
        assert_eq!((c.name.as_str(), c.amount), ("Brute", 75_000));
        assert_eq!(s.flag_len, 2);
        // Réponse : le nom est exigé.
        let ok = round_trip(sparse(s, &[(0, Value::I8(2)), (1, Value::I32(7)), (4, Value::I32(0)), (5, criminal_packet(&Criminal { name: "Brute".into(), amount: 75_000, ..Default::default() }))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &ok), Some(WantedEvent::Arranged { result: 0, name: "Brute".into(), amount: 75_000 }));
        let sans_nom = round_trip(sparse(s, &[(0, Value::I8(2)), (4, Value::I32(1))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &sans_nom), None);
    }

    #[test]
    fn tableau_rang_et_notifications() {
        let s = main::WANTED_CRIMINAL;
        let row = Criminal { name: "Brute".into(), amount: 250_000, level: 42, tid: 4, online: true };
        let v = round_trip(sparse(s, &[(0, Value::I8(4)), (1, Value::I32(7)), (2, Value::I32(1)), (3, Value::I32(12)), (5, criminal_packet(&row))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &v), Some(WantedEvent::Row { page: 1, rank: 12, criminal: row }));
        let end = round_trip(sparse(s, &[(0, Value::I8(4)), (1, Value::I32(7)), (2, Value::I32(PAGE_END))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &end), Some(WantedEvent::PageEnd));
        let (_, page) = command_packet(&WantedCommand::Board(3), 7);
        assert_eq!(round_trip(page)[2].as_ref().and_then(Value::as_i64), Some(3));
        let killer = round_trip(sparse(s, &[(0, Value::I8(6)), (1, Value::I32(7)), (5, criminal_packet(&Criminal { name: "Brute".into(), ..Default::default() }))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &killer), Some(WantedEvent::Killer { victim: 7, name: "Brute".into() }));
        let wanted = round_trip(sparse(s, &[(0, Value::I8(7)), (1, Value::I32(7)), (4, Value::I32(1))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &wanted), Some(WantedEvent::Wanted { cid: 7, wanted: true }));
        let kill = round_trip(sparse(s, &[(0, Value::I8(8)), (1, Value::I32(7)), (4, Value::I32(4)), (5, criminal_packet(&Criminal { name: "Brute".into(), amount: 9, ..Default::default() }))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &kill), Some(WantedEvent::Notify { cid: 7, kind: 4, name: "Brute".into(), amount: 9 }));
        let rank0 = round_trip(sparse(s, &[(0, Value::I8(5)), (1, Value::I32(7)), (3, Value::I32(0))]));
        assert_eq!(translate(packet_type::WANTED_CRIMINAL, &rank0), None);
    }
}
