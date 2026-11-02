//! Distances des niveaux de détail (`loddistancetype.txt`) et sol grossier des secteurs (`R{sx},{sy}.dff`).
//!
//! Relevé dans Archonia.exe (sha256 e9c7609b…) :
//!
//! * **`loddistancetype.txt`** (texte à tabulations, chiffré comme les tables `ini`), lu par
//!   `FUN_1405570d0` : colonnes `default`, `type1`…`type9` (une colonne `type10` est ignorée),
//!   lignes `level 1`…`level 5`, valeurs entières (`atoi`), cases vides = 0.
//! * **Choix du niveau** (`FUN_140556460`) : premier niveau `i` tel que distance <
//!   `dist[type][i] × facteur` (facteur `MapView` des objets : 0,5 / 0,8 / 1,3 / 1,8), sinon le
//!   dernier ; le nombre de niveaux est `LOD_LEVEL` du modèle. Le niveau retombe ensuite sur le plus
//!   haut niveau dont le modèle a une géométrie : un modèle sans niveau > 0 garde la sienne.
//! * **Sol grossier** : `R{sx},{sy}.dff` (archive `a%06dx.ma2`) contient le dictionnaire de la
//!   texture 128 × 128 du secteur, puis `u32 1` et un Atomic autonome : 17 × 17 sommets, 512
//!   triangles, un jeu d'UV qui couvre le secteur, couleurs prélit et normales ; son second matériau
//!   porte la texture `R{sx},{sy}`. Archonia le dessine au-delà des secteurs détaillés.

use crate::rw::{RwError, SectorGeometry, TextureDictionary};

/// Nombre de types de distance lus (`default` + `type1`…`type9`) et de niveaux.
pub const LOD_TYPES: usize = 10;
pub const LOD_LEVELS: usize = 5;

/// Table `loddistancetype.txt` : `dist[type][niveau]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LodDistances {
    pub dist: [[u32; LOD_LEVELS]; LOD_TYPES],
}

impl LodDistances {
    /// Analyse le texte déchiffré.
    pub fn parse(text: &str) -> Self {
        let mut out = LodDistances::default();
        let mut lines = text.lines();
        let Some(header) = lines.next() else { return out };
        let columns: Vec<Option<usize>> = header
            .trim_end_matches('\r')
            .split('\t')
            .map(|c| {
                let c = c.trim().to_ascii_lowercase();
                if c == "default" {
                    Some(0)
                } else {
                    c.strip_prefix("type").and_then(|n| n.parse::<usize>().ok()).filter(|&n| (1..LOD_TYPES).contains(&n))
                }
            })
            .collect();
        for line in lines {
            let cells: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
            let Some(level) = cells.first().and_then(|c| c.trim().strip_prefix("level ")).and_then(|n| n.trim().parse::<usize>().ok()) else {
                continue;
            };
            if !(1..=LOD_LEVELS).contains(&level) {
                continue;
            }
            for (i, cell) in cells.iter().enumerate().skip(1) {
                if let Some(Some(t)) = columns.get(i) {
                    out.dist[*t][level - 1] = atoi(cell);
                }
            }
        }
        out
    }

    /// Niveau d'un modèle de `levels` niveaux et de type `kind` à `distance` (facteur `factor`).
    pub fn level(&self, kind: usize, levels: usize, distance: f32, factor: f32) -> usize {
        let levels = levels.clamp(1, LOD_LEVELS);
        let row = &self.dist[kind.min(LOD_TYPES - 1)];
        (0..levels).find(|&i| distance < row[i] as f32 * factor).unwrap_or(levels - 1)
    }
}

/// `atoi` : entier en tête de la chaîne (espaces sautés), 0 sinon.
fn atoi(s: &str) -> u32 {
    let digits: String = s.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

/// Sol grossier d'un secteur : texture et géométrie de `R{sx},{sy}.dff` (déjà déchiffré).
pub fn parse_rough_sector(data: &[u8]) -> Result<(TextureDictionary, Option<SectorGeometry>), RwError> {
    let dict = TextureDictionary::parse(data)?;
    if data.len() < 12 {
        return Ok((dict, None));
    }
    let txd_len = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize + 12;
    let geometry = data.get(txd_len..).filter(|rest| rest.len() > 16).map(SectorGeometry::parse).transpose()?;
    Ok((dict, geometry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_table_and_levels() {
        let text = "\tdefault\ttype1\ttype2\ttype10\nlevel 1\t4000\t10000\t15000\t15000\nlevel 2\t8000\t20000\t41000\t35000\nlevel 4\t\t42000\t\t\nlevel 5\t\t\t46000   \n\n\t기본값\n";
        let l = LodDistances::parse(text);
        assert_eq!(l.dist[0][..2], [4000, 8000]);
        assert_eq!(l.dist[1][3], 42000);
        assert_eq!(l.dist[2][4], 46000);
        assert_eq!(l.dist[9], [0; 5], "type10 ignoré");
        // default, 2 niveaux, facteur 0,8 : 4000 × 0,8 = 3200.
        assert_eq!(l.level(0, 2, 3000.0, 0.8), 0);
        assert_eq!(l.level(0, 2, 5000.0, 0.8), 1);
        assert_eq!(l.level(0, 2, 90000.0, 0.8), 1);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : la table du jeu.
    #[test]
    fn game_table() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let raw = std::fs::read(std::path::Path::new(&dir).join("ini/loddistancetype.txt")).unwrap();
        let l = LodDistances::parse(&crate::ini::decrypt_ini(&raw));
        assert_eq!(l.dist[0], [4000, 8000, 10000, 0, 0]);
        assert_eq!(l.dist[2][..3], [15000, 41000, 42000]);
        assert_eq!(l.dist[7], [10000, 15000, 20000, 45000, 46000]);
    }
}
