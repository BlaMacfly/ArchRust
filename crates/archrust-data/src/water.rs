//! Eau du monde (AgcmWater) : fichiers `world/water/wt%d.dat` et tables `WaterStatusT1.ini`,
//! `HWaterStatus.ini`.
//!
//! Relevé dans Archonia.exe (sha256 e9c7609b…) : lecture d'un secteur `FUN_14058d210`, écriture
//! (éditeur) `FUN_14058fc80`, tables `FUN_14058a7c0` (T1) et `FUN_140589a80` (HWater).
//!
//! ```text
//! wt{tuile}.dat (tuile = (sx/16)·100 + sy/16, comme a%06dx.ma2)
//!   u32 version (2 ; 1 pour 5 fichiers)
//!   256 × { u32 offset, u32 taille }   secteur (x, z) de la tuile en ((z·16 + x)·8 + 4)
//! bloc d'un secteur
//!   u32 n, n × 16 octets  : u32 statut, i16 x, i16 z, i16 largeur, i16 hauteur (cellules de 400), f32 y
//!   u32 m, m × 40 octets  : u32 statut, i16 x, i16 z (cellule), i32 demi-largeur, i32 demi-hauteur,
//!                           i32 décalage de vie (ms, +2000 en version 1), f32 angle (°), f32 vitesse,
//!                           f32 x, y, z (monde)
//! ```
//!
//! Le même index (version + 256 couples) sert aux fichiers d'herbe (voir [`crate::grass`]) :
//! [`sector_blocks`].
//!
//! Archonia ramène toujours l'option `Water` à un mode ≥ 2 (`FUN_1405909a0`) : l'eau à shaders
//! (`HWaterStatus.ini`) n'y est jamais dessinée. La table est lue ici pour mémoire et pour les outils.

use crate::ini::IniTable;

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum WorldFileError {
    #[error("fichier tronqué")]
    Truncated,
    #[error("bloc du secteur {0} hors du fichier")]
    BadBlock(usize),
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}
fn i16_at(b: &[u8], at: usize) -> Option<i16> {
    b.get(at..at + 2).map(|s| i16::from_le_bytes(s.try_into().unwrap()))
}
fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(f32::from_bits)
}

/// Version et blocs des 256 secteurs d'un fichier par tuile (`wt%d.dat`, `gr%d.dat`), indice
/// `z · 16 + x` (x, z de 0 à 15 dans la tuile).
pub fn sector_blocks(bytes: &[u8]) -> Result<(u32, Vec<&[u8]>), WorldFileError> {
    let version = u32_at(bytes, 0).ok_or(WorldFileError::Truncated)?;
    if bytes.len() < 4 + 256 * 8 {
        return Err(WorldFileError::Truncated);
    }
    let mut blocks = Vec::with_capacity(256);
    for k in 0..256 {
        let off = u32_at(bytes, 4 + 8 * k).ok_or(WorldFileError::Truncated)? as usize;
        let size = u32_at(bytes, 8 + 8 * k).ok_or(WorldFileError::Truncated)? as usize;
        blocks.push(bytes.get(off..off + size).ok_or(WorldFileError::BadBlock(k))?);
    }
    Ok((version, blocks))
}

/// Rectangle d'eau d'un secteur (en cellules de 400 unités).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterRect {
    /// Indice de `WaterStatusT1.ini` (et de `HWaterStatus.ini`).
    pub status: u32,
    pub x: i16,
    pub z: i16,
    pub width: i16,
    pub height: i16,
    /// Hauteur de la surface.
    pub y: f32,
}

/// Vague de rivage : quadrilatère qui avance, grandit et s'efface en boucle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub status: u32,
    pub cell_x: i16,
    pub cell_z: i16,
    /// Demi-étendues : `half_width` en Z, `half_height` en X du repère de la vague
    /// (`FUN_14058c200`).
    pub half_width: i32,
    pub half_height: i32,
    /// Temps de départ (ms) ; négatif = attente avant la première apparition.
    pub start_time: i32,
    /// Direction (degrés, autour de Y).
    pub angle: f32,
    pub speed: f32,
    pub position: [f32; 3],
}

/// Eau d'un secteur.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SectorWater {
    pub rects: Vec<WaterRect>,
    pub waves: Vec<Wave>,
}

