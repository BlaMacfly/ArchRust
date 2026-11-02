//! Herbe du monde (AgcmGrass) : fichiers `world/grass/gr%d.dat` et table `GrassTemplate.ini`.
//!
//! Relevé dans Archonia.exe (sha256 e9c7609b…) : gabarits `FUN_140594920`, touffes `FUN_140595000`,
//! secteurs `FUN_140594090`, instances `FUN_140593b40`, balancement `FUN_140592b40`, rendu
//! `FUN_140592730`.
//!
//! ```text
//! gr{tuile}.dat : même index que wt%d.dat (u32 version, 256 × {offset, taille}, voir water::sector_blocks)
//! bloc d'un secteur, mots de 32 bits
//!   u32 nombre de groupes
//!   groupe : u32 n, u32 sx, u32 sz (secteur ; > 800 = fin), u32 ?, f32 cx, cy, cz,
//!            f32 rayon (forcé à 700 en version 1), f32 y maximal
//!     n × { u32 gabarit, f32 x, y, z, f32 rotX, f32 rotY (degrés), f32 échelle }
//! ```
//!
//! La lecture d'Archonia est tolérante : un mot au-delà du bloc vaut 0 et un brin incomplet tronque
//! son groupe (cas du secteur 56 de `gr1928.dat`) ; ce lecteur fait de même.

use crate::ini::IniTable;
use crate::water::{sector_blocks, WorldFileError};

/// Brin d'herbe placé.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassBlade {
    pub template: u32,
    pub position: [f32; 3],
    /// Rotations (degrés) autour de X puis de Y ; Archonia écarte les valeurs > 360.
    pub rot_x: f32,
    pub rot_y: f32,
    pub scale: f32,
}

/// Groupe de brins (boîte de culling d'AgcmRender).
#[derive(Debug, Clone, PartialEq)]
pub struct GrassGroup {
    pub center: [f32; 3],
    pub radius: f32,
    pub max_y: f32,
    pub blades: Vec<GrassBlade>,
}

/// Fichier `gr%d.dat` : groupes des 256 secteurs d'une tuile, indice `z · 16 + x`.
#[derive(Debug, Clone, Default)]
pub struct GrassFile {
    pub version: u32,
    pub sectors: Vec<Vec<GrassGroup>>,
}

impl GrassFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, WorldFileError> {
        let (version, blocks) = sector_blocks(bytes)?;
        let sectors = blocks.iter().map(|b| parse_sector(b, version)).collect();
        Ok(GrassFile { version, sectors })
    }

    pub fn sector(&self, x: usize, z: usize) -> &[GrassGroup] {
        self.sectors.get(z * 16 + x).map(Vec::as_slice).unwrap_or(&[])
    }
}

fn parse_sector(b: &[u8], version: u32) -> Vec<GrassGroup> {
    let words = b.len() / 4;
    let w = |i: usize| -> u32 {
        if i < words {
            u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())
        } else {
            0
        }
    };
    let f = |i: usize| f32::from_bits(w(i));
    let count = if b.len() > 3 { w(0) } else { 0 };
    let mut groups = Vec::new();
    let mut i = 1;
    for _ in 0..count {
        let n = w(i) as usize;
        if w(i + 1) > 800 || w(i + 2) > 800 {
            break;
        }
        let mut g = GrassGroup {
            center: [f(i + 4), f(i + 5), f(i + 6)],
            radius: if version == 1 { 700.0 } else { f(i + 7) },
            max_y: f(i + 8),
            blades: Vec::new(),
        };
        i += 9;
        for _ in 0..n {
            if i + 7 > words {
                break;
            }
            g.blades.push(GrassBlade {
                template: w(i) & 0xffff,
                position: [f(i + 1), f(i + 2), f(i + 3)],
                rot_x: f(i + 4),
                rot_y: f(i + 5),
                scale: f(i + 6),
            });
            i += 7;
        }
        if !g.blades.is_empty() {
            groups.push(g);
        }
    }
    groups
}

/// Gabarit de `GrassTemplate.ini` (`[GRASSINFO]`, 0x78 octets par gabarit).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GrassTemplate {
    pub id: usize,
    pub name: String,
    /// Indice de `[Textures]`.
    pub texture: usize,
    /// 0 : croix ; 1 : forme en « ㄱ ».
    pub shape: i32,
    pub width: i32,
    pub height: i32,
    pub width_random: i32,
    pub height_random: i32,
    pub anim_type: i32,
    /// Inclinaison maximale du balancement (degrés).
    pub anim_amount: f32,
    /// `ANIM_SPEED` × 0,025 : degrés de phase par milliseconde.
    pub anim_speed: f32,
    /// Rectangle de l'image dans la texture (pixels).
    pub image: [i32; 4],
}

/// `GrassTemplate.ini`.
#[derive(Debug, Clone, Default)]
pub struct GrassTemplates {
    /// `[Textures]` (`TEXTURE_NAME=i:fichier`, dossier `Texture\Etc\`).
    pub textures: Vec<String>,
    /// Par `GRASS_ID` (trous éventuels : gabarit par défaut).
    pub templates: Vec<GrassTemplate>,
}

