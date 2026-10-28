//! Liste d'amis et liste d'ignorés (AgpmBuddy, 0x2B ; réception d'Archonia `FUN_1402ab5f0`).
//!
//! Tout passe par le nom de l'autre joueur (champ 2) ; le champ 1 est le CID du propriétaire de la liste.
//!
//! ```text
//! op 1  ajout        C → S : ignoré (drapeaux 0) ou demande acceptée (0x02000000) ; S → C : entrée ajoutée
//! op 2  demande      C → S : demande d'ami ; S → C : demande reçue (nom du demandeur)
//! op 3  refus        C → S : demande refusée ; S → C : « The player rejected the friend invite. »
//! op 4  retrait      entrée complète, dans les deux sens
//! op 5  options      cases d'un ignoré (bits 0 à 2), dans les deux sens
//! op 6  état         S → C : bit 0x00100000 (en ligne) d'un nom
//! op 7  détail       C → S : ami choisi (région 0) ; S → C : région (RegionTemplate) au champ 4
//! op 8  résultat     S → C : code au champ 6 (1 déjà présent, 2 hors ligne ou inconnu, 3 et 4 refus…)
//! op 9  fin de liste S → C : les messages d'ajout et de retrait commencent ; le client envoie l'op 15
//! op 15 liste        C → S : ignorés d'`IgnoreList.txt` (second schéma `BUDDY_LIST`)
//! ```
//!
//! Les ops 10 à 14 (relation « Buddy », mentor et disciple) ne sont pas traitées.

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

pub mod op {
    pub const ADD: i8 = 1;
    pub const REQUEST: i8 = 2;
    pub const REJECT: i8 = 3;
    pub const REMOVE: i8 = 4;
    pub const OPTIONS: i8 = 5;
    pub const ONLINE: i8 = 6;
    pub const REGION: i8 = 7;
    pub const RESULT: i8 = 8;
    pub const LIST_END: i8 = 9;
    pub const IGNORE_LIST: i8 = 15;
}

/// Drapeaux d'une entrée (champ 3).
pub mod flag {
    /// Ignoré : discussion bloquée (« Block Chat », filtrée par le client).
    pub const BLOCK_CHAT: u32 = 0x0000_0001;
    /// Ignoré : échange bloqué (« Block Trade »).
    pub const BLOCK_TRADE: u32 = 0x0000_0002;
    /// Ignoré : invitations de groupe et de guilde bloquées (« Block Invites »).
    pub const BLOCK_INVITE: u32 = 0x0000_0004;
    /// Joueur en ligne (icône `GD_On`).
    pub const ONLINE: u32 = 0x0010_0000;
    /// Entrée de la liste d'amis ; sans ce bit, liste d'ignorés.
    pub const FRIEND: u32 = 0x0200_0000;
    /// Cases d'un ignoré.
    pub const BLOCKS: u32 = BLOCK_CHAT | BLOCK_TRADE | BLOCK_INVITE;
}

/// Codes de l'op 8 affichés par Archonia (`FUN_140735ae0`).
pub mod result {
    /// `BuddyAlreayExist_MsgBox`.
    pub const ALREADY_EXIST: i32 = 1;
    /// `BuddyOfflineOrNotExist_MsgBox`.
    pub const OFFLINE_OR_NOT_EXIST: i32 = 2;
    /// `sysstr` 715 « Cannot add player in the list. » (liste pleine, nom refusé).
    pub const LIST_FULL: i32 = 3;
    pub const INVALID: i32 = 4;
}

/// Entrées au plus de chaque liste (boucles `< 0x1e` d'Archonia).
pub const MAX_ENTRIES: usize = 30;
/// Longueur maximale d'un nom saisi (`strlen < 0x20`, sinon `BuddyIDLengthWarning_Open`).
pub const NAME_MAX: usize = 31;
/// Octets d'un nom dans les paquets (`char[33]`).
const NAME_FIELD: usize = 33;

/// Entrée d'une liste (`FUN_1402aaff0`) : nom, drapeaux, relation « Buddy ».
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuddyEntry {
    pub name: String,
    pub flags: u32,
    pub relation: i32,
}