/// Fichier `wt%d.dat` : eau des 256 secteurs d'une tuile, indice `z · 16 + x`.
#[derive(Debug, Clone, Default)]
pub struct WaterFile {
    pub version: u32,
    pub sectors: Vec<SectorWater>,
}

impl WaterFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, WorldFileError> {
        let (version, blocks) = sector_blocks(bytes)?;
        let mut sectors = Vec::with_capacity(256);
        for (k, b) in blocks.iter().enumerate() {
            let bad = || WorldFileError::BadBlock(k);
            let n = u32_at(b, 0).ok_or_else(bad)? as usize;
            let mut at = 4;
            let mut rects = Vec::with_capacity(n.min(256));
            for _ in 0..n {
                rects.push(WaterRect {
                    status: u32_at(b, at).ok_or_else(bad)?,
                    x: i16_at(b, at + 4).ok_or_else(bad)?,
                    z: i16_at(b, at + 6).ok_or_else(bad)?,
                    width: i16_at(b, at + 8).ok_or_else(bad)?,
                    height: i16_at(b, at + 10).ok_or_else(bad)?,
                    y: f32_at(b, at + 12).ok_or_else(bad)?,
                });
                at += 16;
            }
            let m = u32_at(b, at).ok_or_else(bad)? as usize;
            at += 4;
            let mut waves = Vec::with_capacity(m.min(4096));
            for _ in 0..m {
                let i = |o: usize| u32_at(b, at + o).ok_or(WorldFileError::BadBlock(k));
                let mut start_time = i(16)? as i32;
                if version == 1 {
                    start_time += 2000;
                }
                waves.push(Wave {
                    status: i(0)?,
                    cell_x: i16_at(b, at + 4).ok_or_else(bad)?,
                    cell_z: i16_at(b, at + 6).ok_or_else(bad)?,
                    half_width: i(8)? as i32,
                    half_height: i(12)? as i32,
                    start_time,
                    angle: f32::from_bits(i(20)?),
                    speed: f32::from_bits(i(24)?),
                    position: [f32::from_bits(i(28)?), f32::from_bits(i(32)?), f32::from_bits(i(36)?)],
                });
                at += 40;
            }
            // Archonia ignore les vagues de statut ≥ 20 (`FUN_14058d210`).
            waves.retain(|w| w.status < 20);
            sectors.push(SectorWater { rects, waves });
        }
        Ok(WaterFile { version, sectors })
    }

    /// Eau du secteur `(x, z)` de la tuile (0 à 15).
    pub fn sector(&self, x: usize, z: usize) -> Option<&SectorWater> {
        self.sectors.get(z * 16 + x)
    }
}

/// Réglage d'un type d'eau de `WaterStatusT1.ini` (0x74 octets en `+0x2320` d'AgcmWater).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaterStatusT1 {
    pub id: usize,
    /// `WTTYPE` : la surface n'est créée que pour 0 et 1 (`FUN_14058bcc0`).
    pub wt_type: i32,
    pub bmode_l1: i32,
    pub bop_l1: i32,
    pub render_bmode_l1: i32,
    /// Indice de `[WaterTextures]` de la couche 1.
    pub water_file_l1: i32,
    /// Couleur prélit des sommets (0–255) : `RED/GREEN/BLUE/ALPHA_L1`.
    pub color_l1: [u8; 4],
    /// Défilement des UV de la couche 1, ajouté à chaque image.
    pub uv_l1: [f32; 2],
    /// Une répétition de texture par `TILESIZE_L1` cellules.
    pub tile_size_l1: i32,
    pub bmode_l2: i32,
    /// Mélange de la seconde passe (valeurs RenderWare : 1 zéro, 2 un, 5 alpha source, 6 1 − alpha).
    pub blend_src_l2: i32,
    pub blend_dest_l2: i32,
    pub water_file_l2: i32,
    pub uv_l2: [f32; 2],
    /// Indice de `[WaveTextures]`.
    pub wave_file: i32,
    pub wave_width: i32,
    pub wave_height: i32,
    /// Durée d'un cycle de vague (ms).
    pub wave_lifetime: i32,
    pub wave_scale_x: f32,
    pub wave_scale_z: f32,
    pub wave_color: [u8; 3],
    pub wave_min_num: i32,
    pub wave_max_num: i32,
}

