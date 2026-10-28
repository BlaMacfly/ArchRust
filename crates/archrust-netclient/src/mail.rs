//! Courrier entre joueurs (AgpmMailBox, 0x2D ; réception d'Archonia `FUN_1402eb520`).
//!
//! Paquet principal : opération, CID du personnage concerné, mini-paquet courrier (`mini::MAIL`), IID de l'objet
//! joint (op 3), code de résultat (0 succès, toute autre valeur échec).
//!
//! ```text
//! op 0 ajout          S → C : courrier complet, inséré en tête (le client ne demande jamais la liste)
//! op 1 retrait        C → S : mail{ID} (courrier lu) ; S → C : résultat, retiré si 0
//! op 2 lecture        C → S : mail{ID} à chaque ouverture (aucune réponse traitée)
//! op 3 écriture       C → S : mail{expéditeur, destinataire, objet, texte}, IID joint ; S → C : résultat
//! op 4 pièce jointe   C → S : mail{ID} ; S → C : résultat, drapeau 0x10, TID et quantité remis à 0
//! op 5 tout retirer   S → C : liste vidée
//! ```

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

pub mod op {
    pub const ADD: i8 = 0;
    pub const REMOVE: i8 = 1;
    pub const READ: i8 = 2;
    pub const WRITE: i8 = 3;
    pub const ITEM_SAVE: i8 = 4;
    pub const REMOVE_ALL: i8 = 5;
}

/// Drapeaux d'un courrier (champ 5 du mini-paquet).
pub mod flag {
    pub const READ: i32 = 0x01;
    /// Pièce jointe récupérée.
    pub const ITEM_SAVED: i32 = 0x10;
}

/// Courriers au plus par personnage (`FUN_1402eac20` : au-delà, le plus ancien est évincé).
pub const MAX_MAILS: usize = 30;
/// « Ordinary Letter Paper » : un en inventaire pour écrire (`FUN_1402ecb70`), consommé par le serveur.
pub const PAPER_TID: i32 = 3601;
/// « Fine Letter Paper » : compté par Archonia mais jamais utilisé.
pub const FINE_PAPER_TID: i32 = 3602;
/// Destinataire et objet : 1 à 32 octets (`FUN_140639270`, `strlen - 1 < 0x20`).
pub const NAME_MAX: usize = 32;
pub const SUBJECT_MAX: usize = 32;
/// Texte : moins de 1000 octets, envoyé avec son NUL.
pub const BODY_MAX: usize = 999;
/// Préfixe d'une réponse (constante 0x140d582a0) et longueur de l'objet repris (`strncat …, 0x2e`).
pub const REPLY_PREFIX: &str = "[RE]";
pub const REPLY_SUBJECT: usize = 46;

/// Contrôle local d'un objet à joindre (`FUN_1402ece60`) : 0 accepté, 5 objet interdit (papiers à lettre,
/// TID 4609 à 4623), 8 hors de l'inventaire. Les objets liés (code 7) et payants (code 10) sont jugés par le
/// serveur : ArchRust ne connaît pas l'état d'objet qu'Archonia lit à +0x218.
pub fn attachment_check(tid: i32, status: Option<i8>) -> i32 {
    if !matches!(status, Some(2 | 13 | 15)) {
        8
    } else if tid == PAPER_TID || tid == FINE_PAPER_TID || (0x1201..=0x120f).contains(&tid) {
        5
    } else {
        0
    }
}

/// Objet d'une réponse : `[RE]` suivi de l'objet tronqué à 46 octets (`FUN_140639cf0`).
pub fn reply_subject(subject: &str) -> String {
    let mut s = String::from(REPLY_PREFIX);
    let mut n = 0;
    for c in subject.chars() {
        n += c.len_utf8();
        if n > REPLY_SUBJECT {
            break;
        }
        s.push(c);
    }
    s
}

/// Un courrier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mail {
    pub id: i32,
    pub from: String,
    pub to: String,
    pub subject: String,
    /// Date d'envoi en secondes Unix (affichée en heure locale).
    pub date: i64,
    pub flags: i32,
    /// Pièce jointe : TID du modèle (0 aucune) et quantité.
    pub item_tid: i32,
    pub item_count: i32,
    pub body: String,
}

impl Mail {
    pub fn read(&self) -> bool {
        self.flags & flag::READ != 0
    }

    /// Pièce jointe à récupérer (`FUN_14063b380` : TID, quantité et drapeau 0x10).
    pub fn has_item(&self) -> bool {
        self.item_tid != 0 && self.item_count != 0 && self.flags & flag::ITEM_SAVED == 0
    }
}

