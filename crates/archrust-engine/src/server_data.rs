//! Données du serveur : tables de valeurs écrites par le serveur (`archcore export-client-tables
//! <dossier>`), affichées par ArchRust à la place de celles du client.
//!
//! Les valeurs de jeu (objets, compétences, options, modèles, régions, portails…) sont celles du
//! serveur. ArchCore les écrit depuis sa base au format exact des tables du client (mêmes noms de
//! fichiers et en-têtes, texte en clair, fins de ligne CRLF), pour `ini/` et pour chaque dossier de
//! langue qui a la table (`ini_fr/`, `ini_de/`…).
//!
//! **Couche prioritaire** : [`crate::assets::Assets::read_file`] consulte ce dossier avant les packs de
//! ressources (`patches/`, `free-assets/`) et les données du jeu. Aucun pack ne peut le désactiver ni le
//! recouvrir. Seuls les chemins des dossiers de tables (`ini/…`, `ini_<langue>/…`) sont servis : les
//! packs gardent l'apparence (textures, modèles, sons), le serveur donne les valeurs.
//!
//! **Emplacement**, le premier qui s'applique :
//! 1. l'option `--server-data <dossier>` ;
//! 2. la variable d'environnement `ARCHRUST_SERVER_DATA` ;
//! 3. un dossier `server-data/` à côté de `realmlist.wtf`, cherché dans les mêmes dossiers que lui
//!    (exécutable, dossier courant, dossier `Game`), voir [`crate::realmlist::search_dirs`].
//!
//! **Empreinte** : l'exporteur écrit `tables.toml` à la racine du dossier (`fingerprint = "…"`, chaîne
//! opaque, et des champs facultatifs `format`, `software`, `generated`). Le serveur monde annonce la même
//! chaîne dans son `HELLO_ACK` (capacité `client-tables/1` des extensions ArchCore, champ 21). Si elles
//! diffèrent, ou si le dossier manque alors que le serveur en annonce une, ArchRust l'écrit dans le
//! journal ([`check`]) ; aucune fenêtre n'est ajoutée.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Nom du dossier cherché à côté de `realmlist.wtf`.
pub const DIR_NAME: &str = "server-data";
/// Variable d'environnement qui donne le dossier.
pub const ENV_VAR: &str = "ARCHRUST_SERVER_DATA";
/// Fichier de l'empreinte, à la racine du dossier.
pub const MANIFEST_FILE: &str = "tables.toml";
/// Version du format de `tables.toml` comprise par ArchRust.
pub const MANIFEST_FORMAT: u32 = 1;

/// Contenu de `tables.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    /// Version du format (1) ; absente = 1.
    #[serde(default)]
    pub format: u32,
    /// Empreinte des tables exportées (chaîne opaque, comparée sans tenir compte de la casse).
    #[serde(default)]
    pub fingerprint: String,
    /// Logiciel qui a écrit le dossier (`ArchCore 0.2.0`), pour le journal.
    #[serde(default)]
    pub software: String,
    /// Date de l'export, pour le journal.
    #[serde(default)]
    pub generated: String,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest> {
        let m: Manifest = toml::from_str(text).map_err(|e| anyhow!("{MANIFEST_FILE} : {e}"))?;
        if m.format > MANIFEST_FORMAT {
            log::warn!("{MANIFEST_FILE} : format {} plus récent que celui d'ArchRust ({MANIFEST_FORMAT}), lu quand même", m.format);
        }
        Ok(m)
    }
}

/// Dossier des données du serveur ouvert.
#[derive(Debug, Clone)]
pub struct ServerData {
    pub dir: PathBuf,
    /// `tables.toml`, `None` s'il manque ou s'il est illisible.
    pub manifest: Option<Manifest>,
}

/// Vrai si `rel_path` (chemin du client) est dans un dossier de tables : `ini\…` ou `ini_<langue>\…`.
pub fn covers(rel_path: &str) -> bool {
    let p = crate::assets::normalize_client_path(rel_path);
    match p.split_once('\\') {
        Some((dir, file)) => (dir == "ini" || dir.starts_with("ini_")) && !file.is_empty(),
        None => false,
    }
}

impl ServerData {
    /// Ouvre le dossier et lit son empreinte (absente ou illisible : signalée dans le journal).
    pub fn open(dir: &Path) -> ServerData {
        let manifest = match std::fs::read_to_string(dir.join(MANIFEST_FILE)) {
            Ok(text) => match Manifest::parse(&text) {
                Ok(m) => Some(m),
                Err(e) => {
                    log::warn!("données du serveur {} : {e:#}", dir.display());
                    None
                }
            },
            Err(_) => {
                log::warn!("données du serveur {} : pas de {MANIFEST_FILE}, empreinte inconnue", dir.display());
                None
            }
        };
        ServerData { dir: dir.to_path_buf(), manifest }
    }

