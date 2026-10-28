//! Titre d'ArchLord (AgpmArchlord 0x55, drapeaux 2, constructeur d'Archonia `FUN_140293b50`, réception
//! `FUN_140293eb0`).
//!
//! Champs : op, nom de l'ArchLord (CHAR[32]), nom du garde (CHAR[32]), résultat, étape, gardes actifs, gardes
//! maximum, identifiant de message. Le client ne traite que les ops 0 à 6 :
//! * 0 ArchLord désigné (nom mémorisé, annonce `sysstr` 630) ; 1 fin de règne (nom effacé, 631) ;
//! * 2 / 3 résultat d'activation / de retrait d'un garde (nom non vide : 638 / 640, vide : 639 / 641) ;
//! * 4 gardes (650 « %d/%d ») ; 5 étape (1 à 4 → 642 à 645, autre valeur : rien) ;
//! * 6 message (0 à 4 → 646 à 649, 656).
//!
//! Seul l'op 7 est envoyé (clic sur un événement de fonction 36) : Archonia passe le CID et l'événement au-delà
//! des 8 champs déclarés, ils ne sont donc pas sérialisés ; le paquet ne porte que `op = 7`.

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

/// Fonction d'événement « ArchLord » (ApmEventManager).
pub const FUNCTION: i32 = 36;
/// Longueur au-delà de laquelle un nom reçu est ignoré (`FUN_140293eb0`).
pub const MAX_NAME: usize = 48;

pub mod op {
    pub const SET: i8 = 0;
    pub const CANCEL: i8 = 1;
    pub const GUARD_ADD: i8 = 2;
    pub const GUARD_REMOVE: i8 = 3;
    pub const GUARD_INFO: i8 = 4;
    pub const STEP: i8 = 5;
    pub const MESSAGE: i8 = 6;
    pub const EVENT_REQUEST: i8 = 7;
    pub const EVENT_GRANT: i8 = 8;
}

/// Étapes de la guerre de l'ArchLord (op 5).
pub mod step {
    pub const NONE: i32 = 0;
    pub const PORTAL: i32 = 1;
    pub const RENSPHERE_FIGHT: i32 = 2;
    pub const BATTLE: i32 = 3;
    pub const END: i32 = 4;
}

/// Messages (op 6) : identifiant → numéro de `sysstr.txt`.
pub fn message_sysstr(id: i32) -> Option<u32> {
    Some(match id {
        0 => 646,
        1 => 647,
        2 => 648,
        3 => 649,
        4 => 656,
        _ => return None,
    })
}

