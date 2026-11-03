//! `realmlist.wtf` : adresse du serveur de connexion, à la manière de World of Warcraft.
//!
//! ```text
//! # Serveur de connexion ArchCore
//! set realmlist 127.0.0.1
//! set realmlist "mon.serveur.fr:11002"
//! ```
//!
//! Une ligne `set realmlist <hôte>[:<port>]` (guillemets facultatifs, casse indifférente) ; port
//! 11002 par défaut ; lignes vides et commentaires `#`, `;` ou `//` ignorés ; la dernière ligne
//! `realmlist` l'emporte. Le fichier est cherché à côté de l'exécutable, puis dans le dossier
//! courant, puis dans le dossier `Game` du client. Il préremplit le serveur du panneau de connexion
//! d'ArchRust et donne celui de `--connect` quand celui-ci ne précise pas d'hôte.
//!
//! Repli : sans `realmlist.wtf`, le fichier d'Archonia `ArchoniaTest.ini` (lu en clair par
//! FUN_1408eb790, mêmes dossiers) : XML `<IPInfo><ServerGroup><GroupName>…</GroupName><IP>hôte:port</IP>…`
//! (FUN_1408ec080) ; premier groupe, première IP (le choix entre plusieurs n'est pas connu).

use std::path::{Path, PathBuf};

/// Nom du fichier.
pub const FILE_NAME: &str = "realmlist.wtf";
/// Fichier d'adresses d'Archonia, lu en repli.
pub const ARCHONIA_FILE: &str = "ArchoniaTest.ini";
/// Port du serveur de connexion ArchLord quand le fichier n'en donne pas.
pub const DEFAULT_PORT: u16 = 11002;

/// Serveur lu dans un `realmlist.wtf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realmlist {
    pub host: String,
    pub port: u16,
    /// Fichier d'où vient l'adresse.
    pub source: PathBuf,
}

/// `hôte[:port]` ; une adresse IPv6 entre crochets garde ses `:` (`[::1]:11002`). `None` si le port
/// est illisible.
fn host_port(value: &str) -> Option<(String, u16)> {
    let (host, port) = match value.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && (!h.contains(':') || h.ends_with(']')) => (h, p.parse::<u16>().ok()?),
        _ => (value, DEFAULT_PORT),
    };
    Some((host.trim_matches(['[', ']']).to_string(), port))
}

/// Groupe, hôte et port de la première `IP` du premier `ServerGroup` d'un `ArchoniaTest.ini`.
pub fn parse_ip_info(text: &str) -> Option<(String, String, u16)> {
    let inner = |s: &str, tag: &str| -> Option<(usize, String)> {
        let open = format!("<{tag}>");
        let start = s.find(&open)? + open.len();
        let end = start + s[start..].find(&format!("</{tag}>"))?;
        Some((end, s[start..end].trim().to_string()))
    };
    let (_, group) = inner(text, "ServerGroup")?;
    let name = inner(&group, "GroupName").map(|(_, n)| n).unwrap_or_default();
    let mut rest = group.as_str();
    while let Some((end, ip)) = inner(rest, "IP") {
        if let Some((host, port)) = host_port(&ip).filter(|(h, _)| !h.is_empty()) {
            return Some((name, host, port));
        }
        rest = &rest[end..];
    }
    None
}

/// Hôte et port d'un texte `realmlist.wtf`, ou `None` s'il n'a pas de ligne `set realmlist`.
pub fn parse(text: &str) -> Option<(String, u16)> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with("//")
        {
            continue;
        }
        let mut words = line.splitn(3, char::is_whitespace);
        let (Some(set), Some(key), Some(value)) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        if !set.eq_ignore_ascii_case("set") || !key.eq_ignore_ascii_case("realmlist") {
            continue;
        }
        let value = value.trim().trim_matches('"').trim();
        if value.is_empty() {
            continue;
        }
        if let Some(hp) = host_port(value) {
            found = Some(hp);
        }
    }
    found
}