/// `WaterStatusT1.ini` : textures et réglages par type d'eau.
#[derive(Debug, Clone, Default)]
pub struct WaterStatusT1Table {
    /// `[WaterTextures]` (`Texture\Etc\`), par indice.
    pub water_textures: Vec<String>,
    /// `[WaveTextures]`.
    pub wave_textures: Vec<String>,
    pub statuses: Vec<WaterStatusT1>,
}

/// Liste `FN=i:fichier` d'une section (« none » = absente).
fn file_list(table: &IniTable, section: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(s) = table.section(section) else { return out };
    for v in s.get_all("FN") {
        let Some((i, name)) = v.split_once(':') else { continue };
        let Ok(i) = i.trim().parse::<usize>() else { continue };
        if out.len() <= i {
            out.resize(i + 1, String::new());
        }
        let name = name.trim();
        out[i] = if name.eq_ignore_ascii_case("none") { String::new() } else { name.to_string() };
    }
    out
}

/// Entrées de `[Setting]` regroupées par `ID=` (ordre du fichier).
fn settings_by_id(table: &IniTable) -> Vec<(usize, Vec<(String, String)>)> {
    let mut out: Vec<(usize, Vec<(String, String)>)> = Vec::new();
    let Some(s) = table.section("Setting") else { return out };
    for (k, v) in &s.entries {
        if k.eq_ignore_ascii_case("ID") {
            if let Ok(id) = v.trim().parse() {
                out.push((id, Vec::new()));
            }
        } else if let Some((_, list)) = out.last_mut() {
            list.push((k.to_ascii_uppercase(), v.trim().to_string()));
        }
    }
    out
}

fn num<T: std::str::FromStr + Default>(v: &str) -> T {
    v.trim().parse().unwrap_or_default()
}

impl WaterStatusT1Table {
    pub fn parse(table: &IniTable) -> Self {
        let mut statuses = Vec::new();
        for (id, entries) in settings_by_id(table) {
            let mut s = WaterStatusT1 { id, ..Default::default() };
            for (k, v) in entries {
                let v = v.as_str();
                match k.as_str() {
                    "WTTYPE" => s.wt_type = num(v),
                    "BMODE_L1" => s.bmode_l1 = num(v),
                    "BOP_L1" => s.bop_l1 = num(v),
                    "RENDERBMODE_L1" => s.render_bmode_l1 = num(v),
                    "WATERFILE_L1" => s.water_file_l1 = num(v),
                    "RED_L1" => s.color_l1[0] = num::<i32>(v).clamp(0, 255) as u8,
                    "GREEN_L1" => s.color_l1[1] = num::<i32>(v).clamp(0, 255) as u8,
                    "BLUE_L1" => s.color_l1[2] = num::<i32>(v).clamp(0, 255) as u8,
                    "ALPHA_L1" => s.color_l1[3] = num::<i32>(v).clamp(0, 255) as u8,
                    "U_L1" => s.uv_l1[0] = num(v),
                    "V_L1" => s.uv_l1[1] = num(v),
                    "TILESIZE_L1" => s.tile_size_l1 = num(v),
                    "BMODE_L2" => s.bmode_l2 = num(v),
                    "BLENDSRC_L2" => s.blend_src_l2 = num(v),
                    "BLENDDEST_L2" => s.blend_dest_l2 = num(v),
                    "WATERFILE_L2" => s.water_file_l2 = num(v),
                    "U_L2" => s.uv_l2[0] = num(v),
                    "V_L2" => s.uv_l2[1] = num(v),
                    "WAVEFILE" => s.wave_file = num(v),
                    "WAVEWIDTH" => s.wave_width = num(v),
                    "WAVEHEIGHT" => s.wave_height = num(v),
                    "WAVELIFETIME" => s.wave_lifetime = num(v),
                    "WAVESCALEX" => s.wave_scale_x = num(v),
                    "WAVESCALEZ" => s.wave_scale_z = num(v),
                    "WAVERED" => s.wave_color[0] = num::<i32>(v).clamp(0, 255) as u8,
                    "WAVEGREEN" => s.wave_color[1] = num::<i32>(v).clamp(0, 255) as u8,
                    "WAVEBLUE" => s.wave_color[2] = num::<i32>(v).clamp(0, 255) as u8,
                    "WAVEMINNUM" => s.wave_min_num = num(v),
                    "WAVEMAXNUM" => s.wave_max_num = num(v),
                    _ => {}
                }
            }
            statuses.push(s);
        }
        WaterStatusT1Table {
            water_textures: file_list(table, "WaterTextures"),
            wave_textures: file_list(table, "WaveTextures"),
            statuses,
        }
    }

