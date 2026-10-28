//! Commutateur d'équipement côté moteur. Protocole : `archrust_netclient::itemset` (AgpmItemSet 0x6A) ;
//! fenêtre d'Archonia : `hud_itemset.rs`.
//!
//! Comportement relevé dans Archonia (AgcmItemSet, sha256 e9c7609b…) :
//! * état poussé par le serveur : drapeau (op 3), jeu actif (op 4), contenu (op 5) ; le client ne demande rien ;
//! * `UI_SwitchEditor` (onglet « Switch » de `UI_SkillMacro`, `Ctrl+M` = `Hotkey=2:50`) : 5 profils (« Default »,
//!   « Profile %d »), 10 configurations par profil (« New Switch », « New Switch %d »), un raccourci par
//!   configuration ; « Capture Set A/B » envoie l'op 0 sans confirmation, « Delete » l'op 2 pour A et B ; clic
//!   droit sur une case : op 7 ; glisser une case sur une autre de la même grille : op 6 ;
//! * raccourci d'une configuration (`FUN_1406bbb90`) : 1er appui → jeu A (B si A est vide), nouvel appui sur la
//!   même configuration → l'autre jeu (anti-rebond 50 ms) ; op 1 ;
//! * fichier `ItemSwitch_<nom échappé _%02X>.dat` (`FUN_1406b7c20`, `FUN_1406bb4b0`) : u32 version 3, u32
//!   profil courant, 5 profils de 0x20C octets : nom[32], 10 configurations de 0x30 octets (nom[32], deux u32
//!   réservés, i32 raccourci (-1 aucun), i32 utilisée), i32 -1, i32 -1, i32 valide ;
//! * messages en anglais codés en dur (« Switch to set %d? », « Press the hotkey now (Esc cancels)... »…).
//!
//! Écarts : le fichier est rangé dans `saved-variables/` (pas `ini\`) ; le raccourci y est noté en code de
//! touche Windows (VK) avec les modificateurs en bits 16-18 (Maj, Ctrl, Alt), le codage d'Archonia n'étant pas
//! relevé ; l'onglet « Macros » (`UI_SkillMacro`) n'existe pas encore dans ArchRust ; les anciens raccourcis
//! `CBItemSetSlotN` / `CBItemSetSlotNSave` (sans contrôle dans l'ini) passent par `/switch N` et `/saveset N`.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, SwitchUi, SwitchView};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::itemset::{parse_items, slot, ItemSetCommand, ItemSetEvent, CONFIGS, PROFILES, SLOTS};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE_VERSION: u32 = 3;
const PROFILE_SIZE: usize = 0x20c;
const CONFIG_SIZE: usize = 0x30;
const NAME_SIZE: usize = 32;
/// Anti-rebond du déclencheur d'une configuration.
const TRIGGER_DEBOUNCE: Duration = Duration::from_millis(50);

