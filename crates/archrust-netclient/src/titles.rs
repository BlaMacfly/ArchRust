//! Titres de l'épisode 8 (AgpmTitle, 0x60 ; AgcmTitle d'Archonia, abonnements `FUN_1405f8f30`).
//!
//! Structure **brute**, sans champs AuPacket (constructeur `FUN_140304fc0`, réception `FUN_1403058b0`, envoi
//! `FUN_1404a66a0` : longueur u32 = taille de la structure + 1, pied 0x6B) :
//!
//! ```text
//!   0x00 0xD6 | 0x01 u32 longueur | 0x05 0x60 | 0x06…0x12 0 | 0x13 u8 = 1 | 0x14 u16 opération
//!   0x16 CHAR[49] nom du personnage concerné (48 o + NUL, retrouvé par son nom) | 0x47 bourrage
//!   0x48 i32 TID du titre | 0x4C… champs de l'opération (i32 LE ; « i64 » = 8 octets) | pied 0x6B
//! ```
//!
//! Opérations (taille de la structure, pied exclu) :
//! * 0 ADD (0x4C) : sans émetteur ni abonné dans Archonia ;
//! * 1 ADD_RESULT (0x54, S → C) : @0x4C résultat (1 : titre ajouté, 500 au plus), @0x50 compétence choisie ;
//! * 2 USE (0x54, C → S) : @0x4C = 2 (constante, `FUN_1405f4ce0`), @0x50 1 équiper / 0 retirer ;
//! * 3 USE_RESULT (0x64, S → C) : @0x4C bUse, @0x50 résultat (1 ok), @0x58 heure du changement (time_t),
//!   @0x60 code (3 : moins de 2 h depuis le dernier changement) ;
//! * 4 LIST (0x4C, C → S) : sans appelant dans Archonia ;
//! * 5 LIST_RESULT (0x68, S → C) : un paquet par titre possédé, @0x50 heure du dernier changement, @0x5C 1 entrée
//!   valide (0 : liste vide), @0x60 0 (≠ 0 : rappel sans ajout), @0x64 compétence choisie ;
//! * 6 QUEST, sous-opération @0x4C : 0 QUEST_ADD (0x50, C → S) ; 1 QUEST_ADD_RESULT (0x5C) @0x50 i64 date
//!   d'acceptation, @0x58 résultat ; 3 QUEST_CHECK (0x68) @0x50 i32[5] valeurs des conditions, @0x64 ≠ 0 :
//!   progression de `UI_TitleMini` ; 4 QUEST_COMPLETE (0x54, C → S) @0x50 compétence ; 5 QUEST_COMPLETE_RESULT
//!   (0x5C) @0x50 i64 date d'achèvement, @0x58 résultat ; 6 QUEST_LIST (0x50, C → S, TID 0) ; 7 QUEST_LIST_RESULT
//!   (0x84) @0x50 1 valide, @0x54 0, @0x58 0 en cours / ≠ 0 acquis, @0x60 i64 acceptation, @0x68 i64 achèvement,
//!   @0x70 i32[5] compteurs ; la sous-opération 2 n'a ni émetteur ni abonné (non reprise) ;
//! * 7 DELETE (0x4C, C → S) : supprime un titre possédé ou abandonne un objectif ;
//! * 8 DELETE_RESULT (0x50, S → C) : @0x4C résultat (≠ 0 : retiré) ;
//! * 9 USE_NEAR (0x50, S → C) : @0x4C 1 afficher / 0 effacer le titre au-dessus du personnage nommé ;
//! * 10 UI_OPEN (0x4C, S → C) : `UI_TitleAll` et `UI_TitleMini` (PNJ « <Title Manager> ») ; Archonia ne lit
//!   pas le TID : ArchCore y met 1 quand les Schola 1-3 et Defender of Chantra sont activés (convention
//!   ArchCore / ArchRust) ;
//! * 11 SKILL_SELECT (0x50, les deux sens) : @0x4C compétence choisie (ajout d'Archonia, rangs 4 / 5) ; codé
//!   pour les essais, ignoré par le moteur.