    pub fn status(&self, id: u32) -> Option<&WaterStatusT1> {
        self.statuses.iter().find(|s| s.id == id as usize)
    }
}

/// Réglage d'un type d'eau de `HWaterStatus.ini` (0x98 octets en `+0x2ff8`) : eau à shaders,
/// jamais dessinée par Archonia (voir l'en-tête du module).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HWaterStatus {
    pub id: usize,
    /// Couleur du reflet spéculaire (`Red`, `Green`, `Blue`).
    pub specular: [f32; 3],
    /// Quatre vagues sinusoïdales : direction (X, Y de l'UV), amplitude, vitesse.
    pub dir_x: [f32; 4],
    pub dir_y: [f32; 4],
    pub height: [f32; 4],
    pub speed: [f32; 4],
    /// Défilement des deux cartes de bosses (`TexX0`, `TexX1`, `TexY0`, `TexY1`).
    pub tex_scroll: [f32; 4],
    pub reflection_weight: f32,
    /// Alpha selon la profondeur (eau − sol) : de `alpha_min` à `alpha_max` sur
    /// `alpha_decrease_height` unités.
    pub alpha_min: f32,
    pub alpha_max: f32,
    pub alpha_decrease_height: f32,
    pub wave_min: f32,
    pub wave_max: f32,
    pub wave_decrease_height: f32,
    pub vertex_per_tile: i32,
    pub bump_tex: i32,
    pub fresnel_tex: i32,
    pub base: [f32; 3],
    pub shader14: i32,
    pub shader11: i32,
}

/// `HWaterStatus.ini`.
#[derive(Debug, Clone, Default)]
pub struct HWaterStatusTable {
    pub bump_textures: Vec<String>,
    pub fresnel_textures: Vec<String>,
    pub statuses: Vec<HWaterStatus>,
}

fn four(v: &str) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (o, p) in out.iter_mut().zip(v.split(':')) {
        *o = num(p);
    }
    out
}

impl HWaterStatusTable {
    pub fn parse(table: &IniTable) -> Self {
        let mut statuses = Vec::new();
        for (id, entries) in settings_by_id(table) {
            let mut s = HWaterStatus { id, ..Default::default() };
            for (k, v) in entries {
                let v = v.as_str();
                match k.as_str() {
                    "RED" => s.specular[0] = num(v),
                    "GREEN" => s.specular[1] = num(v),
                    "BLUE" => s.specular[2] = num(v),
                    "DIRX" => s.dir_x = four(v),
                    "DIRY" => s.dir_y = four(v),
                    "HEIGHT" => s.height = four(v),
                    "SPEED" => s.speed = four(v),
                    "TEXX0" => s.tex_scroll[0] = num(v),
                    "TEXX1" => s.tex_scroll[1] = num(v),
                    "TEXY0" => s.tex_scroll[2] = num(v),
                    "TEXY1" => s.tex_scroll[3] = num(v),
                    "REFLECTIONWEIGHT" => s.reflection_weight = num(v),
                    "ALPHAMIN" => s.alpha_min = num(v),
                    "ALPHAMAX" => s.alpha_max = num(v),
                    "ALPHADECREASEHEIGHT" => s.alpha_decrease_height = num(v),
                    "WAVEMIN" => s.wave_min = num(v),
                    "WAVEMAX" => s.wave_max = num(v),
                    "WAVEDECREASEHEIGHT" => s.wave_decrease_height = num(v),
                    "VERTEXPERTILE" => s.vertex_per_tile = num(v),
                    "BUMPTEXID" => s.bump_tex = num(v),
                    "FRESNELTEXID" => s.fresnel_tex = num(v),
                    "BASER" => s.base[0] = num(v),
                    "BASEG" => s.base[1] = num(v),
                    "BASEB" => s.base[2] = num(v),
                    "SHADER14ID" => s.shader14 = num(v),
                    "SHADER11ID" => s.shader11 = num(v),
                    _ => {}
                }
            }
            statuses.push(s);
        }
        HWaterStatusTable {
            bump_textures: file_list(table, "BumpMap Textures"),
            fresnel_textures: file_list(table, "FresnelMap Textures"),
            statuses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(rects: &[(u32, [i16; 4], f32)], waves: usize) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&(rects.len() as u32).to_le_bytes());
        for (s, r, y) in rects {
            b.extend_from_slice(&s.to_le_bytes());
            for v in r {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&y.to_le_bytes());
        }
        b.extend_from_slice(&(waves as u32).to_le_bytes());
        for i in 0..waves {
            let mut w = vec![0u8; 40];
            w[0..4].copy_from_slice(&(if i == 1 { 25u32 } else { 0 }).to_le_bytes());
            w[4..6].copy_from_slice(&3i16.to_le_bytes());
            w[8..12].copy_from_slice(&160i32.to_le_bytes());
            w[16..20].copy_from_slice(&(-500i32).to_le_bytes());
            w[20..24].copy_from_slice(&45f32.to_le_bytes());
            w[36..40].copy_from_slice(&7f32.to_le_bytes());
            b.extend_from_slice(&w);
        }
        b
    }