/// Modificateurs du raccourci (bits 16-18).
const MOD_SHIFT: i32 = 1 << 16;
const MOD_CTRL: i32 = 2 << 16;
const MOD_ALT: i32 = 4 << 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchConfig {
    pub name: String,
    reserved: [u32; 2],
    /// Raccourci (-1 : aucun).
    pub hotkey: i32,
    pub used: bool,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        SwitchConfig { name: String::new(), reserved: [0; 2], hotkey: -1, used: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchProfile {
    pub name: String,
    pub configs: Vec<SwitchConfig>,
    extra: [i32; 2],
}

impl SwitchProfile {
    fn new(index: usize) -> Self {
        let name = if index == 0 { "Default".to_string() } else { format!("Profile {}", index + 1) };
        SwitchProfile { name, configs: vec![SwitchConfig::default(); CONFIGS], extra: [-1, -1] }
    }
}

/// Profils par défaut (`FUN_1406b7c20`).
fn default_profiles() -> Vec<SwitchProfile> {
    (0..PROFILES).map(SwitchProfile::new).collect()
}

fn put_name(out: &mut Vec<u8>, name: &str) {
    let mut b: Vec<u8> = name.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).take(NAME_SIZE - 1).collect();
    b.resize(NAME_SIZE, 0);
    out.extend(b);
}

fn get_name(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

/// Contenu du fichier `ItemSwitch_*.dat` (version 3).
pub fn encode_file(current: usize, profiles: &[SwitchProfile]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + PROFILES * PROFILE_SIZE);
    out.extend(FILE_VERSION.to_le_bytes());
    out.extend((current as u32).to_le_bytes());
    for p in profiles.iter().take(PROFILES) {
        put_name(&mut out, &p.name);
        for c in p.configs.iter().take(CONFIGS) {
            put_name(&mut out, &c.name);
            out.extend(c.reserved[0].to_le_bytes());
            out.extend(c.reserved[1].to_le_bytes());
            out.extend(c.hotkey.to_le_bytes());
            out.extend((c.used as i32).to_le_bytes());
        }
        out.extend(p.extra[0].to_le_bytes());
        out.extend(p.extra[1].to_le_bytes());
        out.extend(1i32.to_le_bytes());
    }
    out
}

/// Profils d'un fichier version 3 (profils par défaut sinon, profil courant ramené à 0 au-delà de 4).
pub fn decode_file(bytes: &[u8]) -> (usize, Vec<SwitchProfile>) {
    let mut profiles = default_profiles();
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u32_at(0) != Some(FILE_VERSION) {
        return (0, profiles);
    }
    let current = u32_at(4).map_or(0, |c| c as usize);
    for (i, p) in profiles.iter_mut().enumerate() {
        let Some(b) = bytes.get(8 + i * PROFILE_SIZE..8 + (i + 1) * PROFILE_SIZE) else { break };
        let name = get_name(&b[..NAME_SIZE]);
        if !name.is_empty() {
            p.name = name;
        }
        for (k, c) in p.configs.iter_mut().enumerate() {
            let o = NAME_SIZE + k * CONFIG_SIZE;
            let int = |x: usize| i32::from_le_bytes([b[o + x], b[o + x + 1], b[o + x + 2], b[o + x + 3]]);
            *c = SwitchConfig { name: get_name(&b[o..o + NAME_SIZE]), reserved: [int(32) as u32, int(36) as u32], hotkey: int(40), used: int(44) != 0 };
        }
        let o = NAME_SIZE + CONFIGS * CONFIG_SIZE;
        p.extra = [i32::from_le_bytes(b[o..o + 4].try_into().unwrap()), i32::from_le_bytes(b[o + 4..o + 8].try_into().unwrap())];
    }
    (if current > 4 { 0 } else { current }, profiles)
}

/// `ItemSwitch_<nom>.dat`, octets hors lettres et chiffres échappés en `_%02X` (`FUN_1406b4b30`).
fn file_path(dir: &Path, name: &str) -> PathBuf {
    let mut safe = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() {
            safe.push(b as char);
        } else {
            safe.push_str(&format!("_{b:02X}"));
        }
    }
    dir.join(format!("ItemSwitch_{safe}.dat"))
}

/// Code de touche Windows (VK) d'une touche egui, s'il est connu.
fn vk_of(key: egui::Key) -> Option<i32> {
    use egui::Key as K;
    let name = key.name();
    if name.len() == 1 {
        let c = name.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as i32);
        }
    }
    Some(match key {
        K::F1 => 0x70,
        K::F2 => 0x71,
        K::F3 => 0x72,
        K::F4 => 0x73,
        K::F5 => 0x74,
        K::F6 => 0x75,
        K::F7 => 0x76,
        K::F8 => 0x77,
        K::F9 => 0x78,
        K::F10 => 0x79,
        K::F11 => 0x7a,
        K::F12 => 0x7b,
        K::Space => 0x20,
        K::Insert => 0x2d,
        K::Home => 0x24,
        K::End => 0x23,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::Minus => 0xbd,
        K::Equals => 0xbb,
        _ => return None,
    })
}

