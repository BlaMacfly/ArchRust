//! Combats de Shrine de l'épisode 8 (AgpmShrineBattle, 0x62 ; AgcmShrine d'Archonia, init `FUN_140727920`).
//!
//! Structure **brute**, sans champs AuPacket (réception `FUN_140272980`, envoi `FUN_1404a66a0`) :
//!
//! ```text
//!   0x00 0xD6 | 0x01 u32 taille (structure + pied) | 0x05 0x62 | 0x06 drapeaux | 0x07…0x12 propriétaire, horloge
//!   0x13 u8 = 1 | 0x14 u16 opération | 0x16 bourrage | 0x18 corps… | pied 0x6B
//! ```
//!
//! Serveur → client (taille minimale, pied compris) :
//! * op 1 (0xF0) : @0x18 Id, @0x20 Shrine — changement d'état (gravure, occupation) ;
//! * op 2 (0x980) : 10 entrées de 0xF0 dès @0x20, le Shrine à +0x20 de chacune — liste complète ;
//! * op 3 (0x20) : @0x18 Id, @0x1C valeur — jauge `UI_SHRINE_HP` affichée (≠ 0) ou masquée ;
//! * op 4 (0xF0) : @0x20 Shrine — mise à jour d'une entrée ;
//! * op 6 (0x2C) : @0x18 Id, @0x1C CID, @0x20 drapeau, @0x24 état 1-5, @0x28 valeur — gravure d'un joueur ;
//! * op 7 (0x58) : @0x18 Id, @0x1C `MessageTid` de shrinemessage.xml, @0x20 race, @0x24 nom (52 o) ;
//! * op 10 (0x20) : @0x18 Id, @0x1C minutes — « [%s] Shrine opens in %d minute(s) » ;
//! * op 11 / 12 (0x24) : @0x1C victoires, @0x20 défaites (personnelles / de la race) ;
//! * ops 0, 5, 9 (0x20) : acceptées, sans traitement.
//!
//! Client → serveur (0x20 + pied) : op 0 `{0, CID}` à l'ouverture de `UI_Shrine_info` (`Func_OpenShrineInfo`,
//! `FUN_140724ff0`), op 9 `{Id du Shrine, CID}` pour y entrer (`Func_ShrineIconClick`, `FUN_140725a20`).
//!
//! Shrine (0xD0, copié par `FUN_140729200`) : +0 Id, +4 nom (30), +0x22 nom (49), +0x58 5 × i64 (colonnes 2-6
//! de ShrineTable), +0x80 état (0 fermé, 1 ouvert, 2 gravure, 3 fin de gravure, 4 occupation, 5, 6 terminé), +0x84
//! race occupante (1-4), +0x88 race du graveur, +0x90 5 × i64, +0xB8 CID du graveur, +0xBC / +0xC0 colonnes 8 / 7,
//! +0xC4 valeur de la minicarte.

use archrust_protocol::schema::packet_type;

pub const PACKET_TYPE: u8 = packet_type::SHRINE_BATTLE;
/// Taille d'une structure Shrine.
pub const SHRINE_SIZE: usize = 0xD0;
/// Entrées de la liste (op 2) et de la table du client.
pub const MAX_SHRINES: usize = 10;
/// Taille d'une entrée de la liste (op 2).
const ENTRY_SIZE: usize = 0xF0;
/// Décalage de l'opération et du corps.
pub const OP: usize = 0x14;
pub const BODY: usize = 0x18;

pub mod op {
    pub const REQUEST_LIST: u16 = 0;
    pub const STATE: u16 = 1;
    pub const LIST: u16 = 2;
    pub const GAUGE: u16 = 3;
    pub const UPDATE: u16 = 4;
    pub const PLAYER: u16 = 6;
    pub const MESSAGE: u16 = 7;
    pub const JOIN: u16 = 9;
    pub const OPENING: u16 = 10;
    pub const MY_SCORE: u16 = 11;
    pub const RACE_SCORE: u16 = 12;
}

/// États d'un Shrine (+0x80).
pub mod state {
    pub const CLOSED: i32 = 0;
    pub const OPEN: i32 = 1;
    pub const CARVE: i32 = 2;
    pub const CARVED: i32 = 3;
    pub const OCCUPY: i32 = 4;
    pub const FINISHED: i32 = 6;
}

