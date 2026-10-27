//! Combat entre joueurs (AgpmPvP, 0x4A, du serveur au client ; réception d'Archonia `FUN_1402d2c70`).
//!
//! Op 0 : victoires, défaites, mode de la zone (1 sûr, 2 libre, 3 combat). Op 2 / 4 : un ennemi (champ 2) ajouté
//! ou retiré, op 6 : liste vidée. Op 18 : message système, code au champ 1, entier au champ 3, chaînes aux champs
//! 8 et 9 (textes `PvP_Msg_*` d'uimessage.txt, voir le moteur).

use archrust_protocol::packet::Value;
use archrust_protocol::schema::packet_type;

pub mod op {
    pub const INFO: i64 = 0;
    pub const ADD_ENEMY: i64 = 2;
    pub const REMOVE_ENEMY: i64 = 4;
    pub const INIT_ENEMY: i64 = 6;
    pub const SYSTEM_MESSAGE: i64 = 18;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PvpEvent {
    pub op: i64,
    /// CID concerné, ou code du message (op 18).
    pub cid: i64,
    pub target: i64,
    /// Victoires (op 0) ou entier du message (op 18).
    pub win: i64,
    pub lose: i64,
    pub mode: Option<i64>,
    pub text1: String,
    pub text2: String,
}

pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<PvpEvent> {
    if packet_type != packet_type::PVP {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    let text = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default().trim().to_string();
    Some(PvpEvent {
        op: int(0)?,
        cid: int(1).unwrap_or(0),
        target: int(2).unwrap_or(0),
        win: int(3).unwrap_or(0),
        lose: int(4).unwrap_or(0),
        mode: int(5),
        text1: text(8),
        text2: text(9),
    })
}
