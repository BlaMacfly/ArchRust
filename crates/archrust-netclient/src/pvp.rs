//! Combat entre joueurs (AgpmPvP, 0x4A, du serveur au client ; réception d'Archonia `FUN_1402d2c70`).
//!
//! Op 0 : victoires, défaites, mode de la zone (1 sûr, 2 libre, 3 combat). Op 2 / 4 : un ennemi (champ 2) ajouté
//! ou retiré, op 6 : liste vidée, op 8 : ennemi connu actualisé (mode au champ 5). Guildes ennemies (champ 8, ID
//! de guilde) : op 10 ajoutée ou actualisée (mode au champ 5), 12 retirée, 14 liste vidée, 16 actualisée
//! (`FUN_1402d2c70`, listes de l'attache PvP +0x208 et +0x598). Op 18 : message système, code au champ 1, entier au
//! champ 3, chaînes aux champs 8 et 9 (textes `PvP_Msg_*` d'uimessage.txt, voir le moteur). L'op 17
//! (`PvP_RecvCannotUseTeleport`, événement absent de UI_1024X768.ini) n'affiche rien : le serveur envoie l'op 18
//! code 11.

use archrust_protocol::packet::Value;
use archrust_protocol::schema::packet_type;

pub mod op {
    pub const INFO: i64 = 0;
    pub const ADD_ENEMY: i64 = 2;
    pub const REMOVE_ENEMY: i64 = 4;
    pub const INIT_ENEMY: i64 = 6;
    pub const UPDATE_ENEMY: i64 = 8;
    pub const ADD_ENEMY_GUILD: i64 = 10;
    pub const REMOVE_ENEMY_GUILD: i64 = 12;
    pub const INIT_ENEMY_GUILD: i64 = 14;
    pub const UPDATE_ENEMY_GUILD: i64 = 16;
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
    /// Champ 8 : ID de guilde (ops 9 à 16) ou chaîne 1 du message (op 18).
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

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet, sparse};
    use archrust_protocol::schema::main;

    #[test]
    fn guildes_ennemies() {
        let s = main::PVP;
        let p = |pairs: &[(usize, Value)]| decode_packet(s, &build_packet(s, &sparse(s, pairs)).unwrap()).unwrap().values;
        let add = translate(packet_type::PVP, &p(&[(0, Value::I8(10)), (1, Value::I32(7)), (5, Value::I8(2)), (8, Value::str("Lames"))])).unwrap();
        assert_eq!((add.op, add.cid, add.mode, add.text1.as_str()), (op::ADD_ENEMY_GUILD, 7, Some(2), "Lames"));
        let upd = translate(packet_type::PVP, &p(&[(0, Value::I8(8)), (1, Value::I32(7)), (2, Value::I32(9)), (5, Value::I8(3))])).unwrap();
        assert_eq!((upd.op, upd.target, upd.mode), (op::UPDATE_ENEMY, 9, Some(3)));
        let init = translate(packet_type::PVP, &p(&[(0, Value::I8(14)), (1, Value::I32(7))])).unwrap();
        assert_eq!((init.op, init.text1.as_str()), (op::INIT_ENEMY_GUILD, ""));
    }
}
