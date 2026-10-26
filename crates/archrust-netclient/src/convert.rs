//! Conversion des objets chez le forgeron : renforcement (potion), sockets (« Ring of Agreement ») et
//! pierres d'esprit. Types 0x16 (AgpmItemConvert) et 0x38 (AgpmEventItemConvert).
//!
//! Dialogue validé avec ArchCore (scénario `sim convert`, wiki « Conversion des objets ») :
//!
//! ```text
//! 0x38 op 0 (1 EVENT_BASE fonction 22, 2 CID)   → op 1 : fenêtre du forgeron accordée
//! 0x16 op 0 (2 IID, 5 niveau, 6 sockets, 7 remplis, 9 mini TID sertis) : état d'un objet, envoyé
//!     après son AGPMITEM op 0 (aussi dans le champ 6 de la vue optimisée op 2, à l'entrée en jeu)
//! 0x16 op 1 (2 IID, 3 IID de la potion)          → op 5, résultat 1 réussi, 2 plein, 3 inadapté,
//!                                                   5 sans perte, 6 retour à +0, 8 détruit, 9 potion
//! 0x16 op 2 (2 IID)                              → op 6, résultat 1 réussi, 2 plein, 5 sans perte,
//!                                                   6 sertissages perdus, 7 remis à zéro, 8 détruit,
//!                                                   9 inadapté
//! 0x16 op 4 (2 IID, 3 IID de la pierre)          → op 7, résultat 0 réussi (!), 1 échec, 2 pierre
//!                                                   refusée, 3 plein, 5 inadapté ; 8 TID serti
//! 0x16 op 3 (2 IID, 3 IID de la rune)            → op 8, résultat 1 réussi, 2 rune invalide, 3 plein,
//!                                                   6 niveau trop bas, 7 inadapté, 8 même rune (ANTI),
//!                                                   9 sans perte, 10 sertissages perdus, 11 retour à
//!                                                   +0, 12 détruit, 13 objet invalide (10 et 11 : op 0)
//! AGPMITEM champ 20 (mini 5 × UINT16)            options d'itemoptiontable.txt de l'objet, dans
//!                                                   chaque op 0 d'un objet connu
//! ```

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, Schema};

/// Fonction « conversion d'objets » d'un PNJ (`Function` 22 de npc.ini).
pub const FUNCTION_ITEM_CONVERT: i32 = 22;

/// Opérations d'AGPMITEMCONVERT.
pub mod op {
    pub const ADD: i64 = 0;
    pub const REQUEST_PHYSICAL: i8 = 1;
    pub const REQUEST_SOCKET: i8 = 2;
    pub const REQUEST_RUNE: i8 = 3;
    pub const REQUEST_SPIRIT_STONE: i8 = 4;
    pub const RESPONSE_PHYSICAL: i64 = 5;
    pub const RESPONSE_SOCKET: i64 = 6;
    pub const RESPONSE_SPIRIT_STONE: i64 = 7;
    pub const RESPONSE_RUNE: i64 = 8;
}

/// Résultats d'un sertissage de rune (op 8).
pub mod rune_result {
    pub const SUCCESS: i32 = 1;
    pub const INVALID_RUNE: i32 = 2;
    pub const FULL: i32 = 3;
    pub const LOW_ITEM_LEVEL: i32 = 6;
    pub const IMPROPER_ITEM: i32 = 7;
    pub const ALREADY_ANTI_CONVERT: i32 = 8;
    pub const KEEP: i32 = 9;
    /// Sertissages perdus.
    pub const INIT_SAME: i32 = 10;
    /// Retour à +0, sertissages perdus.
    pub const INIT: i32 = 11;
    pub const DESTROY: i32 = 12;
    pub const INVALID_ITEM: i32 = 13;
}

/// Résultats d'un renforcement (op 5) et d'un ajout de socket (op 6).
pub mod result {
    pub const SUCCESS: i32 = 1;
    pub const FULL: i32 = 2;
    pub const IMPROPER_ITEM: i32 = 3;
    pub const KEEP: i32 = 5;
    /// Socket : sertissages perdus ; renforcement : retour à +0.
    pub const INIT_SAME: i32 = 6;
    /// Socket : objet remis à son état d'origine.
    pub const INIT: i32 = 7;
    pub const DESTROY: i32 = 8;
    /// Socket : objet qui n'en reçoit pas ; renforcement : mauvaise potion.
    pub const IMPROPER: i32 = 9;
}

/// Résultats d'un sertissage de pierre d'esprit (op 7) : ici 0 est la réussite.
pub mod stone_result {
    pub const SUCCESS: i32 = 0;
    pub const FAILED: i32 = 1;
    pub const INVALID_STONE: i32 = 2;
    pub const FULL: i32 = 3;
    pub const IMPROPER_ITEM: i32 = 5;
}

