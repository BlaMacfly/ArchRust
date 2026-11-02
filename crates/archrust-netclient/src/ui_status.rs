//! Statut d'interface (AgpmUIStatus, 0x14, drapeaux 2, 12 champs) : barre de raccourcis gardée par le
//! serveur, potions de vie et de mana de la barre, seuils de leur prise automatique (curseurs posés sur
//! les jauges de PV et de PM de `UI_CharInfo`, en pourcentage, 0 : désactivé) et affichage du casque.
//! Archonia boit lui-même la potion quand une jauge passe sous son seuil.
//!
//! * **Barre** : 5 couches × 10 cases, index = couche × 10 + colonne (0-49). Chaque case est un mini
//!   `AGPMSKILL_BASE` (drapeaux 1, `INT8` type de base, `INT32` identifiant) : 5 objet (IID), 7 compétence
//!   (identifiant d'instance de la compétence possédée) ; seuls ces deux types sont affichés et envoyés.
//! * **op 0** (serveur → client, entrée en jeu) : champ 2 = les 50 mini-paquets ; mini absent, vide ou
//!   illisible = case vide. Champs 6/7 copiés tels quels, 9/10 s'ils sont ≥ 0 (FUN_1402671f0 d'Archonia).
//! * **op 1** (dans les deux sens) : champ 3 = index de la case, champ 4 = mini (absent : case vidée) ;
//!   champs 6/7 (TID des potions, jamais 0) et 9/10 (seuils) facultatifs. Reçue : la même base est retirée
//!   des autres cases de sa rangée de 10, puis posée (FUN_1402671f0). Envoyée comme FUN_140266e90 : champ 3
//!   si l'index est ≥ 0, champ 4 si type et identifiant ≠ 0, 6/7 si ≠ 0.
//! * **op 3** : casque affiché (champ 8). L'op 2 (barre encodée en texte, champs 5 et 11) n'est pas
//!   employée : aucun abonné dans le client.

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type};

pub mod op {
    pub const ADD: i8 = 0;
    pub const UPDATE_ITEM: i8 = 1;
    /// Casque visible (champ 8, 0 ou 1), envoyé par la case « Afficher le casque » des options.
    pub const VIEW_HELMET: i8 = 3;
}

/// Cases de la barre gardées par le serveur : 5 couches × 10.
pub const BELT_SLOTS: usize = 50;

/// Types de base d'`AGPMSKILL_BASE` employés par la barre (FUN_1402f56f0).
pub mod base_type {
    /// Objet : IID.
    pub const ITEM: i8 = 5;
    /// Compétence : identifiant d'instance de la compétence possédée.
    pub const SKILL: i8 = 7;
}

/// Contenu d'une case : type de base et identifiant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeltBase {
    pub kind: i8,
    pub id: i32,
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

/// Paquet 0x14 reçu (op 0 : envoi complet à l'entrée en jeu ; op 1 : changement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiStatus {
    pub op: i8,
    /// TID des potions (0 : absent ; op 0 : copiés tels quels, op 1 : appliqués s'ils sont ≠ 0).
    pub hp_potion_tid: i32,
    pub mp_potion_tid: i32,
    pub view_helmet: bool,
    /// Seuils en % (-1 : absent ; appliqués s'ils sont ≥ 0).
    pub auto_hp: i8,
    pub auto_mp: i8,
    /// op 0 : les 50 cases (`None` : champ 2 absent, serveur qui ne garde pas la barre).
    pub belt: Option<Vec<Option<BeltBase>>>,
    /// op 1 : case changée (0-49) et son contenu (`None` : vidée).
    pub slot: Option<(usize, Option<BeltBase>)>,
}

/// Changement envoyé au serveur (op 1) : seuls les champs donnés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiStatusUpdate {
    pub hp_potion_tid: Option<i32>,
    pub mp_potion_tid: Option<i32>,
    pub auto_hp: Option<i8>,
    pub auto_mp: Option<i8>,
    /// Case de la barre (0-49) et son contenu (`None` : retrait).
    pub slot: Option<(usize, Option<BeltBase>)>,
}

/// Mini `AGPMSKILL_BASE` d'une case.
pub fn encode_base(b: BeltBase) -> Vec<u8> {
    build_mini(mini::SKILL_BASE, &sparse(mini::SKILL_BASE, &[(0, Value::I8(b.kind)), (1, Value::I32(b.id))])).unwrap_or_default()
}