use archrust_protocol::schema::packet_type;

pub const PACKET_TYPE: u8 = packet_type::TITLE;
/// Décalages de l'opération, du nom, du TID et de la sous-opération de l'op 6.
pub const OP: usize = 0x14;
pub const NAME: usize = 0x16;
pub const NAME_LEN: usize = 49;
pub const TID: usize = 0x48;
pub const SUB: usize = 0x4C;
/// Constante de l'op 2 (`FUN_1405f4ce0`, sens inconnu).
pub const USE_MAGIC: i32 = 2;

pub mod op {
    pub const ADD: u16 = 0;
    pub const ADD_RESULT: u16 = 1;
    pub const USE: u16 = 2;
    pub const USE_RESULT: u16 = 3;
    pub const LIST: u16 = 4;
    pub const LIST_RESULT: u16 = 5;
    pub const QUEST: u16 = 6;
    pub const DELETE: u16 = 7;
    pub const DELETE_RESULT: u16 = 8;
    pub const USE_NEAR: u16 = 9;
    pub const UI_OPEN: u16 = 10;
    pub const SKILL_SELECT: u16 = 11;
}

/// Sous-opérations de l'op 6.
pub mod quest {
    pub const ADD: i32 = 0;
    pub const ADD_RESULT: i32 = 1;
    pub const CHECK: i32 = 3;
    pub const COMPLETE: i32 = 4;
    pub const COMPLETE_RESULT: i32 = 5;
    pub const LIST: i32 = 6;
    pub const LIST_RESULT: i32 = 7;
}

/// Code d'erreur de l'op 3 : moins de deux heures depuis le dernier changement.
pub const USE_CODE_TOO_SOON: i32 = 3;

/// Corps d'un paquet 0x60, d'après l'opération.
#[derive(Debug, Clone, PartialEq)]
pub enum TitleBody {
    Add,
    AddResult { result: i32, skill: i32 },
    Use { on: bool },
    UseResult { on: bool, result: i32, time: i64, code: i32 },
    List,
    ListEntry { changed_at: i64, valid: bool, skip: i32, skill: i32 },
    QuestAdd,
    QuestAddResult { accepted: i64, result: i32 },
    QuestCheck { values: [i32; 5], progress: bool },
    QuestComplete { skill: i32 },
    QuestCompleteResult { completed: i64, result: i32 },
    QuestList,
    QuestEntry { valid: bool, done: bool, accepted: i64, completed: i64, counters: [i32; 5] },
    Delete,
    DeleteResult { result: i32 },
    UseNear { on: bool },
    UiOpen,
    SkillSelect { skill: i32 },
}

/// Paquet 0x60 : personnage concerné (par son nom), TID du titre, corps.
#[derive(Debug, Clone, PartialEq)]
pub struct TitlePacket {
    pub name: String,
    pub tid: i32,
    pub body: TitleBody,
}

/// Demandes du client (le nom du joueur est ajouté à l'envoi).
#[derive(Debug, Clone, PartialEq)]
pub enum TitleCommand {
    /// Op 2 : équiper (`on`) ou retirer le titre.
    Use { tid: i32, on: bool },
    /// Op 6.0 : fixer le titre comme objectif.
    QuestAdd(i32),
    /// Op 6.4 : terminer l'objectif (compétence 0 : pas de choix dans l'épisode 8).
    QuestComplete(i32),
    /// Op 7 : supprimer un titre possédé ou abandonner un objectif.
    Delete(i32),
    /// Op 4 et 6.6 : listes (sans appelant dans Archonia, gardées pour les essais).
    List,
    QuestList,
}