/// État de conversion d'un objet : champs présents dans le paquet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConvertState {
    pub iid: i32,
    pub level: Option<u8>,
    pub sockets: Option<u8>,
    /// Sockets remplis (champ 7).
    pub filled_count: Option<u8>,
    /// TID sertis dans l'ordre (champ 9), s'il est là.
    pub filled: Option<Vec<i32>>,
}

/// Paquets de conversion reçus.
#[derive(Debug, Clone, PartialEq)]
pub enum ConvertEvent {
    /// Fenêtre du forgeron accordée (0x38 op 1).
    WindowGranted { npc: i32 },
    /// État d'un objet (op 0).
    State(ConvertState),
    /// Réponse à un renforcement (op 5), un ajout de socket (op 6), un sertissage de pierre (op 7) ou
    /// de rune (op 8).
    Result { op: i64, result: i32, state: ConvertState, added_tid: Option<i32> },
    /// Options d'un objet (AGPMITEM op 0, champ 20) : TID d'itemoptiontable.txt.
    Options { iid: i32, options: Vec<u16> },
}

/// Demandes de conversion.
#[derive(Debug, Clone, PartialEq)]
pub enum ConvertCommand {
    /// Demander la fenêtre du forgeron `npc`.
    Open(i32),
    /// Renforcer `iid` avec la potion `catalyst`.
    Physical { iid: i32, catalyst: i32 },
    /// Ajouter un socket à `iid`.
    Socket { iid: i32 },
    /// Sertir la pierre `stone` dans `iid`.
    SpiritStone { iid: i32, stone: i32 },
    /// Sertir la rune `rune` dans `iid`.
    Rune { iid: i32, rune: i32 },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// État d'un AGPMITEMCONVERT décodé.
fn state_of(values: &[Option<Value>]) -> Option<ConvertState> {
    let filled = values
        .get(9)
        .and_then(Option::as_ref)
        .and_then(Value::as_bytes)
        .and_then(|b| decode_mini(mini::ITEM_CONVERT_TIDS, b).ok())
        .map(|(_, v)| v.iter().filter_map(|t| t.as_ref().and_then(Value::as_i64)).filter(|t| *t > 0).map(|t| t as i32).collect());
    Some(ConvertState {
        iid: int(values, 2)? as i32,
        level: int(values, 5).map(|v| v.clamp(0, 255) as u8),
        sockets: int(values, 6).map(|v| v.clamp(0, 255) as u8),
        filled_count: int(values, 7).map(|v| v.clamp(0, 255) as u8),
        filled,
    })
}

/// Traduit un paquet 0x16 ou 0x38, sinon `None`.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<ConvertEvent> {
    use archrust_protocol::schema::packet_type as T;
    match packet_type {
        T::EVENT_ITEM_CONVERT if int(values, 0) == Some(1) => {
            let npc = values
                .get(1)
                .and_then(Option::as_ref)
                .and_then(Value::as_bytes)
                .and_then(|b| decode_mini(mini::EVENT_BASE, b).ok())
                .and_then(|(_, v)| int(&v, 1))
                .unwrap_or(0) as i32;
            Some(ConvertEvent::WindowGranted { npc })
        }
        T::ITEM_CONVERT => match int(values, 0)? {
            op::ADD => state_of(values).map(ConvertEvent::State),
            o @ (op::RESPONSE_PHYSICAL | op::RESPONSE_SOCKET | op::RESPONSE_SPIRIT_STONE | op::RESPONSE_RUNE) => Some(ConvertEvent::Result {
                op: o,
                result: int(values, 4).unwrap_or(-1) as i32,
                state: state_of(values)?,
                added_tid: int(values, 8).filter(|t| *t > 0).map(|t| t as i32),
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Options d'un AGPMITEM op 0 (champ 20), s'il en porte.
pub fn options_of(values: &[Option<Value>]) -> Option<ConvertEvent> {
    if int(values, 0) != Some(0) {
        return None;
    }
    let bytes = values.get(20).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, v) = decode_mini(mini::ITEM_OPTION, bytes).ok()?;
    let options: Vec<u16> = v.iter().filter_map(|t| t.as_ref().and_then(Value::as_i64)).filter(|t| *t > 0).map(|t| t as u16).collect();
    Some(ConvertEvent::Options { iid: int(values, 2)? as i32, options })
}

/// États (AGPMITEMCONVERT op 0) et options (AGPMITEM champ 20) des paquets complets du champ 6 de la vue
/// optimisée op 2 (entrée en jeu).
pub fn states_in_block(mut bytes: &[u8]) -> Vec<ConvertEvent> {
    let mut out = Vec::new();
    while bytes.len() > archrust_protocol::packet::HEADER_SIZE && bytes[0] == archrust_protocol::packet::GUARD_HEAD {
        let Ok(len) = bytes[1..5].try_into().map(u32::from_le_bytes) else { break };
        let len = len as usize;
        if len < archrust_protocol::packet::HEADER_SIZE || len > bytes.len() {
            break;
        }
        let (one, rest) = bytes.split_at(len);
        bytes = rest;
        if one[5] == main::ITEM.id {
            if let Some(ev) = archrust_protocol::packet::decode_packet(main::ITEM, one).ok().and_then(|p| options_of(&p.values)) {
                out.push(ev);
            }
            continue;
        }
        if one[5] != main::ITEM_CONVERT.id {
            continue;
        }
        if let Some(ev) = archrust_protocol::packet::decode_packet(main::ITEM_CONVERT, one).ok().and_then(|p| translate(main::ITEM_CONVERT.id, &p.values)) {
            out.push(ev);
        }
    }
    out
}

/// Paquet d'une demande de conversion pour le personnage `me`.
pub fn command_packet(cmd: &ConvertCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::ITEM_CONVERT;
    match cmd {
        ConvertCommand::Open(npc) => (
            main::EVENT_ITEM_CONVERT,
            sparse(main::EVENT_ITEM_CONVERT, &[(0, Value::I8(0)), (1, crate::npc::event_base(*npc, FUNCTION_ITEM_CONVERT)), (2, Value::I32(me))]),
        ),
        ConvertCommand::Physical { iid, catalyst } => (
            s,
            sparse(s, &[(0, Value::I8(op::REQUEST_PHYSICAL)), (1, Value::I32(me)), (2, Value::I32(*iid)), (3, Value::I32(*catalyst))]),
        ),
        ConvertCommand::Socket { iid } => (s, sparse(s, &[(0, Value::I8(op::REQUEST_SOCKET)), (1, Value::I32(me)), (2, Value::I32(*iid))])),
        ConvertCommand::SpiritStone { iid, stone } => (
            s,
            sparse(s, &[(0, Value::I8(op::REQUEST_SPIRIT_STONE)), (1, Value::I32(me)), (2, Value::I32(*iid)), (3, Value::I32(*stone))]),
        ),
        ConvertCommand::Rune { iid, rune } => (
            s,
            sparse(s, &[(0, Value::I8(op::REQUEST_RUNE)), (1, Value::I32(me)), (2, Value::I32(*iid)), (3, Value::I32(*rune))]),
        ),
    }
}

/// Mini des TID sertis (champ 9), pour les essais.
pub fn filled_value(tids: &[i32]) -> Value {
    let m = mini::ITEM_CONVERT_TIDS;
    let pairs: Vec<(usize, Value)> = tids.iter().take(8).enumerate().map(|(i, t)| (i, Value::I32(*t))).collect();
    Value::Packet(build_mini(m, &sparse(m, &pairs)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::build_packet;

    #[test]
    fn etat_et_reponses() {
        let s = main::ITEM_CONVERT;
        let state = sparse(s, &[(0, Value::I8(0)), (1, Value::I32(1)), (2, Value::I32(42)), (5, Value::I8(2)), (6, Value::I8(2)), (7, Value::I8(1)), (9, filled_value(&[201]))]);
        let packet = build_packet(s, &state).unwrap();
        let mut block = build_packet(main::ITEM, &sparse(main::ITEM, &[(0, Value::U8(0)), (2, Value::I32(42))])).unwrap();
        block.extend(&packet);
        let found = states_in_block(&block);
        assert_eq!(
            found,
            vec![ConvertEvent::State(ConvertState { iid: 42, level: Some(2), sockets: Some(2), filled_count: Some(1), filled: Some(vec![201]) })]
        );
        let stone = sparse(s, &[(0, Value::I8(7)), (2, Value::I32(42)), (4, Value::I8(0)), (7, Value::I8(1)), (8, Value::I32(201))]);
        match translate(0x16, &stone) {
            Some(ConvertEvent::Result { op: 7, result: 0, added_tid: Some(201), .. }) => {}
            other => panic!("réponse inattendue : {other:?}"),
        }
        let (sch, v) = command_packet(&ConvertCommand::SpiritStone { iid: 42, stone: 7 }, 1);
        assert_eq!((sch.id, int(&v, 0), int(&v, 2), int(&v, 3)), (0x16, Some(4), Some(42), Some(7)));
        let (sch, v) = command_packet(&ConvertCommand::Open(9), 1);
        assert_eq!((sch.id, int(&v, 0)), (0x38, Some(0)));
        let (_, v) = command_packet(&ConvertCommand::Rune { iid: 42, rune: 8 }, 1);
        assert_eq!((int(&v, 0), int(&v, 3)), (Some(3), Some(8)));
    }

    #[test]
    fn options_d_un_objet() {
        let m = mini::ITEM_OPTION;
        let opts = Value::Packet(build_mini(m, &sparse(m, &[(0, Value::U16(12)), (1, Value::U16(3002))])).unwrap());
        let item = build_packet(main::ITEM, &sparse(main::ITEM, &[(0, Value::U8(0)), (1, Value::I8(2)), (2, Value::I32(42)), (3, Value::I32(2)), (20, opts)])).unwrap();
        assert_eq!(states_in_block(&item), vec![ConvertEvent::Options { iid: 42, options: vec![12, 3002] }]);
    }
}
