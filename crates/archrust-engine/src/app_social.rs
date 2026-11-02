//! Actions sociales (« Emotes ») côté moteur. Protocole : `archrust_netclient::social` (AGPMCHARACTER op 16) ;
//! fenêtre `UI_SocialAction` : `hud_social.rs`.
//!
//! * Ouverture comme Archonia (`CBSocialActionToggleUI`, FUN_1406d2770) : entrée « Emotes » de
//!   `UI_MainMenuCommunity` (`CONTROL_10281`) et touche Y (`Hotkey=0:21` « Emotes », code de touche 0x15 ;
//!   `0:22` est la boîte aux lettres, U).
//! * `Ini\SocialAction.txt` (`serial, filename, title, desc`, lu par FUN_1406d2b40) : une case par ligne, d'identifiant
//!   serial − 1 = type envoyé (0-16). Infobulle : nom de `sysstr.txt` 459 + type (« Saluer »… « S'asseoir »), les
//!   colonnes `title` / `desc` du fichier n'existant qu'en anglais (déduit : Archonia ne les affiche pas en français).
//!   Les titres (`/greet`, `/clap`… `/sit down`) servent aussi de commandes de discussion.
//! * Double-clic ou clic droit sur une case (FUN_1406d2610) : op 16 (champ 1 son CID, champ 16 type). Le client
//!   n'anime rien à l'envoi (FUN_1404b8570 ne fait qu'envoyer) : l'op 16 renvoyée par le serveur, pour soi comme
//!   pour les personnages en vue, joue l'animation et le son de `INI\SOCIAL_PC.txt` (FUN_1408302a0 : colonnes par
//!   paires ANIMATION / SOUND, TID du personnage sur la première ligne ; lignes 2 à 19 = types 0 à 17, 21 à 40 =
//!   types 18 et suivants, écran de sélection), clips `character\animation\*.rws` et sons `sound\effect\*.wav`.
//! * Écarts : glisser une action vers la barre de raccourcis (objet de grille 0x40) n'est pas repris ; les contrôles
//!   d'envoi d'Archonia (+0xbfc, +0x600, +0x6ae du personnage, sens inconnu) se réduisent à « personnage en jeu ».

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, SocialUi, SocialView};
use crate::net::NetLink;
use archrust_netclient::social::{SocialCommand, SocialEvent, PLAYER_TYPES};
use archrust_netclient::Command;
use std::collections::HashMap;

/// Action de `SocialAction.txt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocialAction {
    /// Type envoyé (serial − 1).
    pub kind: u8,
    pub icon: String,
    /// Commande de discussion (`/greet`…).
    pub title: String,
}

/// Lignes de `SocialAction.txt` triées par type (serial 1 à 17).
pub fn parse_actions(text: &str) -> Vec<SocialAction> {
    let mut out: Vec<SocialAction> = text
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut c = l.split('\t').map(str::trim);
            let serial = c.next()?.parse::<u32>().ok().filter(|s| (1..=u32::from(PLAYER_TYPES)).contains(s))?;
            let icon = c.next().unwrap_or("").to_string();
            let title = c.next().unwrap_or("").to_string();
            Some(SocialAction { kind: (serial - 1) as u8, icon, title })
        })
        .collect();
    out.sort_by_key(|a| a.kind);
    out.dedup_by_key(|a| a.kind);
    out
}

/// `SOCIAL_PC.txt` : (type, TID du personnage) → (clip, son), cellules vides écartées.
pub fn parse_social_pc(text: &str) -> HashMap<(u8, i32), (String, String)> {
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').map(str::trim).collect()).collect();
    let mut out = HashMap::new();
    let Some(head) = rows.first() else { return out };
    let cell = |r: usize, c: usize| rows.get(r).and_then(|row| row.get(c)).copied().unwrap_or("");
    for col in (2..head.len()).step_by(2) {
        let Ok(tid) = cell(0, col).parse::<i32>() else { continue };
        // FUN_1408302a0 : lignes 2 à 0x13 (types 0 à 17), puis 0x15 à 0x28 (types 18 à 37), la ligne 0x14 sautée.
        for row in (2..0x14).chain(0x15..0x29) {
            let kind = (if row < 0x14 { row - 2 } else { row - 3 }) as u8;
            let (anim, sound) = (cell(row, col), cell(row, col + 1));
            if !anim.is_empty() || !sound.is_empty() {
                out.insert((kind, tid), (anim.to_string(), sound.to_string()));
            }
        }
    }
    out
}

#[derive(Default)]
struct SocialData {
    actions: Vec<SocialAction>,
    /// Icône et infobulle de chaque case.
    cells: Vec<(String, String)>,
    pc: HashMap<(u8, i32), (String, String)>,
}

fn load_data(assets: &Assets) -> SocialData {
    let actions = parse_actions(&super::npc_glue::client_text_in(assets, "ini", "socialaction.txt"));
    let cells = actions.iter().map(|a| (a.icon.clone(), super::npc_glue::sysstr(assets, 459 + u32::from(a.kind)).unwrap_or_default())).collect();
    let pc = parse_social_pc(&super::npc_glue::client_text_in(assets, "ini", "social_pc.txt"));
    log::info!("actions sociales : {} action(s), {} animation(s)", actions.len(), pc.len());
    SocialData { actions, cells, pc }
}

#[derive(Default)]
pub struct SocialGlue {
    data: Option<SocialData>,
    /// `UI_SocialAction` ouverte.
    pub open: bool,
}