/// Un Shrine tel que le serveur l'envoie.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shrine {
    pub id: i32,
    pub name: String,
    /// Nom du champ +0x22 (joueur ou race occupante ; sens exact inconnu).
    pub holder: String,
    /// Colonnes 2 à 6 de ShrineTable (+0x58…+0x78) ; Archonia compare +0x68 (`times[2]`) à +0xB0 (`run[4]`) :
    /// moins de 300 000 ms d'écart ouvrent l'entrée.
    pub times: [i64; 5],
    pub state: i32,
    pub race: i32,
    /// Race du graveur (+0x88), clé de `RaceCode` de shrinemessage.xml (`FUN_140726a00`).
    pub carver_race: i32,
    /// Temps d'exécution (+0x90…+0xB0) ; Archonia trie la liste sur le mot bas de `run[0]`.
    pub run: [i64; 5],
    pub carver: u32,
    pub col8: i32,
    pub col7: i32,
    pub minimap: i32,
}

/// Délai d'entrée après l'ouverture (`FUN_140729700`).
pub const ENTRY_DELAY_MS: i64 = 300_000;

impl Shrine {
    /// L'entrée est encore permise sans le bouton de déplacement (`FUN_140729700`) : moins de 300 000 ms entre
    /// +0x68 et +0xB0 (comparaison non signée), Shrine ni fermé ni terminé.
    pub fn entry_window(&self) -> bool {
        (self.times[2].wrapping_sub(self.run[4]) as u64) < ENTRY_DELAY_MS as u64 && self.state != state::FINISHED && self.state != state::CLOSED
    }

    /// Décode une structure de 0xD0 octets.
    pub fn decode(b: &[u8]) -> Option<Shrine> {
        if b.len() < SHRINE_SIZE {
            return None;
        }
        let i64_at = |o: usize| i64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        Some(Shrine {
            id: i32_at(b, 0),
            name: text_at(b, 4, 30),
            holder: text_at(b, 0x22, 49),
            times: std::array::from_fn(|i| i64_at(0x58 + i * 8)),
            state: i32_at(b, 0x80),
            race: i32_at(b, 0x84),
            carver_race: i32_at(b, 0x88),
            run: std::array::from_fn(|i| i64_at(0x90 + i * 8)),
            carver: i32_at(b, 0xB8) as u32,
            col8: i32_at(b, 0xBC),
            col7: i32_at(b, 0xC0),
            minimap: i32_at(b, 0xC4),
        })
    }