    /// Empreinte de `tables.toml` (`None` sans fichier ou si elle est vide).
    pub fn fingerprint(&self) -> Option<&str> {
        self.manifest.as_ref().map(|m| m.fingerprint.trim()).filter(|f| !f.is_empty())
    }

    /// Fichier `rel_path` (`ini/itemdatatable.txt`, `ini_fr\\SKILL_CONST.txt`…) s'il est dans le dossier.
    /// Casse ignorée comme sous Windows ; hors des dossiers de tables, ou avec `..` : `None`.
    pub fn read(&self, rel_path: &str) -> Result<Option<Vec<u8>>> {
        if !covers(rel_path) {
            return Ok(None);
        }
        let rel = crate::assets::normalize_client_path(rel_path);
        if rel.split('\\').any(|c| c == "..") {
            return Ok(None);
        }
        match crate::assets::find_case_insensitive(&self.dir, &rel) {
            Some(path) => Ok(Some(std::fs::read(&path).map_err(|e| anyhow!("{} : {e}", path.display()))?)),
            None => Ok(None),
        }
    }

    /// Nombre de fichiers par dossier de tables, pour le journal (`ini 24, ini_fr 9`).
    pub fn summary(&self) -> String {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return String::new() };
        let mut dirs: Vec<(String, usize)> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                (name == "ini" || name.starts_with("ini_")).then(|| {
                    let n = std::fs::read_dir(e.path()).map(|r| r.flatten().filter(|f| f.file_type().is_ok_and(|t| t.is_file())).count()).unwrap_or(0);
                    (name, n)
                })
            })
            .collect();
        dirs.sort();
        dirs.iter().map(|(d, n)| format!("{d} {n}")).collect::<Vec<_>>().join(", ")
    }
}

/// Dossier des données du serveur : `explicit` (option `--server-data`), sinon `ARCHRUST_SERVER_DATA`,
/// sinon le premier `server-data/` des dossiers de `realmlist.wtf`. Un dossier donné mais absent est
/// signalé et ignoré.
pub fn find_dir(explicit: Option<&Path>, game_dir: Option<&Path>) -> Option<PathBuf> {
    let env = std::env::var_os(ENV_VAR).filter(|v| !v.is_empty()).map(PathBuf::from);
    find_dir_in(explicit, env.as_deref(), &crate::realmlist::search_dirs(game_dir))
}

fn find_dir_in(explicit: Option<&Path>, env: Option<&Path>, search: &[PathBuf]) -> Option<PathBuf> {
    for (what, dir) in [("--server-data", explicit), (ENV_VAR, env)] {
        if let Some(d) = dir {
            if d.is_dir() {
                return Some(d.to_path_buf());
            }
            log::warn!("données du serveur ({what}) : {} n'est pas un dossier, ignoré", d.display());
        }
    }
    search.iter().map(|d| d.join(DIR_NAME)).find(|d| d.is_dir())
}

/// Résultat de la comparaison des empreintes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Même empreinte des deux côtés.
    Same,
    /// Le serveur n'annonce pas d'empreinte (serveur sans la capacité `client-tables`).
    NotAnnounced,
    /// Le serveur annonce une empreinte mais il n'y a pas de dossier de données du serveur.
    MissingDir { server: String },
    /// Le dossier existe sans empreinte lisible.
    NoManifest { server: String },
    /// Empreintes différentes : tables locales périmées ou d'un autre serveur.
    Different { local: String, server: String },
}

/// Compare l'empreinte locale à celle qu'annonce le serveur (espaces ignorés, casse ignorée).
pub fn compare(local: Option<&ServerData>, announced: Option<&str>) -> Check {
    let Some(server) = announced.map(str::trim).filter(|s| !s.is_empty()) else { return Check::NotAnnounced };
    let server = server.to_string();
    match local {
        None => Check::MissingDir { server },
        Some(data) => match data.fingerprint() {
            None => Check::NoManifest { server },
            Some(l) if l.eq_ignore_ascii_case(&server) => Check::Same,
            Some(l) => Check::Different { local: l.to_string(), server },
        },
    }
}

