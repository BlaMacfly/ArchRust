//! Sons des personnages d'Archonia (`AgcmEventEffect`, Archonia.exe sha256 e9c7609b…) : sons
//! d'animation, pas selon le sol, sons des compétences ; lecture 2D pour le joueur, 3D pour les
//! autres (voir `audio.rs`, « Sons 3D »).
//!
//! * **Sons d'animation** — `charactertemplateeventeffect.ini`, colonnes
//!   `AGCMEVENTEFFECT_CAAD_SOUND_DATA…` = `type:équipement:variante:?:fichier` (`FUN_1405752e0`,
//!   `sscanf "%d:%d:%d:%d:%s"`) : le son est accroché à la donnée d'animation `[type][équipement]`
//!   de rang `variante` (celle de `charactertemplateanimation.ini`), plusieurs par animation, tous
//!   joués. `…_CAAD_SOUND_CONDITION_FLAGS…` (`%d:%d:%d:%d:%d`) pose l'octet +0x30 du **dernier** son
//!   ajouté à cette animation : 0 = joué au début de l'animation (`FUN_1405695d0`, types 0 à 15),
//!   1 = joué quand le coup de l'attaque (type 3) touche (`FUN_14056d890`, appelée par le rappel du
//!   coup reçu `FUN_140573960`). Le début d'animation suit chaque nouveau cycle d'un clip en boucle
//!   (déduit du modèle de rappels d'AgcmCharacter, non relevé).
//! * **Pas** — `INI\EE_MOVE_SOUND.TXT` (`FUN_140830090`) : table par TID (ligne `TID LEFT RIGHT nom`,
//!   0 = défaut, puis une ligne par sol `SOIL`…`GRANITE`, `FUN_140831000`), sons du pied gauche et du
//!   pied droit. Au début d'une marche ou d'une course (`FUN_140569b10`, `FUN_140569840` →
//!   `FUN_14056d630`), **pour le joueur seulement**, deux sons sont programmés sur le clip : gauche à
//!   0,25 s, droit à `durée × 0,5 + 0,25` s (`DAT_140e49c40`, `DAT_140e49bfc` ; la durée est
//!   `AgcdCharacter+0x124`, déduit), en boucle avec le clip. Sol : octet 0 de la cellule `.amf`
//!   décalé d'un bit (`zones::cell_ground`) au moment du départ. Table : TID de la monture si le
//!   joueur est monté (`FUN_14017c940`), sinon 0 ; un TID absent du fichier ne donne aucun son.
//!   Joué en 2D au volume 1,1.
//! * **Compétences** — `charactertemplateskillsound.ini`, `SKILL_EFFECT_SOUND…` =
//!   `rang:équipement:n°:fichier` (`FUN_140559c20`, rang < 200) : le son est accroché à la donnée
//!   d'effet n° `n°` des `SKILL_EFFECT_DATA` du même rang et du même ensemble
//!   (`charactertemplateskill.ini`), le premier gagnant. Joué avec cet effet, à son délai (`GAP`,
//!   déduit), sur le lanceur.
//! * **Actions sociales** — colonne SOUND de `SOCIAL_PC.txt` (`FUN_14056da90`, déjà lue par
//!   `app_social.rs`) : 2D pour le joueur, 3D sinon. `socialsound.ini` n'est cité nulle part dans
//!   Archonia.exe : il n'est pas lu.
//!
//! Joueur ou option sans 3D : 2D au volume 1,1 ; autres personnages : 3D au volume 1,0 sur leur
//! position, suivie (`FUN_1405c7420`). Les animations des montures (animateurs de `app_ride.rs`) ne
//! jouent pas leurs sons d'animation ici.