impl SocialGlue {
    fn data(&mut self, assets: &Assets) -> &SocialData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// `CBSocialActionToggleUI`.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

fn send(net: Option<&NetLink>, kind: u8) {
    if let Some(net) = net.filter(|_| kind < PLAYER_TYPES) {
        net.send(Command::Social(SocialCommand::Play(kind)));
    }
}

/// Fenêtre `UI_SocialAction` et ses demandes.
pub fn panel(ctx: &egui::Context, glue: &mut SocialGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    if !glue.open {
        return;
    }
    let cells = glue.data(assets).cells.clone();
    let mut out = Vec::new();
    crate::hud::draw_social_action(ctx, hud, assets, &SocialView { actions: &cells }, &mut out);
    for r in out {
        match r {
            SocialUi::Close => glue.open = false,
            SocialUi::Play(kind) => send(net, kind),
        }
    }
}

impl App {
    /// Op 16 reçue : animation et son de `SOCIAL_PC.txt` pour le TID du personnage.
    pub(super) fn on_social_event(&mut self, ev: SocialEvent) {
        let assets = self.assets.clone();
        let mine = self.player.as_ref().is_some_and(|p| p.cid == ev.cid);
        let tid = if mine { self.player.as_ref().map(|p| p.tid) } else { self.entities.iter().find(|e| e.cid == ev.cid).map(|e| e.tid) };
        let Some(tid) = tid else {
            log::debug!("action sociale {} de CID {} : personnage inconnu", ev.kind, ev.cid);
            return;
        };
        let Some((anim, sound)) = self.social.data(&assets).pc.get(&(ev.kind, tid)).cloned() else {
            log::debug!("action sociale {} : rien pour le TID {tid}", ev.kind);
            return;
        };
        let animator = if mine { self.player_animator.as_mut().and_then(|(_, a)| a.as_mut()) } else { self.animators.get_mut(&ev.cid).and_then(|(_, a)| a.as_mut()) };
        let played = !anim.is_empty() && animator.is_some_and(|a| a.play_file_once(&assets, &anim));
        log::debug!("action sociale {} de CID {} : {anim} ({played}), {sound}", ev.kind, ev.cid);
        if !sound.is_empty() {
            // 2D pour le joueur, 3D sur le personnage sinon (`FUN_14056da90`, voir `app_sounds.rs`).
            self.sounds_play_char(ev.cid, &sound);
        }
    }

    /// Commandes de discussion de `SocialAction.txt` (`/greet`… `/sit down`) ; `false` si `cmd` n'en est pas une.
    pub(super) fn social_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let assets = self.assets.clone();
        let full = if arg.trim().is_empty() { cmd.to_string() } else { format!("{cmd} {}", arg.trim()) };
        let kind = self.social.data(&assets).actions.iter().find(|a| !a.title.is_empty() && (a.title.eq_ignore_ascii_case(cmd) || a.title.eq_ignore_ascii_case(&full))).map(|a| a.kind);
        let Some(kind) = kind else { return false };
        if self.player.is_some() {
            send(self.net.as_ref(), kind);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_et_animations() {
        let actions = parse_actions("serial\tfilename\ttitle\tdesc\r\n2\tSocialCelebration.png\t/clap\tx\r\n1\tSocialGreeting.png\t/greet\ty\r\n17\tSocialSit.png\t/sit down\tz\r\n18\tX.png\t/hors\t\r\n");
        assert_eq!(actions.iter().map(|a| (a.kind, a.title.as_str())).collect::<Vec<_>>(), vec![(0, "/greet"), (1, "/clap"), (16, "/sit down")]);
        assert_eq!(actions[0].icon, "SocialGreeting.png");

        let mut text = String::from("Tid / nom\t\t96\tchevalier\t1\tarcher\r\n\t\tANIMATION\tSOUND\tANIMATION\tSOUND\r\n");
        text += "GREETING\tx\tH_KNI_M_GREET.rws\tAA0000.WAV\tH_ARC_M_GREET.rws\tAB0000.WAV\r\n";
        text += "CELEBRATION\tx\tH_KNI_M_CELE.rws\tAA0001.WAV\t\t\r\n";
        for _ in 4..0x15 {
            text += "\r\n";
        }
        // Ligne 0x15 : premier type de l'écran de sélection (18).
        text += "SELECT1\tx\tH_KNI_M_SELECT1.rws\t\t\t\r\n";
        let pc = parse_social_pc(&text);
        assert_eq!(pc.get(&(0, 96)), Some(&("H_KNI_M_GREET.rws".to_string(), "AA0000.WAV".to_string())));
        assert_eq!(pc.get(&(0, 1)).map(|v| v.1.as_str()), Some("AB0000.WAV"));
        assert_eq!(pc.get(&(1, 1)), None);
        assert_eq!(pc.get(&(18, 96)).map(|v| v.0.as_str()), Some("H_KNI_M_SELECT1.rws"));
    }

    #[test]
    fn donnees_du_client() {
        // Tables réelles si le client est présent (`ARCHRUST_GAME_DIR`).
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let read = |name: &str| std::fs::read(std::path::Path::new(&dir).join("ini").join(name)).ok();
        let text = |raw: Vec<u8>| {
            let plain = raw.iter().take(256).filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t').count() * 20 < raw.len().min(256);
            if plain { archrust_data::ini::decode_text(&raw, archrust_data::ini::CodePage::Cp949) } else { archrust_data::ini::decrypt_ini(&raw) }
        };
        if let Some(raw) = read("socialaction.txt") {
            assert_eq!(parse_actions(&text(raw)).len(), 17);
        }
        if let Some(raw) = read("social_pc.txt") {
            let pc = parse_social_pc(&text(raw));
            assert_eq!(pc.get(&(0, 96)).map(|v| v.0.to_ascii_lowercase()), Some("h_kni_m_greet.rws".to_string()));
            assert_eq!(pc.get(&(16, 1732)).map(|v| v.0.is_empty()), Some(false));
        }
    }
}
