//! Dossiers d'assets déposés par les joueurs et les développeurs : `assets/textures/` et
//! `assets/sounds/`, à côté de l'exécutable.
//!
//! Chaque dossier accepte, sans configuration :
//! * des fichiers isolés, regroupés dans un pack implicite (`assets-textures`, `assets-sounds`) ;
//! * des dossiers et des archives `.zip` / `.ahp`, un pack chacun.
//!
//! Un pack qui contient un `patch.toml` suit son manifeste (voir la documentation de la crate).
//! Sans manifeste, **le nom du fichier est l'ID de l'asset** :
//! * texture : nom du fichier sans extension, le nom RenderWare utilisé par les tables du client
//!   (`a0057aab.png` → texture `a0057aab`, `R325,389.png` → carte de couleurs du secteur) ; les
//!   sous-dossiers ne servent qu'à ranger ;
//! * image d'interface ou de minicarte : nom du fichier avec son extension (`ac_base.png` →
//!   `texture/ui/ac_base.png`, `map1728a.png` → `texture/minimap/map1728a.png`) ;
//! * son : chemin sous `sound/` (`ui/click.wav` → `sound/ui/click.wav`) ; un son posé à la racine du
//!   pack remplace ce nom dans n'importe quel dossier (`click.wav` → `sound/*/click.wav`).
//!
//! [`signature`] résume l'état des dossiers (noms, tailles, dates) pour détecter un changement et
//! recharger les packs en cours de jeu.

use crate::manifest::{Category, FileOverride, Manifest, PatchInfo, SoundOverride, TextureOverride};
use crate::stack::{LoadedPatch, PatchSource, PatchStack};
use crate::PatchError;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Extensions reconnues comme textures (formats lus par le client).
pub const TEXTURE_EXTENSIONS: &[&str] = &["png", "dds", "bmp", "jpg", "jpeg", "tga", "tif", "tiff"];
/// Extensions reconnues comme sons.
pub const SOUND_EXTENSIONS: &[&str] = &["wav", "mp3"];

/// Dossiers de sons du client : dans `assets/sounds/`, un sous-dossier qui porte l'un de ces noms
/// fait partie du chemin des sons (`ui/click.wav` → `sound/ui/click.wav`), ce n'est pas un pack.
pub const CLIENT_SOUND_DIRS: &[&str] = &["bgm", "effect", "stream", "ui"];