use super::App;
use crate::animation::{AnimType, Clip};
use crate::assets::Assets;
use crate::audio::{Listener, CHARACTER_2D_VOLUME};
use archrust_data::IniTable;
use archrust_netclient::skills::SkillEvent;
use archrust_netclient::WorldEvent;
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Moment d'un son d'animation (octet +0x30) : début de l'animation.
pub const COND_START: u8 = 0;
/// Moment d'un son d'animation : coup de l'attaque qui touche.
pub const COND_HIT: u8 = 1;
/// Nombre de types de sol d'`ee_move_sound.txt`.
pub const GROUNDS: usize = 17;
/// Temps du pas gauche dans le clip (s) et facteur de la durée du pas droit (`DAT_140e49bfc`,
/// `DAT_140e49c40`).
pub const STEP_LEFT_AT: f32 = 0.25;
pub const STEP_RIGHT_FACTOR: f32 = 0.5;

/// Son accroché à une animation (`CAAD_SOUND_DATA`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimSound {
    pub kind: u32,
    pub equip: u32,
    pub variant: u32,
    pub cond: u8,
    pub file: String,
}

/// Sons d'animation par TID (`charactertemplateeventeffect.ini`).
pub fn parse_anim_sounds(table: &IniTable) -> HashMap<i32, Vec<AnimSound>> {
    let mut out = HashMap::new();
    let Some(header) = table.section("") else { return out };
    let name_of = |k: &str| header.get(k).unwrap_or("");
    for sec in &table.sections {
        let Ok(tid) = sec.name.trim().parse::<i32>() else { continue };
        let mut list: Vec<AnimSound> = Vec::new();
        for (k, v) in &sec.entries {
            let col = name_of(k);
            let f: Vec<&str> = v.split(':').map(str::trim).collect();
            let num = |i: usize| f.get(i).and_then(|x| x.parse::<i64>().ok());
            let (Some(kind), Some(equip), Some(variant)) = (num(0), num(1), num(2)) else { continue };
            if kind < 0 || equip < 0 || variant < 0 {
                continue;
            }
            let (kind, equip, variant) = (kind as u32, equip as u32, variant as u32);
            if col.starts_with("AGCMEVENTEFFECT_CAAD_SOUND_DATA") {
                let file = f.get(4).copied().unwrap_or("");
                if !file.is_empty() && file != "0" {
                    list.push(AnimSound { kind, equip, variant, cond: COND_START, file: file.to_string() });
                }
            } else if col.starts_with("AGCMEVENTEFFECT_CAAD_SOUND_CONDITION_FLAGS") {
                // Dernier son de cette animation (`*(liste + 8)`, queue de la liste).
                if let Some(s) = list.iter_mut().rev().find(|s| s.kind == kind && s.equip == equip && s.variant == variant) {
                    s.cond = num(4).unwrap_or(0) as u8;
                }
            }
        }
        if !list.is_empty() {
            out.insert(tid, list);
        }
    }
    out
}

/// Clips d'un TID (`charactertemplateanimation.ini`, `ANIMATION_NAME` = `type:équipement:variante:
/// fichier`) : (type, équipement, variante, fichier `.ean` en minuscules).
pub fn parse_anim_clips(table: &IniTable, tid: i32) -> Vec<(u32, u32, u32, String)> {
    let (Some(header), Some(sec)) = (table.section(""), table.section(&tid.to_string())) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, v) in &sec.entries {
        if !header.get(k).unwrap_or("").starts_with("ANIMATION_NAME") {
            continue;
        }
        let mut it = v.splitn(4, ':');
        let (Some(t), Some(e), Some(n), Some(file)) = (it.next(), it.next(), it.next(), it.next()) else { continue };
        let (Ok(t), Ok(e), Ok(n)) = (t.trim().parse(), e.trim().parse(), n.trim().parse()) else { continue };
        let file = file.trim();
        if !file.is_empty() && file != "0" {
            out.push((t, e, n, format!("{}.ean", file.to_ascii_lowercase())));
        }
    }
    out
}

