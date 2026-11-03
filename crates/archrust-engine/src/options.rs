//! Options du client (volumes, langue, onglets Affichage, Fonction et GFX), persistées en TOML dans
//! le dossier des SavedVariables (`saved-variables/options.toml`), à côté des SavedVariables des
//! AddOns et du compte chiffré.

use crate::audio::AudioSettings;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "options.toml";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientOptions {
    pub audio: AudioSettings,
    /// Langue des textes du serveur (code BCP 47 : `fr`, `de`, `pt-BR`…), annoncée au serveur ArchCore
    /// à la connexion (capacité `language/1`) et à chaque changement. `None` : automatique, le serveur
    /// applique la dernière langue enregistrée sur le compte, sinon la sienne.
    pub language: Option<String>,
    /// Onglet « Affichage » (`Option_System`).
    pub display: DisplayOptions,
    /// Onglet « Fonction » (`Option_Option`) : refus.
    pub refuse: RefuseOptions,
    /// Onglet « GFX » (`Option_Video`).
    pub video: VideoOptions,
}

/// Affichage des noms au-dessus des personnages et du casque, comme `[ETC_Option]` de `COption.ini`
/// d'Archonia (`VN_MINE`, `VN_MONSTER`, `VN_GUILD`, `VN_PARTY`, `VN_OTHERS`, `VS_HELMET`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayOptions {
    /// Nom du joueur (`OP_ViewNameMine`).
    pub name_self: bool,
    /// Noms des monstres (`OP_ViewNameMonster`).
    pub name_monster: bool,
    /// Ligne de guilde sous les noms (`OP_ViewNameGuild`).
    pub name_guild: bool,
    /// Cadre des noms des membres du groupe (`OP_ViewNameParty`).
    pub name_party: bool,
    /// Noms des autres joueurs (`OP_ViewNameOthers`).
    pub name_others: bool,
    /// Casque visible (`OP_VisibleViewHelmet`), envoyé au serveur (AgpmUIStatus op 3, champ 8).
    pub helmet: bool,
    /// Bulles de discussion au-dessus des têtes (`OP_ViewBaloonChat`, `VH_BALOONCHAT`, voir `app_balloon.rs`).
    pub balloon_chat: bool,
    /// Barres PV / PM sous le nom du joueur (`OP_ViewSelfBars`, `VS_BARS`, AgcmHPBar).
    pub self_bars: bool,
}

impl Default for DisplayOptions {
    /// `COption.ini` d'Archonia : `VH_BALOONCHAT=1`, `VS_BARS=0`.
    fn default() -> Self {
        Self { name_self: true, name_monster: true, name_guild: true, name_party: true, name_others: true, helmet: true, balloon_chat: true, self_bars: false }
    }
}

/// Refus de l'onglet « Fonction », comme `[Option_Option]` de `COption.ini` (`REF_TRADE`, `REF_PIN`,
/// `REF_GIN`, `REF_GBAT`, `REF_GR`, `REF_BUDDY`). Envoyés au serveur en drapeaux
/// (AgpmCharacter op 0x11, champ 22, voir [`RefuseOptions::flags`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefuseOptions {
    pub trade: bool,
    pub party: bool,
    pub guild: bool,
    pub guild_battle: bool,
    pub guild_relation: bool,
    pub buddy: bool,
}

impl RefuseOptions {
    /// Drapeaux de `character+0xbf8` d'Archonia : 1 échange, 2 groupe, 4 combat de guilde, 0x10
    /// guilde, 0x40 alliance/hostilité, 0x80 amis (8, combat, n'a pas de case).
    pub fn flags(&self) -> u32 {
        [(self.trade, 0x01), (self.party, 0x02), (self.guild_battle, 0x04), (self.guild, 0x10), (self.guild_relation, 0x40), (self.buddy, 0x80)]
            .iter()
            .filter(|(on, _)| *on)
            .fold(0, |f, (_, bit)| f | bit)
    }
}