/// Sous-dossiers de `assets/` et leur catégorie.
pub const ASSET_FOLDERS: [(&str, Category); 2] = [("textures", Category::Graphics), ("sounds", Category::Sounds)];

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn is_archive(path: &Path) -> bool {
    path.extension()
        .map(|x| x.eq_ignore_ascii_case("ahp") || x.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// Manifeste déduit des fichiers d'un pack sans `patch.toml` (chemins relatifs au pack, `/`).
pub fn synthesize<'a>(id: &str, name: &str, category: Category, files: impl IntoIterator<Item = &'a str>) -> Manifest {
    let mut m = Manifest {
        patch: PatchInfo {
            id: id.to_string(),
            name: name.to_string(),
            category: Some(category),
            version: String::new(),
            author: String::new(),
            priority: 0,
            description: "Pack sans manifeste : chaque fichier remplace l'asset qui porte son nom".into(),
            min_archrust: String::new(),
            homepage: String::new(),
        },
        textures: Vec::new(),
        models: Vec::new(),
        files: Vec::new(),
        shaders: Vec::new(),
        sounds: Vec::new(),
    };
    for rel in files {
        let rel = rel.replace('\\', "/");
        let Some(ext) = extension(&rel) else { continue };
        let file_name = rel.rsplit('/').next().unwrap_or(&rel);
        match category {
            Category::Graphics if TEXTURE_EXTENSIONS.contains(&ext.as_str()) => {
                let stem = &file_name[..file_name.len() - ext.len() - 1];
                m.textures.push(TextureOverride { pattern: stem.to_string(), file: rel.clone(), scale: 1.0 });
                // Les images lues par chemin (interface, minicarte) le sont sous leur nom complet.
                m.files.push(FileOverride { path: format!("texture/*/{file_name}"), file: rel.clone() });
            }
            Category::Sounds if SOUND_EXTENSIONS.contains(&ext.as_str()) => {
                let path = if rel.contains('/') { rel.clone() } else { format!("*/{rel}") };
                m.sounds.push(SoundOverride { path, file: rel.clone() });
            }
            _ => {}
        }
    }
    m.textures.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    m.sounds.sort_by(|a, b| a.path.cmp(&b.path));
    m
}

/// Fichiers d'un dossier, relatifs à lui ; `depth` = 1 pour la racine seule.
fn files_in(dir: &Path, depth: usize) -> Vec<String> {
    walkdir::WalkDir::new(dir)
        .max_depth(depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.path().strip_prefix(dir).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
        .collect()
}

fn loaded(manifest: Manifest, source: PatchSource) -> LoadedPatch {
    let priority = manifest.patch.priority;
    let mut lp = LoadedPatch { manifest, source, enabled: true, problems: Vec::new(), official: false, priority };
    lp.check();
    lp
}

/// Un pack du dossier : son manifeste s'il en a un, sinon un manifeste déduit de ses fichiers.
fn load_entry(path: &Path, category: Category) -> Result<Option<LoadedPatch>, PatchError> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if path.is_dir() {
        if path.join("patch.toml").is_file() {
            return PatchStack::load_one(path).map(Some);
        }
        let files = files_in(path, usize::MAX);
        let m = synthesize(&name, &name, category, files.iter().map(String::as_str));
        if m.textures.is_empty() && m.sounds.is_empty() {
            return Ok(None);
        }
        return Ok(Some(loaded(m, PatchSource::Dir(path.to_path_buf()))));
    }
    if !is_archive(path) {
        return Ok(None);
    }
    let f = File::open(path).map_err(|e| PatchError::Io(path.to_path_buf(), e))?;
    let zip = zip::ZipArchive::new(f).map_err(|e| PatchError::Zip(path.to_path_buf(), e))?;
    if zip.file_names().any(|n| n.eq_ignore_ascii_case("patch.toml")) {
        return PatchStack::load_one(path).map(Some);
    }
    let stem = name.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(name);
    let files: Vec<String> = zip.file_names().filter(|n| !n.ends_with('/')).map(str::to_string).collect();
    let m = synthesize(&stem, &stem, category, files.iter().map(String::as_str));
    if m.textures.is_empty() && m.sounds.is_empty() {
        return Ok(None);
    }
    Ok(Some(loaded(m, PatchSource::Archive(path.to_path_buf()))))
}

/// Packs d'un dossier d'assets (`assets/textures` ou `assets/sounds`) : un par sous-dossier ou
/// archive, plus un pack implicite pour les fichiers posés directement dans le dossier.
pub fn load_asset_dir(dir: &Path, category: Category) -> Vec<LoadedPatch> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    let client_sound_dir = |p: &Path| {
        category == Category::Sounds
            && p.is_dir()
            && p.file_name().is_some_and(|n| CLIENT_SOUND_DIRS.iter().any(|d| n.to_string_lossy().eq_ignore_ascii_case(d)))
    };
    for p in &entries {
        if p.is_file() && !is_archive(p) || client_sound_dir(p) {
            continue;
        }
        match load_entry(p, category) {
            Ok(Some(lp)) => out.push(lp),
            Ok(None) => {}
            Err(e) => log::warn!("assets : {} ignoré : {e}", p.display()),
        }
    }
    let folder = dir.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut loose = files_in(dir, 1);
    for sub in entries.iter().filter(|p| client_sound_dir(p)) {
        let name = sub.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        loose.extend(files_in(sub, usize::MAX).into_iter().map(|f| format!("{name}/{f}")));
    }
    let m = synthesize(&format!("assets-{folder}"), &format!("Fichiers de assets/{folder}"), category, loose.iter().map(String::as_str));
    if !m.textures.is_empty() || !m.sounds.is_empty() {
        out.push(loaded(m, PatchSource::Dir(dir.to_path_buf())));
    }
    for lp in &out {
        for problem in &lp.problems {
            log::warn!("pack {} : {problem}", lp.manifest.patch.id);
        }
    }
    out
}

/// Notice déposée dans `assets/` à sa création.
const NOTICE: &str = "\
Dossier des assets d'ArchRust / ArchRust assets folder
=======================================================

textures/   textures (PNG, DDS, BMP, JPEG, TGA, TIFF)
sounds/     sons / sounds (WAV, MP3)

FR : déposez ici un fichier, un dossier ou une archive .zip / .ahp. Le jeu les applique tout seul,
     même en cours de partie. Sans patch.toml, le nom du fichier est l'ID de l'asset remplacé :
       textures/a0057aab.png     remplace la texture a0057aab
       sounds/ui/click.wav       remplace le son sound/ui/click.wav
     Liste des IDs : archrust assets catalogue <dossier>. Guide : https://github.com/BlaMacfly/ArchRust/wiki

EN: drop a file, a folder or a .zip / .ahp archive here. The game applies them on its own, even
    while playing. Without patch.toml, the file name is the ID of the replaced asset:
       textures/a0057aab.png     replaces texture a0057aab
       sounds/ui/click.wav       replaces sound sound/ui/click.wav
    List of IDs: archrust assets catalogue <folder>. Guide: https://github.com/BlaMacfly/ArchRust/wiki
";

/// Crée `assets/textures/` et `assets/sounds/` (et la notice `LISEZMOI.txt`) s'ils manquent.
pub fn ensure_asset_dirs(root: &Path) -> std::io::Result<()> {
    for (sub, _) in ASSET_FOLDERS {
        std::fs::create_dir_all(root.join(sub))?;
    }
    let notice = root.join("LISEZMOI.txt");
    if !notice.exists() {
        std::fs::write(notice, NOTICE.replace('\n', "\r\n"))?;
    }
    Ok(())
}

