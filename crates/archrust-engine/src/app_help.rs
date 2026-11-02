//! Aide et astuces d'Archonia (AgcmUIHelp, AgcmUITips) ; fenêtres : `hud_help.rs`.
//!
//! * **Aide** : `Ini\Help.txt` (FUN_140517b10 « ReadHelp »), table à tabulations `Item` (titre), `Depth`
//!   (0 catégorie, 1 page), `Content` (`___` = retour à la ligne) ; `UI_Help` ouverte et fermée par « Help »
//!   (`MenuBtnHelp`) d'`UI_MainMenuOptions`. Contenu affiché tel que le fichier du jeu le donne.
//! * **Astuces** : `Ini\TipText.txt` (FUN_1404f4610 « ReadTipTexts »), colonnes `Text`, `Hour`, `Type` ; `Hour` ≠ 0
//!   force le type 2 ; tri par type puis par heure. En jeu (FUN_1404f41f0, OnIdle) : une astuce de type 0 tirée
//!   au hasard dès l'entrée, puis 120 s après la fermeture de la précédente (`Tip_OpenText`, `UI_TIPS`), fermée
//!   10 s après (`Tip_CloseText`) ; quand le temps de jeu atteint `Hour` heures, la dernière astuce de type 2 dont
//!   l'heure est atteinte est montrée. `Tip_Next` / `Tip_Prev` (FUN_1404f3af0 / FUN_1404f3b70) : astuce de type 0
//!   suivante / précédente, affichée 10 s de plus. L'option qui coupe les astuces (+0x16e4) n'est pas identifiée :
//!   les astuces sont toujours actives.
//! * **Écran de chargement** : `Ini\Tips.txt` (colonne `Texture`), voir `hud_help.rs`.

use super::App;
use crate::assets::Assets;
use crate::hud::{HelpEntry, HelpState, HudState, TipUi};
use std::time::Instant;

/// Astuce de `TipText.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    pub text: String,
    pub hour: u32,
    pub kind: u8,
}

/// Lignes de `Help.txt`.
pub fn help_entries(rows: &[std::collections::HashMap<String, String>]) -> Vec<HelpEntry> {
    rows.iter()
        .filter_map(|r| {
            let item = r.get("item")?.clone();
            Some(HelpEntry { item, depth: r.get("depth").and_then(|d| d.parse().ok()).unwrap_or(0), content: r.get("content").cloned().unwrap_or_default() })
        })
        .collect()
}

/// Astuces de `TipText.txt`, triées par type puis par heure.
pub fn tips(rows: &[std::collections::HashMap<String, String>]) -> Vec<Tip> {
    let mut list: Vec<Tip> = rows
        .iter()
        .filter_map(|r| {
            let text = r.get("text").filter(|t| !t.is_empty())?.clone();
            let hour = r.get("hour").and_then(|h| h.parse().ok()).unwrap_or(0);
            let kind = if hour != 0 { 2 } else { r.get("type").and_then(|t| t.parse().ok()).unwrap_or(0) };
            Some(Tip { text, hour, kind })
        })
        .collect();
    list.sort_by_key(|t| (t.kind, t.hour));
    list
}

/// Minuterie des astuces (FUN_1404f41f0) ; temps en millisecondes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TipTimer {
    /// Entrée en jeu (+0x10150).
    start: Option<u64>,
    /// Dernière ouverture ou fermeture (+0x10158 ; `None` = 0).
    last: Option<u64>,
    /// Astuce en cours d'affichage (+0x10148).
    shown: bool,
    /// Heures de jeu déjà comptées (+0x1014c).
    hours: u64,
    /// Astuce courante (+0x16e8).
    pub current: usize,
    /// `UI_TIPS` ouverte.
    pub open: bool,
}