/// Distances d'affichage de l'onglet « GFX » (`CharView`, `MapView` de `[Video_Option]`) : 0
/// proche, 1 moyen, 2 loin, 3 global (`Option_Msg_NEAR`… `Option_Msg_ALL`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoOptions {
    pub char_view: u8,
    pub map_view: u8,
    /// `Nature` de `COption.ini` (liste `OP_Nature` « Environnement » : 0 bas, 1 moyen, 2 haut) :
    /// ArchRust y règle la densité des particules de météo.
    pub nature: u8,
    /// `Sky` (`OP_Sky`, « Weather FX » de la fenêtre `Option_Detail_Nature`) : particules de météo.
    pub sky: bool,
    /// `Fog` (`OP_Fog`) : brouillard du ciel.
    pub fog: bool,
    /// `LensFlare` (`OP_LensFlare`) : reflets du soleil.
    pub lens_flare: bool,
    /// `Effect` (`OP_Effect`, « Effets », 0 à 4) : Archonia en tire `EffectField`, `EffectCombat`,
    /// `EffectUI` et `EffectSkill` (FUN_14070fb80, voir [`VideoOptions::effect_levels`]).
    pub effect: u8,
    /// `DisableOtherPlayerSkillEffect` (`OP_TurnOffAllEffects`, « Désactiver les effets des autres
    /// joueurs ») : effets des compétences des autres joueurs masqués.
    pub hide_other_effects: bool,
    /// `Shadow` (`OP_Shadow`, 0 à 3) : 0 coupe l'ombre ronde sous les personnages (`app_decals.rs`),
    /// 2 et plus allument les ombres projetées des objets sur le sol (`shadow`, AgcmShadow2).
    pub shadow: u8,
    /// `Width` / `Height` de `[Patch_Option]` (`OP_Resolution`) ; `None` : taille de la fenêtre gardée.
    pub resolution: Option<(u32, u32)>,
    /// `WindowMode` (`OP_WindowMode`) : 0 plein écran, 1 fenêtré, 2 sans bordure (FUN_140703da0).
    pub window_mode: u8,
    /// `Multisampling` (`OP_AntiAliasing`, ramené à 0 / 1 par FUN_14070ed10). Enregistré seulement :
    /// Archonia ne l'applique jamais (niveau de multiéchantillonnage toujours 1, `FUN_140357fb0`).
    pub multisampling: bool,
    /// `VSync` (`OP_HighOptimization`, case « V-Sync », FUN_140707970).
    pub vsync: bool,
    /// Valeur de `BRIGHT=100:<valeur>` (`OP_BrightVal`, gamma, FUN_14070f5d0), de 0 à 100.
    pub bright: u8,
    /// `NaturalObj` (`OP_NaturalObj`, « Objets ») : herbe du monde, 0 aucune, 1 type de distance 2,
    /// 2 et 3 type 3 (`FUN_140711350` → `FUN_140597390`, voir `grass.rs`).
    pub natural_obj: u8,
}

impl Default for VideoOptions {
    /// Valeurs de `COption.ini` d'Archonia : `Nature=2`, `LensFlare=1`, `Fog=1`, `Sky=1`, `Effect=2`,
    /// `DisableOtherPlayerSkillEffect=0`, `Shadow=3`, `WindowMode=1`, `Multisampling=1`, `VSync=1`, `NaturalObj=1` ;
    /// gamma 50 (valeur du constructeur d'AgcmUIOption, FUN_140703da0 : courbe neutre).
    fn default() -> Self {
        Self { char_view: 3, map_view: 1, nature: 2, sky: true, fog: true, lens_flare: true, effect: 2, hide_other_effects: false, shadow: 3, resolution: None, window_mode: 1, multisampling: true, vsync: true, bright: 50, natural_obj: 1 }
    }
}