/// Dossiers où chercher `realmlist.wtf`, dans l'ordre : exécutable, dossier courant, client.
pub fn search_dirs(game_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Some(g) = game_dir {
        dirs.push(g.to_path_buf());
    }
    dirs
}

/// Premier `realmlist.wtf` lisible et valide parmi `dirs`, sinon premier `ArchoniaTest.ini`.
pub fn find(dirs: &[PathBuf]) -> Option<Realmlist> {
    find_realmlist(dirs).or_else(|| find_archonia(dirs))
}

/// Premier `ArchoniaTest.ini` lisible et valide parmi `dirs`.
fn find_archonia(dirs: &[PathBuf]) -> Option<Realmlist> {
    for dir in dirs {
        let path = dir.join(ARCHONIA_FILE);
        let Ok(bytes) = std::fs::read(&path) else { continue };
        match parse_ip_info(&String::from_utf8_lossy(&bytes)) {
            Some((group, host, port)) => {
                log::info!("{} : groupe « {group} »", path.display());
                return Some(Realmlist { host, port, source: path });
            }
            None => log::warn!("{} : aucune adresse <IP>", path.display()),
        }
    }
    None
}

fn find_realmlist(dirs: &[PathBuf]) -> Option<Realmlist> {
    for dir in dirs {
        let path = dir.join(FILE_NAME);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match parse(&String::from_utf8_lossy(&bytes)) {
            Some((host, port)) => {
                return Some(Realmlist {
                    host,
                    port,
                    source: path,
                })
            }
            None => log::warn!("{} : aucune ligne « set realmlist »", path.display()),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wow_style_lines() {
        assert_eq!(
            parse("set realmlist 127.0.0.1\n"),
            Some(("127.0.0.1".into(), 11002))
        );
        assert_eq!(
            parse("# commentaire\nSET realmlist \"logon.exemple.fr:12000\"\r\n"),
            Some(("logon.exemple.fr".into(), 12000))
        );
        assert_eq!(
            parse("set realmlist a.fr\nset realmlist b.fr:1\n"),
            Some(("b.fr".into(), 1))
        );
        assert_eq!(
            parse("set realmlist [::1]:11002"),
            Some(("::1".into(), 11002))
        );
        assert_eq!(parse("set realmlist ::1"), Some(("::1".into(), 11002)));
        assert_eq!(parse("set realmlist hote:abc"), None);
        assert_eq!(parse("set patchlist x\n// rien"), None);
    }

    #[test]
    fn lit_archonia_test_ini() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<IPInfo>\r\n\t<ServerGroup>\r\n\t\t<GroupName>ArchCore</GroupName>\r\n\t\t<IP>127.0.0.1:11002</IP>\r\n\t</ServerGroup>\r\n</IPInfo>\r\n";
        assert_eq!(parse_ip_info(xml), Some(("ArchCore".into(), "127.0.0.1".into(), 11002)));
        // IP illisible puis valide ; sans port : port par défaut.
        assert_eq!(
            parse_ip_info("<IPInfo><ServerGroup><GroupName>G</GroupName><IP>a:x</IP><IP>b.fr</IP></ServerGroup></IPInfo>"),
            Some(("G".into(), "b.fr".into(), 11002))
        );
        assert_eq!(parse_ip_info("<IPInfo></IPInfo>"), None);
        // realmlist.wtf d'abord, ArchoniaTest.ini en repli.
        let dir = std::env::temp_dir().join("archrust-test-realmlist");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join(ARCHONIA_FILE), xml).unwrap();
        let _ = std::fs::remove_file(dir.join(FILE_NAME));
        assert_eq!(find(std::slice::from_ref(&dir)).map(|r| (r.host, r.port)), Some(("127.0.0.1".into(), 11002)));
        std::fs::write(dir.join(FILE_NAME), "set realmlist autre.fr:1234\n").unwrap();
        assert_eq!(find(std::slice::from_ref(&dir)).map(|r| (r.host, r.port)), Some(("autre.fr".into(), 1234)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