impl TipTimer {
    /// Une image ; `n0` : astuces de type 0, `type2` : (rang, heure) des astuces de type 2, `pick` : tirage dans
    /// `0..n0`.
    pub fn tick(&mut self, now: u64, in_game: bool, n0: usize, type2: &[(usize, u32)], pick: impl FnOnce(usize) -> usize) {
        if in_game && n0 > 0 {
            self.start.get_or_insert(now);
            if self.last.is_none() || (!self.shown && self.last.is_some_and(|l| l + 120_000 <= now)) {
                if !self.shown {
                    self.current = pick(n0);
                    self.open = true;
                }
                self.shown = true;
                self.last = Some(now);
            }
        }
        if self.shown && self.last.is_none_or(|l| l + 10_000 <= now) {
            self.shown = false;
            self.open = false;
            self.last = Some(now);
        }
        if let Some(start) = self.start {
            let h = now.saturating_sub(start) / 3_600_000;
            if self.hours < h {
                self.hours = h;
                for &(i, hour) in type2 {
                    if (h as u32) < hour {
                        break;
                    }
                    self.current = i;
                }
                if n0 <= self.current {
                    self.open = true;
                    self.shown = true;
                    self.last = Some(now);
                }
            }
        }
    }

    /// `Tip_Next` (`step` = 1) / `Tip_Prev` (−1) : astuce de type 0 voisine, affichée 10 s de plus.
    pub fn step(&mut self, n0: usize, step: i64) {
        if n0 == 0 || self.current >= n0 {
            return;
        }
        self.current = (self.current as i64 + step).rem_euclid(n0 as i64) as usize;
        self.last = None;
    }
}

#[derive(Default)]
pub struct HelpGlue {
    help: Option<HelpState>,
    /// `UI_Help` ouverte.
    pub open: bool,
    tips: Option<Vec<Tip>>,
    timer: TipTimer,
    epoch: Option<Instant>,
    seed: u32,
}

impl HelpGlue {
    /// « Help » d'`UI_MainMenuOptions` : bascule `UI_Help`.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Échap : ferme `UI_Help`.
    pub fn close_windows(&mut self) -> bool {
        std::mem::take(&mut self.open)
    }

    fn tips(&mut self, assets: &Assets) -> &[Tip] {
        self.tips.get_or_insert_with(|| {
            let list = tips(&crate::hud::read_tab_table(assets, "ini/TipText.txt"));
            log::info!("astuces : {} (TipText.txt)", list.len());
            list
        })
    }

    fn counts(&mut self, assets: &Assets) -> (usize, Vec<(usize, u32)>) {
        let list = self.tips(assets);
        let n0 = list.iter().filter(|t| t.kind == 0).count();
        let type2 = list.iter().enumerate().filter(|(_, t)| t.kind == 2).map(|(i, t)| (i, t.hour)).collect();
        (n0, type2)
    }

    /// Tirage pseudo-aléatoire (xorshift32).
    fn random(&mut self, n: usize) -> usize {
        if self.seed == 0 {
            self.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() | 1);
        }
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        x as usize % n.max(1)
    }
}

/// Fenêtres `UI_Help` et `UI_TIPS`.
pub fn panel(ctx: &egui::Context, glue: &mut HelpGlue, hud: &mut HudState, assets: &Assets) {
    if glue.open {
        let st = glue.help.get_or_insert_with(|| {
            let entries = help_entries(&crate::hud::read_tab_table(assets, "ini/Help.txt"));
            log::info!("aide : {} entrée(s) (Help.txt)", entries.len());
            HelpState::new(entries)
        });
        if crate::hud::draw_help(ctx, hud, assets, st) {
            glue.open = false;
        }
    }
    if glue.timer.open {
        let current = glue.timer.current;
        let text = glue.tips(assets).get(current).map(|t| t.text.clone()).unwrap_or_default();
        if let Some(choice) = crate::hud::draw_tips(ctx, hud, assets, &text) {
            let (n0, _) = glue.counts(assets);
            glue.timer.step(n0, if choice == TipUi::Next { 1 } else { -1 });
        }
    }
}