/// Rampe gamma d'Archonia (FUN_14070f5d0) pour `bright` sur 100 : `(i / 255)^(1 / (bright/100 + 0,5))`
/// × 65 536, bornée à 65 535 (contraste et luminosité à 100/100 : sans effet).
pub fn gamma_ramp(bright: u8) -> [u16; 256] {
    let exp = 1.0 / (bright.min(100) as f32 / 100.0 + 0.5);
    let mut out = [0u16; 256];
    for (i, v) in out.iter_mut().enumerate() {
        *v = ((i as f32 / 255.0).powf(exp) * 65_536.0).clamp(0.0, 65_535.0) as u16;
    }
    out
}

impl VideoOptions {
    /// Niveaux (terrain, combat, interface, compétences) d'`OP_Effect`, comme FUN_14070fb80 :
    /// 0 → (0, 0, 0, 0) ; 1 → (1, 1, 0, 1) ; 2 → (2, 2, 1, 1) ; 3 → (3, 3, 1, 1) ; 4 → (4, 4, 1, 1).
    pub fn effect_levels(&self) -> (u8, u8, u8, u8) {
        match self.effect.min(4) {
            0 => (0, 0, 0, 0),
            1 => (1, 1, 0, 1),
            n => (n, n, 1, 1),
        }
    }

    /// Facteur de densité des particules de météo selon `Nature` (hypothèse : 1/3, 2/3, 1).
    pub fn nature_density(&self) -> f32 {
        [1.0 / 3.0, 2.0 / 3.0, 1.0][self.nature.min(2) as usize]
    }
}

/// Distance de référence d'ArchRust à laquelle s'appliquent les facteurs d'Archonia (`MapView` 1,
/// facteur 0,8, donne les 25 000 unités de tracé des objets d'avant ces réglages).
pub const VIEW_BASE: f32 = 31_250.0;

impl VideoOptions {
    /// Facteur `CharView` d'Archonia (AgcmTuner, `FUN_14070f7f0`) : 0,25 / 0,45 / 0,875 / 999.
    pub fn char_factor(&self) -> f32 {
        [0.25, 0.45, 0.875, 999.0][self.char_view.min(3) as usize]
    }

    /// Distance de tracé des personnages ; `None` : sans limite (« Global »).
    pub fn char_distance(&self) -> Option<f32> {
        (self.char_view < 3).then(|| VIEW_BASE * self.char_factor())
    }

    /// Facteur `MapView` d'Archonia (`FUN_140711050`) : 0,5 / 0,8 / 1,3 / 1,8.
    pub fn map_factor(&self) -> f32 {
        [0.5, 0.8, 1.3, 1.8][self.map_view.min(3) as usize]
    }

    /// Distance de tracé des objets du décor.
    pub fn object_distance(&self) -> f32 {
        VIEW_BASE * self.map_factor()
    }

    /// Rayon (en secteurs de 6400) du sol et des objets chargés autour du joueur : 2 à 5. Archonia
    /// charge un sol grossier sur 4 / 6 / 7 / 9 secteurs et détaillé sur 1 / 1 / 2 / 3, comme ArchRust
    /// (`world::TerrainView`) ; ce rayon-ci garde les hauteurs, le blocage et les objets.
    pub fn sector_radius(&self) -> i32 {
        2 + self.map_view.min(3) as i32
    }
}

pub fn path(saved_dir: &Path) -> PathBuf {
    saved_dir.join(FILE_NAME)
}

/// Options enregistrées, ou les valeurs par défaut (fichier absent ou illisible, journalisé).
pub fn load(saved_dir: &Path) -> ClientOptions {
    let p = path(saved_dir);
    match std::fs::read_to_string(&p) {
        Ok(text) => match toml::from_str(&text) {
            Ok(o) => o,
            Err(e) => {
                log::warn!("{} illisible ({e}) : options par défaut", p.display());
                ClientOptions::default()
            }
        },
        Err(_) => ClientOptions::default(),
    }
}