impl GrassTemplates {
    pub fn parse(table: &IniTable) -> Self {
        let mut textures: Vec<String> = Vec::new();
        if let Some(s) = table.section("Textures") {
            for v in s.get_all("TEXTURE_NAME") {
                let Some((i, name)) = v.split_once(':') else { continue };
                let Ok(i) = i.trim().parse::<usize>() else { continue };
                if textures.len() <= i {
                    textures.resize(i + 1, String::new());
                }
                textures[i] = name.trim().to_string();
            }
        }
        let mut templates: Vec<GrassTemplate> = Vec::new();
        let mut current: Option<usize> = None;
        if let Some(s) = table.section("GRASSINFO") {
            for (k, v) in &s.entries {
                let v = v.trim();
                let n = || v.parse::<i32>().unwrap_or(0);
                if k.eq_ignore_ascii_case("GRASS_ID") {
                    let Ok(id) = v.parse::<usize>() else { current = None; continue };
                    if templates.len() <= id {
                        templates.resize_with(id + 1, GrassTemplate::default);
                    }
                    templates[id].id = id;
                    current = Some(id);
                    continue;
                }
                let Some(t) = current.and_then(|c| templates.get_mut(c)) else { continue };
                match k.to_ascii_uppercase().as_str() {
                    "GRASS_NAME" => t.name = v.to_string(),
                    "TEXTURE_ID" => t.texture = n().max(0) as usize,
                    "SHAPE_TYPE" => t.shape = n(),
                    "GRASS_WIDTH" => t.width = n(),
                    "GRASS_HEIGHT" => t.height = n(),
                    "GRASS_WIDTH_RANDOM" => t.width_random = n(),
                    "GRASS_HEIGHT_RANDOM" => t.height_random = n(),
                    "ANIM_TYPE" => t.anim_type = n(),
                    "ANIM_AMOUNT" => t.anim_amount = n() as f32,
                    "ANIM_SPEED" => t.anim_speed = n() as f32 * 0.025,
                    "IMAGE_START_X" => t.image[0] = n(),
                    "IMAGE_START_Y" => t.image[1] = n(),
                    "IMAGE_WIDTH" => t.image[2] = n(),
                    "IMAGE_HEIGHT" => t.image[3] = n(),
                    _ => {}
                }
            }
        }
        GrassTemplates { textures, templates }
    }
}

/// Sommet d'une touffe : position (repère du brin) et UV.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Tirage `rand() % (2r) − r + base` d'Archonia (`r` nul : `base`).
fn jitter(base: i32, r: i32, rand: &mut impl FnMut() -> u32) -> f32 {
    let v = rand();
    if r <= 0 {
        return base as f32;
    }
    (v as i64 % (2 * r as i64) - r as i64 + base as i64) as f32
}

/// Les 12 sommets (4 triangles) d'une touffe, comme `FUN_140595000` : deux quadrilatères croisés
/// (forme 0) ou décalés en « ㄱ » (forme 1), tailles tirées une fois par gabarit. `tex_size` =
/// dimensions de la texture (pour les UV de `IMAGE_*`), `rand` = générateur (Archonia : `rand()`).
pub fn tuft_vertices(t: &GrassTemplate, tex_size: (u32, u32), mut rand: impl FnMut() -> u32) -> [GrassVertex; 12] {
    let (tw, th) = (tex_size.0.max(1) as f32, tex_size.1.max(1) as f32);
    let u0 = t.image[0] as f32 / tw;
    let u1 = (t.image[0] + t.image[2]) as f32 / tw;
    let v0 = t.image[1] as f32 / th;
    let v1 = (t.image[1] + t.image[3]) as f32 / th;
    let w1 = jitter(t.width, t.width_random, &mut rand);
    let h1 = jitter(t.height, t.height_random, &mut rand);
    let w2 = jitter(t.width, t.width_random, &mut rand);
    let h2 = jitter(t.height, t.height_random, &mut rand);
    let vx = |x: f32, y: f32, z: f32, u: f32, v: f32| GrassVertex { position: [x, y, z], uv: [u, v] };
    if t.shape == 1 {
        let skew = w1 * 0.2;
        let a = w1 * 0.05;
        let off = w1 * 0.15;
        let w2 = w2 * 0.9;
        let h2 = h2 * 0.8;
        let (b, half) = (w2 * 0.05, w2 * 0.5);
        let (zp, zn) = (off + half, off - half);
        [
            vx(w1 * 0.5, 0.0, -skew, u1, v1),
            vx(-w1 * 0.5, 0.0, 0.0, u0, v1),
            vx(w1 * 0.5, h1, 0.0, u1, v0),
            vx(w1 * 0.5, h1, 0.0, u1, v0),
            vx(-w1 * 0.5, 0.0, 0.0, u0, v1),
            vx(-w1 * 0.5, h1, skew, u0, v0),
            vx(b + a, 0.0, zp, u1, v1),
            vx(a, 0.0, zn, u0, v1),
            vx(a, h2, zp, u1, v0),
            vx(a, h2, zp, u1, v0),
            vx(a, 0.0, zn, u0, v1),
            vx(a - b, h2, zn, u0, v0),
        ]
    } else {
        let (s1, s2) = (w1 * 0.05, w2 * 0.05);
        [
            vx(w1 * 0.5, 0.0, -s1, u1, v1),
            vx(-w1 * 0.5, 0.0, 0.0, u0, v1),
            vx(w1 * 0.5, h1, 0.0, u1, v0),
            vx(w1 * 0.5, h1, 0.0, u1, v0),
            vx(-w1 * 0.5, 0.0, 0.0, u0, v1),
            vx(-w1 * 0.5, h1, s1, u0, v0),
            vx(-s2, 0.0, w2 * 0.5, u1, v1),
            vx(0.0, 0.0, -w2 * 0.5, u0, v1),
            vx(0.0, h2, w2 * 0.5, u1, v0),
            vx(0.0, h2, w2 * 0.5, u1, v0),
            vx(0.0, 0.0, -w2 * 0.5, u0, v1),
            vx(s2, h2, -w2 * 0.5, u0, v0),
        ]
    }
}