    fn file(version: u32, blocks: &[(usize, Vec<u8>)]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut index = vec![(0u32, 0u32); 256];
        let empty = [0u8; 8];
        for k in 0..256 {
            let b = blocks.iter().find(|(i, _)| *i == k).map(|(_, b)| b.as_slice()).unwrap_or(&empty);
            index[k] = ((4 + 256 * 8 + data.len()) as u32, b.len() as u32);
            data.extend_from_slice(b);
        }
        let mut out = version.to_le_bytes().to_vec();
        for (o, s) in index {
            out.extend_from_slice(&o.to_le_bytes());
            out.extend_from_slice(&s.to_le_bytes());
        }
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn parses_rects_and_waves() {
        let raw = file(1, &[(16 * 2 + 5, block(&[(0, [15, 10, 1, 6], 2120.5)], 2))]);
        let f = WaterFile::parse(&raw).unwrap();
        assert_eq!(f.version, 1);
        let s = f.sector(5, 2).unwrap();
        assert_eq!(s.rects, vec![WaterRect { status: 0, x: 15, z: 10, width: 1, height: 6, y: 2120.5 }]);
        // Statut 25 écarté ; version 1 : +2000 ms.
        assert_eq!(s.waves.len(), 1);
        assert_eq!((s.waves[0].cell_x, s.waves[0].half_width, s.waves[0].start_time), (3, 160, 1500));
        assert_eq!((s.waves[0].angle, s.waves[0].position[2]), (45.0, 7.0));
        assert!(f.sector(0, 0).unwrap().rects.is_empty());
        assert_eq!(WaterFile::parse(&raw[..60]).unwrap_err(), WorldFileError::Truncated);
        assert!(WaterFile::parse(&raw[..3000]).is_err());
    }

    #[test]
    fn parses_status_tables() {
        let t1 = IniTable::parse(
            "[WaterTextures]\nNUM=2\nFN=0:water_nomal_01.bmp\nFN=1:none\n[WaveTextures]\nFN=0:water_wave.tif\n[Setting]\nMAX_ID_USED=9\nID=0\nWTTYPE=1\nRED_L1=110\nALPHA_L1=150\nU_L1=0.0002\nTILESIZE_L1=8\nBLENDSRC_L2=2\nWAVELIFETIME=7400\nID=3\nWTTYPE=0\n",
        );
        let t = WaterStatusT1Table::parse(&t1);
        assert_eq!(t.water_textures, vec!["water_nomal_01.bmp".to_string(), String::new()]);
        assert_eq!(t.statuses.len(), 2);
        let s = t.status(0).unwrap();
        assert_eq!((s.wt_type, s.color_l1, s.tile_size_l1, s.blend_src_l2, s.wave_lifetime), (1, [110, 0, 0, 150], 8, 2, 7400));
        assert!((s.uv_l1[0] - 0.0002).abs() < 1e-9);
        let h = HWaterStatusTable::parse(&IniTable::parse("[Setting]\nID=0\nDirX=2:0:-7:-8\nAlphaDecreaseHeight=150\nBaseR=0.11\n"));
        assert_eq!(h.statuses[0].dir_x, [2.0, 0.0, -7.0, -8.0]);
        assert_eq!((h.statuses[0].alpha_decrease_height, h.statuses[0].base[0]), (150.0, 0.11));
    }
}