impl TitleBody {
    /// Opération, sous-opération (op 6) et taille de la structure (pied exclu).
    pub fn layout(&self) -> (u16, Option<i32>, usize) {
        use TitleBody::*;
        match self {
            Add => (op::ADD, None, 0x4C),
            AddResult { .. } => (op::ADD_RESULT, None, 0x54),
            Use { .. } => (op::USE, None, 0x54),
            UseResult { .. } => (op::USE_RESULT, None, 0x64),
            List => (op::LIST, None, 0x4C),
            ListEntry { .. } => (op::LIST_RESULT, None, 0x68),
            QuestAdd => (op::QUEST, Some(quest::ADD), 0x50),
            QuestAddResult { .. } => (op::QUEST, Some(quest::ADD_RESULT), 0x5C),
            QuestCheck { .. } => (op::QUEST, Some(quest::CHECK), 0x68),
            QuestComplete { .. } => (op::QUEST, Some(quest::COMPLETE), 0x54),
            QuestCompleteResult { .. } => (op::QUEST, Some(quest::COMPLETE_RESULT), 0x5C),
            QuestList => (op::QUEST, Some(quest::LIST), 0x50),
            QuestEntry { .. } => (op::QUEST, Some(quest::LIST_RESULT), 0x84),
            Delete => (op::DELETE, None, 0x4C),
            DeleteResult { .. } => (op::DELETE_RESULT, None, 0x50),
            UseNear { .. } => (op::USE_NEAR, None, 0x50),
            UiOpen => (op::UI_OPEN, None, 0x4C),
            SkillSelect { .. } => (op::SKILL_SELECT, None, 0x50),
        }
    }
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map_or(0, |x| i32::from_le_bytes(x.try_into().unwrap()))
}

fn i64_at(b: &[u8], o: usize) -> i64 {
    b.get(o..o + 8).map_or(0, |x| i64::from_le_bytes(x.try_into().unwrap()))
}

