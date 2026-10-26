//! Statut d'interface (AgpmUIStatus, 0x14) : potions de vie et de mana de la barre, seuils de leur prise
//! automatique (curseurs posés sur les jauges de PV et de PM de `UI_CharInfo`, en pourcentage, 0 :
//! désactivé) et affichage du casque. Le serveur les envoie à l'entrée en jeu (op 0) ; le client renvoie
//! chaque changement (op 1). Archonia boit lui-même la potion quand une jauge passe sous son seuil.

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type};

pub mod op {
    pub const ADD: i8 = 0;
    pub const UPDATE_ITEM: i8 = 1;
    /// Casque visible (champ 8, 0 ou 1), envoyé par la case « Afficher le casque » des options.
    pub const VIEW_HELMET: i8 = 3;
}

/// Paquet op 3 : casque visible ou non (`FUN_140267160` d'Archonia).
pub fn helmet_packet(view: bool, me: i32) -> (&'static archrust_protocol::schema::Schema, Vec<Option<Value>>) {
    (main::UI_STATUS, sparse(main::UI_STATUS, &[(0, Value::I8(op::VIEW_HELMET)), (1, Value::I32(me)), (8, Value::I8(view as i8))]))
}

/// Paquet AgpmCharacter op 0x11 : drapeaux d'options du personnage (champ 22).
pub fn option_flags_packet(flags: u32, me: i32) -> (&'static archrust_protocol::schema::Schema, Vec<Option<Value>>) {
    use archrust_protocol::schema::character_op;
    (main::CHARACTER, sparse(main::CHARACTER, &[(0, Value::I8(character_op::UPDATE_OPTION_FLAG)), (1, Value::I32(me)), (22, Value::I32(flags as i32))]))
}

/// Réglages reçus à l'entrée en jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiStatus {
    pub hp_potion_tid: i32,
    pub mp_potion_tid: i32,
    pub view_helmet: bool,
    pub auto_hp: i8,
    pub auto_mp: i8,
}

/// Changement envoyé au serveur : seuls les champs donnés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiStatusUpdate {
    pub hp_potion_tid: Option<i32>,
    pub mp_potion_tid: Option<i32>,
    pub auto_hp: Option<i8>,
    pub auto_mp: Option<i8>,
}

/// Paquet op 0 reçu.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<UiStatus> {
    if packet_type != packet_type::UI_STATUS {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    (int(0) == Some(op::ADD as i64)).then(|| UiStatus {
        hp_potion_tid: int(6).unwrap_or(0) as i32,
        mp_potion_tid: int(7).unwrap_or(0) as i32,
        view_helmet: int(8).unwrap_or(1) != 0,
        auto_hp: int(9).unwrap_or(0) as i8,
        auto_mp: int(10).unwrap_or(0) as i8,
    })
}

/// Paquet op 1 d'un changement.
pub fn update_packet(u: &UiStatusUpdate, me: i32) -> (&'static archrust_protocol::schema::Schema, Vec<Option<Value>>) {
    let mut pairs = vec![(0, Value::I8(op::UPDATE_ITEM)), (1, Value::I32(me))];
    if let Some(t) = u.hp_potion_tid {
        pairs.push((6, Value::I32(t)));
    }
    if let Some(t) = u.mp_potion_tid {
        pairs.push((7, Value::I32(t)));
    }
    if let Some(v) = u.auto_hp {
        pairs.push((9, Value::I8(v)));
    }
    if let Some(v) = u.auto_mp {
        pairs.push((10, Value::I8(v)));
    }
    (main::UI_STATUS, sparse(main::UI_STATUS, &pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statut_d_interface_aller_retour() {
        let (schema, values) = update_packet(&UiStatusUpdate { auto_hp: Some(40), ..Default::default() }, 7);
        assert_eq!(schema.name, "AGPMUISTATUS");
        assert_eq!(values[9], Some(Value::I8(40)));
        assert!(values[6].is_none() && values[10].is_none(), "seuls les champs donnés");
        let add = sparse(main::UI_STATUS, &[(0, Value::I8(op::ADD)), (1, Value::I32(7)), (6, Value::I32(190)), (9, Value::I8(30))]);
        let st = translate(packet_type::UI_STATUS, &add).unwrap();
        assert_eq!((st.hp_potion_tid, st.auto_hp, st.view_helmet), (190, 30, true));
    }

    #[test]
    fn casque_et_drapeaux_d_options() {
        let (schema, v) = helmet_packet(false, 7);
        assert_eq!(schema.name, "AGPMUISTATUS");
        assert_eq!((v[0].clone(), v[1].clone(), v[8].clone()), (Some(Value::I8(3)), Some(Value::I32(7)), Some(Value::I8(0))));
        let (schema, v) = option_flags_packet(0x12, 7);
        assert_eq!(schema.name, "AGPMCHARACTER");
        assert_eq!((v[0].clone(), v[1].clone(), v[22].clone()), (Some(Value::I8(0x11)), Some(Value::I32(7)), Some(Value::I32(0x12))));
        assert!(v[2].is_none() && v[23].is_none());
    }
}