/// Case lue d'un mini `AGPMSKILL_BASE` : vide si le mini est vide ou illisible, d'un autre type que 5
/// ou 7, ou d'identifiant nul.
pub fn decode_base(raw: &[u8]) -> Option<BeltBase> {
    if raw.is_empty() {
        return None;
    }
    let (_, v) = decode_mini(mini::SKILL_BASE, raw).ok()?;
    let kind = v.first()?.as_ref()?.as_i64()? as i8;
    let id = v.get(1)?.as_ref()?.as_i64()? as i32;
    (matches!(kind, base_type::ITEM | base_type::SKILL) && id != 0).then_some(BeltBase { kind, id })
}

/// Paquet op 0 ou op 1 reçu.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<UiStatus> {
    if packet_type != packet_type::UI_STATUS {
        return None;
    }
    let field = |i: usize| values.get(i).and_then(Option::as_ref);
    let int = |i: usize| field(i).and_then(Value::as_i64);
    let op = int(0)? as i8;
    if op != op::ADD && op != op::UPDATE_ITEM {
        return None;
    }
    let belt = field(2).filter(|_| op == op::ADD).map(|v| {
        let list: Vec<&[u8]> = match v {
            Value::PacketList(l) => l.iter().map(Vec::as_slice).collect(),
            Value::Packet(m) => vec![m.as_slice()],
            _ => Vec::new(),
        };
        (0..BELT_SLOTS).map(|i| list.get(i).and_then(|m| decode_base(m))).collect::<Vec<_>>()
    });
    // Index −1…49 ; −1 (potions ou seuils seuls) : pas de case.
    let slot = int(3)
        .filter(|i| op == op::UPDATE_ITEM && (0..BELT_SLOTS as i64).contains(i))
        .map(|i| (i as usize, field(4).and_then(Value::as_bytes).and_then(decode_base)));
    Some(UiStatus {
        op,
        hp_potion_tid: int(6).unwrap_or(0) as i32,
        mp_potion_tid: int(7).unwrap_or(0) as i32,
        view_helmet: int(8).unwrap_or(1) != 0,
        auto_hp: int(9).map_or(-1, |v| v as i8),
        auto_mp: int(10).map_or(-1, |v| v as i8),
        belt,
        slot,
    })
}

/// Applique un changement de case reçu (op 1) aux 50 cases : la même base est retirée des autres cases
/// de sa rangée de 10 (`index / 10 * 10` … `+ 9`), puis posée ; sans contenu, la case est vidée.
pub fn apply_slot(belt: &mut [Option<BeltBase>], index: usize, base: Option<BeltBase>) {
    if index >= belt.len() {
        return;
    }
    if let Some(b) = base {
        let row = index / 10 * 10;
        for cell in belt.iter_mut().skip(row).take(10) {
            if *cell == Some(b) {
                *cell = None;
            }
        }
    }
    belt[index] = base;
}