/// Empreinte des dossiers (chemins, tailles, dates de modification, jusqu'à 6 niveaux) : elle change
/// dès qu'un fichier, dossier ou archive est ajouté, modifié ou retiré.
pub fn signature(dirs: &[PathBuf]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for dir in dirs {
        dir.hash(&mut h);
        for e in walkdir::WalkDir::new(dir).max_depth(6).sort_by_file_name().into_iter().filter_map(|e| e.ok()) {
            e.path().hash(&mut h);
            if let Ok(meta) = e.metadata() {
                meta.len().hash(&mut h);
                if let Ok(t) = meta.modified() {
                    t.hash(&mut h);
                }
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::ResourceKind;
    use std::io::Write;

    #[test]
    fn file_names_are_asset_ids() {
        let m = synthesize("p", "P", Category::Graphics, ["sol/A0057AAB.png", "R325,389.dds", "lisezmoi.txt", "x.wav"]);
        let ids: Vec<_> = m.textures.iter().map(|t| (t.pattern.as_str(), t.file.as_str())).collect();
        assert_eq!(ids, [("A0057AAB", "sol/A0057AAB.png"), ("R325,389", "R325,389.dds")]);
        assert!(m.files.iter().any(|f| f.path == "texture/*/A0057AAB.png" && f.file == "sol/A0057AAB.png"));
        let s = synthesize("s", "S", Category::Sounds, ["ui/click.wav", "bip.mp3", "image.png"]);
        let paths: Vec<_> = s.sounds.iter().map(|x| x.client_path()).collect();
        assert_eq!(paths, ["sound/*/bip.mp3", "sound/ui/click.wav"]);
    }

    #[test]
    fn folders_archives_and_loose_files() {
        let tmp = tempfile::tempdir().unwrap();
        let tex = tmp.path().join("textures");
        std::fs::create_dir_all(tex.join("mon-pack/sol")).unwrap();
        std::fs::write(tex.join("mon-pack/sol/a0057aab.png"), b"pack").unwrap();
        std::fs::write(tex.join("b00060aa.png"), b"vrac").unwrap();
        let mut zip = zip::ZipWriter::new(File::create(tex.join("zippe.zip")).unwrap());
        zip.start_file("c0000001.png", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"zip").unwrap();
        zip.finish().unwrap();
        let mut stack = PatchStack::new();
        for p in load_asset_dir(&tex, Category::Graphics) {
            stack.push(p);
        }
        let read = |name: &str| stack.read(ResourceKind::Texture, name).map(|r| r.unwrap().0);
        assert_eq!(read("A0057AAB").as_deref(), Some(&b"pack"[..]));
        assert_eq!(read("b00060aa").as_deref(), Some(&b"vrac"[..]));
        assert_eq!(read("c0000001").as_deref(), Some(&b"zip"[..]));
        let ids: Vec<_> = stack.patches.iter().map(|p| p.manifest.patch.id.as_str()).collect();
        assert!(ids.contains(&"assets-textures") && ids.contains(&"mon-pack") && ids.contains(&"zippe"), "{ids:?}");

        let snd = tmp.path().join("sounds");
        std::fs::create_dir_all(snd.join("bruits/ui")).unwrap();
        std::fs::write(snd.join("bruits/ui/click.wav"), b"clic").unwrap();
        std::fs::write(snd.join("bip.wav"), b"bip").unwrap();
        std::fs::create_dir_all(snd.join("bgm")).unwrap();
        std::fs::write(snd.join("bgm/hft_001.mp3"), b"musique").unwrap();
        let mut sounds = PatchStack::new();
        for p in load_asset_dir(&snd, Category::Sounds) {
            sounds.push(p);
        }
        let file = |path: &str| sounds.candidates(ResourceKind::File, path).first().map(|r| sounds.read_resolved(r).unwrap());
        assert_eq!(file("sound/ui/click.wav").as_deref(), Some(&b"clic"[..]));
        assert_eq!(file("sound/effect/bip.wav").as_deref(), Some(&b"bip"[..]));
        assert!(file("sound/ui/autre.wav").is_none());
        // `bgm/` est un dossier du client, pas un pack : son chemin compte.
        assert_eq!(file("sound/bgm/hft_001.mp3").as_deref(), Some(&b"musique"[..]));
        assert!(file("sound/stream/hft_001.mp3").is_none());
        assert!(!sounds.patches.iter().any(|p| p.manifest.patch.id == "bgm"));
    }

    #[test]
    fn signature_follows_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = [tmp.path().to_path_buf()];
        let a = signature(&dirs);
        assert_eq!(a, signature(&dirs));
        std::fs::write(tmp.path().join("a0057aab.png"), b"x").unwrap();
        let b = signature(&dirs);
        assert_ne!(a, b);
        std::fs::remove_file(tmp.path().join("a0057aab.png")).unwrap();
        assert_ne!(b, signature(&dirs));
    }
}