/// Demandes du client (personnage `me`).
#[derive(Debug, Clone, PartialEq)]
pub enum MailCommand {
    /// Suppression d'un courrier lu.
    Remove(i32),
    /// Courrier ouvert.
    Read(i32),
    /// Écriture : l'expéditeur est le nom du joueur, `item` l'IID de l'objet joint (0 sans).
    Write { from: String, to: String, subject: String, body: String, item: i32 },
    /// Récupération de la pièce jointe.
    SaveItem(i32),
}

/// Paquets reçus.
#[derive(Debug, Clone, PartialEq)]
pub enum MailEvent {
    Added { cid: i32, mail: Mail },
    Removed { cid: i32, id: i32, result: i32 },
    Written { cid: i32, result: i32 },
    ItemSaved { cid: i32, id: i32, result: i32 },
    RemoveAll { cid: i32 },
}

fn text_value(s: &str, max: usize) -> Value {
    let mut b = crate::guild::to_cp1252(s);
    b.truncate(max);
    Value::Str(b)
}

fn cp1252(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    encoding_rs::WINDOWS_1252.decode(&bytes[..end]).0.into_owned()
}

/// Mini-paquet d'un courrier complet (serveur, essais, op 3 du client).
pub fn encode_mail(m: &Mail) -> Vec<u8> {
    let s = mini::MAIL;
    let mut body = crate::guild::to_cp1252(&m.body);
    body.truncate(BODY_MAX);
    body.push(0);
    build_mini(
        s,
        &sparse(
            s,
            &[
                (0, Value::I32(m.id)),
                (1, text_value(&m.from, 32)),
                (2, text_value(&m.to, 32)),
                (3, text_value(&m.subject, 50)),
                (4, Value::I32(m.date as i32)),
                (5, Value::I32(m.flags)),
                (6, Value::I32(m.item_tid)),
                (7, Value::I16(m.item_count as i16)),
                (8, Value::Block(body)),
            ],
        ),
    )
    .unwrap_or_default()
}

/// Courrier d'un mini-paquet ; `None` sans ID.
pub fn decode_mail(bytes: &[u8]) -> Option<Mail> {
    let (_, v) = decode_mini(mini::MAIL, bytes).ok()?;
    let int = |i: usize| v.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0);
    let text = |i: usize| v.get(i).and_then(Option::as_ref).and_then(Value::as_bytes).map(cp1252).unwrap_or_default();
    let id = v.first().and_then(Option::as_ref).and_then(Value::as_i64)? as i32;
    Some(Mail {
        id,
        from: text(1).trim().to_string(),
        to: text(2).trim().to_string(),
        subject: text(3),
        date: int(4) as u32 as i64,
        flags: int(5) as i32,
        item_tid: int(6) as i32,
        item_count: int(7) as i32,
        body: text(8),
    })
}

/// Mini-paquet réduit à l'ID (`FUN_1402ed310`).
fn id_only(id: i32) -> Value {
    let s = mini::MAIL;
    Value::Packet(build_mini(s, &sparse(s, &[(0, Value::I32(id))])).unwrap_or_default())
}

pub fn command_packet(cmd: &MailCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::MAILBOX;
    let simple = |op: i8, id: i32| sparse(s, &[(0, Value::I8(op)), (1, Value::I32(me)), (2, id_only(id))]);
    match cmd {
        MailCommand::Remove(id) => (s, simple(op::REMOVE, *id)),
        MailCommand::Read(id) => (s, simple(op::READ, *id)),
        MailCommand::SaveItem(id) => (s, simple(op::ITEM_SAVE, *id)),
        MailCommand::Write { from, to, subject, body, item } => {
            let m = Mail { from: from.clone(), to: to.clone(), subject: subject.clone(), body: body.clone(), ..Default::default() };
            (s, sparse(s, &[(0, Value::I8(op::WRITE)), (1, Value::I32(me)), (2, Value::Packet(encode_mail(&m))), (3, Value::I32(*item))]))
        }
    }
}