/// Étape (op 5) → numéro de `sysstr.txt`.
pub fn step_sysstr(s: i32) -> Option<u32> {
    Some(match s {
        step::PORTAL => 642,
        step::RENSPHERE_FIGHT => 643,
        step::BATTLE => 644,
        step::END => 645,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchlordCommand {
    /// Demande d'événement ArchLord (op 7), au clic sur un événement de fonction 36.
    EventRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchlordEvent {
    /// Nouvel ArchLord (op 0).
    Set(String),
    /// Fin du règne (op 1) : nom reçu (vide s'il manque).
    Cancel(String),
    /// Activation (op 2) ou retrait (op 3) d'un garde : nom du garde (vide : échec), résultat.
    Guard { add: bool, guard: String, result: i8 },
    /// Gardes (op 4) : actifs, maximum.
    Guards { current: i32, max: i32 },
    /// Étape courante (op 5).
    Step(i32),
    /// Message (op 6).
    Message(i32),
}

pub fn command_packet(cmd: &ArchlordCommand) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::ARCHLORD;
    match cmd {
        ArchlordCommand::EventRequest => (s, sparse(s, &[(0, Value::I8(op::EVENT_REQUEST))])),
    }
}

fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn txt(v: &[Option<Value>], i: usize) -> String {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default()
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<ArchlordEvent> {
    if ptype != packet_type::ARCHLORD {
        return None;
    }
    Some(match int(values, 0)? as i8 {
        op::SET => {
            let name = txt(values, 1);
            if name.is_empty() || name.chars().count() > MAX_NAME {
                return None;
            }
            ArchlordEvent::Set(name)
        }
        op::CANCEL => ArchlordEvent::Cancel(txt(values, 1)),
        o @ (op::GUARD_ADD | op::GUARD_REMOVE) => ArchlordEvent::Guard { add: o == op::GUARD_ADD, guard: txt(values, 2), result: int(values, 3).unwrap_or(0) as i8 },
        op::GUARD_INFO => ArchlordEvent::Guards { current: int(values, 5).unwrap_or(0) as i32, max: int(values, 6).unwrap_or(0) as i32 },
        op::STEP => ArchlordEvent::Step(int(values, 4).unwrap_or(0) as i32),
        op::MESSAGE => ArchlordEvent::Message(int(values, 7).unwrap_or(-1) as i32),
        _ => return None,
    })
}

/// Paquet d'un événement reçu (essais et serveur de test).
pub fn event_packet(ev: &ArchlordEvent) -> Vec<Option<Value>> {
    let s = main::ARCHLORD;
    match ev {
        ArchlordEvent::Set(n) => sparse(s, &[(0, Value::I8(op::SET)), (1, Value::str(n))]),
        ArchlordEvent::Cancel(n) => sparse(s, &[(0, Value::I8(op::CANCEL)), (1, Value::str(n))]),
        ArchlordEvent::Guard { add, guard, result } => {
            sparse(s, &[(0, Value::I8(if *add { op::GUARD_ADD } else { op::GUARD_REMOVE })), (2, Value::str(guard)), (3, Value::I8(*result))])
        }
        ArchlordEvent::Guards { current, max } => sparse(s, &[(0, Value::I8(op::GUARD_INFO)), (5, Value::I32(*current)), (6, Value::I32(*max))]),
        ArchlordEvent::Step(v) => sparse(s, &[(0, Value::I8(op::STEP)), (4, Value::I32(*v))]),
        ArchlordEvent::Message(v) => sparse(s, &[(0, Value::I8(op::MESSAGE)), (7, Value::I32(*v))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round(values: &[Option<Value>]) -> Vec<Option<Value>> {
        let s = main::ARCHLORD;
        decode_packet(s, &build_packet(s, values).unwrap()).unwrap().values
    }

    #[test]
    fn demande_d_evenement() {
        let (s, v) = command_packet(&ArchlordCommand::EventRequest);
        assert_eq!(s.id, 0x55);
        // Drapeaux u16 = 0x0001 : l'op seule.
        let v = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap().values;
        assert_eq!(int(&v, 0), Some(7));
        assert!(v.iter().skip(1).all(Option::is_none));
    }

    #[test]
    fn paquets_du_serveur() {
        let all = [
            ArchlordEvent::Set("Bob".into()),
            ArchlordEvent::Cancel("Bob".into()),
            ArchlordEvent::Guard { add: true, guard: "Gardien".into(), result: 1 },
            ArchlordEvent::Guard { add: false, guard: String::new(), result: 0 },
            ArchlordEvent::Guards { current: 2, max: 5 },
            ArchlordEvent::Step(3),
            ArchlordEvent::Message(4),
        ];
        for ev in all {
            assert_eq!(translate(0x55, &round(&event_packet(&ev))), Some(ev));
        }
        // Nom vide ignoré, op inconnue ignorée, autre type de paquet ignoré.
        assert_eq!(translate(0x55, &round(&event_packet(&ArchlordEvent::Set(String::new())))), None);
        assert_eq!(translate(0x55, &round(&sparse(main::ARCHLORD, &[(0, Value::I8(8))]))), None);
        assert_eq!(translate(0x53, &round(&event_packet(&ArchlordEvent::Step(1)))), None);
        assert_eq!((step_sysstr(4), step_sysstr(0), message_sysstr(4), message_sysstr(5)), (Some(645), None, Some(656), None));
    }
}