/// Donnée d'animation (équipement, variante) d'un clip lancé : type et fichier, de préférence dans
/// l'ensemble courant `equip`, sinon dans l'ensemble 0 (repli d'`animation::AnimationSet`).
pub fn clip_slot(clips: &[(u32, u32, u32, String)], kind: u32, equip: i32, file: &str) -> Option<(u32, u32)> {
    let found: Vec<(u32, u32)> = clips.iter().filter(|c| c.0 == kind && c.3.eq_ignore_ascii_case(file)).map(|c| (c.1, c.2)).collect();
    found
        .iter()
        .find(|(e, _)| equip >= 0 && *e == equip as u32)
        .or_else(|| found.iter().find(|(e, _)| *e == 0))
        .or_else(|| found.first())
        .copied()
}

/// Sons d'une donnée d'animation au moment `cond`.
pub fn sounds_for(list: &[AnimSound], kind: u32, equip: u32, variant: u32, cond: u8) -> Vec<String> {
    list.iter().filter(|s| s.kind == kind && s.equip == equip && s.variant == variant && s.cond == cond).map(|s| s.file.clone()).collect()
}

/// Indice d'un sol d'`ee_move_sound.txt` (`FUN_140831000`).
pub fn ground_index(name: &str) -> Option<u8> {
    const NAMES: [&str; GROUNDS] = ["SOIL", "SWAMP", "GRASS", "SAND", "LEAF", "SNOW", "WATER", "STONE", "WOOD", "METAL", "BONE", "MUD", "SOILGRASS", "SOILSOLID", "SPORE", "MOSS", "GRANITE"];
    NAMES.iter().position(|n| *n == name).map(|i| i as u8)
}

/// Sons de pas par TID : `[sol][pied]` (0 gauche, 1 droit).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoveSounds {
    pub tables: HashMap<i32, Vec<[Option<String>; 2]>>,
}

impl MoveSounds {
    /// Lecture d'`ee_move_sound.txt` comme `FUN_140830090` : la ligne 0 nomme les colonnes
    /// (`LEFT`, `RIGHT`) ; une ligne dont la première case n'est pas un sol et vaut un nombre non
    /// nul ouvre la table de ce TID (la table 0 est ouverte d'office) ; une ligne de sol remplit la
    /// table courante.
    pub fn parse(text: &str) -> Self {
        let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').map(str::trim).collect()).collect();
        let Some(head) = rows.first() else { return Self::default() };
        let left = head.iter().position(|c| *c == "LEFT");
        let right = head.iter().position(|c| *c == "RIGHT");
        let mut tables: HashMap<i32, Vec<[Option<String>; 2]>> = HashMap::new();
        let mut current = 0;
        tables.insert(0, vec![[None, None]; GROUNDS]);
        for row in rows.iter().skip(1) {
            let Some(first) = row.first().filter(|c| !c.is_empty()) else { continue };
            match ground_index(first) {
                None => {
                    let tid = first.parse::<i32>().unwrap_or(0);
                    if tid != 0 {
                        current = tid;
                        tables.entry(tid).or_insert_with(|| vec![[None, None]; GROUNDS]);
                    }
                }
                Some(g) => {
                    let table = tables.entry(current).or_insert_with(|| vec![[None, None]; GROUNDS]);
                    for (foot, col) in [left, right].into_iter().enumerate() {
                        if let Some(v) = col.and_then(|c| row.get(c)).filter(|v| !v.is_empty()) {
                            table[g as usize][foot] = Some(v.to_string());
                        }
                    }
                }
            }
        }
        Self { tables }
    }

    /// Son du pied `foot` (0 gauche, 1 droit) sur le sol `ground` pour la table `tid`.
    pub fn get(&self, tid: i32, ground: u8, foot: usize) -> Option<&str> {
        self.tables.get(&tid)?.get(ground as usize)?.get(foot)?.as_deref()
    }
}

/// Son de compétence (`SKILL_EFFECT_SOUND`) : rang, ensemble, n° de donnée d'effet, fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSound {
    pub rank: u32,
    pub equip: u32,
    pub index: u32,
    pub file: String,
}

