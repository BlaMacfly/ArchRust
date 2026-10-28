//! Invocations (AgpmSummons, 0x4F ; module d'Archonia `FUN_14028f6d0`, réception `FUN_14028fe70`).
//!
//! Drapeaux 2 : opération, CID du maître, CID de la créature, TID, type (ou limite), propension, deux horodatages
//! U64, octet « forcée ». Du serveur : op 0 ajout, op 1 retrait, op 2 propension (renvoyée au maître), op 3 limite
//! des invocations, op 4 limite des apprivoisées. Du client : op 2 seulement (`FUN_1402937c0`, boutons de
//! `UI_TargetSummonsMenu2`), octet « forcée » toujours posé à 0.
//!
//! Registre du client ([`Registry`], attache de 0x6F0 octets par personnage d'Archonia, `FUN_14028fb40`) : trois
//! listes de 10 entrées par maître — invocations (types 1, 8 et 0x20, limite 1 par défaut), apprivoisées (type 2,
//! limite 10), fixes (type 4, limite 10). L'op 0 n'est retenue que si le maître et la créature sont déjà connus et que
//! la créature porte l'état spécial de son type (0x800 pour 1 et 8, 0x1000 pour 2, 0xC00 pour 4 ; aucun pour 0x20,
//! `FUN_140291390`). Sans « forcée », l'ajout est refusé quand la liste est pleine ; un familier (type 8) ou une
//! invocation « forcée » prend un emplacement de plus (limite + 1, rendue au retrait).

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};
use std::collections::HashMap;

pub mod op {
    pub const ADD: i8 = 0;
    pub const REMOVE: i8 = 1;
    pub const SET_PROPENSITY: i8 = 2;
    pub const SET_MAX_SUMMONS: i8 = 3;
    pub const SET_MAX_TAME: i8 = 4;
}

/// Types d'invocation (champ 4 de l'op 0).
pub mod kind {
    /// Invocation ordinaire (esprits, Sacred One, Legion of the Dead, parchemins).
    pub const SUMMONS: i8 = 1;
    /// Créature apprivoisée.
    pub const TAME: i8 = 2;
    /// Créature fixe (pièges, Holy Ground).
    pub const FIXED: i8 = 4;
    /// Familier d'un objet : non ciblable (`FUN_140293280`), hors limite.
    pub const PET: i8 = 8;
    /// Invocation de monstre (sans état spécial exigé).
    pub const MONSTER: i8 = 0x20;
}

/// Propensions (champ 5).
pub mod propensity {
    pub const ATTACK: i8 = 0;
    pub const DEFENSE: i8 = 1;
    pub const SHADOW: i8 = 2;
}

/// Bits de l'état spécial d'un personnage (AGPMCHARACTER champ 17, u64 `+0xb80` d'Archonia).
pub mod status {
    pub const DONT_MOVE: u64 = 0x400;
    pub const SUMMONER: u64 = 0x800;
    pub const TAME: u64 = 0x1000;
}

/// Listes de l'attache d'un maître.
pub const LIST_SUMMONS: usize = 0;
pub const LIST_TAME: usize = 1;
pub const LIST_FIXED: usize = 2;
/// Entrées par liste.
pub const LIST_LEN: usize = 10;
/// Limites par défaut des trois listes.
pub const DEFAULT_MAX: [i8; 3] = [1, 10, 10];