/// Facteurs d'échelle (x, y, z) d'un brin (`FUN_140593b40`) : x et z de 0,9 à 1,09, y de 0,8 à 1,19,
/// fois l'échelle du brin. `rand` = générateur (Archonia : `rand()`).
pub fn blade_scale(scale: f32, mut rand: impl FnMut() -> u32) -> [f32; 3] {
    let x = (rand() % 20) as f32 * 0.01 + 0.9;
    let y = (rand() % 40) as f32 * 0.01 + 0.8;
    let z = (rand() % 20) as f32 * 0.01 + 0.9;
    [x * scale, y * scale, z * scale]
}

/// Inclinaison du balancement (degrés) pour une phase (degrés) : table de 256 sinus d'Archonia
/// (`0x140f9c360`), indice `round(phase · 256 / 360) & 0xff`.
pub fn sway_tilt(phase_deg: f32, amount: f32) -> f32 {
    let i = (phase_deg * 0.711_111_07).round() as i64 & 0xff;
    (i as f32 * std::f32::consts::TAU / 256.0).sin() * amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerant_sector_reader() {
        let mut words: Vec<u32> = vec![2, 2, 324, 384, 3476];
        words.extend([-480800.0f32, 4280.0, -101600.0, 1400.0, 3858.0].map(f32::to_bits));
        for x in [-480594.0f32, -480792.0] {
            words.push(28);
            words.extend([x, 3843.0, -102399.0, 4.0, 81.0, 0.69].map(f32::to_bits));
        }
        // Second groupe annoncé mais tronqué : un brin entier puis un brin coupé.
        words.extend([3, 324, 384, 0]);
        words.extend([0.0f32, 0.0, 0.0, 700.0, 0.0].map(f32::to_bits));
        words.push(5);
        words.extend([1.0f32, 2.0, 3.0, 0.0, 0.0, 1.0].map(f32::to_bits));
        words.extend([6, 0]);
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let groups = parse_sector(&bytes, 2);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].blades.len(), 2);
        assert_eq!((groups[0].radius, groups[0].blades[1].template), (1400.0, 28));
        assert_eq!(groups[1].blades.len(), 1);
        assert_eq!(groups[1].blades[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(parse_sector(&bytes, 1)[0].radius, 700.0);
    }

    #[test]
    fn templates_and_tufts() {
        let t = IniTable::parse("[Textures]\nTEXTURE_NAME=0:GRASS_HU_01.tif\n[GRASSINFO]\nGRASS_GROUP=x\nGRASS_ID=1\nTEXTURE_ID=0\nSHAPE_TYPE=0\nGRASS_WIDTH=140\nGRASS_HEIGHT=100\nANIM_AMOUNT=5\nANIM_SPEED=3\nIMAGE_START_X=0\nIMAGE_START_Y=128\nIMAGE_WIDTH=128\nIMAGE_HEIGHT=128\n");
        let g = GrassTemplates::parse(&t);
        assert_eq!(g.textures, vec!["GRASS_HU_01.tif".to_string()]);
        let tpl = &g.templates[1];
        assert_eq!((tpl.width, tpl.anim_amount, tpl.image), (140, 5.0, [0, 128, 128, 128]));
        assert!((tpl.anim_speed - 0.075).abs() < 1e-6);
        let v = tuft_vertices(tpl, (256, 256), || 0);
        // Sans aléa (largeur et hauteur exactes) : quad A dans XY, quad B dans ZY.
        assert_eq!(v[0].position, [70.0, 0.0, -7.0]);
        assert_eq!(v[5].position, [-70.0, 100.0, 7.0]);
        assert_eq!(v[7].position, [0.0, 0.0, -70.0]);
        assert_eq!((v[0].uv, v[5].uv), ([0.5, 1.0], [0.0, 0.5]));
        assert_eq!(blade_scale(2.0, || 0), [1.8, 1.6, 1.8]);
        assert!((sway_tilt(90.0, 5.0) - 5.0).abs() < 1e-4);
        assert!(sway_tilt(0.0, 5.0).abs() < 1e-6);
    }
}