/// Sons de compétences par TID (`charactertemplateskillsound.ini`), le premier par donnée.
pub fn parse_skill_sounds(table: &IniTable) -> HashMap<i32, Vec<SkillSound>> {
    let mut out = HashMap::new();
    let Some(header) = table.section("") else { return out };
    for sec in &table.sections {
        let Ok(tid) = sec.name.trim().parse::<i32>() else { continue };
        let mut list: Vec<SkillSound> = Vec::new();
        for (k, v) in &sec.entries {
            if !header.get(k).unwrap_or("").starts_with("SKILL_EFFECT_SOUND") {
                continue;
            }
            let f: Vec<&str> = v.splitn(4, ':').map(str::trim).collect();
            let num = |i: usize| f.get(i).and_then(|x| x.parse::<u32>().ok());
            let (Some(rank), Some(equip), Some(index), Some(file)) = (num(0), num(1), num(2), f.get(3)) else { continue };
            if rank >= 200 || file.is_empty() || list.iter().any(|s| (s.rank, s.equip, s.index) == (rank, equip, index)) {
                continue;
            }
            list.push(SkillSound { rank, equip, index, file: file.to_string() });
        }
        if !list.is_empty() {
            out.insert(tid, list);
        }
    }
    out
}

/// Délais (ms) des données d'effet `SKILL_EFFECT_DATA` (`rang:équipement:n°:EID:x:y:z:échelle:nœud:
/// délai`) d'un TID, par (rang, ensemble, n°).
pub fn skill_effect_delays(table: &IniTable, tid: i32) -> HashMap<(u32, u32, u32), u32> {
    let mut out = HashMap::new();
    let (Some(header), Some(sec)) = (table.section(""), table.section(&tid.to_string())) else { return out };
    for (k, v) in &sec.entries {
        if !header.get(k).unwrap_or("").starts_with("SKILL_EFFECT_DATA") {
            continue;
        }
        let f: Vec<&str> = v.split(':').map(str::trim).collect();
        let num = |i: usize| f.get(i).and_then(|x| x.parse::<i64>().ok());
        if let (Some(r), Some(e), Some(n)) = (num(0), num(1), num(2)) {
            if r >= 0 && e >= 0 && n >= 0 {
                out.insert((r as u32, e as u32, n as u32), num(9).unwrap_or(0).clamp(0, 60_000) as u32);
            }
        }
    }
    out
}

/// Sons d'une compétence lancée : ceux du rang `rank` dans l'ensemble `equip` s'il en a (sons ou
/// effets), sinon dans le plus petit ensemble des effets (choix des effets, `app_effects.rs`), avec le
/// délai de leur donnée d'effet.
pub fn skill_sounds_for(sounds: &[SkillSound], delays: &HashMap<(u32, u32, u32), u32>, rank: u32, equip: u32) -> Vec<(String, u32)> {
    let has = |e: u32| sounds.iter().any(|s| s.rank == rank && s.equip == e) || delays.keys().any(|k| k.0 == rank && k.1 == e);
    let chosen = if has(equip) {
        equip
    } else {
        delays.keys().filter(|k| k.0 == rank).map(|k| k.1).min().or_else(|| sounds.iter().filter(|s| s.rank == rank).map(|s| s.equip).min()).unwrap_or(0)
    };
    sounds
        .iter()
        .filter(|s| s.rank == rank && s.equip == chosen)
        .map(|s| (s.file.clone(), delays.get(&(rank, chosen, s.index)).copied().unwrap_or(0)))
        .collect()
}

/// Moments des pas dans un clip de durée `duration` (s) : gauche puis droit, ceux qui tombent dans
/// le clip (`FUN_14073ccc0` refuse un rappel au-delà de la fin).
pub fn step_times(duration: f32) -> Vec<(f32, usize)> {
    let right = duration * STEP_RIGHT_FACTOR + STEP_LEFT_AT;
    [(STEP_LEFT_AT, 0), (right, 1)].into_iter().filter(|(t, _)| *t <= duration).collect()
}

