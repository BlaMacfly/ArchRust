//! Zones du monde : région d'une position, musique et ambiance de la région, heure du jeu.
//!
//! Chaîne reconstituée sur les données du client et sur Archonia (module ciel `AgcmSkySet`) :
//!
//! 1. la grille d'attributs du secteur `C{sx},{sy}.amf` (archive `world/a%06dx.ma1`, 12 octets
//!    d'en-tête puis 16 × 16 `u32`, puis les segments de blocage) porte dans les 16 bits hauts de
//!    chaque cellule l'indice de la région (`RegionTemplate.ini`) ; les 16 bits bas sont les
//!    attributs de cellule (octet 1, bit 0 : sol bloqué, voir [`cell_ground_blocked`]).
//!    Vérifié : le secteur d'Anchorville (`C325,389`) ne contient que des cellules `0x0009xxxx`
//!    et la région 9 est « 앵커빌 » (Anchorville) ; la salle du trône de la scène de connexion
//!    (`C272,272`) donne la région 22 « 3D_UI 휴먼컨셉 ». L'ordre des cellules (ligne = z,
//!    colonne = x, comme la grille de hauteurs) est une hypothèse sans effet sur la musique tant que
//!    le secteur est d'une seule région ;
//! 2. `ini/RegionTemplate.ini` : section `[région]`, clés `Name`, `SkySet`, `WorldMap`, `Type` ;
//! 3. `ini/skyset.ini` : section `[SkySet]`, six tranches horaires fixes `00`, `05`, `07`, `13`,
//!    `19`, `21` (Archonia : 0–4 h, 5–6 h, 7–12 h, 13–17 h, 18–20 h, 21–23 h), chacune avec :
//!    * `HH_BackGroundMusic` : MP3 de `sound/bgm/` ; Archonia tire au hasard parmi ceux des six
//!      tranches, sans suivre l'heure (voir [`crate::sky::MusicPlaylist`]) ;
//!    * `HH_EffectIndex_01..05` : jeux d'effets (`effect/ini/new/efffile_set.eff`) dont les effets
//!      **son** nomment un MP3 de `sound/stream/` ; Archonia les joue tous en boucle au changement
//!      de tranche ou de ciel (`HH_EffectRate_NN` ne sert qu'au tirage de l'effet visuel). Pour le
//!      `SkySet` 13 (Anchorville, nord de Limelight) : 3828 « 휴먼-심야 » `A_GRN_M.mp3` de 0 à 6 h,
//!      3825 « 휴먼-새벽 » `A_GRN_E.mp3` de 7 à 12 h, 3826 « 휴먼-낮 » `A_GRN_D.mp3` de 13 à 17 h,
//!      3827 « 휴먼-저녁 » `A_GRN_N.mp3` ensuite. Vérifié en jeu sur ArchCore à 7 h UTC :
//!      région 0 → `hft_008.mp3` + `a_grn_e.mp3`.
//! 4. l'heure est celle du jeu : `(temps_ms / 3 600 000) % 24` sur le temps du paquet
//!    `AGPMTIMER` (champ 0 vitesse, champ 1 millisecondes) entretenu par le client ([`GameClock`]) ;
//!    sans serveur, 12 h 00 à la vitesse ×6 comme Archonia.

use crate::assets::Assets;
use crate::world::{sector_of, sector_origin, tile_of, CELLS, CELL_SIZE};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// En-tête d'un `.amf` : version `0x1003`, un `u32` (0), largeur de la grille (16) — écriture
/// d'ApWorldSector, `FUN_1402241d0`.
const AMF_HEADER: usize = 12;
/// Taille d'une grille `.amf` : en-tête + 256 cellules.
const AMF_LEN: usize = AMF_HEADER + CELLS * CELLS * 4;

/// Préfixes des six tranches horaires de `skyset.ini`, dans l'ordre de la journée.
pub const SKY_SLOTS: [&str; 6] = ["00", "05", "07", "13", "19", "21"];