/// Compare et écrit le résultat dans le journal (avertissement si les tables ne suivent pas le serveur).
pub fn check(local: Option<&ServerData>, announced: Option<&str>) -> Check {
    let c = compare(local, announced);
    const HELP: &str = "lancer `archcore export-client-tables <dossier>` côté serveur et placer le dossier en server-data/ à côté de realmlist.wtf (ou --server-data / ARCHRUST_SERVER_DATA)";
    match &c {
        Check::Same => log::info!("données du serveur : empreinte {} conforme au serveur", local.and_then(ServerData::fingerprint).unwrap_or("")),
        Check::NotAnnounced => match local {
            Some(d) => log::info!("données du serveur : le serveur n'annonce pas d'empreinte, {} utilisé sans contrôle", d.dir.display()),
            None => log::debug!("données du serveur : ni dossier ni empreinte annoncée"),
        },
        Check::MissingDir { server } => log::warn!("données du serveur absentes alors que le serveur annonce l'empreinte {server} : les valeurs affichées (infobulles, prix, conditions) sont celles du client ; {HELP}"),
        Check::NoManifest { server } => log::warn!("données du serveur {} sans empreinte ({MANIFEST_FILE}) : impossible de vérifier qu'elles suivent le serveur (empreinte {server}) ; {HELP}", local.map(|d| d.dir.display().to_string()).unwrap_or_default()),
        Check::Different { local: l, server } => log::warn!("données du serveur périmées ou d'un autre serveur : empreinte locale {l}, serveur {server} ; les valeurs affichées peuvent différer de celles du serveur ; {HELP}"),
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("archrust-server-data-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dossiers_de_tables_seulement() {
        assert!(covers("ini/itemdatatable.txt"));
        assert!(covers("INI_FR\\SKILL_CONST.txt"));
        assert!(!covers("ini"));
        assert!(!covers("texture/ui/x.png"));
        assert!(!covers("initial/x.txt"));
        assert!(!covers("character/ini/x.txt"));
    }

    #[test]
    fn empreinte_et_lecture() {
        let d = temp("lecture");
        std::fs::create_dir_all(d.join("ini_fr")).unwrap();
        std::fs::write(d.join("ini_fr/ItemDataTable.txt"), b"TID\tName\r\n1\tx\r\n").unwrap();
        std::fs::write(d.join("hors.txt"), b"x").unwrap();
        std::fs::write(d.join(MANIFEST_FILE), "format = 1\nfingerprint = \"AB12\"\nsoftware = \"ArchCore 0.2.0\"\n").unwrap();
        let data = ServerData::open(&d);
        assert_eq!(data.fingerprint(), Some("AB12"));
        assert_eq!(data.read("ini_fr/itemdatatable.txt").unwrap().as_deref(), Some(&b"TID\tName\r\n1\tx\r\n"[..]), "casse ignorée");
        assert_eq!(data.read("hors.txt").unwrap(), None, "hors des dossiers de tables");
        assert_eq!(data.read("ini/../hors.txt").unwrap(), None);
        assert_eq!(data.read("ini/absent.txt").unwrap(), None);
        assert_eq!(data.summary(), "ini_fr 1");

        assert_eq!(compare(Some(&data), Some(" ab12 ")), Check::Same);
        assert_eq!(compare(Some(&data), Some("cd34")), Check::Different { local: "AB12".into(), server: "cd34".into() });
        assert_eq!(compare(Some(&data), None), Check::NotAnnounced);
        assert_eq!(compare(Some(&data), Some("")), Check::NotAnnounced);
        assert_eq!(compare(None, Some("cd34")), Check::MissingDir { server: "cd34".into() });
        std::fs::remove_file(d.join(MANIFEST_FILE)).unwrap();
        assert_eq!(compare(Some(&ServerData::open(&d)), Some("cd34")), Check::NoManifest { server: "cd34".into() });
        assert!(Manifest::parse("fingerprint = 3").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn emplacement() {
        let d = temp("emplacement");
        let (exe, game) = (d.join("exe"), d.join("Game"));
        std::fs::create_dir_all(game.join(DIR_NAME)).unwrap();
        std::fs::create_dir_all(&exe).unwrap();
        let search = [exe.clone(), game.clone()];
        assert_eq!(find_dir_in(None, None, &search), Some(game.join(DIR_NAME)), "à côté de realmlist.wtf");
        std::fs::create_dir_all(exe.join(DIR_NAME)).unwrap();
        assert_eq!(find_dir_in(None, None, &search), Some(exe.join(DIR_NAME)), "même ordre que realmlist.wtf");
        let other = d.join("autre");
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(find_dir_in(None, Some(&other), &search), Some(other.clone()), "variable d'environnement");
        assert_eq!(find_dir_in(Some(&game), Some(&other), &search), Some(game.clone()), "option d'abord");
        assert_eq!(find_dir_in(Some(&d.join("absent")), None, &search), Some(exe.join(DIR_NAME)), "option invalide ignorée");
        let _ = std::fs::remove_dir_all(&d);
    }
}