/// Tables lues une fois.
#[derive(Default)]
struct SoundData {
    anim: HashMap<i32, Vec<AnimSound>>,
    moves: MoveSounds,
    skills: HashMap<i32, Vec<SkillSound>>,
}

/// Son programmé : pas du joueur (pied) ou son de personnage.
enum Pending {
    Step { foot: usize },
    Char { cid: i32, file: String },
}

/// État de la colle (champ `sounds` d'[`App`]).
#[derive(Default)]
pub(super) struct SoundsGlue {
    data: Option<Arc<SoundData>>,
    /// Clips par TID (`charactertemplateanimation.ini`).
    clips: HashMap<i32, Arc<Vec<(u32, u32, u32, String)>>>,
    /// Délais des données d'effet de compétence par TID.
    skill_delays: HashMap<i32, Arc<HashMap<(u32, u32, u32), u32>>>,
    /// Dernière attaque lancée par personnage : (TID, ensemble, variante).
    last_attack: HashMap<i32, (i32, u32, u32)>,
    /// Attaquants dont le coup vient de toucher (sons de condition 1 à jouer).
    hits: Vec<i32>,
    pending: Vec<(Instant, Pending)>,
}

impl SoundsGlue {
    fn data(&mut self, assets: &Assets) -> Arc<SoundData> {
        self.data
            .get_or_insert_with(|| {
                let anim = assets.table("ini", "charactertemplateeventeffect.ini").map(|t| parse_anim_sounds(&t)).unwrap_or_default();
                let moves = MoveSounds::parse(&super::npc_glue::client_text_in(assets, "ini", "ee_move_sound.txt"));
                let skills = assets.table("ini", "charactertemplateskillsound.ini").map(|t| parse_skill_sounds(&t)).unwrap_or_default();
                log::info!("sons : {} modèles avec sons d'animation, {} tables de pas, {} modèles avec sons de compétences", anim.len(), moves.tables.len(), skills.len());
                Arc::new(SoundData { anim, moves, skills })
            })
            .clone()
    }

    fn clips(&mut self, assets: &Assets, tid: i32) -> Arc<Vec<(u32, u32, u32, String)>> {
        self.clips.entry(tid).or_insert_with(|| Arc::new(assets.table("ini", "charactertemplateanimation.ini").map(|t| parse_anim_clips(&t, tid)).unwrap_or_default())).clone()
    }
}

impl App {
    /// Position et TID d'un personnage en vue (joueur compris).
    fn sounds_char(&self, cid: i32) -> Option<(Vec3, i32)> {
        self.player.as_ref().filter(|p| p.cid == cid).or_else(|| self.entities.iter().find(|e| e.cid == cid)).map(|e| (e.position, e.tid))
    }

    /// Son `sound/effect/<file>` d'un personnage : 2D au volume 1,1 pour le joueur, 3D sur sa
    /// position (suivie) au volume 1,0 pour les autres (`FUN_1405695d0`, `FUN_14056da90`).
    pub(super) fn sounds_play_char(&mut self, cid: i32, file: &str) {
        let path = format!("sound/effect/{file}");
        let me = self.player.as_ref().is_some_and(|p| p.cid == cid);
        if me {
            self.audio.play_effect_volume(&path, CHARACTER_2D_VOLUME);
        } else if let Some((pos, _)) = self.sounds_char(cid) {
            self.audio.play_effect_at(&path, pos, Some(cid), 1.0, false);
        }
    }