/// Tranche horaire (indice dans [`SKY_SLOTS`]) d'une heure 0–23, comme Archonia.
pub fn sky_slot(hour: u32) -> usize {
    match hour % 24 {
        0..=4 => 0,
        5..=6 => 1,
        7..=12 => 2,
        13..=17 => 3,
        18..=20 => 4,
        _ => 5,
    }
}

/// Indice de région porté par une cellule d'attributs.
pub fn cell_region(cell: u32) -> u16 {
    (cell >> 16) as u16
}

/// Grille de 256 cellules d'un `.amf` (sans l'en-tête), ou `None` si la taille ne correspond pas.
/// Les cellules suivent un en-tête de 12 octets ; la lecture à 16 octets, d'avant le 2026-10-29,
/// décalait la grille d'une cellule (la dernière était le nombre de segments).
pub fn parse_amf(bytes: &[u8]) -> Option<Vec<u32>> {
    if bytes.len() < AMF_LEN {
        return None;
    }
    Some(
        bytes[AMF_HEADER..AMF_LEN]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect(),
    )
}

/// Segments de blocage d'un `.amf`, après la grille : `u32` nombre, puis 28 octets par segment
/// (deux points x, y, z et un `f32` inutilisé). Archonia (`FUN_140197780`) empêche de les franchir.
pub fn parse_amf_lines(bytes: &[u8]) -> Vec<[Vec3; 2]> {
    let f = |o: usize| bytes.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()));
    let Some(n) = bytes.get(AMF_LEN..AMF_LEN + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())) else {
        return Vec::new();
    };
    (0..n as usize)
        .map_while(|k| {
            let o = AMF_LEN + 4 + k * 28;
            Some([Vec3::new(f(o)?, f(o + 4)?, f(o + 8)?), Vec3::new(f(o + 12)?, f(o + 16)?, f(o + 20)?)])
        })
        .collect()
}

/// Cellule bloquée au sol : bit 0 de l'octet 1 (`FUN_14018cc80`, `FUN_140183bd0` mode 0). Le bit 1
/// du même octet est le blocage « ciel », qu'Archonia ne consulte que pour un vol.
pub fn cell_ground_blocked(cell: u32) -> bool {
    cell & 0x100 != 0
}

/// Musiques d'une section `skyset.ini` : `HH_BackGroundMusic` distincts et non vides des six
/// tranches (`sound/bgm/…`). Archonia en tire une au hasard, sans suivre l'heure (voir
/// [`crate::sky::MusicPlaylist`]).
pub fn sky_music(section: &archrust_data::ini::IniSection) -> Vec<String> {
    crate::sky::SkyTemplate::parse(section).map(|t| t.music()).unwrap_or_default()
}

/// Jeux d'effets (`HH_EffectIndex_01..05`, non nuls) de la tranche de l'heure.
pub fn sky_effect_sets(section: &archrust_data::ini::IniSection, hour: u32) -> Vec<u32> {
    let prefix = SKY_SLOTS[sky_slot(hour)];
    (1..=5)
        .filter_map(|n| section.get_i64(&format!("{prefix}_EffectIndex_{n:02}")))
        .filter(|&id| id > 0)
        .map(|id| id as u32)
        .collect()
}

/// Heure locale (0–23) ; hors Windows, heure UTC.
pub fn local_hour() -> u32 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        };
        // SAFETY : `GetLocalTime` remplit la structure fournie, valide et correctement alignée.
        unsafe { GetLocalTime(&mut t) };
        return (t.wHour as u32) % 24;
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ((secs / 3600) % 24) as u32
    }
}

/// Temps de jeu d'Archonia avant tout paquet `AGPMTIMER` : 12 h 00 (`FUN_1400f72d0`).
pub const DEFAULT_GAME_MS: u64 = 43_200_000;
/// Vitesse du temps d'Archonia avant tout paquet : ×6 (1 h de jeu = 10 min, 1 jour = 4 h réelles).
pub const DEFAULT_TIME_SPEED: u32 = 6;