/// Paquet op 1 d'un changement.
pub fn update_packet(u: &UiStatusUpdate, me: i32) -> (&'static archrust_protocol::schema::Schema, Vec<Option<Value>>) {
    let mut pairs = vec![(0, Value::I8(op::UPDATE_ITEM)), (1, Value::I32(me))];
    if let Some((index, base)) = u.slot.filter(|(i, _)| *i < BELT_SLOTS) {
        pairs.push((3, Value::I8(index as i8)));
        if let Some(b) = base.filter(|b| b.kind != 0 && b.id != 0) {
            pairs.push((4, Value::Packet(encode_base(b))));
        }
    }
    // Potions : jamais envoyées avec 0 (FUN_140650800 / FUN_140650860).
    if let Some(t) = u.hp_potion_tid.filter(|t| *t != 0) {
        pairs.push((6, Value::I32(t)));
    }
    if let Some(t) = u.mp_potion_tid.filter(|t| *t != 0) {
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
    fn barre_gardee_par_le_serveur() {
        use archrust_protocol::packet::{build_packet, decode_packet};
        // op 0 : 50 minis, cases 0 (compétence 12) et 13 (objet 900), les autres vides (mini vide).
        let mut minis = vec![Vec::new(); BELT_SLOTS];
        minis[0] = encode_base(BeltBase { kind: base_type::SKILL, id: 12 });
        minis[13] = encode_base(BeltBase { kind: base_type::ITEM, id: 900 });
        // Type non affiché (6, modèle d'objet) : case vide.
        minis[20] = encode_base(BeltBase { kind: 6, id: 4 });
        let add = sparse(
            main::UI_STATUS,
            &[(0, Value::I8(op::ADD)), (1, Value::I32(7)), (2, Value::PacketList(minis)), (6, Value::I32(190)), (7, Value::I32(0)), (9, Value::I8(30)), (10, Value::I8(0))],
        );
        let raw = build_packet(main::UI_STATUS, &add).unwrap();
        let p = decode_packet(main::UI_STATUS, &raw).unwrap();
        let st = translate(packet_type::UI_STATUS, &p.values).unwrap();
        let belt = st.belt.clone().unwrap();
        assert_eq!(belt.len(), BELT_SLOTS);
        assert_eq!(belt[0], Some(BeltBase { kind: base_type::SKILL, id: 12 }));
        assert_eq!(belt[13], Some(BeltBase { kind: base_type::ITEM, id: 900 }));
        assert_eq!(belt.iter().filter(|c| c.is_some()).count(), 2);
        assert_eq!((st.op, st.hp_potion_tid, st.mp_potion_tid, st.auto_hp, st.auto_mp, st.slot), (op::ADD, 190, 0, 30, 0, None));
        // Serveur sans barre (champ 2 absent) ; liste courte : cases manquantes vides.
        let st = translate(packet_type::UI_STATUS, &sparse(main::UI_STATUS, &[(0, Value::I8(op::ADD))])).unwrap();
        assert_eq!((st.belt, st.auto_hp), (None, -1));
        let short = sparse(main::UI_STATUS, &[(0, Value::I8(op::ADD)), (2, Value::PacketList(vec![encode_base(BeltBase { kind: 5, id: 3 })]))]);
        let belt = translate(packet_type::UI_STATUS, &short).unwrap().belt.unwrap();
        assert_eq!((belt[0], belt[1..].iter().all(Option::is_none)), (Some(BeltBase { kind: 5, id: 3 }), true));

        // op 1 envoyée : pose (index + mini), retrait (index seul), potion jamais 0.
        let pose = UiStatusUpdate { slot: Some((42, Some(BeltBase { kind: base_type::SKILL, id: 12 }))), ..Default::default() };
        let (_, v) = update_packet(&pose, 7);
        assert_eq!((v[0].clone(), v[3].clone()), (Some(Value::I8(op::UPDATE_ITEM)), Some(Value::I8(42))));
        let Some(Value::Packet(m)) = &v[4] else { panic!("mini attendu") };
        assert_eq!(m, &vec![0x03, 7, 12, 0, 0, 0]);
        assert_eq!(decode_base(m), Some(BeltBase { kind: 7, id: 12 }));
        let raw = build_packet(main::UI_STATUS, &v).unwrap();
        let back = translate(packet_type::UI_STATUS, &decode_packet(main::UI_STATUS, &raw).unwrap().values).unwrap();
        assert_eq!((back.op, back.slot), (op::UPDATE_ITEM, Some((42, Some(BeltBase { kind: 7, id: 12 })))));
        let (_, v) = update_packet(&UiStatusUpdate { slot: Some((5, None)), hp_potion_tid: Some(0), ..Default::default() }, 7);
        assert_eq!((v[3].clone(), v[4].is_none(), v[6].is_none()), (Some(Value::I8(5)), true, true));
        assert_eq!(translate(packet_type::UI_STATUS, &v).unwrap().slot, Some((5, None)));
        // op 1 sans case (potion seule).
        let (_, v) = update_packet(&UiStatusUpdate { mp_potion_tid: Some(191), ..Default::default() }, 7);
        assert!(v[3].is_none() && v[4].is_none());
        assert_eq!(v[7], Some(Value::I32(191)));
        assert_eq!(translate(packet_type::UI_STATUS, &v).unwrap().slot, None);
    }

    #[test]
    fn op_1_recue_retire_la_meme_base_de_la_rangee() {
        let s = |id| Some(BeltBase { kind: base_type::SKILL, id });
        let mut belt = vec![None; BELT_SLOTS];
        belt[10] = s(4);
        belt[19] = s(4);
        belt[3] = s(4);
        apply_slot(&mut belt, 15, s(4));
        // Rangée 10-19 : seule la case 15 garde la compétence ; la rangée 0-9 n'est pas touchée.
        assert_eq!((belt[10], belt[19], belt[15], belt[3]), (None, None, s(4), s(4)));
        apply_slot(&mut belt, 3, None);
        assert_eq!(belt[3], None);
        apply_slot(&mut belt, 60, s(1));
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