/// Paquet AgpmMailBox traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<MailEvent> {
    if ptype != packet_type::MAILBOX {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    let mail = values.get(2).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(decode_mail);
    let cid = int(1).unwrap_or(0) as i32;
    let result = int(4).unwrap_or(0) as i32;
    Some(match int(0)? as i8 {
        op::ADD => MailEvent::Added { cid, mail: mail? },
        op::REMOVE => MailEvent::Removed { cid, id: mail?.id, result },
        op::WRITE => MailEvent::Written { cid, result },
        // L'ID est obligatoire (`FUN_14063a050`).
        op::ITEM_SAVE => MailEvent::ItemSaved { cid, id: mail?.id, result },
        op::REMOVE_ALL => MailEvent::RemoveAll { cid },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(main::MAILBOX, v).unwrap();
        assert_eq!(bytes[5], 0x2d);
        decode_packet(main::MAILBOX, &bytes).unwrap().values
    }

    fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
        v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
    }

    #[test]
    fn ecrire_une_lettre() {
        let cmd = MailCommand::Write { from: "Moi".into(), to: "Toi".into(), subject: "Salut".into(), body: "Ça va ?".into(), item: 77 };
        let (s, v) = command_packet(&cmd, 42);
        assert_eq!(s.id, 0x2d);
        let v = roundtrip(&v);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 3), int(&v, 4)), (Some(3), Some(42), Some(77), None));
        let raw = v[2].as_ref().and_then(Value::as_bytes).unwrap();
        // Texte avec son NUL, en CP1252 (« Ç » sur un octet).
        let (_, m) = decode_mini(mini::MAIL, raw).unwrap();
        assert_eq!(m[8].as_ref().and_then(Value::as_bytes), Some(&b"\xc7a va ?\0"[..]));
        let mail = decode_mail(raw).unwrap();
        assert_eq!((mail.id, mail.from.as_str(), mail.to.as_str(), mail.subject.as_str(), mail.body.as_str()), (0, "Moi", "Toi", "Salut", "Ça va ?"));
    }

    #[test]
    fn demandes_sur_un_courrier() {
        for (cmd, op) in [(MailCommand::Remove(9), 1), (MailCommand::Read(9), 2), (MailCommand::SaveItem(9), 4)] {
            let (_, v) = command_packet(&cmd, 42);
            let v = roundtrip(&v);
            assert_eq!(int(&v, 0), Some(op));
            let (flags, m) = decode_mini(mini::MAIL, v[2].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
            // Seul le champ 0 est rempli.
            assert_eq!(flags, 1);
            assert_eq!(int(&m, 0), Some(9));
        }
    }

    #[test]
    fn courriers_recus() {
        let mail = Mail { id: 5, from: "Arthas".into(), to: "Jaina".into(), subject: "Bonjour".into(), date: 1_700_000_000, flags: 0, item_tid: 4963, item_count: 3, body: "Texte".into() };
        let s = main::MAILBOX;
        let add = roundtrip(&sparse(s, &[(0, Value::I8(0)), (1, Value::I32(42)), (2, Value::Packet(encode_mail(&mail)))]));
        assert_eq!(translate(0x2d, &add), Some(MailEvent::Added { cid: 42, mail: mail.clone() }));
        assert!(mail.has_item() && !mail.read());
        let saved = roundtrip(&sparse(s, &[(0, Value::I8(4)), (1, Value::I32(42)), (2, id_only(5)), (4, Value::I32(0))]));
        assert_eq!(translate(0x2d, &saved), Some(MailEvent::ItemSaved { cid: 42, id: 5, result: 0 }));
        // Op 4 sans mini-paquet : refusée comme par Archonia.
        let bad = roundtrip(&sparse(s, &[(0, Value::I8(4)), (1, Value::I32(42)), (4, Value::I32(0))]));
        assert_eq!(translate(0x2d, &bad), None);
        let written = roundtrip(&sparse(s, &[(0, Value::I8(3)), (1, Value::I32(42)), (4, Value::I32(2))]));
        assert_eq!(translate(0x2d, &written), Some(MailEvent::Written { cid: 42, result: 2 }));
        let removed = roundtrip(&sparse(s, &[(0, Value::I8(1)), (1, Value::I32(42)), (2, id_only(5))]));
        assert_eq!(translate(0x2d, &removed), Some(MailEvent::Removed { cid: 42, id: 5, result: 0 }));
        let all = roundtrip(&sparse(s, &[(0, Value::I8(5)), (1, Value::I32(42))]));
        assert_eq!(translate(0x2d, &all), Some(MailEvent::RemoveAll { cid: 42 }));
    }

    #[test]
    fn controles_locaux() {
        assert_eq!(attachment_check(4963, Some(2)), 0);
        assert_eq!(attachment_check(PAPER_TID, Some(2)), 5);
        assert_eq!(attachment_check(4615, Some(2)), 5);
        assert_eq!(attachment_check(4963, Some(1)), 8);
        assert_eq!(reply_subject("Bonjour"), "[RE]Bonjour");
        assert_eq!(reply_subject(&"x".repeat(60)).len(), 4 + 46);
    }
}