/// Horloge du jeu d'AgpmTimer (`FUN_14022e640`) : `temps = base + (tick − tick_base) × vitesse`,
/// heure `(temps / 3 600 000) % 24`. Le paquet `AGPMTIMER` donne la vitesse (champ 0) et le temps
/// (champ 1, ms) ; un champ nul est ignoré, et seul le temps remet la base à l'instant de
/// réception (une vitesse seule s'applique donc à tout le temps écoulé depuis la base, comme dans
/// Archonia). Sans serveur : 12 h 00 ×6 depuis le lancement, comme Archonia (ArchCore envoie
/// l'époque Unix décalée du fuseau du serveur, `world.timeZone`).
#[derive(Debug, Clone, Copy)]
pub struct GameClock {
    base_ms: u64,
    speed: u32,
    at: Instant,
    from_server: bool,
    /// Heure du serveur (champ 2, s Unix, 0 = inconnue), fuseau (champ 3, s) et réception du dernier paquet.
    server_unix: u32,
    server_zone: i32,
    server_at: Instant,
}

impl Default for GameClock {
    fn default() -> Self {
        let now = Instant::now();
        Self { base_ms: DEFAULT_GAME_MS, speed: DEFAULT_TIME_SPEED, at: now, from_server: false, server_unix: 0, server_zone: 0, server_at: now }
    }
}

impl GameClock {
    /// Paquet `AGPMTIMER` reçu : vitesse et temps, `None` = inchangé.
    pub fn receive(&mut self, speed: Option<u8>, time_ms: Option<u64>) {
        self.receive_at(speed, time_ms, Instant::now());
    }

    pub fn receive_at(&mut self, speed: Option<u8>, time_ms: Option<u64>, now: Instant) {
        if let Some(s) = speed.filter(|&s| s != 0) {
            self.speed = s as u32;
        }
        if let Some(t) = time_ms.filter(|&t| t != 0) {
            self.base_ms = t;
            self.at = now;
            self.from_server = true;
        }
    }

    /// Temps reçu du serveur, vitesse inchangée.
    pub fn set(&mut self, server_ms: u64) {
        self.receive(None, Some(server_ms));
    }

    /// Retour aux valeurs d'Archonia (sortie du monde).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Le serveur a donné le temps.
    pub fn is_set(&self) -> bool {
        self.from_server
    }

    pub fn speed(&self) -> u32 {
        self.speed
    }

    /// Temps du jeu courant en millisecondes.
    pub fn now_ms(&self) -> u64 {
        self.ms_at(Instant::now())
    }

    pub fn ms_at(&self, now: Instant) -> u64 {
        let elapsed = now.saturating_duration_since(self.at).as_millis() as u64;
        self.base_ms.saturating_add(elapsed.saturating_mul(self.speed as u64))
    }

    /// Heure du jeu (0–23).
    pub fn hour(&self) -> u32 {
        hour_of_ms(self.now_ms())
    }

    /// Champs 2 (heure du serveur, s Unix) et 3 (fuseau, s) d'un paquet `AGPMTIMER` : un champ nul est
    /// ignoré, et chaque paquet remet le départ du décompte à l'instant de réception, même sans champ 2
    /// (`FUN_14022e800`).
    pub fn receive_server_at(&mut self, unix: Option<u32>, zone: Option<i32>, now: Instant) {
        if let Some(u) = unix.filter(|&u| u != 0) {
            self.server_unix = u;
        }
        if let Some(z) = zone.filter(|&z| z != 0) {
            self.server_zone = z;
        }
        self.server_at = now;
    }

    /// Heure du serveur en secondes Unix (champ 2 + secondes écoulées, `FUN_1401a6fd0`) ; `None` sans champ 2.
    pub fn server_unix_at(&self, now: Instant) -> Option<u64> {
        (self.server_unix != 0).then(|| self.server_unix as u64 + now.saturating_duration_since(self.server_at).as_secs())
    }

    /// Heure du serveur affichée (« Server Time: », `FUN_140677c90`) : champ 2 + champ 3 + écoulé.
    pub fn server_display_at(&self, now: Instant) -> Option<i64> {
        self.server_unix_at(now).map(|u| u as i64 + self.server_zone as i64)
    }
}