impl BuddyEntry {
    pub fn friend(&self) -> bool {
        self.flags & flag::FRIEND != 0
    }

    pub fn online(&self) -> bool {
        self.flags & flag::ONLINE != 0
    }
}

/// Demandes du client.
#[derive(Debug, Clone, PartialEq)]
pub enum BuddyCommand {
    /// Demande d'ami (`BuddyAddFriend_Click`).
    RequestFriend(String),
    /// Demande reçue acceptée (op 1, drapeau ami) ou refusée (op 3).
    Answer { name: String, accept: bool },
    /// Ajout d'un ignoré (`BanAdd_Click`, drapeaux 0).
    AddIgnore(String),
    /// Retrait d'un ami ou d'un ignoré (entrée complète).
    Remove(BuddyEntry),
    /// Nouvelles cases d'un ignoré (op 5).
    Options(BuddyEntry),
    /// Région d'un ami choisi dans la liste (op 7).
    Region(String),
    /// Ignorés lus dans `IgnoreList.txt` après l'op 9 (op 15, 30 au plus).
    IgnoreList(Vec<(String, u32)>),
}

/// Paquets reçus.
#[derive(Debug, Clone, PartialEq)]
pub enum BuddyEvent {
    /// Entrée ajoutée à la liste d'amis ou d'ignorés (bit `FRIEND`).
    Added(BuddyEntry),
    /// Demande d'ami reçue de `name`.
    Requested(String),
    /// Demande refusée par `name`.
    Rejected(String),
    Removed(BuddyEntry),
    /// Cases d'un ignoré remplacées.
    Options(BuddyEntry),
    /// Changement d'état en ligne de `name` (seul le bit `ONLINE` compte).
    Online { name: String, flags: u32 },
    /// Région d'un ami (`region` < 0 : inconnue).
    Region { name: String, region: i32 },
    /// Résultat d'une demande (op 8).
    Result(i32),
    /// Fin de la liste initiale (op 9).
    ListEnd,
}

/// Nom en `char[33]` (32 octets utiles au plus), en CP1252 comme la discussion.
fn name_value(name: &str) -> Value {
    let mut b = crate::guild::to_cp1252(name);
    b.truncate(NAME_FIELD - 1);
    Value::Str(b)
}

fn entry_packet(s: &'static Schema, op: i8, me: i32, name: &str, flags: u32, region: i32, relation: i32) -> Vec<Option<Value>> {
    sparse(
        s,
        &[
            (0, Value::I8(op)),
            (1, Value::I32(me)),
            (2, name_value(name)),
            (3, Value::U32(flags)),
            (4, Value::I32(region)),
            (5, Value::I32(relation)),
            (6, Value::I32(0)),
        ],
    )
}

/// Paquet d'une demande pour le personnage `me` (champs remplis comme `FUN_1402abdc0` : région -1 sauf op 7,
/// code 0).
pub fn command_packet(cmd: &BuddyCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::BUDDY;
    match cmd {
        BuddyCommand::RequestFriend(name) => (s, entry_packet(s, op::REQUEST, me, name, flag::FRIEND, -1, 0)),
        BuddyCommand::Answer { name, accept: true } => (s, entry_packet(s, op::ADD, me, name, flag::FRIEND, -1, 0)),
        BuddyCommand::Answer { name, accept: false } => (s, entry_packet(s, op::REJECT, me, name, 0, -1, 0)),
        BuddyCommand::AddIgnore(name) => (s, entry_packet(s, op::ADD, me, name, 0, -1, 0)),
        BuddyCommand::Remove(e) => (s, entry_packet(s, op::REMOVE, me, &e.name, e.flags, -1, e.relation)),
        BuddyCommand::Options(e) => (s, entry_packet(s, op::OPTIONS, me, &e.name, e.flags, -1, e.relation)),
        BuddyCommand::Region(name) => (s, entry_packet(s, op::REGION, me, name, flag::FRIEND, 0, 0)),
        BuddyCommand::IgnoreList(list) => {
            let l = main::BUDDY_LIST;
            let list = &list[..list.len().min(MAX_ENTRIES)];
            let mut names = vec![0u8; NAME_FIELD * MAX_ENTRIES];
            let mut flags = vec![0i64; MAX_ENTRIES];
            for (i, (name, f)) in list.iter().enumerate() {
                let mut b = crate::guild::to_cp1252(name);
                b.truncate(NAME_FIELD - 1);
                names[i * NAME_FIELD..i * NAME_FIELD + b.len()].copy_from_slice(&b);
                flags[i] = *f as i64;
            }
            (
                l,
                sparse(
                    l,
                    &[(0, Value::I8(op::IGNORE_LIST)), (1, Value::I32(me)), (2, Value::I16(list.len() as i16)), (3, Value::Str(names)), (4, Value::Ints(flags))],
                ),
            )
        }
    }
}

