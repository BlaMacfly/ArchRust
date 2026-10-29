//! Catalogue des IDs d'assets du client : la liste de ce qu'un pack peut remplacer.
//!
//! L'ID d'un asset est le nom que les tables et les modèles du client utilisent déjà :
//! * **texture** : nom RenderWare sans extension (`a0057aab`), quel que soit le dossier du client
//!   (`texture/character`, `texture/world/high`…) ; les cartes de couleurs des secteurs s'appellent
//!   `R{x},{y}` (non listées : une par secteur) ;
//! * **image lue par chemin** (interface, minicarte) : nom du fichier avec son extension (`ac_base.png`
//!   dans `texture/ui`, `map1728a.png` dans `texture/minimap`) ;
//! * **son** : chemin sous `sound/` (`bgm/hft_001.mp3`, `ui/click.wav`, `effect/sd0000.wav` pour les
//!   effets rangés dans les packs `.pk` d'après `soundpak.info`).
//!
//! [`write`] écrit `textures.csv` et `sons.csv` (séparateur `;`, UTF-8) : ID, famille, source dans
//! le client, taille en octets.

use crate::assets::Assets;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Dossiers de textures du client (comme `assets::texture_search_path`), sans les qualités.
const TEXTURE_DIRS: &[&str] = &[
    "texture/character",
    "texture/object",
    "texture/world",
    "texture/effect",
    "texture/skill",
    "texture/item",
    "texture/etc",
    "texture/worldmap",
    "texture/notpacked",
    "texture/amboccl",
];
const QUALITIES: &[&str] = &["", "/high", "/medium", "/low"];
/// Dossiers de sons servis tels quels (pack ou disque).
const SOUND_DIRS: &[&str] = &["sound/bgm", "sound/stream", "sound/ui"];

/// Un asset du client.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetId {
    pub id: String,
    /// Famille lisible (`character`, `world`, `ui`, `bgm`, `effect`…).
    pub family: String,
    /// Où le client le range (`texture/world/high`, `sound/effect/sd.pk`…), plusieurs séparées par `,`.
    pub source: String,
    pub size: u64,
}

/// Entrées d'un dossier du client : pack (`Reference.Dat`) s'il y en a un, sinon fichiers du disque.
fn listing(assets: &Assets, dir: &str) -> Vec<(String, u64)> {
    if let Some(entries) = assets.pack_listing(dir) {
        return entries;
    }
    let Some(game) = &assets.game_dir else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(game.join(dir)) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| (e.file_name().to_string_lossy().to_ascii_lowercase(), e.metadata().map(|m| m.len()).unwrap_or(0)))
        .filter(|(n, _)| !n.ends_with(".dat"))
        .collect()
}

fn family(dir: &str) -> String {
    dir.split('/').nth(1).unwrap_or(dir).to_string()
}

/// Textures et images d'interface du client, triées par ID.
pub fn textures(assets: &Assets) -> Vec<AssetId> {
    let mut by_id: BTreeMap<String, AssetId> = BTreeMap::new();
    let mut add = |id: String, family: String, source: &str, size: u64| {
        let e = by_id.entry(id.clone()).or_insert_with(|| AssetId { id, family, source: String::new(), size });
        if !e.source.split(',').any(|s| s == source) {
            if !e.source.is_empty() {
                e.source.push(',');
            }
            e.source.push_str(source);
        }
    };
    for base in TEXTURE_DIRS {
        for q in QUALITIES {
            let dir = format!("{base}{q}");
            for (name, size) in listing(assets, &dir) {
                let Some((stem, ext)) = name.rsplit_once('.') else { continue };
                if matches!(ext, "tx1" | "png" | "dds" | "bmp" | "tga" | "tif" | "jpg") {
                    add(stem.to_string(), family(base), &dir, size);
                }
            }
        }
    }
    // Images lues par chemin : leur ID garde l'extension.
    for dir in ["texture/ui", "texture/minimap"] {
        for (name, size) in listing(assets, dir) {
            add(name, family(dir), dir, size);
        }
    }
    by_id.into_values().collect()
}

/// Sons du client (musiques, ambiances, interface, effets des packs `.pk`), triés par ID.
pub fn sounds(assets: &Assets) -> Vec<AssetId> {
    let mut out = Vec::new();
    for dir in SOUND_DIRS {
        for (name, size) in listing(assets, dir) {
            if name.ends_with(".wav") || name.ends_with(".mp3") {
                let id = format!("{}/{name}", dir.trim_start_matches("sound/"));
                out.push(AssetId { id, family: family(dir), source: dir.to_string(), size });
            }
        }
    }
    if let Ok(bytes) = assets.read_file("sound/soundpak.info") {
        if let Ok(index) = archrust_data::soundpak::SoundIndex::parse(&bytes) {
            for pack in index.groups.iter().flat_map(|g| &g.packs) {
                for e in &pack.entries {
                    out.push(AssetId {
                        id: format!("effect/{}", e.name.to_ascii_lowercase()),
                        family: "effect".into(),
                        source: format!("sound/effect/{}", pack.file.to_ascii_lowercase()),
                        size: e.size as u64,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn csv(rows: &[AssetId]) -> String {
    let mut s = String::from("id;famille;source;octets\r\n");
    for r in rows {
        s.push_str(&format!("{};{};{};{}\r\n", r.id, r.family, r.source, r.size));
    }
    s
}

/// Écrit `textures.csv` et `sons.csv` dans `out` ; rend le nombre de textures et de sons.
pub fn write(assets: &Assets, out: &Path) -> Result<(usize, usize)> {
    std::fs::create_dir_all(out).with_context(|| format!("dossier {}", out.display()))?;
    let (tex, snd) = (textures(assets), sounds(assets));
    std::fs::write(out.join("textures.csv"), csv(&tex)).context("textures.csv")?;
    std::fs::write(out.join("sons.csv"), csv(&snd)).context("sons.csv")?;
    Ok((tex.len(), snd.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_lists_ids() {
        let rows = [AssetId { id: "a0057aab".into(), family: "world".into(), source: "texture/world,texture/world/high".into(), size: 42 }];
        assert_eq!(csv(&rows), "id;famille;source;octets\r\na0057aab;world;texture/world,texture/world/high;42\r\n");
        assert_eq!(family("texture/world/high"), "world");
        assert_eq!(family("sound/bgm"), "bgm");
    }

    #[test]
    fn no_client_gives_empty_catalogue() {
        let assets = Assets::new(None, &[]);
        assert!(textures(&assets).is_empty());
        assert!(sounds(&assets).is_empty());
    }
}