impl App {
    /// Minuterie des astuces, à chaque image.
    pub(super) fn help_frame(&mut self) {
        let in_game = self.phase == super::Phase::InWorld;
        let glue = &mut self.help;
        if !in_game && glue.timer.start.is_none() {
            return;
        }
        let now = glue.epoch.get_or_insert_with(Instant::now).elapsed().as_millis() as u64 + 1;
        let (n0, type2) = glue.counts(&self.assets);
        let pick = glue.random(n0);
        glue.timer.tick(now, in_game, n0, &type2, |_| pick);
        if !in_game {
            glue.timer.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minuterie_des_astuces() {
        let mut t = TipTimer::default();
        // Entrée en jeu : astuce tout de suite, fermée 10 s après, la suivante 120 s après la fermeture.
        t.tick(1, true, 3, &[], |_| 2);
        assert!(t.open && t.current == 2);
        t.tick(9_000, true, 3, &[], |_| 0);
        assert!(t.open);
        t.tick(10_001, true, 3, &[], |_| 0);
        assert!(!t.open);
        t.tick(130_000, true, 3, &[], |_| 0);
        assert!(!t.open);
        t.tick(130_001, true, 3, &[], |_| 1);
        assert!(t.open && t.current == 1);
        // Suivante : rouvre 10 s.
        t.step(3, 1);
        assert_eq!(t.current, 2);
        t.tick(135_000, true, 3, &[], |_| 0);
        assert!(t.open);
        t.step(3, 1);
        assert_eq!(t.current, 0);
        t.step(3, -1);
        assert_eq!(t.current, 2);
        // Astuce horaire de type 2 (rang 3, 1 h).
        let mut t = TipTimer::default();
        t.tick(1, true, 3, &[(3, 1)], |_| 0);
        t.tick(20_000, true, 3, &[(3, 1)], |_| 0);
        assert!(!t.open);
        t.tick(3_600_002, true, 3, &[(3, 1)], |_| 0);
        assert!(t.open && t.current == 3);
    }

    #[test]
    fn fichiers_d_aide_et_d_astuces() {
        let help = crate::hud::parse_tab_table("Item\tDepth\tContent\r\nIntroduction\t0\tA___B\r\nPage 1\t1\tC\r\n");
        let e = help_entries(&help);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].item.as_str(), e[0].depth, e[0].content.as_str()), ("Introduction", 0, "A___B"));
        assert_eq!(e[1].depth, 1);
        let st = HelpState::new(e);
        assert_eq!(st.visible(), vec![0], "pages cachées tant que la catégorie est fermée");
        assert!(st.has_children(0) && !st.has_children(1));
        let rows = crate::hud::parse_tab_table("Text\tHour\tType\r\n<HH00FFFF>Un\t\t\t\t\r\nDeux\t2\t0\r\nTrois\t\t1\r\nQuatre\t1\t\r\n");
        let list = tips(&rows);
        let v: Vec<(&str, u32, u8)> = list.iter().map(|t| (t.text.as_str(), t.hour, t.kind)).collect();
        assert_eq!(v, vec![("<HH00FFFF>Un", 0, 0), ("Trois", 0, 1), ("Quatre", 1, 2), ("Deux", 2, 2)]);
        let seg = crate::hud::tip_segments("<HH00FFFF>Un", egui::Color32::WHITE);
        assert_eq!(seg, vec![("Un".to_string(), egui::Color32::from_rgb(0, 0xff, 0xff))]);
    }

    /// Fichiers du jeu : `Help.txt` et `TipText.txt` chiffrés, lus.
    #[test]
    fn fichiers_du_jeu() {
        let Ok(dir) = std::env::var("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(std::path::PathBuf::from(dir)), &[]);
        let help = help_entries(&crate::hud::read_tab_table(&assets, "ini/Help.txt"));
        assert!(help.iter().any(|e| e.depth == 0) && help.iter().any(|e| e.depth == 1));
        let list = tips(&crate::hud::read_tab_table(&assets, "ini/TipText.txt"));
        assert!(list.len() > 100);
        let images = crate::hud::read_tab_table(&assets, "ini/Tips.txt");
        assert!(images.iter().any(|r| r.get("texture").is_some_and(|t| t == "loading_help_0001")));
        // Image de `texture/ui` (nom sans extension : `.png` ajouté comme pour les autres images d'interface).
        assert!(assets.image("texture/ui", "loading_help_0001.png").is_ok());
    }
}