    /// Événements qui déclenchent un son : compétence lancée, coup qui touche.
    pub(super) fn sounds_observe(&mut self, ev: &WorldEvent) {
        match ev {
            WorldEvent::Connected | WorldEvent::Disconnected(_) => {
                self.sounds.pending.clear();
                self.sounds.hits.clear();
                self.sounds.last_attack.clear();
            }
            WorldEvent::Skill(SkillEvent::Cast { caster, tid, .. }) => {
                let assets = self.assets.clone();
                let Some((_, char_tid)) = self.sounds_char(*caster) else { return };
                let Some(rank) = super::effects_glue::skill_rank(&assets, char_tid, *tid) else { return };
                let data = self.sounds.data(&assets);
                let Some(list) = data.skills.get(&char_tid) else { return };
                let delays = self
                    .sounds
                    .skill_delays
                    .entry(char_tid)
                    .or_insert_with(|| Arc::new(assets.table("ini", "charactertemplateskill.ini").map(|t| skill_effect_delays(&t, char_tid)).unwrap_or_default()))
                    .clone();
                let equip = self.anim_equip_of(*caster).max(0) as u32;
                let now = Instant::now();
                for (file, delay) in skill_sounds_for(list, &delays, rank, equip) {
                    self.sounds.pending.push((now + Duration::from_millis(delay as u64), Pending::Char { cid: *caster, file }));
                }
            }
            WorldEvent::CharacterAction { cid, result, .. } => {
                if matches!(result, Some(archrust_netclient::combat::outcome::SUCCESS | archrust_netclient::combat::outcome::CRITICAL)) {
                    self.sounds.hits.push(*cid);
                }
            }
            WorldEvent::CharacterRemoved { cid, .. } => {
                self.sounds.last_attack.remove(cid);
            }
            _ => {}
        }
    }