/// Raccourci noté : touche et modificateurs.
fn hotkey_code(key: egui::Key, m: egui::Modifiers) -> Option<i32> {
    let mut code = vk_of(key)?;
    if m.shift {
        code |= MOD_SHIFT;
    }
    if m.ctrl {
        code |= MOD_CTRL;
    }
    if m.alt {
        code |= MOD_ALT;
    }
    Some(code)
}

/// Texte d'un raccourci (« Ctrl+F1 »).
fn hotkey_text(code: i32) -> String {
    if code < 0 {
        return String::new();
    }
    let mut out = String::new();
    for (bit, name) in [(MOD_CTRL, "Ctrl+"), (MOD_ALT, "Alt+"), (MOD_SHIFT, "Shift+")] {
        if code & bit != 0 {
            out.push_str(name);
        }
    }
    let vk = code & 0xffff;
    match vk {
        0x30..=0x39 | 0x41..=0x5a => out.push(vk as u8 as char),
        0x70..=0x7b => out.push_str(&format!("F{}", vk - 0x6f)),
        0x20 => out.push_str("Space"),
        _ => out.push_str(&format!("#{vk}")),
    }
    out
}

/// Confirmation des anciens raccourcis (`/switch`, `/saveset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Equip(i32),
    Save(i32),
}

#[derive(Default)]
pub struct ItemSetGlue {
    pub show: bool,
    flags: Vec<bool>,
    /// Jeu actif (-1 : aucun).
    active: i32,
    contents: HashMap<i32, String>,
    /// IID de chaque identifiant 64 bits (AGPMITEM champ 26).
    keys: HashMap<u64, i32>,
    profiles: Vec<SwitchProfile>,
    current: usize,
    selected: Option<usize>,
    name_edit: String,
    profile_edit: String,
    recording: bool,
    /// Dernier déclenchement : configuration, jeu, heure.
    last_trigger: Option<(usize, usize, Instant)>,
    /// Personnage dont le fichier est chargé.
    owner: Option<String>,
    pending: Option<Pending>,
}

impl ItemSetGlue {
    fn flag(&self, slot: i32) -> bool {
        self.flags.get(slot as usize).copied().unwrap_or(false)
    }

    fn profile(&mut self) -> &mut SwitchProfile {
        if self.profiles.len() < PROFILES {
            self.profiles = default_profiles();
        }
        let c = self.current.min(PROFILES - 1);
        &mut self.profiles[c]
    }

    /// Ferme la fenêtre (Échap) ; vrai si elle était ouverte.
    pub fn close_windows(&mut self) -> bool {
        let open = self.show;
        self.show = false;
        self.recording = false;
        open
    }

    pub fn toggle(&mut self) {
        self.show = !self.show;
        self.recording = false;
    }

    /// Objets d'un jeu : TID et quantité par case.
    fn grid(&self, slot: i32, items: &HashMap<i32, ItemRecord>) -> [Option<(i32, i32)>; 10] {
        let mut out = [None; 10];
        if let Some(text) = self.contents.get(&slot) {
            for (pos, (_, key)) in parse_items(text).into_iter().enumerate().take(10) {
                out[pos] = self.keys.get(&key).and_then(|iid| items.get(iid)).and_then(|i| Some((i.template_id?, i.count.unwrap_or(1))));
            }
        }
        out
    }

    /// Déclencheur d'une configuration du profil courant (`FUN_1406bbb90`) : jeu à équiper.
    fn trigger(&mut self, config: usize, now: Instant) -> Option<i32> {
        if let Some((c, _, t)) = self.last_trigger {
            if c == config && now.duration_since(t) < TRIGGER_DEBOUNCE {
                return None;
            }
        }
        let p = self.current;
        let set = match self.last_trigger {
            Some((c, s, _)) if c == config => 1 - s,
            _ => usize::from(!self.flag(slot(p, config, 0)) && self.flag(slot(p, config, 1))),
        };
        self.last_trigger = Some((config, set, now));
        Some(slot(p, config, set))
    }
}