    /// Structure de 0xD0 octets (serveur, essais).
    pub fn encode(&self) -> [u8; SHRINE_SIZE] {
        let mut b = [0u8; SHRINE_SIZE];
        b[0..4].copy_from_slice(&self.id.to_le_bytes());
        put_text(&mut b, 4, 30, &self.name);
        put_text(&mut b, 0x22, 49, &self.holder);
        for i in 0..5 {
            b[0x58 + i * 8..0x60 + i * 8].copy_from_slice(&self.times[i].to_le_bytes());
            b[0x90 + i * 8..0x98 + i * 8].copy_from_slice(&self.run[i].to_le_bytes());
        }
        for (o, v) in [(0x80, self.state), (0x84, self.race), (0x88, self.carver_race), (0xB8, self.carver as i32), (0xBC, self.col8), (0xC0, self.col7), (0xC4, self.minimap)] {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShrineEvent {
    /// Op 1 : changement d'état d'un Shrine.
    State { id: i32, shrine: Shrine },
    /// Op 2 : les 10 entrées (Id 0 = vide).
    List(Vec<Shrine>),
    /// Op 3 : jauge de gravure affichée (`value` ≠ 0) ou masquée.
    Gauge { id: i32, value: i32 },
    /// Op 4 : mise à jour d'une entrée.
    Update(Shrine),
    /// Op 6 : gravure d'un joueur (drapeau 0 : fin, état 2 annulée ; sinon état 1-5).
    Player { id: i32, cid: i32, flag: i32, state: i32, value: i32 },
    /// Op 7 : message `tid` de shrinemessage.xml, race (ou valeur) et nom.
    Message { id: i32, tid: i32, race: i32, name: String },
    /// Op 10 : ouverture dans `minutes` minutes.
    Opening { id: i32, minutes: i32 },
    /// Op 11 (personnel) / 12 (race) : victoires et défaites.
    Score { race: bool, win: i32, lose: i32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShrineCommand {
    /// Op 0 : liste à l'ouverture de `UI_Shrine_info`.
    RequestList,
    /// Op 9 : entrer dans le Shrine `id`.
    Join(i32),
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map_or(0, |x| i32::from_le_bytes(x.try_into().unwrap()))
}

fn text_at(b: &[u8], o: usize, max: usize) -> String {
    let s = b.get(o..(o + max).min(b.len())).unwrap_or_default();
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    encoding_rs::WINDOWS_1252.decode(&s[..end]).0.trim().to_string()
}

fn put_text(b: &mut [u8], o: usize, max: usize, s: &str) {
    let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(s);
    let n = bytes.len().min(max - 1);
    b[o..o + n].copy_from_slice(&bytes[..n]);
}

/// Paquet brut de l'opération `op` : structure de `size` octets (en-tête compris, pied exclu) remplie par `fill`
/// à partir du corps ; le pied 0x6B est ajouté et la taille posée (`FUN_1404a66a0`).
pub fn packet(op: u16, size: usize, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut out = vec![0u8; size.max(BODY)];
    out[0] = 0xd6;
    out[5] = PACKET_TYPE;
    out[0x13] = 1;
    out[OP..OP + 2].copy_from_slice(&op.to_le_bytes());
    fill(&mut out);
    out.push(0x6b);
    let len = out.len() as u32;
    out[1..5].copy_from_slice(&len.to_le_bytes());
    out
}

/// Demande du client (`me` = CID du joueur).
pub fn command_packet(cmd: &ShrineCommand, me: i32) -> Vec<u8> {
    let (o, a) = match cmd {
        ShrineCommand::RequestList => (op::REQUEST_LIST, 0),
        ShrineCommand::Join(id) => (op::JOIN, *id),
    };
    packet(o, 0x20, |b| {
        b[0x18..0x1C].copy_from_slice(&a.to_le_bytes());
        b[0x1C..0x20].copy_from_slice(&me.to_le_bytes());
    })
}

/// Taille minimale d'une opération reçue, pied compris (`FUN_140272980`).
fn min_size(o: u16) -> Option<usize> {
    Some(match o {
        op::STATE | op::UPDATE => 0xF0,
        op::LIST => 0x980,
        op::PLAYER => 0x2C,
        op::MESSAGE => 0x58,
        op::MY_SCORE | op::RACE_SCORE => 0x24,
        0 | op::GAUGE | 5 | op::JOIN | op::OPENING => 0x20,
        _ => return None,
    })
}

/// Paquet 0x62 reçu (tampon complet, en-tête et pied compris).
pub fn decode(raw: &[u8]) -> Option<ShrineEvent> {
    if raw.len() < 0x1C || raw.get(5) != Some(&PACKET_TYPE) {
        return None;
    }
    let o = u16::from_le_bytes([raw[OP], raw[OP + 1]]);
    if raw.len() < min_size(o)? {
        return None;
    }
    let at = |off: usize| i32_at(raw, off);
    Some(match o {
        op::STATE => ShrineEvent::State { id: at(0x18), shrine: Shrine::decode(&raw[0x20..])? },
        op::LIST => ShrineEvent::List((0..MAX_SHRINES).filter_map(|i| Shrine::decode(&raw[0x20 + i * ENTRY_SIZE + 0x20..])).collect()),
        op::GAUGE => ShrineEvent::Gauge { id: at(0x18), value: at(0x1C) },
        op::UPDATE => ShrineEvent::Update(Shrine::decode(&raw[0x20..])?),
        op::PLAYER => ShrineEvent::Player { id: at(0x18), cid: at(0x1C), flag: at(0x20), state: at(0x24), value: at(0x28) },
        op::MESSAGE => ShrineEvent::Message { id: at(0x18), tid: at(0x1C), race: at(0x20), name: text_at(raw, 0x24, 52) },
        op::OPENING => ShrineEvent::Opening { id: at(0x18), minutes: at(0x1C) },
        op::MY_SCORE | op::RACE_SCORE => ShrineEvent::Score { race: o == op::RACE_SCORE, win: at(0x1C), lose: at(0x20) },
        _ => return None,
    })
}

/// Paquets du serveur (essais, outils) : op 1 et 4 portent un Shrine en @0x20.
pub fn shrine_packet(o: u16, shrine: &Shrine) -> Vec<u8> {
    packet(o, 0xF0, |b| {
        b[0x18..0x1C].copy_from_slice(&shrine.id.to_le_bytes());
        b[0x20..0xF0].copy_from_slice(&shrine.encode());
    })
}

/// Op 2 : liste des Shrines (10 entrées au plus, le reste vide).
pub fn list_packet(shrines: &[Shrine]) -> Vec<u8> {
    packet(op::LIST, 0x20 + MAX_SHRINES * ENTRY_SIZE, |b| {
        for (i, s) in shrines.iter().take(MAX_SHRINES).enumerate() {
            let o = 0x20 + i * ENTRY_SIZE;
            b[o..o + 4].copy_from_slice(&s.id.to_le_bytes());
            b[o + 0x20..o + 0x20 + SHRINE_SIZE].copy_from_slice(&s.encode());
        }
    })
}

/// Op 7 : message de shrinemessage.xml.
pub fn message_packet(id: i32, tid: i32, race: i32, name: &str) -> Vec<u8> {
    packet(op::MESSAGE, 0x58, |b| {
        for (o, v) in [(0x18, id), (0x1C, tid), (0x20, race)] {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        put_text(b, 0x24, 52, name);
    })
}

/// Ops à deux entiers en @0x18 et @0x1C (3 jauge, 10 ouverture), ou en @0x1C et @0x20 (11, 12 scores).
pub fn pair_packet(o: u16, a: i32, c: i32) -> Vec<u8> {
    let (size, off) = if matches!(o, op::MY_SCORE | op::RACE_SCORE) { (0x24, 0x1C) } else { (0x20, 0x18) };
    packet(o, size, |b| {
        b[off..off + 4].copy_from_slice(&a.to_le_bytes());
        b[off + 4..off + 8].copy_from_slice(&c.to_le_bytes());
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Shrine {
        Shrine { id: 3, name: "Shrine of Fire".into(), holder: "Sim".into(), times: [1, 2, 3, 4, 5], state: state::CARVE, race: 2, carver_race: 7, run: [9, 8, 7, 6, 5], carver: 0x1234, col8: 8, col7: 7, minimap: 42 }
    }

    #[test]
    fn demandes_du_client() {
        let p = command_packet(&ShrineCommand::RequestList, 77);
        assert_eq!(p.len(), 0x21);
        assert_eq!((p[0], u32::from_le_bytes(p[1..5].try_into().unwrap()), p[5], p[0x13], *p.last().unwrap()), (0xd6, 0x21, 0x62, 1, 0x6b));
        assert_eq!((u16::from_le_bytes([p[0x14], p[0x15]]), i32_at(&p, 0x18), i32_at(&p, 0x1C)), (0, 0, 77));
        let p = command_packet(&ShrineCommand::Join(5), 77);
        assert_eq!((u16::from_le_bytes([p[0x14], p[0x15]]), i32_at(&p, 0x18), i32_at(&p, 0x1C)), (9, 5, 77));
    }

    #[test]
    fn shrine_aller_retour() {
        let s = sample();
        assert_eq!(Shrine::decode(&s.encode()), Some(s.clone()));
        assert_eq!(decode(&shrine_packet(op::STATE, &s)), Some(ShrineEvent::State { id: 3, shrine: s.clone() }));
        assert_eq!(decode(&shrine_packet(op::UPDATE, &s)), Some(ShrineEvent::Update(s.clone())));
        let list = list_packet(&[s.clone()]);
        assert_eq!(list.len(), 0x981);
        match decode(&list) {
            Some(ShrineEvent::List(l)) => {
                assert_eq!(l.len(), 10);
                assert_eq!(l[0], s);
                assert_eq!(l[1].id, 0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn messages_et_compteurs() {
        assert_eq!(decode(&message_packet(3, 9, 1, "Carver")), Some(ShrineEvent::Message { id: 3, tid: 9, race: 1, name: "Carver".into() }));
        assert_eq!(decode(&pair_packet(op::GAUGE, 3, 1)), Some(ShrineEvent::Gauge { id: 3, value: 1 }));
        assert_eq!(decode(&pair_packet(op::OPENING, 3, 5)), Some(ShrineEvent::Opening { id: 3, minutes: 5 }));
        assert_eq!(decode(&pair_packet(op::MY_SCORE, 4, 2)), Some(ShrineEvent::Score { race: false, win: 4, lose: 2 }));
        assert_eq!(decode(&pair_packet(op::RACE_SCORE, 1, 0)), Some(ShrineEvent::Score { race: true, win: 1, lose: 0 }));
        let p = packet(op::PLAYER, 0x2C, |b| {
            for (o, v) in [(0x18, 3), (0x1C, 42), (0x20, 0), (0x24, 2), (0x28, 9)] {
                b[o..o + 4].copy_from_slice(&i32::to_le_bytes(v));
            }
        });
        assert_eq!(decode(&p), Some(ShrineEvent::Player { id: 3, cid: 42, flag: 0, state: 2, value: 9 }));
        // Trop court, opération inconnue ou sans traitement, autre type : ignorés.
        assert_eq!(decode(&packet(op::MESSAGE, 0x40, |_| {})), None);
        assert_eq!(decode(&packet(13, 0x40, |_| {})), None);
        assert_eq!(decode(&packet(5, 0x20, |_| {})), None);
        let mut other = pair_packet(op::GAUGE, 1, 1);
        other[5] = 0x58;
        assert_eq!(decode(&other), None);
    }

    #[test]
    fn delai_d_entree() {
        let mut s = Shrine { state: state::OPEN, ..Default::default() };
        s.times[2] = 1_000_000;
        s.run[4] = 900_000;
        assert!(s.entry_window());
        s.run[4] = 600_000;
        assert!(!s.entry_window());
        s.run[4] = 900_000;
        s.state = state::FINISHED;
        assert!(!s.entry_window());
    }
}