fn put32(b: &mut [u8], o: usize, v: i32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn put64(b: &mut [u8], o: usize, v: i64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
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

impl TitlePacket {
    pub fn new(name: &str, tid: i32, body: TitleBody) -> TitlePacket {
        TitlePacket { name: name.to_string(), tid, body }
    }

    /// Paquet complet (en-tête, structure, pied 0x6B).
    pub fn encode(&self) -> Vec<u8> {
        use TitleBody::*;
        let (o, sub, size) = self.body.layout();
        let mut b = vec![0u8; size];
        b[0] = 0xd6;
        b[5] = PACKET_TYPE;
        b[0x13] = 1;
        b[OP..OP + 2].copy_from_slice(&o.to_le_bytes());
        put_text(&mut b, NAME, NAME_LEN, &self.name);
        put32(&mut b, TID, self.tid);
        if let Some(sub) = sub {
            put32(&mut b, SUB, sub);
        }
        match &self.body {
            Add | List | QuestAdd | QuestList | Delete | UiOpen => {}
            AddResult { result, skill } => {
                put32(&mut b, 0x4C, *result);
                put32(&mut b, 0x50, *skill);
            }
            Use { on } => {
                put32(&mut b, 0x4C, USE_MAGIC);
                put32(&mut b, 0x50, i32::from(*on));
            }
            UseResult { on, result, time, code } => {
                put32(&mut b, 0x4C, i32::from(*on));
                put32(&mut b, 0x50, *result);
                put64(&mut b, 0x58, *time);
                put32(&mut b, 0x60, *code);
            }
            ListEntry { changed_at, valid, skip, skill } => {
                put64(&mut b, 0x50, *changed_at);
                put32(&mut b, 0x5C, i32::from(*valid));
                put32(&mut b, 0x60, *skip);
                put32(&mut b, 0x64, *skill);
            }
            QuestAddResult { accepted: t, result } | QuestCompleteResult { completed: t, result } => {
                put64(&mut b, 0x50, *t);
                put32(&mut b, 0x58, *result);
            }
            QuestCheck { values, progress } => {
                for (i, v) in values.iter().enumerate() {
                    put32(&mut b, 0x50 + i * 4, *v);
                }
                put32(&mut b, 0x64, i32::from(*progress));
            }
            QuestComplete { skill } => put32(&mut b, 0x50, *skill),
            QuestEntry { valid, done, accepted, completed, counters } => {
                put32(&mut b, 0x50, i32::from(*valid));
                put32(&mut b, 0x58, i32::from(*done));
                put64(&mut b, 0x60, *accepted);
                put64(&mut b, 0x68, *completed);
                for (i, v) in counters.iter().enumerate() {
                    put32(&mut b, 0x70 + i * 4, *v);
                }
            }
            DeleteResult { result } => put32(&mut b, 0x4C, *result),
            UseNear { on } => put32(&mut b, 0x4C, i32::from(*on)),
            SkillSelect { skill } => put32(&mut b, 0x4C, *skill),
        }
        b.push(0x6b);
        let len = b.len() as u32;
        b[1..5].copy_from_slice(&len.to_le_bytes());
        b
    }

    /// Paquet 0x60 reçu (tampon complet) ; `None` : autre type, opération inconnue ou structure trop courte.
    pub fn decode(raw: &[u8]) -> Option<TitlePacket> {
        use TitleBody::*;
        if raw.len() < SUB || raw.get(5) != Some(&PACKET_TYPE) {
            return None;
        }
        let o = u16::from_le_bytes([raw[OP], raw[OP + 1]]);
        let at = |off: usize| i32_at(raw, off);
        let body = match o {
            op::ADD => Add,
            op::ADD_RESULT => AddResult { result: at(0x4C), skill: at(0x50) },
            op::USE => Use { on: at(0x50) != 0 },
            op::USE_RESULT => UseResult { on: at(0x4C) != 0, result: at(0x50), time: i64_at(raw, 0x58), code: at(0x60) },
            op::LIST => List,
            op::LIST_RESULT => ListEntry { changed_at: i64_at(raw, 0x50), valid: at(0x5C) != 0, skip: at(0x60), skill: at(0x64) },
            op::QUEST => match at(SUB) {
                quest::ADD => QuestAdd,
                quest::ADD_RESULT => QuestAddResult { accepted: i64_at(raw, 0x50), result: at(0x58) },
                quest::CHECK => QuestCheck { values: std::array::from_fn(|i| at(0x50 + i * 4)), progress: at(0x64) != 0 },
                quest::COMPLETE => QuestComplete { skill: at(0x50) },
                quest::COMPLETE_RESULT => QuestCompleteResult { completed: i64_at(raw, 0x50), result: at(0x58) },
                quest::LIST => QuestList,
                quest::LIST_RESULT => QuestEntry { valid: at(0x50) != 0, done: at(0x58) != 0, accepted: i64_at(raw, 0x60), completed: i64_at(raw, 0x68), counters: std::array::from_fn(|i| at(0x70 + i * 4)) },
                _ => return None,
            },
            op::DELETE => Delete,
            op::DELETE_RESULT => DeleteResult { result: at(0x4C) },
            op::USE_NEAR => UseNear { on: at(0x4C) != 0 },
            op::UI_OPEN => UiOpen,
            op::SKILL_SELECT => SkillSelect { skill: at(0x4C) },
            _ => return None,
        };
        if raw.len() < body.layout().2 {
            return None;
        }
        Some(TitlePacket { name: text_at(raw, NAME, NAME_LEN), tid: at(TID), body })
    }
}

/// Paquet d'une demande du client ; `me` : nom du personnage du joueur (copié de char+0x64 par Archonia).
pub fn command_packet(cmd: &TitleCommand, me: &str) -> Vec<u8> {
    let (tid, body) = match cmd {
        TitleCommand::Use { tid, on } => (*tid, TitleBody::Use { on: *on }),
        TitleCommand::QuestAdd(tid) => (*tid, TitleBody::QuestAdd),
        TitleCommand::QuestComplete(tid) => (*tid, TitleBody::QuestComplete { skill: 0 }),
        TitleCommand::Delete(tid) => (*tid, TitleBody::Delete),
        TitleCommand::List => (0, TitleBody::List),
        TitleCommand::QuestList => (0, TitleBody::QuestList),
    };
    TitlePacket::new(me, tid, body).encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op_of(p: &[u8]) -> u16 {
        u16::from_le_bytes([p[OP], p[OP + 1]])
    }

    /// En-tête relevé dans `FUN_1405f4ce0` (équiper le titre 140001 pour « Sim ») : longueur 0x55, 0x60,
    /// drapeau 1 en 0x13, op 2, nom en 0x16, TID en 0x48, 2 en 0x4C, 1 en 0x50, pied 0x6B.
    #[test]
    fn equiper_comme_archonia() {
        let p = command_packet(&TitleCommand::Use { tid: 140_001, on: true }, "Sim");
        assert_eq!(p.len(), 0x55);
        assert_eq!((p[0], u32::from_le_bytes(p[1..5].try_into().unwrap()), p[5], p[0x13], *p.last().unwrap()), (0xd6, 0x55, 0x60, 1, 0x6b));
        assert!(p[6..0x13].iter().all(|&b| b == 0));
        assert_eq!(op_of(&p), op::USE);
        assert_eq!(&p[0x16..0x1A], b"Sim\0");
        assert_eq!((i32_at(&p, 0x48), i32_at(&p, 0x4C), i32_at(&p, 0x50)), (140_001, 2, 1));
        let off = command_packet(&TitleCommand::Use { tid: 140_001, on: false }, "Sim");
        assert_eq!(i32_at(&off, 0x50), 0);
    }

    #[test]
    fn demandes_du_client() {
        // Longueur = taille de la structure + 1 pour chaque demande (`FUN_1404a66a0`).
        for (cmd, o, sub, len) in [
            (TitleCommand::QuestAdd(10_001), op::QUEST, Some(quest::ADD), 0x51),
            (TitleCommand::QuestComplete(10_001), op::QUEST, Some(quest::COMPLETE), 0x55),
            (TitleCommand::Delete(40_001), op::DELETE, None, 0x4D),
            (TitleCommand::List, op::LIST, None, 0x4D),
            (TitleCommand::QuestList, op::QUEST, Some(quest::LIST), 0x51),
        ] {
            let p = command_packet(&cmd, "Bob");
            assert_eq!((p.len(), u32::from_le_bytes(p[1..5].try_into().unwrap()) as usize, op_of(&p)), (len, len, o), "{cmd:?}");
            if let Some(sub) = sub {
                assert_eq!(i32_at(&p, SUB), sub);
            }
        }
        assert_eq!(i32_at(&command_packet(&TitleCommand::QuestComplete(7), "Bob"), 0x50), 0);
        assert_eq!(i32_at(&command_packet(&TitleCommand::QuestList, "Bob"), TID), 0);
    }

    #[test]
    fn toutes_les_operations_aller_retour() {
        use TitleBody::*;
        let bodies = [
            (Add, 0x4D),
            (AddResult { result: 1, skill: 0 }, 0x55),
            (Use { on: true }, 0x55),
            (UseResult { on: true, result: 1, time: 1_761_955_200, code: 0 }, 0x65),
            (List, 0x4D),
            (ListEntry { changed_at: 1_761_955_200, valid: true, skip: 0, skill: 0 }, 0x69),
            (QuestAdd, 0x51),
            (QuestAddResult { accepted: 1_761_000_000, result: 1 }, 0x5D),
            (QuestCheck { values: [37, 0, 0, 0, 0], progress: true }, 0x69),
            (QuestComplete { skill: 0 }, 0x55),
            (QuestCompleteResult { completed: 1_761_500_000, result: 1 }, 0x5D),
            (QuestList, 0x51),
            (QuestEntry { valid: true, done: false, accepted: 1_761_000_000, completed: 0, counters: [12, 0, 0, 0, 0] }, 0x85),
            (Delete, 0x4D),
            (DeleteResult { result: 1 }, 0x51),
            (UseNear { on: true }, 0x51),
            (UiOpen, 0x4D),
            (SkillSelect { skill: 1422 }, 0x51),
        ];
        for (body, len) in bodies {
            let p = TitlePacket::new("Sim", 140_003, body.clone());
            let raw = p.encode();
            assert_eq!(raw.len(), len, "{body:?}");
            assert_eq!(TitlePacket::decode(&raw), Some(p), "{body:?}");
        }
    }

    #[test]
    fn champs_aux_offsets_de_la_spec() {
        let raw = TitlePacket::new("Sim", 140_003, TitleBody::UseResult { on: false, result: 0, time: 0x1122_3344, code: USE_CODE_TOO_SOON }).encode();
        assert_eq!((i32_at(&raw, 0x4C), i32_at(&raw, 0x50), i32_at(&raw, 0x58), i32_at(&raw, 0x60)), (0, 0, 0x1122_3344, 3));
        let raw = TitlePacket::new("Sim", 10_001, TitleBody::ListEntry { changed_at: 5, valid: true, skip: 0, skill: 9 }).encode();
        assert_eq!((i32_at(&raw, 0x50), i32_at(&raw, 0x5C), i32_at(&raw, 0x64)), (5, 1, 9));
        let raw = TitlePacket::new("Sim", 10_001, TitleBody::QuestEntry { valid: true, done: true, accepted: 7, completed: 8, counters: [1, 2, 3, 4, 5] }).encode();
        assert_eq!((i32_at(&raw, SUB), i32_at(&raw, 0x50), i32_at(&raw, 0x54), i32_at(&raw, 0x58), i32_at(&raw, 0x60), i32_at(&raw, 0x68)), (7, 1, 0, 1, 7, 8));
        assert_eq!((i32_at(&raw, 0x70), i32_at(&raw, 0x80)), (1, 5));
        let raw = TitlePacket::new("Sim", 10_001, TitleBody::QuestCheck { values: [50, 0, 0, 0, 0], progress: true }).encode();
        assert_eq!((i32_at(&raw, SUB), i32_at(&raw, 0x50), i32_at(&raw, 0x64)), (3, 50, 1));
    }

    #[test]
    fn rejets() {
        // Trop court, sous-opération 2 (sans abonné), opération inconnue, autre type.
        let mut short = TitlePacket::new("Sim", 1, TitleBody::QuestEntry { valid: true, done: false, accepted: 0, completed: 0, counters: [0; 5] }).encode();
        short.truncate(0x70);
        assert_eq!(TitlePacket::decode(&short), None);
        let mut sub2 = TitlePacket::new("Sim", 1, TitleBody::QuestAdd).encode();
        sub2[SUB] = 2;
        assert_eq!(TitlePacket::decode(&sub2), None);
        let mut unknown = TitlePacket::new("Sim", 1, TitleBody::UiOpen).encode();
        unknown[OP] = 12;
        assert_eq!(TitlePacket::decode(&unknown), None);
        let mut other = TitlePacket::new("Sim", 1, TitleBody::UiOpen).encode();
        other[5] = 0x62;
        assert_eq!(TitlePacket::decode(&other), None);
        // Nom de 48 caractères au plus (CHAR[49]).
        let long = "x".repeat(60);
        let p = TitlePacket::decode(&TitlePacket::new(&long, 1, TitleBody::UiOpen).encode()).unwrap();
        assert_eq!(p.name.len(), 48);
    }
}