/// Heure (0–23) d'un temps en millisecondes, comme Archonia (`AgpmEventNature`).
pub fn hour_of_ms(ms: u64) -> u32 {
    ((ms / 3_600_000) % 24) as u32
}

/// Ce qu'une position commande à l'audio.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ZoneAudio {
    pub region: u16,
    /// Musiques (`sound/bgm/…`) du gabarit de ciel de la région (toutes tranches).
    pub music: Vec<String>,
    /// Ambiances (`sound/stream/…`) de la région à l'heure donnée, dans l'ordre des
    /// `EffectIndex` ; Archonia les joue toutes, ArchRust la première.
    pub ambience: Vec<String>,
}

/// Région, musique et ambiance d'une position, avec cache des grilles `.amf` lues.
#[derive(Default)]
pub struct Zones {
    /// Grille par secteur (`None` = tuile ou entrée absente, on ne réessaie pas).
    grids: HashMap<(i32, i32), Option<Arc<Vec<u32>>>>,
}

impl Zones {
    fn grid(&mut self, assets: &Assets, sx: i32, sy: i32) -> Option<Arc<Vec<u32>>> {
        if let Some(g) = self.grids.get(&(sx, sy)) {
            return g.clone();
        }
        let loaded = (|| {
            let pack = assets.magpack('a', tile_of(sx, sy), "ma1").ok()?;
            let bytes = pack.unpack_named(&format!("C{sx},{sy}.amf"))?.ok()?;
            parse_amf(&bytes).map(Arc::new)
        })();
        if loaded.is_none() {
            log::debug!("zones : grille d'attributs C{sx},{sy}.amf indisponible");
        }
        self.grids.insert((sx, sy), loaded.clone());
        loaded
    }

    /// Indice de région (`RegionTemplate.ini`) de la cellule contenant (x, z).
    pub fn region_at(&mut self, assets: &Assets, x: f32, z: f32) -> Option<u16> {
        let (sx, sy) = sector_of(x, z);
        let grid = self.grid(assets, sx, sy)?;
        let origin = sector_origin(sx, sy);
        let cx = (((x - origin.x) / CELL_SIZE).floor() as i64).clamp(0, CELLS as i64 - 1) as usize;
        let cz = (((z - origin.z) / CELL_SIZE).floor() as i64).clamp(0, CELLS as i64 - 1) as usize;
        Some(cell_region(grid[cz * CELLS + cx]))
    }

    /// Nom (coréen) d'une région.
    pub fn region_name(assets: &Assets, region: u16) -> Option<String> {
        let t = assets.table("ini", "RegionTemplate.ini").ok()?;
        t.section(&region.to_string())?.get("Name").map(str::to_string)
    }

    /// Jeu de ciel (`SkySet`) d'une région.
    pub fn sky_set(assets: &Assets, region: u16) -> Option<i64> {
        let t = assets.table("ini", "RegionTemplate.ini").ok()?;
        t.section(&region.to_string())?.get_i64("SkySet")
    }

    /// Musiques (`sound/bgm/…`) du gabarit de ciel d'une région, d'après `skyset.ini`.
    pub fn music_for_region(assets: &Assets, region: u16) -> Vec<String> {
        let Some(sky) = Self::sky_set(assets, region) else { return Vec::new() };
        let Ok(t) = assets.table("ini", "skyset.ini") else { return Vec::new() };
        t.section(&sky.to_string()).map(sky_music).unwrap_or_default()
    }

