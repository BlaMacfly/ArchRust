//! Fenêtre des boss d'Archonia (AgpmBoss 0x64, constructeur `FUN_1402ed3e0`, réception `FUN_1402eda60`, envoi
//! `FUN_1402ef090` / `FUN_1402ef150`). Drapeaux 1 octet partout (voir `schema::main::BOSS` et les minis
//! `BOSS_*`).
//!
//! * op 1 (S→C, champs 1, 2) : boss suivis, un mini A (`stAgpmBossSpawnData` brut de 48 o) par boss ;
//! * op 3 (S→C, champ 3) : butin d'un boss (mini B brut de 196 o, grille 4 × 4) ;
//! * op 4 (C→S, champ 4) : boss choisi dans la liste (`Boss_Tab_Click`) ;
//! * op 5 (S→C, champ 5) : classement (mini C : page, pages, rang / nom / victoires du joueur, 16 lignes D) ;
//! * op 6 (C→S, champs 4, 5) : page du classement (mini C, champ 0 seul) ;
//! * op 7 (S→C, champ 6) : compteur de dégâts (mini F : boss, nombre, 20 lignes E).

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

pub mod op {
    pub const LIST: u8 = 1;
    pub const DROPS: u8 = 3;
    pub const SELECT: u8 = 4;
    pub const RANKING: u8 = 5;
    pub const RANK_PAGE: u8 = 6;
    pub const DAMAGE: u8 = 7;
}

/// Boss suivis au plus (champ 2), entrées de butin, lignes de classement et du compteur de dégâts.
pub const MAX_BOSSES: usize = 32;
pub const DROP_SLOTS: usize = 16;
pub const RANK_ROWS: usize = 16;
pub const DAMAGE_ROWS: usize = 20;

/// Mini A : état d'apparition d'un boss.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BossSpawn {
    pub tid: i32,
    /// Prochaine apparition, secondes Unix (base d'AgpmTimer champ 2) ; 0 = inconnue.
    pub next: u64,
    /// Heure et minute de la prochaine apparition, ou de la dernière mort si elle est inconnue.
    pub hour: i8,
    pub minute: i8,
    pub alive: bool,
}

impl BossSpawn {
    pub fn decode(b: &[u8]) -> Option<BossSpawn> {
        if b.len() < 48 {
            return None;
        }
        Some(BossSpawn {
            tid: i32::from_le_bytes(b[0..4].try_into().unwrap()),
            next: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            hour: b[0x20] as i8,
            minute: b[0x21] as i8,
            alive: i32::from_le_bytes(b[0x28..0x2C].try_into().unwrap()) != 0,
        })
    }

    pub fn encode(&self) -> [u8; 48] {
        let mut b = [0u8; 48];
        b[0..4].copy_from_slice(&self.tid.to_le_bytes());
        b[0x18..0x20].copy_from_slice(&self.next.to_le_bytes());
        b[0x20] = self.hour as u8;
        b[0x21] = self.minute as u8;
        b[0x28..0x2C].copy_from_slice(&(self.alive as i32).to_le_bytes());
        b
    }
}

/// Mini B : butin affiché d'un boss (TID d'objet, quantité), cases vides retirées.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BossDrops {
    pub boss: i32,
    pub items: Vec<(i32, i32)>,
}

/// Ligne du classement (mini D).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RankRow {
    pub rank: i8,
    pub name: String,
    pub kills: i32,
}

/// Mini C : classement d'un boss.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BossRanking {
    pub page: i8,
    pub pages: i8,
    pub my_rank: i8,
    pub my_name: String,
    pub my_kills: i32,
    pub rows: Vec<RankRow>,
}

/// Ligne du compteur de dégâts (mini E).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DamageRow {
    pub name: String,
    /// TID du modèle de personnage (icône de classe).
    pub tid: i32,
    pub level: i32,
    pub guild: String,
    /// Marque de guilde (0 : aucune) et entier suivant (couleur de la marque ?).
    pub mark: i32,
    pub mark_color: i32,
    pub damage: i64,
}