/// Noms et drapeaux d'un paquet op 15 (essais, serveur).
pub fn decode_ignore_list(values: &[Option<Value>]) -> Vec<(String, u32)> {
    let count = values.get(2).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0).clamp(0, MAX_ENTRIES as i64) as usize;
    let names = values.get(3).and_then(Option::as_ref).and_then(Value::as_bytes).unwrap_or(&[]);
    let flags = match values.get(4).and_then(Option::as_ref) {
        Some(Value::Ints(v)) => v.clone(),
        _ => Vec::new(),
    };
    (0..count)
        .filter_map(|i| {
            let raw = names.get(i * NAME_FIELD..(i + 1) * NAME_FIELD)?;
            Some((cp1252(raw), flags.get(i).copied().unwrap_or(0) as u32))
        })
        .collect()
}

fn cp1252(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    encoding_rs::WINDOWS_1252.decode(&bytes[..end]).0.trim().to_string()
}

/// Paquet AgpmBuddy traduit, sinon `None` (ops 10 à 15 comprises : rien à faire).
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<BuddyEvent> {
    if ptype != packet_type::BUDDY {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    let name = values.get(2).and_then(Option::as_ref).and_then(Value::as_bytes).map(cp1252).unwrap_or_default();
    let entry = || BuddyEntry { name: name.clone(), flags: int(3).unwrap_or(0) as u32, relation: int(5).unwrap_or(0) as i32 };
    Some(match int(0)? as i8 {
        op::ADD => BuddyEvent::Added(entry()),
        op::REQUEST => BuddyEvent::Requested(name),
        op::REJECT => BuddyEvent::Rejected(name),
        op::REMOVE => BuddyEvent::Removed(entry()),
        op::OPTIONS => BuddyEvent::Options(entry()),
        op::ONLINE => BuddyEvent::Online { flags: int(3).unwrap_or(0) as u32, name },
        op::REGION => BuddyEvent::Region { name, region: int(4).unwrap_or(-1) as i32 },
        op::RESULT => BuddyEvent::Result(int(6).unwrap_or(0) as i32),
        op::LIST_END => BuddyEvent::ListEnd,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(s: &'static Schema, v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, v).unwrap();
        assert_eq!(bytes[5], 0x2b);
        decode_packet(s, &bytes).unwrap().values
    }

    fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
        v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
    }

    #[test]
    fn demandes_du_client() {
        let (s, v) = command_packet(&BuddyCommand::RequestFriend("Arthas".into()), 42);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 3), int(&v, 4), int(&v, 6)), (Some(2), Some(42), Some(0x0200_0000), Some(-1), Some(0)));
        assert_eq!(v[2].as_ref().and_then(Value::as_bytes).map(|b| b.len()), Some(33));
        assert_eq!(v[2].as_ref().and_then(Value::as_text).as_deref(), Some("Arthas"));

        let (s, v) = command_packet(&BuddyCommand::Answer { name: "Jaina".into(), accept: true }, 42);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 3)), (Some(1), Some(0x0200_0000)));
        let (_, v) = command_packet(&BuddyCommand::Answer { name: "Jaina".into(), accept: false }, 42);
        assert_eq!(int(&v, 0), Some(3));
        let (_, v) = command_packet(&BuddyCommand::AddIgnore("Gêneur".into()), 42);
        assert_eq!((int(&v, 0), int(&v, 3)), (Some(1), Some(0)));
        // CP1252 : « ê » sur un octet.
        assert_eq!(v[2].as_ref().and_then(Value::as_bytes), Some(&b"G\xeaneur"[..]));
        let e = BuddyEntry { name: "Gêneur".into(), flags: flag::BLOCK_CHAT | flag::BLOCK_INVITE, relation: 0 };
        let (_, v) = command_packet(&BuddyCommand::Options(e.clone()), 42);
        assert_eq!((int(&v, 0), int(&v, 3)), (Some(5), Some(5)));
        let (_, v) = command_packet(&BuddyCommand::Remove(e), 42);
        assert_eq!(int(&v, 0), Some(4));
        let (_, v) = command_packet(&BuddyCommand::Region("Arthas".into()), 42);
        assert_eq!((int(&v, 0), int(&v, 4)), (Some(7), Some(0)));
    }

    #[test]
    fn liste_des_ignores() {
        let list = vec![("Un".to_string(), 1u32), ("Deux".to_string(), 7u32)];
        let (s, v) = command_packet(&BuddyCommand::IgnoreList(list.clone()), 42);
        assert_eq!(s.name, "AGPMBUDDY_LIST");
        let bytes = build_packet(s, &v).unwrap();
        // En-tête 19 o + drapeaux 4 + op 1 + CID 4 + nombre 2 + noms 990 + drapeaux 120 + garde.
        assert_eq!(bytes.len(), 19 + 4 + 1 + 4 + 2 + 990 + 120 + 1);
        let v = decode_packet(s, &bytes).unwrap().values;
        assert_eq!((int(&v, 0), int(&v, 2)), (Some(15), Some(2)));
        assert_eq!(decode_ignore_list(&v), list);
        // Plus de 30 noms : tronqué.
        let many: Vec<(String, u32)> = (0..40).map(|i| (format!("N{i}"), 0)).collect();
        let (_, v) = command_packet(&BuddyCommand::IgnoreList(many), 42);
        assert_eq!(int(&v, 2), Some(30));
    }

    #[test]
    fn evenements_du_serveur() {
        let s = main::BUDDY;
        let pkt = |op: i8, name: &str, flags: u32, region: i32, code: i32| roundtrip(s, &entry_packet(s, op, 42, name, flags, region, 0)).into_iter().enumerate().map(|(i, v)| if i == 6 { Some(Value::I32(code)) } else { v }).collect::<Vec<_>>();
        let online = flag::FRIEND | flag::ONLINE;
        assert_eq!(translate(0x2b, &pkt(1, "Arthas", online, -1, 0)), Some(BuddyEvent::Added(BuddyEntry { name: "Arthas".into(), flags: online, relation: 0 })));
        assert_eq!(translate(0x2b, &pkt(2, "Jaina", flag::FRIEND, -1, 0)), Some(BuddyEvent::Requested("Jaina".into())));
        assert_eq!(translate(0x2b, &pkt(3, "Jaina", 0, -1, 0)), Some(BuddyEvent::Rejected("Jaina".into())));
        assert_eq!(translate(0x2b, &pkt(6, "Arthas", flag::FRIEND, -1, 0)), Some(BuddyEvent::Online { name: "Arthas".into(), flags: flag::FRIEND }));
        assert_eq!(translate(0x2b, &pkt(7, "Arthas", flag::FRIEND, 12, 0)), Some(BuddyEvent::Region { name: "Arthas".into(), region: 12 }));
        assert_eq!(translate(0x2b, &pkt(8, "", 0, -1, 2)), Some(BuddyEvent::Result(2)));
        assert_eq!(translate(0x2b, &pkt(9, "", 0, 0, 0)), Some(BuddyEvent::ListEnd));
        // Mentor (ops 10 à 14) : ignoré.
        assert_eq!(translate(0x2b, &pkt(12, "Arthas", 0, -1, 0)), None);
        assert_eq!(translate(0x2a, &pkt(1, "Arthas", 0, -1, 0)), None);
    }
}