    /// Ambiances (`sound/stream/…`) d'une région à l'heure donnée : jeux d'effets
    /// `HH_EffectIndex_NN` de `skyset.ini` → effets son de `efffile_var.eff`.
    pub fn ambience_for_region(assets: &Assets, region: u16, hour: u32) -> Vec<String> {
        let Some(sky) = Self::sky_set(assets, region) else {
            return Vec::new();
        };
        let Ok(t) = assets.table("ini", "skyset.ini") else {
            return Vec::new();
        };
        let Some(section) = t.section(&sky.to_string()) else {
            return Vec::new();
        };
        let ids = sky_effect_sets(section, hour);
        if ids.is_empty() {
            return Vec::new();
        }
        let sets = assets.effect_sets();
        let mut out = Vec::new();
        for id in ids {
            match sets.get(id) {
                Some(set) => {
                    if set.sounds.is_empty() {
                        log::debug!("jeu d'effets {id} ({}) sans son", set.name);
                    }
                    out.extend(
                        set.sounds
                            .iter()
                            .map(|s| format!("sound/stream/{}", s.to_ascii_lowercase())),
                    );
                }
                None => log::debug!("jeu d'effets {id} absent de efffile_set.eff"),
            }
        }
        out
    }

    /// Région, musique et ambiance d'une position à l'heure donnée.
    pub fn audio_at(&mut self, assets: &Assets, x: f32, z: f32, hour: u32) -> Option<ZoneAudio> {
        let region = self.region_at(assets, x, z)?;
        Some(ZoneAudio {
            region,
            music: Self::music_for_region(assets, region),
            ambience: Self::ambience_for_region(assets, region, hour),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_data::ini::IniTable;

    #[test]
    fn amf_cells_carry_the_region_in_the_high_half() {
        assert_eq!(cell_region(0x0009_000e), 9);
        assert_eq!(cell_region(0x0016_020e), 22);
        // En-tête de C344,411.amf : 0x1003, 0, 16 ; puis la grille et un segment.
        let mut bytes = Vec::new();
        for v in [0x1003u32, 0, 16] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for i in 0..256u32 {
            bytes.extend_from_slice(&((i << 16) | if i == 5 { 0x30e } else { 0x20e }).to_le_bytes());
        }
        assert!(parse_amf_lines(&bytes).is_empty());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let grid = parse_amf(&bytes).unwrap();
        assert_eq!(grid.len(), 256);
        assert_eq!(cell_region(grid[0]), 0);
        assert_eq!(cell_region(grid[17]), 17);
        assert!(cell_ground_blocked(grid[5]) && !cell_ground_blocked(grid[6]));
        assert_eq!(parse_amf_lines(&bytes), vec![[Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)]]);
        assert!(parse_amf(&bytes[..100]).is_none());
    }

    #[test]
    fn sky_slots_follow_archonia_periods() {
        assert_eq!(sky_slot(0), 0);
        assert_eq!(sky_slot(4), 0);
        assert_eq!(sky_slot(5), 1);
        assert_eq!(sky_slot(6), 1);
        assert_eq!(sky_slot(7), 2);
        assert_eq!(sky_slot(12), 2);
        assert_eq!(sky_slot(13), 3);
        assert_eq!(sky_slot(17), 3);
        assert_eq!(sky_slot(18), 4);
        assert_eq!(sky_slot(20), 4);
        assert_eq!(sky_slot(21), 5);
        assert_eq!(sky_slot(23), 5);
        assert_eq!(sky_slot(27), 0);
    }

    #[test]
    fn sky_music_lists_every_slot() {
        let t = IniTable::parse(
            "[13]\nTID=13\n00_BackGroundMusic=HFT_001.mp3\n05_BackGroundMusic=HFT_006.mp3\n07_BackGroundMusic=\n13_BackGroundMusic=-\n19_BackGroundMusic=HFT_002.MP3\n21_BackGroundMusic=hft_001.mp3\n[3]\n00_BackGroundMusic=\n05_BackGroundMusic=\n",
        );
        let s = t.section("13").unwrap();
        // Toutes les tranches, sans doublon ni vide : la musique ne suit pas l'heure (Archonia).
        assert_eq!(sky_music(s), ["sound/bgm/hft_001.mp3", "sound/bgm/hft_006.mp3", "sound/bgm/hft_002.mp3"]);
        assert!(sky_music(t.section("3").unwrap()).is_empty());
        assert!(local_hour() < 24);
    }

    #[test]
    fn sky_effect_sets_follow_the_slot() {
        let t = IniTable::parse(
            "[13]\n00_EffectIndex_01=3828\n00_EffectIndex_02=0\n07_EffectIndex_01=3825\n07_EffectIndex_02=3868\n07_EffectIndex_03=0\n13_EffectIndex_01=3826\n",
        );
        let s = t.section("13").unwrap();
        assert_eq!(sky_effect_sets(s, 2), vec![3828]);
        assert_eq!(sky_effect_sets(s, 9), vec![3825, 3868]);
        assert_eq!(sky_effect_sets(s, 13), vec![3826]);
        // Tranche sans clé : rien.
        assert!(sky_effect_sets(s, 22).is_empty());
    }

    #[test]
    fn game_clock_uses_the_server_time_when_known() {
        let mut clock = GameClock::default();
        assert!(!clock.is_set());
        // Sans serveur : 12 h ×6, comme Archonia.
        assert_eq!(clock.hour(), 12);
        assert_eq!(clock.speed(), 6);
        // 1970-01-01 15:30 UTC.
        clock.set(15 * 3_600_000 + 30 * 60_000);
        assert!(clock.is_set());
        assert_eq!(clock.hour(), 15);
        assert_eq!(hour_of_ms(24 * 3_600_000 + 5), 0);
        assert_eq!(hour_of_ms(u64::MAX), ((u64::MAX / 3_600_000) % 24) as u32);
        clock.clear();
        assert!(!clock.is_set());
        assert_eq!(clock.speed(), DEFAULT_TIME_SPEED);
    }

    #[test]
    fn game_clock_applies_the_time_speed() {
        let t0 = Instant::now();
        let mut clock = GameClock::default();
        clock.receive_at(Some(6), Some(10_000), t0);
        assert_eq!(clock.ms_at(t0 + std::time::Duration::from_secs(10)), 70_000);
        // Champs nuls : ignorés.
        clock.receive_at(Some(0), Some(0), t0 + std::time::Duration::from_secs(1));
        assert_eq!(clock.speed(), 6);
        assert_eq!(clock.ms_at(t0 + std::time::Duration::from_secs(10)), 70_000);
        // Vitesse seule : appliquée depuis la base (Archonia ne remet pas la base à zéro).
        clock.receive_at(Some(1), None, t0 + std::time::Duration::from_secs(5));
        assert_eq!(clock.ms_at(t0 + std::time::Duration::from_secs(10)), 20_000);
        // ×6 : une heure de jeu = 10 minutes réelles.
        clock.receive_at(Some(6), Some(3_600_000), t0);
        assert_eq!(hour_of_ms(clock.ms_at(t0 + std::time::Duration::from_secs(600))), 2);
    }

    #[test]
    fn game_clock_keeps_the_server_unix_time() {
        let t0 = Instant::now();
        let s = std::time::Duration::from_secs;
        let mut clock = GameClock::default();
        assert_eq!(clock.server_unix_at(t0), None);
        clock.receive_server_at(Some(1_000), Some(3_600), t0);
        assert_eq!(clock.server_unix_at(t0 + s(5)), Some(1_005));
        assert_eq!(clock.server_display_at(t0 + s(5)), Some(4_605));
        // Paquet sans champ 2 : valeurs gardées, mais le décompte repart (comme Archonia).
        clock.receive_server_at(None, None, t0 + s(5));
        assert_eq!(clock.server_unix_at(t0 + s(6)), Some(1_001));
        assert_eq!(clock.server_display_at(t0 + s(6)), Some(4_601));
    }

    #[test]
    fn no_client_means_no_region() {
        let assets = Assets::new(None, &[]);
        let mut z = Zones::default();
        assert_eq!(z.region_at(&assets, -478_000.0, -68_000.0), None);
        assert!(Zones::music_for_region(&assets, 9).is_empty());
        assert!(Zones::ambience_for_region(&assets, 9, 12).is_empty());
        assert!(z.audio_at(&assets, 0.0, 0.0, 12).is_none());
        assert!(assets.effect_sets().is_empty());
    }
}