fn send(net: Option<&NetLink>, cmd: ItemSetCommand) {
    if let Some(net) = net {
        net.send(Command::ItemSet(cmd));
    }
}

/// Fenêtre du commutateur, raccourcis des configurations, confirmations.
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut ItemSetGlue, items: &HashMap<i32, ItemRecord>, owner: Option<&str>, saved_dir: &Path, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    // Fichier du personnage chargé à l'entrée en jeu.
    if let Some(name) = owner.filter(|n| !n.is_empty() && glue.owner.as_deref() != Some(*n)) {
        let (current, profiles) = decode_file(&std::fs::read(file_path(saved_dir, name)).unwrap_or_default());
        glue.current = current;
        glue.profiles = profiles;
        glue.selected = None;
        glue.owner = Some(name.to_string());
        glue.profile_edit = glue.profile().name.clone();
    }
    let save = |glue: &mut ItemSetGlue| {
        if let Some(name) = glue.owner.clone() {
            let path = file_path(saved_dir, &name);
            if let Err(e) = std::fs::create_dir_all(saved_dir).and_then(|_| std::fs::write(&path, encode_file(glue.current, &glue.profiles))) {
                log::warn!("{} : {e}", path.display());
            }
        }
    };
    // Enregistrement d'un raccourci, puis déclencheurs (sans saisie de texte en cours).
    let typing = ctx.memory(|m| m.focused().is_some());
    let presses: Vec<(egui::Key, egui::Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    if glue.recording {
        for (key, m) in presses {
            let code = match key {
                egui::Key::Escape => None,
                egui::Key::Delete => Some(-1),
                _ => match hotkey_code(key, m) {
                    Some(c) => Some(c),
                    None => continue,
                },
            };
            glue.recording = false;
            if let (Some(code), Some(k)) = (code, glue.selected) {
                glue.profile().configs[k].hotkey = code;
                save(glue);
            }
            break;
        }
    } else if !typing && owner.is_some() {
        for (key, m) in presses {
            let Some(code) = hotkey_code(key, m) else { continue };
            let found = glue.profile().configs.iter().position(|c| c.used && c.hotkey == code);
            if let Some(slot) = found.and_then(|k| glue.trigger(k, Instant::now())) {
                send(net, ItemSetCommand::Equip(slot));
            }
        }
    }
    if glue.show {
        if glue.profiles.len() < PROFILES {
            glue.profiles = default_profiles();
        }
        let p = glue.current.min(PROFILES - 1);
        let profile = glue.profiles[p].clone();
        let configs: [&str; 10] = std::array::from_fn(|k| profile.configs.get(k).filter(|c| c.used).map_or("", |c| c.name.as_str()));
        let hotkey = if glue.recording { "Press a key... (Esc to cancel)".to_string() } else { glue.selected.map(|k| hotkey_text(profile.configs[k].hotkey)).unwrap_or_default() };
        let grids = match glue.selected {
            Some(k) => [glue.grid(slot(p, k, 0), items), glue.grid(slot(p, k, 1), items)],
            None => [[None; 10]; 2],
        };
        let active_set = glue.selected.and_then(|k| (0..2).find(|s| slot(p, k, *s) == glue.active));
        let mut out = Vec::new();
        let (mut name_edit, mut profile_edit) = (std::mem::take(&mut glue.name_edit), std::mem::take(&mut glue.profile_edit));
        let view = SwitchView { profile: &profile.name, profile_edit: &mut profile_edit, configs, selected: glue.selected, name_edit: &mut name_edit, hotkey: &hotkey, grids, active_set };
        crate::hud::draw_switch_editor(ctx, hud, assets, view, &mut out);
        glue.name_edit = name_edit;
        glue.profile_edit = profile_edit;
        for a in out {
            match a {
                SwitchUi::Close => {
                    glue.close_windows();
                }
                SwitchUi::Macros => log::info!("commutateur : onglet « Macros » (UI_SkillMacro) absent d'ArchRust"),
                SwitchUi::NextProfile => {
                    glue.current = (p + 1) % PROFILES;
                    glue.selected = None;
                    glue.name_edit.clear();
                    glue.profile_edit = glue.profile().name.clone();
                    save(glue);
                }
                SwitchUi::Rename => {
                    let name = glue.profile_edit.trim().to_string();
                    if name.is_empty() {
                        ui.push_chat("Profile name cannot be empty.");
                    } else {
                        glue.profile().name = name;
                        save(glue);
                    }
                }
                SwitchUi::SelectConfig(k) => {
                    glue.selected = Some(k);
                    glue.name_edit = profile.configs[k].name.clone();
                    glue.recording = false;
                }
                SwitchUi::SetHotkey => {
                    glue.recording = true;
                    ui.push_chat("Press the hotkey now (Esc cancels)...");
                    ui.push_chat("Press Delete to remove hotkey from this switch.");
                }
                SwitchUi::Save => {
                    let Some(k) = glue.selected else { continue };
                    let name = glue.name_edit.trim().to_string();
                    let c = &mut glue.profile().configs[k];
                    if !name.is_empty() {
                        c.name = name;
                    }
                    c.used = true;
                    log::info!("SwitchEditor: Save -> config {k}");
                    save(glue);
                }
                SwitchUi::Delete => {
                    let Some(k) = glue.selected.take() else { continue };
                    send(net, ItemSetCommand::Clear(slot(p, k, 0)));
                    send(net, ItemSetCommand::Clear(slot(p, k, 1)));
                    glue.profile().configs[k] = SwitchConfig::default();
                    glue.name_edit.clear();
                    save(glue);
                }
                SwitchUi::Add => {
                    let used = glue.profile().configs.iter().filter(|c| c.used).count();
                    let Some(k) = glue.profile().configs.iter().position(|c| !c.used) else { continue };
                    let name = if used == 0 { "New Switch".to_string() } else { format!("New Switch {}", used + 1) };
                    glue.profile().configs[k] = SwitchConfig { name: name.clone(), used: true, ..Default::default() };
                    glue.selected = Some(k);
                    glue.name_edit = name;
                    save(glue);
                }
                SwitchUi::Capture(set) => {
                    let Some(k) = glue.selected else { continue };
                    send(net, ItemSetCommand::Save(slot(p, k, set)));
                    if !glue.profile().configs[k].used {
                        glue.profile().configs[k].used = true;
                        save(glue);
                    }
                }
                SwitchUi::Move { set, from, to } => {
                    if let Some(k) = glue.selected.filter(|_| from < 10 && to < 10 && from != to) {
                        send(net, ItemSetCommand::Move { slot: slot(p, k, set), from: from as u8, to: to as u8 });
                    }
                }
                SwitchUi::Remove { set, pos } => {
                    if let Some(k) = glue.selected.filter(|_| pos < 10) {
                        send(net, ItemSetCommand::Remove { slot: slot(p, k, set), pos: pos as u8 });
                    }
                }
            }
        }
    }
    if let Some(p) = glue.pending {
        let text = match p {
            Pending::Equip(s) if glue.flag(s) => format!("Switch to set {}?", s + 1),
            Pending::Equip(s) => format!("Switch to empty set {} (unequip everything)?", s + 1),
            Pending::Save(s) => format!("Save your currently equipped gear into set {}?", s + 1),
        };
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("itemset-confirm", format!("{p:?}")), &text) {
            glue.pending = None;
            if yes {
                send(net, match p {
                    Pending::Equip(s) => ItemSetCommand::Equip(s),
                    Pending::Save(s) => ItemSetCommand::Save(s),
                });
            }
        }
    }
}