pub fn save(saved_dir: &Path, options: &ClientOptions) -> Result<()> {
    std::fs::create_dir_all(saved_dir)?;
    let text = toml::to_string_pretty(options)?;
    let p = path(saved_dir);
    std::fs::write(&p, format!("# Options du client ArchRust : son (volumes de 0 à 1), langue, affichage, refus, distances (video).\n{text}"))
        .with_context(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let tmp = std::env::temp_dir().join(format!("archrust-options-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(load(&tmp), ClientOptions::default());
        let mut o = ClientOptions::default();
        o.audio.music = 0.25;
        o.audio.muted = true;
        o.language = Some("pt-BR".into());
        save(&tmp, &o).unwrap();
        let text = std::fs::read_to_string(path(&tmp)).unwrap();
        assert!(text.contains("[audio]") && text.contains("music = 0.25"), "{text}");
        assert!(text.contains("language = \"pt-BR\""), "{text}");
        assert_eq!(load(&tmp), o);
        // Onglets Affichage, Fonction, GFX.
        o.display.name_monster = false;
        o.refuse.party = true;
        o.refuse.trade = true;
        o.video.char_view = 1;
        save(&tmp, &o).unwrap();
        let text = std::fs::read_to_string(path(&tmp)).unwrap();
        assert!(text.contains("[display]") && text.contains("name_monster = false"), "{text}");
        assert!(text.contains("[refuse]") && text.contains("[video]") && text.contains("char_view = 1"), "{text}");
        assert_eq!(load(&tmp), o);
        assert_eq!(o.refuse.flags(), 0x03);
        // Sans langue : automatique.
        o.language = None;
        save(&tmp, &o).unwrap();
        assert_eq!(load(&tmp).language, None);
        // Ancienne option `archonia_login` (écran de connexion d'Archonia, retiré) : ignorée.
        std::fs::write(path(&tmp), "archonia_login = true\n[audio]\nmusic = 0.25\nmuted = true\n").unwrap();
        let old = load(&tmp);
        assert_eq!(old.audio.music, 0.25);
        assert!(!toml::to_string(&old).unwrap().contains("archonia_login"));
        // Clé inconnue ou manquante : valeurs par défaut pour le reste.
        std::fs::write(path(&tmp), "[audio]\neffects = 0.1\nfutur = 3\n").unwrap();
        let o = load(&tmp);
        assert_eq!(o.audio.effects, 0.1);
        assert_eq!(o.audio.music, AudioSettings::default().music);
        assert_eq!(o.display, DisplayOptions::default());
        assert_eq!((o.video.char_view, o.video.map_view), (3, 1));
        std::fs::write(path(&tmp), "pas du toml =").unwrap();
        assert_eq!(load(&tmp), ClientOptions::default());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn refus_et_distances() {
        let all = RefuseOptions { trade: true, party: true, guild: true, guild_battle: true, guild_relation: true, buddy: true };
        assert_eq!(all.flags(), 0x01 | 0x02 | 0x04 | 0x10 | 0x40 | 0x80);
        assert_eq!(RefuseOptions { guild: true, ..Default::default() }.flags(), 0x10);
        let v = VideoOptions::default();
        // Par défaut : personnages sans limite, carte au rayon et à la distance d'avant les réglages.
        assert_eq!((v.char_distance(), v.sector_radius(), v.object_distance()), (None, 3, 25_000.0));
        let near = VideoOptions { char_view: 0, map_view: 0, ..Default::default() };
        assert_eq!((near.char_distance(), near.sector_radius(), near.object_distance()), (Some(7_812.5), 2, 15_625.0));
        assert_eq!(VideoOptions { char_view: 9, map_view: 9, ..Default::default() }.sector_radius(), 5);
        // Nature, ciel, brouillard, reflets : valeurs de COption.ini ; un ancien fichier sans ces
        // clés les reçoit aussi.
        assert_eq!((v.nature, v.sky, v.fog, v.lens_flare), (2, true, true, true));
        assert_eq!(v.nature_density(), 1.0);
        let old: VideoOptions = toml::from_str("char_view = 1\nmap_view = 2\n").unwrap();
        assert_eq!((old.map_view, old.nature, old.fog), (2, 2, true));
    }
}
