//! Heure du monde et météo, serveur → client : AgpmTimer (0x11), AgpmEventNature (0x30),
//! AgpmNatureEffect (0x47). Aucun des trois n'a d'octet d'opération et le client ne les envoie
//! jamais (Archonia ne fait que les recevoir).
//!
//! * **AgpmTimer** (drapeaux 1) : champ 0 `UINT8` vitesse du temps (multiplicateur), champ 1
//!   `UINT64` temps du jeu en millisecondes, champ 2 `UINT32` heure du serveur (secondes Unix), champ 3
//!   `UINT32` décalage d'affichage (fuseau, secondes) ; un champ à 0 est ignoré (`FUN_14022e800`). La
//!   fenêtre des boss compte avec le champ 2 + les secondes écoulées depuis le dernier AgpmTimer
//!   (`FUN_1401a6fd0`) et affiche champ 2 + champ 3 (« Server Time: », `FUN_140677c90`).
//! * **AgpmEventNature** (drapeaux 1) : champ 0 `INT8` météo (−1 absent ; AgcmEventNature :
//!   0 = fin en fondu, 3 = pluie → effet 1, 5 = neige → effet 0, autres valeurs sans effet
//!   visible), champs 1 et 2 `INT64` non lus.
//! * **AgpmNatureEffect** (drapeaux **2**) : champ 0 `INT32` identifiant d'effet de
//!   `natureeffect.ini`, −1 = arrêt en fondu (valeur par défaut si le champ manque).

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

/// Météo d'AgpmEventNature comprise par AgcmEventNature (`FUN_140579990`).
pub mod weather {
    /// Fin de l'effet de nature en fondu.
    pub const CLEAR: i8 = 0;
    /// Pluie : effet de nature 1.
    pub const RAIN: i8 = 3;
    /// Neige : effet de nature 0.
    pub const SNOW: i8 = 5;
}

/// Identifiant d'effet « arrêt » d'AgpmNatureEffect.
pub const EFFECT_STOP: i32 = -1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatureEvent {
    /// AgpmTimer : `None` = champ absent ou nul (valeur précédente conservée).
    Timer { speed: Option<u8>, time_ms: Option<u64>, unix: Option<u32>, zone: Option<i32> },
    /// AgpmEventNature : météo (champ 0, présent et ≠ −1).
    Weather(i8),
    /// AgpmNatureEffect : effet de nature (−1 = arrêt).
    Effect(i32),
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<NatureEvent> {
    match ptype {
        packet_type::TIMER => {
            let speed = values.first().and_then(Option::as_ref).and_then(|v| match v {
                Value::U8(s) => Some(*s),
                other => other.as_i64().map(|s| s as u8),
            });
            let time = values.get(1).and_then(Option::as_ref).and_then(|v| match v {
                Value::U64(t) => Some(*t),
                other => other.as_i64().map(|t| t as u64),
            });
            // Champs 2 et 3 (UINT32) : le fuseau peut être négatif, relu en entier signé.
            let unix = int(values, 2).map(|v| v as u32).filter(|&v| v != 0);
            let zone = int(values, 3).map(|v| v as u32 as i32).filter(|&v| v != 0);
            Some(NatureEvent::Timer { speed: speed.filter(|&s| s != 0), time_ms: time.filter(|&t| t != 0), unix, zone })
        }
        packet_type::EVENT_NATURE => {
            let w = int(values, 0)? as i8;
            (w != -1).then_some(NatureEvent::Weather(w))
        }
        packet_type::NATURE_EFFECT => Some(NatureEvent::Effect(int(values, 0).map_or(EFFECT_STOP, |v| v as i32))),
        _ => None,
    }
}

/// Paquet AgpmTimer tel que l'envoie un serveur (pour les essais et les outils).
pub fn timer_packet(speed: u8, time_ms: u64) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::TIMER;
    (s, sparse(s, &[(0, Value::U8(speed)), (1, Value::U64(time_ms))]))
}

/// Paquet AgpmTimer complet : vitesse, temps du jeu, heure du serveur (Unix, s) et fuseau (s).
pub fn server_time_packet(speed: u8, time_ms: u64, unix: u32, zone: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::TIMER;
    (s, sparse(s, &[(0, Value::U8(speed)), (1, Value::U64(time_ms)), (2, Value::U32(unix)), (3, Value::U32(zone as u32))]))
}

/// Paquet AgpmEventNature (météo seule).
pub fn weather_packet(weather: i8) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::EVENT_NATURE;
    (s, sparse(s, &[(0, Value::I8(weather))]))
}

/// Paquet AgpmNatureEffect.
pub fn effect_packet(id: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::NATURE_EFFECT;
    (s, sparse(s, &[(0, Value::I32(id))]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round_trip(schema: &'static Schema, values: &[Option<Value>]) -> Option<NatureEvent> {
        let bytes = build_packet(schema, values).unwrap();
        let p = decode_packet(schema, &bytes).unwrap();
        translate(schema.id, &p.values)
    }

    #[test]
    fn timer_vitesse_et_temps() {
        let (s, v) = timer_packet(6, 43_200_000);
        assert_eq!(s.id, 0x11);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: Some(6), time_ms: Some(43_200_000), unix: None, zone: None }));
        // Champs nuls ou absents : ignorés, comme Archonia.
        let v = sparse(s, &[(0, Value::U8(0)), (1, Value::U64(5))]);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: None, time_ms: Some(5), unix: None, zone: None }));
        let v = sparse(s, &[(0, Value::U8(1))]);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: Some(1), time_ms: None, unix: None, zone: None }));
        // Temps au-delà de i64::MAX : lu tel quel en UINT64.
        let v = sparse(s, &[(1, Value::U64(u64::MAX - 1))]);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: None, time_ms: Some(u64::MAX - 1), unix: None, zone: None }));
        // Heure du serveur et fuseau (négatif : UINT32 relu en entier signé) ; nuls : ignorés.
        let (s, v) = server_time_packet(6, 1, 1_793_000_000, -3600);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: Some(6), time_ms: Some(1), unix: Some(1_793_000_000), zone: Some(-3600) }));
        let (s, v) = server_time_packet(6, 1, 0, 0);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Timer { speed: Some(6), time_ms: Some(1), unix: None, zone: None }));
    }

    #[test]
    fn meteo_et_effet() {
        let (s, v) = weather_packet(weather::RAIN);
        assert_eq!(s.id, 0x30);
        assert_eq!(s.flag_len, 1);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Weather(3)));
        assert_eq!(round_trip(s, &weather_packet(-1).1), None);
        // Champs 1 et 2 (INT64) présents mais ignorés.
        let v = sparse(s, &[(0, Value::I8(5)), (1, Value::I64(1)), (2, Value::I64(-2))]);
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Weather(5)));

        let (s, v) = effect_packet(1);
        assert_eq!(s.id, 0x47);
        assert_eq!(s.flag_len, 2);
        let bytes = build_packet(s, &v).unwrap();
        assert_eq!(round_trip(s, &v), Some(NatureEvent::Effect(1)));
        assert_eq!(round_trip(s, &effect_packet(-1).1), Some(NatureEvent::Effect(-1)));
        // Champ absent : arrêt.
        assert_eq!(round_trip(s, &sparse(s, &[])), Some(NatureEvent::Effect(-1)));
        assert!(bytes.len() > 4);
        assert_eq!(translate(packet_type::WORLD, &[]), None);
    }
}