/// Mini F : compteur de dégâts d'un combat de boss.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BossDamage {
    pub boss: i32,
    pub rows: Vec<DamageRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BossEvent {
    List(Vec<BossSpawn>),
    Drops(BossDrops),
    Ranking(BossRanking),
    Damage(BossDamage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BossCommand {
    /// Op 4 : boss choisi (butin et classement en retour).
    Select(i32),
    /// Op 6 : page du classement d'un boss.
    RankPage { tid: i32, page: i8 },
}

fn int(v: &[Option<Value>], i: usize) -> i64 {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0)
}

fn text(v: &[Option<Value>], i: usize) -> String {
    let b = v.get(i).and_then(Option::as_ref).and_then(Value::as_bytes).unwrap_or_default();
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    encoding_rs::WINDOWS_1252.decode(&b[..end]).0.trim().to_string()
}

fn minis(v: Option<&Value>) -> Vec<Vec<u8>> {
    match v {
        Some(Value::PacketList(l)) => l.clone(),
        Some(Value::Packet(p)) => vec![p.clone()],
        _ => Vec::new(),
    }
}

fn raw_of(schema: &Schema, bytes: &[u8]) -> Option<Vec<u8>> {
    let (_, v) = decode_mini(schema, bytes).ok()?;
    v.first().and_then(Option::as_ref).and_then(Value::as_bytes).map(<[u8]>::to_vec)
}

fn decode_drops(bytes: &[u8]) -> Option<BossDrops> {
    let b = raw_of(mini::BOSS_DROP, bytes)?;
    if b.len() < 196 {
        return None;
    }
    let at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let items = (0..DROP_SLOTS).map(|i| (at(4 + i * 4), at(0x84 + i * 4))).filter(|(t, _)| *t != 0).collect();
    Some(BossDrops { boss: at(0), items })
}

fn decode_ranking(bytes: &[u8]) -> Option<BossRanking> {
    let (_, v) = decode_mini(mini::BOSS_RANK, bytes).ok()?;
    let rows = minis(v.get(5).and_then(Option::as_ref))
        .iter()
        .filter_map(|m| decode_mini(mini::BOSS_RANK_ROW, m).ok())
        .map(|(_, r)| RankRow { rank: int(&r, 0) as i8, name: text(&r, 1), kills: int(&r, 2) as i32 })
        .collect();
    Some(BossRanking { page: int(&v, 0) as i8, pages: int(&v, 1) as i8, my_rank: int(&v, 2) as i8, my_name: text(&v, 3), my_kills: int(&v, 4) as i32, rows })
}

fn decode_damage(bytes: &[u8]) -> Option<BossDamage> {
    let (_, v) = decode_mini(mini::BOSS_DPS, bytes).ok()?;
    let count = (int(&v, 1).max(0) as usize).min(DAMAGE_ROWS);
    let rows = minis(v.get(2).and_then(Option::as_ref))
        .iter()
        .take(count)
        .filter_map(|m| decode_mini(mini::BOSS_DPS_ROW, m).ok())
        .map(|(_, r)| DamageRow { name: text(&r, 0), tid: int(&r, 1) as i32, level: int(&r, 2) as i32, guild: text(&r, 3), mark: int(&r, 4) as i32, mark_color: int(&r, 5) as i32, damage: int(&r, 6) })
        .collect();
    Some(BossDamage { boss: int(&v, 0) as i32, rows })
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<BossEvent> {
    if ptype != packet_type::BOSS {
        return None;
    }
    let field = |i: usize| values.get(i).and_then(Option::as_ref);
    match int(values, 0) as u8 {
        op::LIST => Some(BossEvent::List(minis(field(2)).iter().filter_map(|m| raw_of(mini::BOSS_SPAWN, m)).filter_map(|b| BossSpawn::decode(&b)).collect())),
        op::DROPS => field(3).and_then(Value::as_bytes).and_then(decode_drops).map(BossEvent::Drops),
        op::RANKING => field(5).and_then(Value::as_bytes).and_then(decode_ranking).map(BossEvent::Ranking),
        op::DAMAGE => field(6).and_then(Value::as_bytes).and_then(decode_damage).map(BossEvent::Damage),
        _ => None,
    }
}

pub fn command_packet(cmd: &BossCommand) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::BOSS;
    match cmd {
        BossCommand::Select(tid) => (s, sparse(s, &[(0, Value::U8(op::SELECT)), (4, Value::I32(*tid))])),
        BossCommand::RankPage { tid, page } => {
            let c = mini::BOSS_RANK;
            let rank = Value::Packet(build_mini(c, &sparse(c, &[(0, Value::I8(*page))])).unwrap_or_default());
            (s, sparse(s, &[(0, Value::U8(op::RANK_PAGE)), (4, Value::I32(*tid)), (5, rank)]))
        }
    }
}

/// Paquets du serveur (essais, outils).
pub mod server {
    use super::*;

    fn raw_mini(schema: &Schema, bytes: &[u8]) -> Vec<u8> {
        build_mini(schema, &sparse(schema, &[(0, Value::Str(bytes.to_vec()))])).unwrap_or_default()
    }

    /// Op 1 : boss suivis (32 au plus).
    pub fn list_packet(spawns: &[BossSpawn]) -> (&'static Schema, Vec<Option<Value>>) {
        let s = main::BOSS;
        let list = spawns.iter().take(MAX_BOSSES).map(|b| raw_mini(mini::BOSS_SPAWN, &b.encode())).collect();
        (s, sparse(s, &[(0, Value::U8(op::LIST)), (1, Value::I8(0)), (2, Value::PacketList(list))]))
    }

    /// Op 3 : butin (16 objets au plus).
    pub fn drops_packet(d: &BossDrops) -> (&'static Schema, Vec<Option<Value>>) {
        let mut b = vec![0u8; 196];
        b[0..4].copy_from_slice(&d.boss.to_le_bytes());
        for (i, (tid, count)) in d.items.iter().take(DROP_SLOTS).enumerate() {
            b[4 + i * 4..8 + i * 4].copy_from_slice(&tid.to_le_bytes());
            b[0x84 + i * 4..0x88 + i * 4].copy_from_slice(&count.to_le_bytes());
        }
        let s = main::BOSS;
        (s, sparse(s, &[(0, Value::U8(op::DROPS)), (3, Value::Packet(raw_mini(mini::BOSS_DROP, &b)))]))
    }

    /// Op 5 : classement (16 lignes au plus).
    pub fn ranking_packet(r: &BossRanking) -> (&'static Schema, Vec<Option<Value>>) {
        let d = mini::BOSS_RANK_ROW;
        let rows = r.rows.iter().take(RANK_ROWS).map(|x| build_mini(d, &sparse(d, &[(0, Value::I8(x.rank)), (1, Value::str(&x.name)), (2, Value::I32(x.kills))])).unwrap_or_default()).collect();
        let c = mini::BOSS_RANK;
        let m = build_mini(c, &sparse(c, &[(0, Value::I8(r.page)), (1, Value::I8(r.pages)), (2, Value::I8(r.my_rank)), (3, Value::str(&r.my_name)), (4, Value::I32(r.my_kills)), (5, Value::PacketList(rows))])).unwrap_or_default();
        let s = main::BOSS;
        (s, sparse(s, &[(0, Value::U8(op::RANKING)), (5, Value::Packet(m))]))
    }

    /// Op 7 : compteur de dégâts (20 lignes au plus).
    pub fn damage_packet(d: &BossDamage) -> (&'static Schema, Vec<Option<Value>>) {
        let e = mini::BOSS_DPS_ROW;
        let rows: Vec<Vec<u8>> = d
            .rows
            .iter()
            .take(DAMAGE_ROWS)
            .map(|x| build_mini(e, &sparse(e, &[(0, Value::str(&x.name)), (1, Value::I32(x.tid)), (2, Value::I32(x.level)), (3, Value::str(&x.guild)), (4, Value::I32(x.mark)), (5, Value::I32(x.mark_color)), (6, Value::I64(x.damage))])).unwrap_or_default())
            .collect();
        let f = mini::BOSS_DPS;
        let m = build_mini(f, &sparse(f, &[(0, Value::I32(d.boss)), (1, Value::I8(rows.len() as i8)), (2, Value::PacketList(rows))])).unwrap_or_default();
        let s = main::BOSS;
        (s, sparse(s, &[(0, Value::U8(op::DAMAGE)), (6, Value::Packet(m))]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round_trip((schema, values): (&'static Schema, Vec<Option<Value>>)) -> Option<BossEvent> {
        let bytes = build_packet(schema, &values).unwrap();
        assert_eq!(bytes[5], 0x64);
        let p = decode_packet(main::by_id(0x64).unwrap(), &bytes).unwrap();
        translate(0x64, &p.values)
    }

    #[test]
    fn liste_des_boss() {
        let spawns = vec![
            BossSpawn { tid: 2073, next: 1_793_000_000, hour: 14, minute: 5, alive: false },
            BossSpawn { tid: 2067, next: 0, hour: 9, minute: 30, alive: true },
        ];
        assert_eq!(round_trip(server::list_packet(&spawns)), Some(BossEvent::List(spawns)));
        assert_eq!(round_trip(server::list_packet(&[])), Some(BossEvent::List(vec![])));
    }

    #[test]
    fn butin_classement_degats() {
        let d = BossDrops { boss: 2073, items: vec![(4845, 1), (60240, 3)] };
        assert_eq!(round_trip(server::drops_packet(&d)), Some(BossEvent::Drops(d)));
        let r = BossRanking { page: 1, pages: 3, my_rank: 7, my_name: "Moi".into(), my_kills: 2, rows: vec![RankRow { rank: 1, name: "SimA".into(), kills: 9 }, RankRow { rank: 2, name: "SimB".into(), kills: 4 }] };
        assert_eq!(round_trip(server::ranking_packet(&r)), Some(BossEvent::Ranking(r)));
        let m = BossDamage { boss: 2073, rows: vec![DamageRow { name: "SimA".into(), tid: 96, level: 30, guild: "Gardiens".into(), mark: 0, mark_color: 0, damage: 1_500_000 }] };
        assert_eq!(round_trip(server::damage_packet(&m)), Some(BossEvent::Damage(m)));
    }

    #[test]
    fn demandes_du_client() {
        let (s, v) = command_packet(&BossCommand::Select(2073));
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        assert_eq!((p.op(), p.get(4).and_then(Value::as_i64), p.get(5).is_none()), (Some(4), Some(2073), true));
        let (s, v) = command_packet(&BossCommand::RankPage { tid: 2073, page: 2 });
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        assert_eq!((p.op(), p.get(4).and_then(Value::as_i64)), (Some(6), Some(2073)));
        let (_, c) = decode_mini(mini::BOSS_RANK, p.get(5).and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!((int(&c, 0), c[1].is_none()), (2, true));
        // Ops sans abonné côté client (2, 4 reçue, 6 reçue) : ignorées.
        assert_eq!(translate(0x64, &sparse(main::BOSS, &[(0, Value::U8(2))])), None);
    }
}