/// Invocation annoncée par l'op 0.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summon {
    pub owner: i32,
    pub cid: i32,
    pub tid: i32,
    pub kind: i8,
    pub propensity: i8,
    pub time_a: u64,
    pub time_b: u64,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SummonsEvent {
    Add(Summon),
    Remove { owner: i32, cid: i32 },
    Propensity { owner: i32, cid: i32, propensity: i8 },
    MaxSummons { owner: i32, max: i8 },
    MaxTame { owner: i32, max: i8 },
    /// État spécial d'un personnage (AGPMCHARACTER champ 17), dont dépend l'op 0.
    SpecialStatus { cid: i32, status: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SummonsCommand {
    /// Propension d'une invocation du joueur (`FUN_1404ff360`).
    SetPropensity { cid: i32, propensity: i8 },
}

pub fn command_packet(cmd: &SummonsCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::SUMMONS;
    match cmd {
        SummonsCommand::SetPropensity { cid, propensity } => (
            s,
            sparse(s, &[(0, Value::I8(op::SET_PROPENSITY)), (1, Value::I32(me)), (2, Value::I32(*cid)), (5, Value::I8(*propensity)), (8, Value::U8(0))]),
        ),
    }
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<SummonsEvent> {
    if ptype != packet_type::SUMMONS {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0);
    let u64_at = |i: usize| match values.get(i).and_then(Option::as_ref) {
        Some(Value::U64(v)) => *v,
        Some(v) => v.as_i64().unwrap_or(0) as u64,
        None => 0,
    };
    let (owner, cid) = (int(1) as i32, int(2) as i32);
    let opcode = values.first().and_then(Option::as_ref).and_then(Value::as_i64)? as i8;
    Some(match opcode {
        op::ADD => SummonsEvent::Add(Summon {
            owner,
            cid,
            tid: int(3) as i32,
            kind: int(4) as i8,
            propensity: int(5) as i8,
            time_a: u64_at(6),
            time_b: u64_at(7),
            force: int(8) != 0,
        }),
        op::REMOVE => SummonsEvent::Remove { owner, cid },
        op::SET_PROPENSITY => SummonsEvent::Propensity { owner, cid, propensity: int(5) as i8 },
        op::SET_MAX_SUMMONS => SummonsEvent::MaxSummons { owner, max: int(4) as i8 },
        op::SET_MAX_TAME => SummonsEvent::MaxTame { owner, max: int(4) as i8 },
        _ => return None,
    })
}

/// État spécial porté par un paquet AGPMCHARACTER (ops 0 et 1, champ 17).
pub fn special_status_of(ptype: u8, values: &[Option<Value>]) -> Option<SummonsEvent> {
    if ptype != packet_type::CHARACTER {
        return None;
    }
    let field = |i: usize| values.get(i).and_then(Option::as_ref);
    if !matches!(field(0).and_then(Value::as_i64), Some(0 | 1)) {
        return None;
    }
    let cid = field(1).and_then(Value::as_i64)? as i32;
    let status = match field(17)? {
        Value::U64(v) => *v,
        v => v.as_i64()? as u64,
    };
    Some(SummonsEvent::SpecialStatus { cid, status })
}

/// Liste d'un type d'invocation (`0x100000102 >> type` : 1, 8 et 0x20 dans la liste 0) et état spécial exigé.
pub fn list_of(k: i8) -> Option<(usize, u64)> {
    match k {
        kind::SUMMONS | kind::PET => Some((LIST_SUMMONS, status::SUMMONER)),
        kind::MONSTER => Some((LIST_SUMMONS, 0)),
        kind::TAME => Some((LIST_TAME, status::TAME)),
        kind::FIXED => Some((LIST_FIXED, status::DONT_MOVE | status::SUMMONER)),
        _ => None,
    }
}

/// Entrée d'une liste de maître (0x38 octets d'Archonia).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub cid: i32,
    pub tid: i32,
    pub kind: i8,
    pub propensity: i8,
    pub time_a: u64,
    pub time_b: u64,
    /// Emplacement « forcé » (familier ou invocation forcée) : la limite a été relevée d'un cran.
    pub forced: bool,
}

/// Attache d'un maître : limites et listes.
#[derive(Debug, Clone, PartialEq)]
pub struct Attach {
    pub max: [i8; 3],
    pub lists: [Vec<Entry>; 3],
}

impl Default for Attach {
    fn default() -> Self {
        Self { max: DEFAULT_MAX, lists: Default::default() }
    }
}

/// Ce que l'événement a changé (rappels d'AgpmSummons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Rappel 0 : entrée ajoutée.
    Added,
    /// Rappel 1 : entrée retirée ; l'état spécial « invoqué » de la créature est à effacer.
    Removed,
    /// Rappel 2 : propension changée (rafraîchit le menu de la cible).
    Propensity,
    /// Rappels 3 et 4 : limite changée.
    Limit,
    /// Refus de l'op 0 (liste pleine, créature inconnue ou sans l'état exigé, doublon).
    Refused,
    /// Sans effet.
    Ignored,
}

/// Invocations connues du client, par maître, et maître de chaque créature (attache `+0x6a8`).
#[derive(Debug, Default)]
pub struct Registry {
    pub attach: HashMap<i32, Attach>,
    /// Créature → son entrée et son maître.
    pub creatures: HashMap<i32, (i32, Entry)>,
}

impl Registry {
    /// Applique un événement reçu du serveur. `me` : CID du joueur ; `known` : personnage en vue (ou le joueur) ;
    /// `status_of` : état spécial connu d'un personnage.
    pub fn apply(&mut self, ev: &SummonsEvent, me: i32, known: impl Fn(i32) -> bool, status_of: impl Fn(i32) -> u64) -> Applied {
        match ev {
            SummonsEvent::Add(s) => self.add(s, &known, &status_of),
            SummonsEvent::Remove { owner, cid } => self.remove(*owner, *cid),
            // Op 2 : seulement pour le joueur, une invocation de type 1 de sa liste 0.
            SummonsEvent::Propensity { owner, cid, propensity } => {
                if *owner != me || *propensity < 0 {
                    return Applied::Ignored;
                }
                let Some(e) = self.attach.get_mut(owner).and_then(|a| a.lists[LIST_SUMMONS].iter_mut().find(|e| e.cid == *cid)) else {
                    return Applied::Ignored;
                };
                if e.kind != kind::SUMMONS {
                    return Applied::Ignored;
                }
                e.propensity = *propensity;
                if let Some((_, c)) = self.creatures.get_mut(cid) {
                    c.propensity = *propensity;
                }
                Applied::Propensity
            }
            SummonsEvent::MaxSummons { owner, max } | SummonsEvent::MaxTame { owner, max } => {
                if *max < 0 || *owner <= 0 {
                    return Applied::Ignored;
                }
                let list = if matches!(ev, SummonsEvent::MaxSummons { .. }) { LIST_SUMMONS } else { LIST_TAME };
                self.attach.entry(*owner).or_default().max[list] = *max;
                Applied::Limit
            }
            SummonsEvent::SpecialStatus { .. } => Applied::Ignored,
        }
    }

    fn add(&mut self, s: &Summon, known: &impl Fn(i32) -> bool, status_of: &impl Fn(i32) -> u64) -> Applied {
        if s.owner < 1 || s.cid < 1 || s.tid < 1 || s.kind < 0 || s.propensity < 0 || s.time_a == 0 || s.time_b == 0 {
            return Applied::Ignored;
        }
        let Some((list, needed)) = list_of(s.kind) else { return Applied::Ignored };
        if !known(s.owner) || !known(s.cid) || status_of(s.cid) & needed != needed {
            return Applied::Refused;
        }
        let a = self.attach.entry(s.owner).or_default();
        let entries = &a.lists[list];
        if (!s.force && entries.len() as i32 >= a.max[list] as i32) || entries.iter().any(|e| e.cid == s.cid) || entries.len() >= LIST_LEN {
            return Applied::Refused;
        }
        // Familier ou invocation forcée (liste 0) : un emplacement de plus.
        let forced = list == LIST_SUMMONS && (s.kind == kind::PET || s.force);
        if forced {
            a.max[list] = a.max[list].saturating_add(1);
        }
        let e = Entry { cid: s.cid, tid: s.tid, kind: s.kind, propensity: s.propensity, time_a: s.time_a, time_b: s.time_b, forced };
        a.lists[list].push(e.clone());
        self.creatures.insert(s.cid, (s.owner, e));
        Applied::Added
    }

    fn remove(&mut self, owner: i32, cid: i32) -> Applied {
        let had = self.creatures.remove(&cid).is_some();
        let mut found = false;
        if let Some(a) = self.attach.get_mut(&owner) {
            for list in 0..3 {
                if let Some(i) = a.lists[list].iter().position(|e| e.cid == cid) {
                    let e = a.lists[list].remove(i);
                    if e.forced {
                        a.max[list] = a.max[list].saturating_sub(1).max(0);
                    }
                    found = true;
                }
            }
        }
        if had || found {
            Applied::Removed
        } else {
            Applied::Ignored
        }
    }

    /// Personnage retiré du monde (rappel n° 4 d'AgpmCharacter) : son attache de maître et de créature s'efface.
    pub fn forget(&mut self, cid: i32) {
        self.attach.remove(&cid);
        self.creatures.remove(&cid);
    }

    /// La créature `cid` figure dans la liste des invocations de `owner` (`FUN_140292810`).
    pub fn is_summons_of(&self, owner: i32, cid: i32) -> bool {
        self.attach.get(&owner).is_some_and(|a| a.lists[LIST_SUMMONS].iter().any(|e| e.cid == cid))
    }

    /// Entrée de `cid` dans la liste des invocations de `owner`.
    pub fn entry(&self, owner: i32, cid: i32) -> Option<&Entry> {
        self.attach.get(&owner)?.lists[LIST_SUMMONS].iter().find(|e| e.cid == cid)
    }

    /// Familier (type 8, `FUN_140293280`) : écarté de la sélection.
    pub fn is_pet(&self, cid: i32) -> bool {
        self.creatures.get(&cid).is_some_and(|(_, e)| e.kind == kind::PET)
    }

    /// Maître d'une créature invoquée (`FUN_1402931e0`).
    pub fn owner_of(&self, cid: i32) -> Option<i32> {
        self.creatures.get(&cid).map(|(o, _)| *o)
    }

    /// Limite et nombre d'entrées d'une liste d'un maître.
    pub fn usage(&self, owner: i32, list: usize) -> (i8, usize) {
        self.attach.get(&owner).map_or((DEFAULT_MAX[list], 0), |a| (a.max[list], a.lists[list].len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round_trip(values: Vec<Option<Value>>) -> Vec<Option<Value>> {
        let s = main::SUMMONS;
        decode_packet(s, &build_packet(s, &values).unwrap()).unwrap().values
    }

    fn add_packet(owner: i32, cid: i32, tid: i32, k: i8, force: u8) -> Vec<Option<Value>> {
        let s = main::SUMMONS;
        sparse(s, &[
            (0, Value::I8(op::ADD)), (1, Value::I32(owner)), (2, Value::I32(cid)), (3, Value::I32(tid)), (4, Value::I8(k)),
            (5, Value::I8(0)), (6, Value::U64(1_800_000_000_000)), (7, Value::U64(1_800_000_600_000)), (8, Value::U8(force)),
        ])
    }

    #[test]
    fn schema_et_commande() {
        let s = main::SUMMONS;
        assert_eq!((s.id, s.flag_len, s.fields.len()), (0x4f, 2, 9));
        let (s2, v) = command_packet(&SummonsCommand::SetPropensity { cid: 0x6000_0001, propensity: propensity::DEFENSE }, 7);
        assert_eq!(s2.id, packet_type::SUMMONS);
        let bytes = build_packet(s2, &v).unwrap();
        // Type à l'octet 5, puis drapeaux u16 après l'en-tête (19 octets) : bits 0, 1, 2, 5 et 8.
        assert_eq!(bytes[5], 0x4f);
        assert_eq!(u16::from_le_bytes([bytes[19], bytes[20]]), 0b1_0010_0111);
        let v = round_trip(v);
        let int = |i: usize| v[i].as_ref().and_then(Value::as_i64);
        assert_eq!((int(0), int(1), int(2), int(5), int(8)), (Some(2), Some(7), Some(0x6000_0001), Some(1), Some(0)));
        assert!(v[3].is_none() && v[4].is_none() && v[6].is_none());
    }

    #[test]
    fn decodage_des_operations() {
        let v = round_trip(add_packet(7, 0x6000_0001, 543, kind::SUMMONS, 0));
        assert_eq!(
            translate(packet_type::SUMMONS, &v),
            Some(SummonsEvent::Add(Summon { owner: 7, cid: 0x6000_0001, tid: 543, kind: 1, propensity: 0, time_a: 1_800_000_000_000, time_b: 1_800_000_600_000, force: false }))
        );
        let s = main::SUMMONS;
        let v = round_trip(sparse(s, &[(0, Value::I8(1)), (1, Value::I32(7)), (2, Value::I32(9)), (8, Value::U8(0))]));
        assert_eq!(translate(packet_type::SUMMONS, &v), Some(SummonsEvent::Remove { owner: 7, cid: 9 }));
        let v = round_trip(sparse(s, &[(0, Value::I8(2)), (1, Value::I32(7)), (2, Value::I32(9)), (5, Value::I8(2)), (8, Value::U8(0))]));
        assert_eq!(translate(packet_type::SUMMONS, &v), Some(SummonsEvent::Propensity { owner: 7, cid: 9, propensity: 2 }));
        let v = round_trip(sparse(s, &[(0, Value::I8(3)), (1, Value::I32(7)), (4, Value::I8(3)), (8, Value::U8(0))]));
        assert_eq!(translate(packet_type::SUMMONS, &v), Some(SummonsEvent::MaxSummons { owner: 7, max: 3 }));
        assert_eq!(translate(packet_type::PVP, &v), None);
        // État spécial d'AGPMCHARACTER op 1.
        let c = main::CHARACTER;
        let st = decode_packet(c, &build_packet(c, &sparse(c, &[(0, Value::I8(1)), (1, Value::I32(9)), (17, Value::U64(0xC00))])).unwrap()).unwrap().values;
        assert_eq!(special_status_of(packet_type::CHARACTER, &st), Some(SummonsEvent::SpecialStatus { cid: 9, status: 0xC00 }));
    }

    #[test]
    fn regles_du_registre() {
        let me = 7;
        let mut r = Registry::default();
        let statuses: HashMap<i32, u64> = [(101, 0x800), (102, 0x800), (103, 0x800), (104, 0xC00), (105, 0x400)].into();
        let known = |c: i32| c == me || statuses.contains_key(&c);
        let st = |c: i32| statuses.get(&c).copied().unwrap_or(0);
        let add = |cid, k, force| SummonsEvent::Add(Summon { owner: me, cid, tid: 543, kind: k, propensity: 0, time_a: 1, time_b: 2, force });
        // Première invocation acceptée, la deuxième refusée (limite 1).
        assert_eq!(r.apply(&add(101, kind::SUMMONS, false), me, known, st), Applied::Added);
        assert_eq!(r.apply(&add(102, kind::SUMMONS, false), me, known, st), Applied::Refused);
        // Le familier est hors limite seulement s'il est « forcé » ; il relève alors la limite.
        assert_eq!(r.apply(&add(102, kind::PET, false), me, known, st), Applied::Refused);
        assert_eq!(r.apply(&add(102, kind::PET, true), me, known, st), Applied::Added);
        assert_eq!(r.usage(me, LIST_SUMMONS), (2, 2));
        assert!(r.is_pet(102) && !r.is_pet(101));
        // Créature inconnue ou sans l'état exigé : refusée.
        assert_eq!(r.apply(&add(999, kind::SUMMONS, true), me, known, st), Applied::Refused);
        assert_eq!(r.apply(&add(105, kind::FIXED, false), me, known, st), Applied::Refused);
        assert_eq!(r.apply(&add(104, kind::FIXED, false), me, known, st), Applied::Added);
        // Op 3 : limite relevée.
        assert_eq!(r.apply(&SummonsEvent::MaxSummons { owner: me, max: 3 }, me, known, st), Applied::Limit);
        assert_eq!(r.apply(&add(103, kind::SUMMONS, false), me, known, st), Applied::Added);
        // Propension : type 1 du joueur seulement.
        assert_eq!(r.apply(&SummonsEvent::Propensity { owner: me, cid: 101, propensity: 2 }, me, known, st), Applied::Propensity);
        assert_eq!(r.entry(me, 101).map(|e| e.propensity), Some(2));
        assert_eq!(r.apply(&SummonsEvent::Propensity { owner: me, cid: 102, propensity: 1 }, me, known, st), Applied::Ignored);
        assert_eq!(r.apply(&SummonsEvent::Propensity { owner: 8, cid: 101, propensity: 1 }, me, known, st), Applied::Ignored);
        // Retrait du familier : l'emplacement forcé est rendu.
        assert_eq!(r.apply(&SummonsEvent::Remove { owner: me, cid: 102 }, me, known, st), Applied::Removed);
        assert_eq!(r.usage(me, LIST_SUMMONS), (2, 2));
        assert!(r.is_summons_of(me, 101) && !r.is_summons_of(me, 102) && !r.is_summons_of(me, 104));
        assert_eq!(r.owner_of(104), Some(me));
    }
}
