//! Annonces du Battleground (AgcmBattleGround, 0x58 ; constructeur d'Archonia `FUN_14062e4c0`, réception
//! `FUN_14062e9f0`). Structure **brute**, du serveur au client seulement :
//!
//! ```text
//!   0x00…0x12 en-tête | 0x13 u8 opération (seule 1, annonce, est traitée) | 0x14 texte (1024 o)
//!   0x414 u32 couleur ARGB | 0x418 i32 type | 0x41C i32 variante | pied 0x6B
//! ```
//!
//! Types (`FUN_14062ec70`) : 2 début (sysstr 780 « Battleground Race Battle Starts », 782 « Unlimited Hunting
//! Starts » si la variante ≠ 0), 4 fin (781 / 783), 5 texte au centre de l'écran ; les autres vont à la fenêtre
//! d'AgcmUISiegeWar (texte et couleur).

use archrust_protocol::schema::packet_type;

pub const PACKET_TYPE: u8 = packet_type::BATTLEGROUND;
/// Opération d'annonce (0x13).
pub const OP_ANNOUNCE: u8 = 1;
const TEXT: usize = 0x14;
const TEXT_LEN: usize = 1024;
const COLOR: usize = 0x414;
const KIND: usize = 0x418;
const VARIANT: usize = 0x41C;
/// Taille de la structure, pied exclu.
const SIZE: usize = 0x420;

pub mod kind {
    pub const START: i32 = 2;
    pub const END: i32 = 4;
    pub const CENTER: i32 = 5;
}

#[derive(Debug, Clone, PartialEq)]
pub struct BattlegroundEvent {
    pub text: String,
    /// Couleur ARGB (0xAARRGGBB).
    pub color: u32,
    pub kind: i32,
    pub variant: i32,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Paquet 0x58 reçu (tampon complet).
pub fn decode(raw: &[u8]) -> Option<BattlegroundEvent> {
    if raw.len() < SIZE || raw.get(5) != Some(&PACKET_TYPE) || raw[0x13] != OP_ANNOUNCE {
        return None;
    }
    let t = &raw[TEXT..TEXT + TEXT_LEN];
    let end = t.iter().position(|&c| c == 0).unwrap_or(t.len());
    Some(BattlegroundEvent {
        text: encoding_rs::WINDOWS_1252.decode(&t[..end]).0.trim().to_string(),
        color: u32_at(raw, COLOR),
        kind: u32_at(raw, KIND) as i32,
        variant: u32_at(raw, VARIANT) as i32,
    })
}

/// Annonce telle que l'envoie un serveur (essais, outils).
pub fn announce_packet(ev: &BattlegroundEvent) -> Vec<u8> {
    let mut out = vec![0u8; SIZE];
    out[0] = 0xd6;
    out[5] = PACKET_TYPE;
    out[0x13] = OP_ANNOUNCE;
    let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(&ev.text);
    let n = bytes.len().min(TEXT_LEN - 1);
    out[TEXT..TEXT + n].copy_from_slice(&bytes[..n]);
    out[COLOR..COLOR + 4].copy_from_slice(&ev.color.to_le_bytes());
    out[KIND..KIND + 4].copy_from_slice(&ev.kind.to_le_bytes());
    out[VARIANT..VARIANT + 4].copy_from_slice(&ev.variant.to_le_bytes());
    out.push(0x6b);
    let len = out.len() as u32;
    out[1..5].copy_from_slice(&len.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annonce_aller_retour() {
        let ev = BattlegroundEvent { text: "Les orcs mènent".into(), color: 0xFF00_FF00, kind: kind::CENTER, variant: 0 };
        let p = announce_packet(&ev);
        assert_eq!((p.len(), p[0x13], *p.last().unwrap()), (0x421, 1, 0x6b));
        assert_eq!(decode(&p), Some(ev));
        // Autre opération, paquet tronqué : ignorés.
        let mut q = p.clone();
        q[0x13] = 2;
        assert_eq!(decode(&q), None);
        assert_eq!(decode(&p[..0x300]), None);
    }
}