impl App {
    /// Paquet AgpmItemSet et identifiants 64 bits des objets.
    pub(super) fn on_itemset_event(&mut self, ev: ItemSetEvent) {
        let glue = &mut self.itemset;
        if glue.flags.len() < SLOTS as usize {
            glue.flags = vec![false; SLOTS as usize];
            glue.active = -1;
        }
        match ev {
            ItemSetEvent::Flag { slot, filled } => glue.flags[slot as usize] = filled,
            ItemSetEvent::Active(slot) => glue.active = slot,
            ItemSetEvent::Content { slot, items } => {
                glue.contents.insert(slot, items);
            }
            ItemSetEvent::ItemKey { iid, key } => {
                glue.keys.retain(|_, v| *v != iid);
                glue.keys.insert(key, iid);
            }
        }
    }

    /// Connexion au monde perdue : état oublié (le serveur le renvoie), fichier relu.
    pub(super) fn itemset_reset(&mut self) {
        let show = self.itemset.show;
        self.itemset = ItemSetGlue { show, ..Default::default() };
    }

    /// `/commutateur` (`/itemswitch`) : fenêtre ; `/switch N`, `/saveset N` : anciens raccourcis des jeux 1 à 5.
    pub(super) fn itemset_slash(&mut self, cmd: &str, arg: &str) -> bool {
        match cmd {
            "/commutateur" | "/itemswitch" => self.itemset.toggle(),
            "/switch" | "/saveset" => {
                let Some(n) = arg.trim().parse::<i32>().ok().filter(|n| (1..=5).contains(n)) else { return true };
                self.itemset.pending = Some(if cmd == "/switch" { Pending::Equip(n - 1) } else { Pending::Save(n - 1) });
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_version_3() {
        let mut profiles = default_profiles();
        profiles[1].name = "PvP".into();
        profiles[1].configs[2] = SwitchConfig { name: "Arc".into(), hotkey: 0x70 | MOD_CTRL, used: true, ..Default::default() };
        let bytes = encode_file(1, &profiles);
        assert_eq!(bytes.len(), 8 + 5 * 0x20c);
        assert_eq!(&bytes[..4], &3u32.to_le_bytes());
        let (current, back) = decode_file(&bytes);
        assert_eq!(current, 1);
        assert_eq!(back, profiles);
        assert_eq!((back[0].name.as_str(), back[4].name.as_str()), ("Default", "Profile 5"));
        // Autre version : profils par défaut.
        let (c, d) = decode_file(&[2, 0, 0, 0]);
        assert_eq!((c, d), (0, default_profiles()));
    }

    #[test]
    fn nom_de_fichier_et_raccourcis() {
        assert_eq!(file_path(Path::new("sv"), "Gm Test!").file_name().unwrap().to_str(), Some("ItemSwitch_Gm_20Test_21.dat"));
        assert_eq!(hotkey_text(0x70 | MOD_CTRL), "Ctrl+F1");
        assert_eq!(hotkey_text(0x31 | MOD_SHIFT), "Shift+1");
        assert_eq!(hotkey_code(egui::Key::A, egui::Modifiers::ALT), Some(0x41 | MOD_ALT));
        assert_eq!(hotkey_text(-1), "");
    }

    #[test]
    fn declencheur_a_b() {
        let mut g = ItemSetGlue { flags: vec![false; 100], ..Default::default() };
        let t0 = Instant::now();
        // A vide, B rempli : B d'abord.
        g.flags[slot(0, 3, 1) as usize] = true;
        assert_eq!(g.trigger(3, t0), Some(slot(0, 3, 1)));
        // Anti-rebond, puis bascule.
        assert_eq!(g.trigger(3, t0 + Duration::from_millis(10)), None);
        assert_eq!(g.trigger(3, t0 + Duration::from_millis(100)), Some(slot(0, 3, 0)));
        assert_eq!(g.trigger(3, t0 + Duration::from_millis(200)), Some(slot(0, 3, 1)));
        // Autre configuration : A.
        assert_eq!(g.trigger(4, t0 + Duration::from_millis(300)), Some(slot(0, 4, 0)));
    }
}