    /// Chaque image (après les animations) : départs de clips → sons d'animation et pas, coups
    /// portés, sons programmés, puis auditeur et sources des sons 3D.
    pub(super) fn sounds_frame(&mut self, _dt: f32) {
        let assets = self.assets.clone();
        let data = self.sounds.data(&assets);
        let me = self.player.as_ref().map(|p| (p.cid, p.tid));
        // Départs de clips de tous les animateurs (joueur compris).
        let mut started: Vec<(i32, i32, i32, Vec<(u32, Arc<Clip>)>)> = Vec::new();
        if let (Some((cid, tid)), Some((_, Some(a)))) = (me, self.player_animator.as_mut()) {
            started.push((cid, tid, a.equip, a.take_started()));
        }
        for e in &self.entities {
            if let Some((_, Some(a))) = self.animators.get_mut(&e.cid) {
                let list = a.take_started();
                if !list.is_empty() {
                    started.push((e.cid, e.tid, a.equip, list));
                }
            }
        }
        let now = Instant::now();
        let mut play: Vec<(i32, String)> = Vec::new();
        for (cid, tid, equip, list) in started {
            let mine = me.is_some_and(|(c, _)| c == cid);
            let clips = self.sounds.clips(&assets, tid);
            for (kind, clip) in list {
                if mine && (kind == AnimType::Walk as u32 || kind == AnimType::Run as u32) {
                    // Nouveau clip de déplacement : il remplace les pas programmés de l'ancien.
                    self.sounds.pending.retain(|(_, p)| !matches!(p, Pending::Step { .. }));
                    for (t, foot) in step_times(clip.duration) {
                        self.sounds.pending.push((now + Duration::from_secs_f32(t), Pending::Step { foot }));
                    }
                } else if mine {
                    self.sounds.pending.retain(|(_, p)| !matches!(p, Pending::Step { .. }));
                }
                if kind >= 16 {
                    continue;
                }
                let Some((e, v)) = clip_slot(&clips, kind, equip, &clip.name) else { continue };
                if kind == AnimType::Attack as u32 {
                    self.sounds.last_attack.insert(cid, (tid, e, v));
                }
                if let Some(sounds) = data.anim.get(&tid) {
                    play.extend(sounds_for(sounds, kind, e, v, COND_START).into_iter().map(|f| (cid, f)));
                }
            }
        }
        // Coups qui touchent : sons de condition 1 de l'attaque de l'attaquant.
        for cid in std::mem::take(&mut self.sounds.hits) {
            if let Some((tid, e, v)) = self.sounds.last_attack.get(&cid).copied() {
                if let Some(sounds) = data.anim.get(&tid) {
                    play.extend(sounds_for(sounds, AnimType::Attack as u32, e, v, COND_HIT).into_iter().map(|f| (cid, f)));
                }
            }
        }
        // Sons programmés arrivés à échéance.
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.sounds.pending).into_iter().partition(|(t, _)| *t <= now);
        self.sounds.pending = later;
        for (_, p) in due {
            match p {
                Pending::Char { cid, file } => play.push((cid, file)),
                Pending::Step { foot } => {
                    let Some(p) = self.player.as_ref() else { continue };
                    let (cid, pos) = (p.cid, p.position);
                    let table = if self.ride.riding(cid) { self.ride.mount_tid(cid).unwrap_or(0) } else { 0 };
                    let ground = self.zones.ground_at(&assets, pos.x, pos.z).unwrap_or(0);
                    if let Some(file) = data.moves.get(table, ground, foot) {
                        self.audio.play_effect_volume(&format!("sound/effect/{file}"), CHARACTER_2D_VOLUME);
                    }
                }
            }
        }
        for (cid, file) in play {
            self.sounds_play_char(cid, &file);
        }
        // Auditeur : la caméra ; sources : les personnages suivis.
        let view = self.camera.view();
        let listener = Listener { position: self.camera.eye(), right: Vec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x) };
        let positions: HashMap<i32, Vec3> = self.player.iter().chain(self.entities.iter()).map(|e| (e.cid, e.position)).collect();
        self.audio.update_spatial(listener, |cid| positions.get(&cid).copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_sounds_attach_to_the_animation_and_conditions_to_the_last_sound() {
        let t = IniTable::parse(
            "[]\n1=AGCMEVENTEFFECT_CAAD_SOUND_DATA3000\n2=AGCMEVENTEFFECT_CAAD_SOUND_CONDITION_FLAGS3000\n3=AGCMEVENTEFFECT_CAAD_SOUND_DATA5000\n\
             [1]\n1=3:0:0:0:CA0110.WAV\n1=3:0:0:0:CA0210.WAV\n2=3:0:0:0:1\n3=5:0:0:0:CA0000.WAV\n",
        );
        let all = parse_anim_sounds(&t);
        let list = &all[&1];
        assert_eq!(list.len(), 3);
        assert_eq!(sounds_for(list, 3, 0, 0, COND_START), vec!["CA0110.WAV".to_string()]);
        assert_eq!(sounds_for(list, 3, 0, 0, COND_HIT), vec!["CA0210.WAV".to_string()]);
        assert_eq!(sounds_for(list, 5, 0, 0, COND_START), vec!["CA0000.WAV".to_string()]);
        assert!(sounds_for(list, 4, 0, 0, COND_START).is_empty());
    }

    #[test]
    fn started_clip_maps_to_its_animation_data() {
        let t = IniTable::parse("[]\n2=ANIMATION_NAME000\n3=ANIMATION_NAME001\n4=ANIMATION_NAME002\n[96]\n2=3:0:0:A0060300\n3=3:0:1:A0060301\n4=3:1:0:A0060300\n");
        let clips = parse_anim_clips(&t, 96);
        assert_eq!(clips.len(), 3);
        assert_eq!(clip_slot(&clips, 3, 1, "a0060300.ean"), Some((1, 0)));
        assert_eq!(clip_slot(&clips, 3, 2, "a0060300.ean"), Some((0, 0)));
        assert_eq!(clip_slot(&clips, 3, 0, "a0060301.ean"), Some((0, 1)));
        assert_eq!(clip_slot(&clips, 4, 0, "a0060300.ean"), None);
    }

    #[test]
    fn move_sounds_select_by_mount_ground_and_foot() {
        let text = "0\tLEFT\tRIGHT\tdéfaut\nSOIL\tMV0000.WAV\tMV0001.WAV\nGRASS\tMV0004.WAV\tMV0005.WAV\n\n567\tLEFT\tRIGHT\tcheval\nSOIL\tMV0050.WAV\tMV0051.WAV\n";
        let m = MoveSounds::parse(text);
        assert_eq!(m.get(0, 0, 0), Some("MV0000.WAV"));
        assert_eq!(m.get(0, 2, 1), Some("MV0005.WAV"));
        assert_eq!(m.get(567, 0, 1), Some("MV0051.WAV"));
        // Sol sans son, TID absent : rien.
        assert_eq!(m.get(567, 2, 0), None);
        assert_eq!(m.get(999, 0, 0), None);
        assert_eq!(ground_index("GRANITE"), Some(16));
        assert_eq!(crate::zones::cell_ground(0x0009_0004), 2);
    }

    #[test]
    fn steps_fall_at_a_quarter_second_and_half_the_clip_later() {
        assert_eq!(step_times(0.8), vec![(0.25, 0), (0.65, 1)]);
        // Clip trop court : le pas droit tomberait après la fin.
        assert_eq!(step_times(0.4), vec![(0.25, 0)]);
    }

    #[test]
    fn skill_sounds_follow_the_effect_data_and_its_delay() {
        let t = IniTable::parse("[]\n1=SKILL_EFFECT_SOUND012\n2=SKILL_EFFECT_SOUND022\n3=SKILL_EFFECT_SOUND110\n[1]\n1=0:1:2:SA0010.WAV\n2=0:2:2:SA0013.WAV\n3=1:1:0:SA0010.WAV\n9=0:1:2:XX.WAV\n");
        let all = parse_skill_sounds(&t);
        assert_eq!(all[&1].len(), 3);
        let d = IniTable::parse("[]\n5=SKILL_EFFECT_DATA0\n[1]\n5=0:1:2:5001:0:150:0:1:0:300\n");
        let delays = skill_effect_delays(&d, 1);
        assert_eq!(delays.get(&(0, 1, 2)), Some(&300));
        assert_eq!(skill_sounds_for(&all[&1], &delays, 0, 1), vec![("SA0010.WAV".to_string(), 300)]);
        assert_eq!(skill_sounds_for(&all[&1], &delays, 0, 2), vec![("SA0013.WAV".to_string(), 0)]);
        // Ensemble sans rien : celui des effets.
        assert_eq!(skill_sounds_for(&all[&1], &delays, 0, 7), vec![("SA0010.WAV".to_string(), 300)]);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : tables d'Archonia lues.
    #[test]
    fn client_tables_are_read() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        let anim = parse_anim_sounds(&assets.table("ini", "charactertemplateeventeffect.ini").unwrap());
        // TID 1, attaque sans arme : CA0210 au départ, CA0110 (deux fois) quand le coup touche.
        assert_eq!(sounds_for(&anim[&1], 3, 0, 0, COND_START), vec!["CA0210.WAV".to_string()]);
        assert_eq!(sounds_for(&anim[&1], 3, 0, 0, COND_HIT), vec!["CA0110.WAV".to_string(); 2]);
        let moves = MoveSounds::parse(&super::super::npc_glue::client_text_in(&assets, "ini", "ee_move_sound.txt"));
        assert_eq!(moves.get(0, 2, 0), Some("MV0004.WAV"));
        assert_eq!(moves.get(567, 0, 1), Some("MV0051.WAV"));
        let skills = parse_skill_sounds(&assets.table("ini", "charactertemplateskillsound.ini").unwrap());
        assert!(skills[&1].iter().any(|s| (s.rank, s.equip, s.index) == (0, 1, 2) && s.file == "SA0010.WAV"));
        // Les fichiers se trouvent dans les packs de `sound/effect` (soundpak.info).
        let lib = crate::audio::SoundLibrary::new(Arc::new(assets), None);
        for f in ["CA0210.WAV", "CA0110.WAV", "MV0004.WAV", "MV0051.WAV", "SA0010.WAV"] {
            assert!(lib.read(&format!("sound/effect/{f}")).is_ok(), "{f}");
        }
    }
}
