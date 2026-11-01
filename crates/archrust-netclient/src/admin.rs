//! Niveau de maître du jeu (AgpmAdmin, 0x12). À l'entrée en jeu, le serveur envoie l'op 0 avec un
//! mini-paquet `AGPMADMIN_CHAR` (champ 2) : CID, nom, niveau MJ (champ 3) et compte. Le client s'en
//! sert pour les fonctions réservées aux MJ (téléportation par clic sur les cartes, voir
//! `app_worldmap.rs` du moteur).

use archrust_protocol::packet::{decode_mini, Value};
use archrust_protocol::schema::{mini, packet_type};

pub mod op {
    pub const ADD_CHARACTER: i8 = 0;
}

/// Niveau MJ d'un personnage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdminLevel {
    pub cid: i32,
    pub level: i32,
}

/// Paquet AGPMADMIN op 0 reçu.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<AdminLevel> {
    if packet_type != packet_type::ADMIN {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    if int(0) != Some(op::ADD_CHARACTER as i64) {
        return None;
    }
    let bytes = values.get(2).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, info) = decode_mini(mini::ADMIN_CHAR, bytes).ok()?;
    let field = |i: usize| info.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    Some(AdminLevel { cid: field(1).or(int(1)).unwrap_or(0) as i32, level: field(3).unwrap_or(0) as i32 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_mini, sparse};
    use archrust_protocol::schema::main;

    #[test]
    fn niveau_mj_lu_du_paquet_op_0() {
        let info = build_mini(mini::ADMIN_CHAR, &sparse(mini::ADMIN_CHAR, &[(0, Value::I8(1)), (1, Value::I32(42)), (2, Value::str("GmTest")), (3, Value::I16(5)), (5, Value::str("test"))])).unwrap();
        let values = sparse(main::ADMIN, &[(0, Value::I8(0)), (1, Value::I32(42)), (2, Value::Packet(info))]);
        assert_eq!(translate(packet_type::ADMIN, &values), Some(AdminLevel { cid: 42, level: 5 }));
        // Autre op : rien.
        let other = sparse(main::ADMIN, &[(0, Value::I8(3)), (1, Value::I32(42))]);
        assert_eq!(translate(packet_type::ADMIN, &other), None);
    }
}
