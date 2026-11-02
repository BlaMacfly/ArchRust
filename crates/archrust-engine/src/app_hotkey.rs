//! Touches personnalisables d'Archonia (AgcmUIHotkey) et fenêtre de sortie `UI_EXIT`.
//!
//! **Table des touches** : blocs `Hotkey=<modificateur>:<code>` de la section `[EventMap]` d'`ini/UI_1024X768.ini`
//! (`HotkeyFlag=1` : modifiable, `HotkeyDesc` : libellé, `Action=…` : fonctions d'Archonia). Le code est un
//! code DirectInput (DIK, position physique de la touche) ; le modificateur vaut 0 aucun, 1 Maj, 2 Ctrl, 3 Alt,
//! 4 Ctrl+Maj (préfixes « Shift+ », « Ctrl+ », « Alt+ », « Ctrl+Shift+ » de FUN_140745f50). Un bloc redéfini
//! plus loin remplace le premier (`Hotkey=1:16` « Self Quest » est repris en `HotkeyFlag=0` : il n'est pas dans
//! la liste du jeu). Les actions reconnues sont branchées sur les fonctions d'ArchRust ([`HotkeyAction`]) ; les
//! ajouts d'Archonia (`CashMallOpenUI` « Chantra Shop », `ItemAutoPickup` « Collect item », `UI_SkillMacro`,
//! `Raider_ToggleUI`, `ExpandedInventory_UIUpdate`) sont écartés : sans effet et absents de la fenêtre.
//! `CInven_UIOpen` « Chantra Bag » ouvre l'inventaire Cash (statut 51, `app_petbag.rs`), sans la boutique. Déplacements (W A S D) : sans action dans l'EventMap, retrouvés par leur couple par
//! défaut (`0:17`, `0:30`, `0:31`, `0:32`, comme FUN_14054e900).
//!
//! **Liste modifiable** : les blocs `HotkeyFlag=1`, triés par code puis par modificateur (ordre du fichier
//! `Ini/UI_Hotkey.ini` écrit par Archonia : `0:4`, `1:4`, `2:4`, `3:4`, `0:5`…). Affichage (FUN_14054e900) :
//! « Move Forward » est montré juste avant « Move Left », à la place de la ligne qu'il échange.
//!
//! **Fichier `Ini/UI_Hotkey.ini`** (FUN_14054e1e0 lecture, FUN_14054ee70 écriture, chiffré RC4 « 1111 ») :
//! `[CONFIG] VERSION=4` puis `[CustomHotkey<i>]` `DefaultType/DefaultCode/CustomType/CustomCode`, lignes en CRLF.
//! Archonia supprime un fichier dont le nombre de sections n'est pas N + 1 ou la version pas 4, et ne reprend
//! une entrée que si son défaut est celui de l'entrée de même rang. Le dossier `Game` est partagé avec Archonia,
//! qui ajoute par son code deux touches à la liste (`0:67` F9 et `4:50` Ctrl+Maj+M, réglages de ses macros et du
//! commutateur d'équipement) : son fichier a 83 entrées pour les 81 de l'EventMap. ArchRust **cohabite** : il ne
//! supprime jamais le fichier ; il en applique les entrées qu'il connaît (même défaut, quel que soit le rang),
//! garde les autres telles quelles et, à l'écriture (« Confirm » seulement), réécrit le fichier lu dans le même
//! ordre avec ses seules entrées connues changées. Fichier illisible ou d'une autre version : ignoré (défauts),
//! jamais écrasé tant que le joueur ne confirme pas ; sans fichier valide, l'écriture ajoute aux 81 entrées les
//! deux d'Archonia avec leur défaut ([`ARCHONIA_CODE_KEYS`]) pour qu'Archonia le relise.
//!
//! **`UI_Hotkey`** (« Custom Keys ») : ouverte par `HotKey_FuncOnUIOpen` (relit le fichier ; Archonia y écrit
//! ses défauts s'il est refusé, ArchRust non) ; clic sur une case de touche → la touche suivante la remplace, refusée avec
//! `Hotkey_EventDispOverwriteMsg` (« Duplicated shortcut key. ») si une autre entrée l'a déjà (FUN_14054d590) ;
//! « Confirm » applique et écrit, « Cancel » revient aux touches appliquées, « Reset » aux défauts (écrits au
//! « Confirm » seulement). Échap annule la saisie (choix d'ArchRust : Échap n'est pas modifiable).
//!
//! **`UI_EXIT`** (Échap sans fenêtre à fermer, `EXIT_Open_Window` ; « Menu (Esc) » d'`UI_MainMenuOptions`) :
//! « Return to the game », « Select Character » (`EXIT_ReturnLoginCharacterSelect` : contrôles puis AgpmReturnToLogin
//! 0x2F op 1, retour à la scène de sélection, voir `app_return_login.rs`), « Options » (`Option_Video`),
//! « Custom Keys » (`UI_Hotkey`), « Exit Game » (`Quit_Event_Call` → `MainUI_Quit_Question`, boîte oui / non). Titre `LoginGameServerName` : « <monde> » + sysstr 458.
//!
//! Commandes propres à ArchRust gardées hors de la table : Espace (ramasser), Ctrl+M (commutateur
//! d'équipement), console déplacée sur Maj+` (`Hotkey=1:41`, sans action dans l'EventMap) car ` est « Switch
//! quick slot up ». F n'attaque plus : c'est « Friends » d'Archonia (attaque au clic droit ou par la barre).

use super::App;
use crate::assets::Assets;
use crate::hud::{ExitUi, HotkeyRow, HotkeyUi, HotkeyView, HudState};
use archrust_data::ini::{IniSection, IniTable};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use winit::keyboard::{KeyCode, ModifiersState};

/// Version du fichier `UI_Hotkey.ini` (FUN_14054e1e0).
pub const FILE_VERSION: i64 = 4;
/// Lignes visibles de la liste d'`UI_Hotkey` (`CONTROL_26981` 322 px, lignes `CONTROL_10154` de 32 px).
pub const HOTKEY_ROWS: usize = 10;

/// Modificateur d'une touche d'Archonia.
pub mod kind {
    pub const NONE: u8 = 0;
    pub const SHIFT: u8 = 1;
    pub const CTRL: u8 = 2;
    pub const ALT: u8 = 3;
    pub const CTRL_SHIFT: u8 = 4;
}

/// Couple (modificateur, code DIK) d'Archonia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Key {
    pub kind: u8,
    pub code: u8,
}

/// Bloc `Hotkey=` de l'EventMap.
#[derive(Debug, Clone, PartialEq)]
pub struct HotkeyDef {
    pub key: Key,
    pub flag: bool,
    pub desc: String,
    pub actions: Vec<String>,
}

/// Fonction d'ArchRust associée à une action d'Archonia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// `QuickBeltUseHPPotion` (0) / `QuickBeltUseMPPotion` (1).
    Potion(usize),
    /// `QuickBeltUse_<couche>_<case>` : couche 1 à 4, case 1 à 10.
    QuickBelt { layer: usize, col: usize },
    /// `Targeting_FuncAutoTargetingF` / `…B`.
    Target { reverse: bool },
    /// `QuestMainHoeKey_Open2` (journal des quêtes).
    Quests,
    /// `Chat_ReplyImmediate`.
    Reply,
    /// `MAIL_LIST_OPEN`.
    Mailbox,
    /// `Inventory_UIUpdate`.
    Inventory,
    /// `1:3:1:1:Option_Video`.
    Options,
    /// `PartyOptionUI_OpenUI`.
    PartyOptions,
    /// `Chat_Active_Input`.
    Chat,
    /// `1:3:0:1:UI_BuddyList`.
    Friends,
    /// `Guild_Notice_Open_Request`.
    Guild,
    /// `PDSkill_Open`.
    CraftSkills,
    /// `Skill_UI_Open`.
    Skills,
    /// `QuickBeltLayerUp` (+1) / `QuickBeltLayerDown` (−1).
    BeltLayer(i32),
    /// `1:3:0:1:UI_CreateChatRoom`.
    CreateChatRoom,
    /// `1:3:0:1:UI_EnterChatRoom`.
    EnterChat,
    /// `1:3:0:1:Character_Status`.
    Character,
    /// `1:3:0:1:UI_BanList`.
    IgnoreList,
    /// `WorldMap_Toggle`.
    WorldMap,
    /// `SubInventory_UIUpdate` (« Pet Inventory », sac du familier, voir `app_petbag.rs`).
    PetInventory,
    /// `CInven_UIOpen` (« Chantra Bag », inventaire Cash, voir `app_petbag.rs`).
    CashInventory,
    /// `1:3:0:1:UI_TitleCurrent` (« Open Title Window », voir `app_titles.rs`).
    Titles,
    /// Déplacement tenu : 0 avant, 1 arrière, 2 gauche, 3 droite (indices de `App::keys`).
    Move(usize),
}

/// Actions ajoutées par Archonia, écartées (voir l'en-tête).
const ARCHONIA_ADDITIONS: [&str; 5] = ["CashMallOpenUI", "ItemAutoPickup", "UI_SkillMacro", "Raider_ToggleUI", "ExpandedInventory_UIUpdate"];

/// Blocs `Hotkey=` d'une section `[EventMap]`, dans l'ordre ; un bloc redéfini remplace le premier.
pub fn parse_event_map(sec: &IniSection) -> Vec<HotkeyDef> {
    let mut out: Vec<HotkeyDef> = Vec::new();
    let mut current: Option<HotkeyDef> = None;
    let push = |d: Option<HotkeyDef>, out: &mut Vec<HotkeyDef>| {
        if let Some(d) = d {
            out.retain(|o| o.key != d.key);
            out.push(d);
        }
    };
    for (k, v) in &sec.entries {
        match k.as_str() {
            "Hotkey" => {
                push(current.take(), &mut out);
                current = parse_key(v).map(|key| HotkeyDef { key, flag: false, desc: String::new(), actions: Vec::new() });
            }
            "EventName" => push(current.take(), &mut out),
            "HotkeyFlag" => {
                if let Some(d) = current.as_mut() {
                    d.flag = v.trim() == "1";
                }
            }
            "HotkeyDesc" => {
                if let Some(d) = current.as_mut() {
                    d.desc = v.trim().to_string();
                }
            }
            "Action" => {
                if let Some(d) = current.as_mut() {
                    d.actions.push(v.trim().to_string());
                }
            }
            _ => {}
        }
    }
    push(current.take(), &mut out);
    out
}

/// `<modificateur>:<code>`.
fn parse_key(v: &str) -> Option<Key> {
    let (a, b) = v.trim().split_once(':')?;
    Some(Key { kind: a.trim().parse().ok()?, code: b.trim().parse().ok()? })
}

/// Vrai pour les actions ajoutées par Archonia.
pub fn is_archonia_addition(def: &HotkeyDef) -> bool {
    def.actions.iter().any(|a| ARCHONIA_ADDITIONS.iter().any(|x| a.ends_with(x)))
}

/// Fonction d'ArchRust d'un bloc (première action reconnue) ; `None` : sans effet.
pub fn action_of(def: &HotkeyDef) -> Option<HotkeyAction> {
    if is_archonia_addition(def) {
        return None;
    }
    let fixed = |a: &str| -> Option<HotkeyAction> {
        use HotkeyAction::*;
        if let Some(rest) = a.strip_prefix("3:QuickBeltUse_") {
            let (l, c) = rest.split_once('_')?;
            let (layer, col): (usize, usize) = (l.parse().ok()?, c.parse().ok()?);
            return ((1..=4).contains(&layer) && (1..=10).contains(&col)).then_some(QuickBelt { layer, col });
        }
        // Ouvertures de fenêtres `1:<1|3>:x:y:Fenêtre` (les fermetures `1:2` accompagnent l'ouverture).
        if let Some(rest) = a.strip_prefix("1:") {
            let mut p = rest.splitn(4, ':');
            let mode = p.next()?;
            let window = p.nth(2)?;
            if mode == "2" {
                return None;
            }
            return match window {
                "Option_Video" => Some(Options),
                "UI_BuddyList" => Some(Friends),
                "UI_CreateChatRoom" => Some(CreateChatRoom),
                "UI_EnterChatRoom" => Some(EnterChat),
                "Character_Status" => Some(Character),
                "UI_BanList" => Some(IgnoreList),
                "UI_TitleCurrent" => Some(Titles),
                _ => None,
            };
        }
        Some(match a.strip_prefix("3:")? {
            "QuickBeltUseHPPotion" => Potion(0),
            "QuickBeltUseMPPotion" => Potion(1),
            "Targeting_FuncAutoTargetingF" => Target { reverse: false },
            "Targeting_FuncAutoTargetingB" => Target { reverse: true },
            "QuestMainHoeKey_Open2" => Quests,
            "Chat_ReplyImmediate" => Reply,
            "MAIL_LIST_OPEN" => Mailbox,
            "Inventory_UIUpdate" => Inventory,
            "PartyOptionUI_OpenUI" => PartyOptions,
            "Chat_Active_Input" => Chat,
            "Guild_Notice_Open_Request" => Guild,
            "PDSkill_Open" => CraftSkills,
            "Skill_UI_Open" => Skills,
            "QuickBeltLayerUp" => BeltLayer(1),
            "QuickBeltLayerDown" => BeltLayer(-1),
            "WorldMap_Toggle" => WorldMap,
            "SubInventory_UIUpdate" => PetInventory,
            "CInven_UIOpen" => CashInventory,
            _ => return None,
        })
    };
    if let Some(a) = def.actions.iter().find_map(|a| fixed(a)) {
        return Some(a);
    }
    match (def.key.kind, def.key.code) {
        (0, 17) => Some(HotkeyAction::Move(0)),
        (0, 31) => Some(HotkeyAction::Move(1)),
        (0, 30) => Some(HotkeyAction::Move(2)),
        (0, 32) => Some(HotkeyAction::Move(3)),
        _ => None,
    }
}

/// Entrée de la liste modifiable.
#[derive(Debug, Clone, PartialEq)]
pub struct HotkeyEntry {
    pub default: Key,
    pub desc: String,
    pub action: Option<HotkeyAction>,
    /// Ajout d'Archonia : compté dans le fichier, absent de la fenêtre.
    pub hidden: bool,
}

/// Liste modifiable : blocs `HotkeyFlag=1` triés par code puis par modificateur.
pub fn build_entries(defs: &[HotkeyDef]) -> Vec<HotkeyEntry> {
    let mut list: Vec<&HotkeyDef> = defs.iter().filter(|d| d.flag).collect();
    list.sort_by_key(|d| (d.key.code, d.key.kind));
    list.into_iter()
        .map(|d| HotkeyEntry { default: d.key, desc: d.desc.clone(), action: action_of(d), hidden: is_archonia_addition(d) })
        .collect()
}

/// Ordre d'affichage (FUN_14054e900) : rang → entrée ; « Move Forward » (`0:17`) échange sa ligne avec celle qui
/// précède « Move Left » (`0:30`).
pub fn display_order(entries: &[HotkeyEntry]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    let find = |k: Key| entries.iter().position(|e| e.default == k);
    if let (Some(w), Some(a)) = (find(Key { kind: 0, code: 0x11 }), find(Key { kind: 0, code: 0x1e })) {
        if a > 0 {
            order.swap(w, a - 1);
        }
    }
    order
}

/// Touches qu'Archonia ajoute par son code à la liste (absentes de l'EventMap), avec leur défaut : écrites quand
/// ArchRust crée le fichier, pour que les 83 entrées attendues par Archonia y soient.
pub const ARCHONIA_CODE_KEYS: [Key; 2] = [Key { kind: 0, code: 67 }, Key { kind: 4, code: 50 }];

/// Entrée `[CustomHotkey<i>]` du fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    pub default: Key,
    pub custom: Key,
}

/// Texte du fichier `UI_Hotkey.ini` (en clair, CRLF), entrées dans l'ordre donné.
pub fn file_text(file: &[FileEntry]) -> String {
    let mut s = format!("[CONFIG]\r\nVERSION={FILE_VERSION}\r\n");
    for (i, e) in file.iter().enumerate() {
        s.push_str(&format!(
            "[CustomHotkey{i}]\r\nDefaultType={}\r\nDefaultCode={}\r\nCustomType={}\r\nCustomCode={}\r\n",
            e.default.kind, e.default.code, e.custom.kind, e.custom.code
        ));
    }
    s
}

/// Entrées d'un fichier `UI_Hotkey.ini` en clair, toutes gardées (connues d'ArchRust ou non) ; `None` : version
/// autre que 4, ou sections qui ne sont pas `[CONFIG]` + `[CustomHotkey0…N-1]` (fichier ignoré, jamais supprimé).
pub fn parse_file(text: &str) -> Option<Vec<FileEntry>> {
    let t = IniTable::parse(text);
    if t.section("CONFIG").and_then(|s| s.get_i64("VERSION")) != Some(FILE_VERSION) {
        return None;
    }
    let int = |s: &IniSection, k: &str| s.get_i64(k).unwrap_or(0);
    let mut out = Vec::new();
    while let Some(s) = t.section(&format!("CustomHotkey{}", out.len())) {
        out.push(FileEntry {
            default: Key { kind: int(s, "DefaultType") as u8, code: int(s, "DefaultCode") as u8 },
            custom: Key { kind: int(s, "CustomType") as u8, code: int(s, "CustomCode") as u8 },
        });
    }
    (t.len() == out.len() + 1).then_some(out)
}

/// Touches à appliquer : pour chaque entrée connue, celle du fichier qui a le même défaut (quel que soit son rang).
pub fn apply_file(entries: &[HotkeyEntry], file: &[FileEntry]) -> Vec<Key> {
    entries.iter().map(|e| file.iter().find(|f| f.default == e.default).map_or(e.default, |f| f.custom)).collect()
}

/// Entrées à écrire : le fichier lu, dans son ordre, ses entrées connues remplacées par `custom` et les autres
/// gardées telles quelles ; sans fichier valide, les entrées connues et [`ARCHONIA_CODE_KEYS`] triées comme la liste.
pub fn merge_file(entries: &[HotkeyEntry], custom: &[Key], read: Option<&[FileEntry]>) -> Vec<FileEntry> {
    let mine = |d: Key| entries.iter().position(|e| e.default == d).map(|i| custom.get(i).copied().unwrap_or(d));
    match read {
        Some(file) => file.iter().map(|f| FileEntry { default: f.default, custom: mine(f.default).unwrap_or(f.custom) }).collect(),
        None => {
            let mut out: Vec<FileEntry> = entries.iter().enumerate().map(|(i, e)| FileEntry { default: e.default, custom: custom.get(i).copied().unwrap_or(e.default) }).collect();
            out.extend(ARCHONIA_CODE_KEYS.iter().filter(|k| !entries.iter().any(|e| e.default == **k)).map(|&k| FileEntry { default: k, custom: k }));
            out.sort_by_key(|f| (f.default.code, f.default.kind));
            out
        }
    }
}

/// Chiffre ou déchiffre un fichier `ini` du jeu (RC4, clé CryptoAPI « 1111 »).
pub fn crypt(bytes: &[u8]) -> Vec<u8> {
    archrust_crypto::capi_crypt(archrust_crypto::KEY_INI, bytes)
}

/// Texte d'un fichier lu sur le disque : chiffré comme Archonia l'écrit, ou déjà en clair.
pub fn file_plain(raw: &[u8]) -> String {
    if raw.first() == Some(&b'[') {
        return String::from_utf8_lossy(raw).into_owned();
    }
    String::from_utf8_lossy(&crypt(raw)).into_owned()
}

/// Modificateur d'Archonia d'après l'état du clavier.
pub fn kind_of(m: ModifiersState) -> u8 {
    match (m.control_key(), m.shift_key(), m.alt_key()) {
        (true, true, _) => kind::CTRL_SHIFT,
        (true, false, _) => kind::CTRL,
        (false, true, _) => kind::SHIFT,
        (false, false, true) => kind::ALT,
        _ => kind::NONE,
    }
}

/// Code DIK d'une touche physique.
pub fn dik_of(code: KeyCode) -> Option<u8> {
    use KeyCode::*;
    Some(match code {
        Escape => 1,
        Digit1 => 2,
        Digit2 => 3,
        Digit3 => 4,
        Digit4 => 5,
        Digit5 => 6,
        Digit6 => 7,
        Digit7 => 8,
        Digit8 => 9,
        Digit9 => 10,
        Digit0 => 11,
        Minus => 12,
        Equal => 13,
        Backspace => 14,
        Tab => 15,
        KeyQ => 16,
        KeyW => 17,
        KeyE => 18,
        KeyR => 19,
        KeyT => 20,
        KeyY => 21,
        KeyU => 22,
        KeyI => 23,
        KeyO => 24,
        KeyP => 25,
        BracketLeft => 26,
        BracketRight => 27,
        Enter => 28,
        ControlLeft => 29,
        KeyA => 30,
        KeyS => 31,
        KeyD => 32,
        KeyF => 33,
        KeyG => 34,
        KeyH => 35,
        KeyJ => 36,
        KeyK => 37,
        KeyL => 38,
        Semicolon => 39,
        Quote => 40,
        Backquote => 41,
        ShiftLeft => 42,
        Backslash => 43,
        KeyZ => 44,
        KeyX => 45,
        KeyC => 46,
        KeyV => 47,
        KeyB => 48,
        KeyN => 49,
        KeyM => 50,
        Comma => 51,
        Period => 52,
        Slash => 53,
        ShiftRight => 54,
        NumpadMultiply => 55,
        AltLeft => 56,
        Space => 57,
        CapsLock => 58,
        F1 => 59,
        F2 => 60,
        F3 => 61,
        F4 => 62,
        F5 => 63,
        F6 => 64,
        F7 => 65,
        F8 => 66,
        F9 => 67,
        F10 => 68,
        NumLock => 69,
        ScrollLock => 70,
        Numpad7 => 71,
        Numpad8 => 72,
        Numpad9 => 73,
        NumpadSubtract => 74,
        Numpad4 => 75,
        Numpad5 => 76,
        Numpad6 => 77,
        NumpadAdd => 78,
        Numpad1 => 79,
        Numpad2 => 80,
        Numpad3 => 81,
        Numpad0 => 82,
        NumpadDecimal => 83,
        IntlBackslash => 86,
        F11 => 87,
        F12 => 88,
        NumpadEnter => 156,
        ControlRight => 157,
        NumpadDivide => 181,
        AltRight => 184,
        Pause => 197,
        Home => 199,
        ArrowUp => 200,
        PageUp => 201,
        ArrowLeft => 203,
        ArrowRight => 205,
        End => 207,
        ArrowDown => 208,
        PageDown => 209,
        Insert => 210,
        Delete => 211,
        _ => return None,
    })
}

/// Touches de modification seules (jamais capturées).
fn is_modifier(dik: u8) -> bool {
    matches!(dik, 29 | 42 | 54 | 56 | 157 | 184)
}

/// Nom d'un code DIK d'après la table d'Archonia (0x140f45a00) ; `None` : touche sans nom (non affichée).
pub fn key_name(dik: u8) -> Option<&'static str> {
    const LETTERS: [(u8, &str); 26] = [
        (16, "Q"), (17, "W"), (18, "E"), (19, "R"), (20, "T"), (21, "Y"), (22, "U"), (23, "I"), (24, "O"), (25, "P"),
        (30, "A"), (31, "S"), (32, "D"), (33, "F"), (34, "G"), (35, "H"), (36, "J"), (37, "K"), (38, "L"),
        (44, "Z"), (45, "X"), (46, "C"), (47, "V"), (48, "B"), (49, "N"), (50, "M"),
    ];
    if let Some((_, n)) = LETTERS.iter().find(|(c, _)| *c == dik) {
        return Some(n);
    }
    Some(match dik {
        1 => "ESC",
        2 => "1",
        3 => "2",
        4 => "3",
        5 => "4",
        6 => "5",
        7 => "6",
        8 => "7",
        9 => "8",
        10 => "9",
        11 => "0",
        12 => "-",
        13 => "=",
        14 => "BACKSP",
        15 => "TAB",
        26 => "[",
        27 => "]",
        28 => "ENTER",
        39 => ";",
        40 => "'",
        41 => "~",
        43 => "\\",
        51 => ",",
        52 => ".",
        53 => "/",
        55 => "TIMES",
        57 => "SPACE",
        58 => "CAPSLK",
        59 => "F1",
        60 => "F2",
        61 => "F3",
        62 => "F4",
        63 => "F5",
        64 => "F6",
        65 => "F7",
        66 => "F8",
        67 => "F9",
        68 => "F10",
        69 => "NUMLOCK",
        71 => "PADHOME",
        72 => "PADUP",
        73 => "PADPGUP",
        74 => "PADMINUS",
        75 => "PADLEFT",
        76 => "PAD5",
        77 => "PADRIGHT",
        78 => "PADPLUS",
        79 => "PADEND",
        80 => "PADDOWN",
        81 => "PADPGDN",
        82 => "PADINS",
        83 => "PADDEL",
        87 => "F11",
        88 => "F12",
        156 => "PADENTER",
        181 => "DIVIDE",
        197 => "NUMLOCK",
        199 => "HOME",
        200 => "UP",
        201 => "PGUP",
        203 => "LEFT",
        205 => "RIGHT",
        207 => "END",
        208 => "DOWN",
        209 => "PGDN",
        210 => "INS",
        211 => "DEL",
        _ => return None,
    })
}

/// Libellé d'une touche (« Shift+ », « Ctrl+ », « Alt+ », « Ctrl+Shift+ » puis le nom, FUN_140745f50).
pub fn key_label(k: Key) -> String {
    let Some(name) = key_name(k.code) else { return String::new() };
    let prefix = match k.kind {
        kind::SHIFT => "Shift+",
        kind::CTRL => "Ctrl+",
        kind::ALT => "Alt+",
        kind::CTRL_SHIFT => "Ctrl+Shift+",
        _ => "",
    };
    format!("{prefix}{name}")
}

/// Liste, touches appliquées et en cours d'édition.
#[derive(Debug, Clone, Default)]
pub struct HotkeyTable {
    pub entries: Vec<HotkeyEntry>,
    /// Touches appliquées (+0x3008 d'AgcmUIHotkey).
    pub applied: Vec<Key>,
    /// Touches de la fenêtre (+0x2808).
    pub edit: Vec<Key>,
}

impl HotkeyTable {
    pub fn new(entries: Vec<HotkeyEntry>) -> Self {
        let defaults: Vec<Key> = entries.iter().map(|e| e.default).collect();
        Self { entries, applied: defaults.clone(), edit: defaults }
    }

    pub fn defaults(&self) -> Vec<Key> {
        self.entries.iter().map(|e| e.default).collect()
    }

    /// Fonction de la touche appuyée (déplacements exclus).
    pub fn action_for(&self, key: Key) -> Option<HotkeyAction> {
        self.applied
            .iter()
            .zip(&self.entries)
            .find(|(k, e)| **k == key && !matches!(e.action, Some(HotkeyAction::Move(_))))
            .and_then(|(_, e)| e.action)
    }

    /// Déplacement tenu par ce code, quel que soit le modificateur.
    pub fn movement(&self, code: u8) -> Option<usize> {
        self.applied.iter().zip(&self.entries).find_map(|(k, e)| match e.action {
            Some(HotkeyAction::Move(d)) if k.code == code => Some(d),
            _ => None,
        })
    }

    /// Saisie d'une touche pour l'entrée `i` (FUN_14054d590) : `false` si une autre entrée l'a déjà.
    pub fn assign(&mut self, i: usize, key: Key) -> bool {
        if self.edit.iter().enumerate().any(|(j, k)| j != i && *k == key) {
            return false;
        }
        if let Some(k) = self.edit.get_mut(i) {
            *k = key;
        }
        true
    }
}

/// Fichier `Ini/UI_Hotkey.ini` du jeu (dossier `ini` existant, quelle que soit sa casse).
fn ini_path(game_dir: &Path) -> PathBuf {
    let dir = std::fs::read_dir(game_dir)
        .ok()
        .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("ini"))))
        .unwrap_or_else(|| game_dir.join("Ini"));
    std::fs::read_dir(&dir)
        .ok()
        .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("UI_Hotkey.ini"))))
        .unwrap_or_else(|| dir.join("UI_Hotkey.ini"))
}

/// Demande de l'interface, traitée par le moteur à l'image suivante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Hotkey(HotkeyUi),
    Exit(ExitUi),
    /// Réponse à `MainUI_Quit_Question`.
    Quit(bool),
}

#[derive(Default)]
pub struct HotkeyGlue {
    table: Option<HotkeyTable>,
    path: Option<PathBuf>,
    /// Entrées du fichier lu (toutes, connues ou non) ; `None` : absent ou ignoré.
    file: Option<Vec<FileEntry>>,
    /// `UI_Hotkey` ouverte.
    window: bool,
    /// Entrée qui attend sa touche.
    capture: Option<usize>,
    /// Première ligne affichée.
    first: usize,
    /// `UI_EXIT` ouverte.
    pub exit: bool,
    quit_question: bool,
    duplicate: bool,
    /// Monde choisi à la connexion (`LoginGameServerName`).
    pub world: Option<String>,
    requests: Vec<Request>,
    events: Option<HashMap<String, String>>,
}

impl HotkeyGlue {
    /// Table construite au premier besoin, touches connues du fichier appliquées.
    fn ensure(&mut self, assets: &Assets) -> &mut HotkeyTable {
        if self.table.is_none() {
            let defs = assets.table("ini", "UI_1024X768.ini").ok().and_then(|t| t.section("EventMap").map(parse_event_map)).unwrap_or_default();
            let mut entries = build_entries(&defs);
            // Libellés du dossier de langue (même table, textes traduits).
            let lang = crate::assets::text_dir();
            if lang != "ini" {
                if let Some(fr) = assets.table(lang, "UI_1024X768.ini").ok().and_then(|t| t.section("EventMap").map(parse_event_map)) {
                    for e in &mut entries {
                        if let Some(d) = fr.iter().find(|d| d.key == e.default).filter(|d| !d.desc.is_empty()) {
                            e.desc = d.desc.clone();
                        }
                    }
                }
            }
            let mut table = HotkeyTable::new(entries);
            self.path = assets.game_dir.as_deref().map(ini_path);
            self.read_file();
            if let Some(file) = &self.file {
                table.applied = apply_file(&table.entries, file);
                table.edit = table.applied.clone();
            }
            log::info!("touches : {} modifiable(s) (EventMap), fichier {}", table.entries.len(), self.path.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "absent".into()));
            self.table = Some(table);
        }
        self.table.as_mut().unwrap()
    }

    /// Lecture du fichier (FUN_14054e1e0) : toutes ses entrées gardées ; illisible ou d'une autre version : ignoré,
    /// jamais supprimé (le dossier `Game` est partagé avec Archonia).
    fn read_file(&mut self) {
        self.file = None;
        let Some(path) = self.path.as_ref() else { return };
        let Ok(raw) = std::fs::read(path) else { return };
        self.file = parse_file(&file_plain(&raw));
        match &self.file {
            Some(f) => log::info!("{} : {} entrée(s)", path.display(), f.len()),
            None => log::info!("{} : version ou forme inattendues, ignoré (touches par défaut, fichier gardé)", path.display()),
        }
    }

    /// Écriture (« Confirm » seulement) : le fichier lu réécrit dans son ordre, entrées connues changées, les autres
    /// à l'identique ; chiffré comme Archonia.
    fn write_file(&mut self) {
        let (Some(path), Some(t)) = (self.path.as_ref(), self.table.as_ref()) else { return };
        let merged = merge_file(&t.entries, &t.applied, self.file.as_deref());
        match std::fs::write(path, crypt(file_text(&merged).as_bytes())) {
            Ok(()) => self.file = Some(merged),
            Err(e) => log::warn!("{} : {e}", path.display()),
        }
    }

    /// `HotKey_FuncOnUIOpen` (FUN_14054d890) : relit le fichier ; refusé, défauts (rien n'est écrit).
    fn open_window(&mut self, assets: &Assets) {
        self.ensure(assets);
        self.read_file();
        let t = self.table.as_mut().unwrap();
        t.applied = match &self.file {
            Some(file) => apply_file(&t.entries, file),
            None => t.defaults(),
        };
        t.edit = t.applied.clone();
        self.window = true;
        self.capture = None;
        self.first = 0;
    }

    /// Lignes affichées (rang → entrée), ajouts d'Archonia retirés.
    fn rows(&self) -> Vec<usize> {
        let Some(t) = &self.table else { return Vec::new() };
        display_order(&t.entries).into_iter().filter(|&i| !t.entries[i].hidden).collect()
    }

    /// Fenêtre ou boîte de `UI_Hotkey` / `UI_EXIT` fermée par Échap : `true` si Échap a servi.
    fn escape(&mut self) -> bool {
        if self.quit_question || self.duplicate {
            // Les boîtes traitent Échap elles-mêmes.
            return true;
        }
        if self.window {
            self.window = false;
            self.capture = None;
            return true;
        }
        std::mem::take(&mut self.exit)
    }
}

/// Fenêtres `UI_EXIT`, `UI_Hotkey` et leurs boîtes.
pub fn panel(ctx: &egui::Context, glue: &mut HotkeyGlue, hud: &mut HudState, assets: &Assets) {
    if glue.exit {
        let world = glue.world.clone().unwrap_or_default();
        let suffix = super::npc_glue::sysstr(assets, 458).unwrap_or_default();
        let title = format!("{world} {suffix}").trim().to_string();
        let mut out = Vec::new();
        crate::hud::draw_exit(ctx, hud, assets, &title, &mut out);
        glue.requests.extend(out.into_iter().map(Request::Exit));
    }
    if glue.window {
        let rows = glue.rows();
        let t = glue.table.as_ref().unwrap();
        let view = HotkeyView {
            rows: rows.iter().map(|&i| HotkeyRow { desc: t.entries[i].desc.clone(), key: key_label(t.edit[i]) }).collect(),
            first: glue.first,
            capturing: glue.capture.and_then(|c| rows.iter().position(|&i| i == c)),
        };
        let mut out = Vec::new();
        crate::hud::draw_hotkey(ctx, hud, assets, &view, &mut out);
        // Rang affiché → entrée de la liste.
        glue.requests.extend(out.into_iter().map(|r| match r {
            HotkeyUi::Arm(row) => Request::Hotkey(HotkeyUi::Arm(rows.get(row).copied().unwrap_or(usize::MAX))),
            other => Request::Hotkey(other),
        }));
    }
    if glue.duplicate {
        let text = super::portal_glue::ui_event_cached(&mut glue.events, assets, "Hotkey_EventDispOverwriteMsg");
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.duplicate = false;
        }
    }
    if glue.quit_question {
        let text = super::portal_glue::ui_event_cached(&mut glue.events, assets, "MainUI_Quit_Question");
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "archonia-quit-question", &text) {
            glue.quit_question = false;
            glue.requests.push(Request::Quit(yes));
        }
    }
}

impl App {
    /// Touche du clavier d'après la table d'Archonia : saisie d'`UI_Hotkey`, déplacements, puis actions (en jeu).
    /// Rend `true` si la touche a servi.
    pub(super) fn hotkey_key(&mut self, code: KeyCode, down: bool, repeat: bool) -> bool {
        let Some(dik) = dik_of(code) else { return false };
        let mods = self.addon_glue.modifiers;
        let glue = &mut self.hotkey;
        glue.ensure(&self.assets);
        // Saisie : la prochaine touche appuyée (les relâchements passent, un déplacement ne reste pas bloqué).
        if let (true, Some(i), true, false) = (glue.window, glue.capture, down, is_modifier(dik)) {
            glue.capture = None;
            if dik != 1 {
                let key = Key { kind: kind_of(mods), code: dik };
                if !glue.table.as_mut().unwrap().assign(i, key) {
                    glue.duplicate = true;
                }
            }
            return true;
        }
        let table = glue.table.as_ref().unwrap();
        if let Some(dir) = table.movement(dik) {
            self.keys[dir] = down;
            return true;
        }
        if !down || self.phase != super::Phase::InWorld {
            return false;
        }
        let Some(action) = table.action_for(Key { kind: kind_of(mods), code: dik }) else { return false };
        if repeat && !matches!(action, HotkeyAction::Target { .. }) {
            return true;
        }
        self.hotkey_run(action);
        true
    }

    /// Fonction d'ArchRust d'une action d'Archonia.
    fn hotkey_run(&mut self, action: HotkeyAction) {
        use HotkeyAction::*;
        match action {
            Potion(i) => self.skills_hotkey_potion(i),
            QuickBelt { layer, col } => self.skills_hotkey_belt(layer, col),
            Target { reverse } => self.tab_target(reverse),
            Quests => {
                self.npc_slash("/quetes", "");
            }
            Reply => {
                if let Some(name) = self.ui.last_whisper.clone() {
                    self.ui.whisper_target = Some(name);
                    self.ui.chat_focus = true;
                }
            }
            Mailbox => self.mail.toggle(),
            Inventory => self.ui.show_inventory = !self.ui.show_inventory,
            Options => {
                self.ui.show_options = !self.ui.show_options;
                if self.ui.show_options {
                    self.hud.options_tab = 0;
                }
            }
            PartyOptions => self.party.frames_ui.options = !self.party.frames_ui.options,
            Chat => self.ui.chat_focus = true,
            Friends => self.buddy.toggle_friends(),
            Guild => self.guild_toggle(),
            CraftSkills => self.product.show_skills = !self.product.show_skills,
            Skills => self.skills.show_book = !self.skills.show_book,
            BeltLayer(delta) => self.skills_hotkey_layer(delta),
            CreateChatRoom => self.chat.open_create(),
            EnterChat => self.chat.open_enter(),
            Character => self.item_ui.show_character = !self.item_ui.show_character,
            IgnoreList => self.buddy.toggle_bans(),
            WorldMap => self.worldmap_toggle(),
            PetInventory => self.petbag_toggle_bag(),
            CashInventory => self.petbag.toggle_cash(),
            Titles => self.titles_toggle_current(),
            Move(_) => {}
        }
    }

    /// Échap : boîtes et fenêtres d'`UI_Hotkey` / `UI_EXIT` d'abord.
    pub(super) fn hotkey_escape(&mut self) -> bool {
        self.hotkey.escape()
    }

    /// `UI_EXIT` ouverte ou fermée (Échap sans fenêtre à fermer, « Menu (Esc) »).
    pub(super) fn hotkey_toggle_exit(&mut self) {
        self.hotkey.exit = !self.hotkey.exit;
    }

    /// Choix des fenêtres de l'image précédente.
    pub(super) fn hotkey_frame(&mut self) {
        // Monde choisi dans `UI_SelectServer` (`AgcmLogin`+0x1764).
        self.hotkey.world = self.ui.selected_world.clone();
        for r in std::mem::take(&mut self.hotkey.requests) {
            match r {
                Request::Exit(e) => {
                    match e {
                        ExitUi::ReturnGame | ExitUi::Close => {}
                        // Contrôles, fenêtres fermées et AgpmReturnToLogin op 1 (voir `app_return_login.rs`).
                        ExitUi::SelectCharacter => self.return_login_request(),
                        ExitUi::Options => {
                            self.ui.show_options = true;
                            self.hud.options_tab = 0;
                        }
                        ExitUi::CustomKeys => self.hotkey.open_window(&self.assets),
                        // « Exit Game » ne ferme pas `UI_EXIT` : la question s'ouvre par-dessus.
                        ExitUi::ExitGame => {
                            self.hotkey.quit_question = true;
                            continue;
                        }
                    }
                    self.hotkey.exit = false;
                }
                Request::Quit(yes) => {
                    if yes {
                        self.exit = true;
                    }
                }
                Request::Hotkey(h) => {
                    let glue = &mut self.hotkey;
                    let Some(t) = glue.table.as_mut() else { continue };
                    match h {
                        HotkeyUi::Arm(i) if i < t.entries.len() => glue.capture = Some(i),
                        HotkeyUi::Arm(_) => {}
                        HotkeyUi::Confirm => {
                            t.applied = t.edit.clone();
                            glue.window = false;
                            glue.capture = None;
                            glue.write_file();
                        }
                        HotkeyUi::Cancel => {
                            t.edit = t.applied.clone();
                            glue.window = false;
                            glue.capture = None;
                        }
                        HotkeyUi::Reset => {
                            t.edit = t.defaults();
                            glue.capture = None;
                        }
                        HotkeyUi::Close => {
                            glue.window = false;
                            glue.capture = None;
                        }
                        HotkeyUi::Scroll(d) => {
                            let max = glue.rows().len().saturating_sub(HOTKEY_ROWS);
                            glue.first = (glue.first as i64 + d as i64).clamp(0, max as i64) as usize;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(text: &str) -> IniSection {
        IniTable::parse(&format!("[EventMap]\n{text}")).section("EventMap").cloned().unwrap()
    }

    const SAMPLE: &str = "EventName=X\nAction=1:1:0:1:UI_X\nHotkey=0:1\nHotkeyFlag=0\nAction=3:Close_UI_Window\n\
        Hotkey=0:2\nHotkeyFlag=1\nHotkeyDesc=HP Potion\nAction=3:QuickBeltUseHPPotion\n\
        Hotkey=1:16\nHotkeyFlag=1\nHotkeyDesc=Self Quest\nAction=3:QuestAutoHoeKey2_Open2\n\
        Hotkey=0:17\nHotkeyFlag=1\nHotkeyDesc=Move Forward\n\
        Hotkey=0:20\nHotkeyFlag=1\nHotkeyDesc=Chantra Shop\nAction=3:CashMallOpenUI\n\
        Hotkey=0:28\nHotkeyFlag=1\nHotkeyDesc=Chat\nAction=3:Chat_Active_Input\n\
        Hotkey=0:30\nHotkeyFlag=1\nHotkeyDesc=Move Left\n\
        Hotkey=0:33\nHotkeyFlag=1\nHotkeyDesc=Friends\nAction=1:3:0:1:UI_BuddyList\nAction=3:BuddyList_Clear\n\
        Hotkey=1:4\nHotkeyFlag=1\nHotkeyDesc=Quick Slot 2-1\nAction=3:QuickBeltUse_2_1\n\
        Hotkey=1:16\nHotkeyFlag=0\n\
        Hotkey=0:22\nHotkeyFlag=1\nHotkeyDesc=Mailbox\nAction=1:2:0:1:UI_MainMenuCommunity\nAction=1:3:0:1:MailBox\nAction=3:MAIL_LIST_OPEN\n\
        EventName=Y\nAction=3:Z\n";

    #[test]
    fn table_de_l_eventmap() {
        let defs = parse_event_map(&section(SAMPLE));
        // `1:16` redéfini en HotkeyFlag=0 : hors de la liste ; les actions d'un EventName suivant ne s'y collent pas.
        assert!(defs.iter().any(|d| d.key == Key { kind: 1, code: 16 } && !d.flag && d.actions.is_empty()));
        let entries = build_entries(&defs);
        let keys: Vec<(u8, u8)> = entries.iter().map(|e| (e.default.kind, e.default.code)).collect();
        assert_eq!(keys, vec![(0, 2), (1, 4), (0, 17), (0, 20), (0, 22), (0, 28), (0, 30), (0, 33)]);
        let by = |c: u8, k: u8| entries.iter().find(|e| e.default == Key { kind: k, code: c }).unwrap();
        assert_eq!(by(2, 0).action, Some(HotkeyAction::Potion(0)));
        assert_eq!(by(4, 1).action, Some(HotkeyAction::QuickBelt { layer: 2, col: 1 }));
        assert_eq!(by(17, 0).action, Some(HotkeyAction::Move(0)));
        assert_eq!(by(30, 0).action, Some(HotkeyAction::Move(2)));
        assert_eq!(by(33, 0).action, Some(HotkeyAction::Friends));
        assert_eq!(by(22, 0).action, Some(HotkeyAction::Mailbox));
        assert_eq!(by(28, 0).action, Some(HotkeyAction::Chat));
        // Ajout d'Archonia : compté, sans effet, caché.
        assert!(by(20, 0).hidden && by(20, 0).action.is_none());
        // Move Forward affiché juste avant Move Left (échange avec Chat).
        let order = display_order(&entries);
        let w = entries.iter().position(|e| e.default.code == 17).unwrap();
        let a = entries.iter().position(|e| e.default.code == 30).unwrap();
        assert_eq!(order[a - 1], w);
        assert_eq!(order[w], a - 1);
    }

    #[test]
    fn fichier_ui_hotkey_aller_retour() {
        let entries = build_entries(&parse_event_map(&section(SAMPLE)));
        let mut t = HotkeyTable::new(entries.clone());
        // Doublon refusé, touche libre acceptée.
        assert!(!t.assign(0, Key { kind: 0, code: 28 }));
        assert!(t.assign(0, Key { kind: 2, code: 2 }));
        // Sans fichier : entrées connues et les deux touches d'Archonia, triées.
        let merged = merge_file(&entries, &t.edit, None);
        assert_eq!(merged.len(), entries.len() + 2);
        assert!(ARCHONIA_CODE_KEYS.iter().all(|k| merged.iter().any(|f| f.default == *k && f.custom == *k)));
        let text = file_text(&merged);
        assert!(text.starts_with("[CONFIG]\r\nVERSION=4\r\n[CustomHotkey0]\r\nDefaultType=0\r\nDefaultCode=2\r\nCustomType=2\r\nCustomCode=2\r\n"));
        // Chiffré comme Archonia, relu ; touches connues appliquées.
        let raw = crypt(text.as_bytes());
        assert_ne!(raw.first(), Some(&b'['));
        let back = parse_file(&file_plain(&raw)).unwrap();
        assert_eq!(back, merged);
        assert_eq!(apply_file(&entries, &back), t.edit);
        // Entrée inconnue et autre ordre : gardée telle quelle à la réécriture, connues retrouvées par leur défaut.
        let mut read = back.clone();
        read.reverse();
        read.push(FileEntry { default: Key { kind: 0, code: 99 }, custom: Key { kind: 1, code: 98 } });
        let text = file_text(&read);
        let read = parse_file(&text).unwrap();
        assert_eq!(apply_file(&entries, &read), t.edit);
        assert!(t.assign(0, Key { kind: 3, code: 2 }));
        let again = merge_file(&entries, &t.edit, Some(&read));
        assert_eq!(again.len(), read.len());
        assert_eq!(again.last(), read.last(), "entrée inconnue à l'identique");
        assert_eq!(again.iter().find(|f| f.default == entries[0].default).unwrap().custom, Key { kind: 3, code: 2 });
        // Version différente ou sections manquantes : ignoré.
        assert_eq!(parse_file(&text.replace("VERSION=4", "VERSION=3")), None);
        assert_eq!(parse_file(&text.replace("[CustomHotkey1]", "[Autre]")), None);
    }

    #[test]
    fn touches_et_libelles() {
        assert_eq!(dik_of(KeyCode::Tab), Some(15));
        assert_eq!(dik_of(KeyCode::KeyM), Some(50));
        assert_eq!(dik_of(KeyCode::F12), Some(88));
        assert_eq!(dik_of(KeyCode::ArrowDown), Some(208));
        assert_eq!(key_label(Key { kind: 1, code: 50 }), "Shift+M");
        assert_eq!(key_label(Key { kind: 4, code: 50 }), "Ctrl+Shift+M");
        assert_eq!(key_label(Key { kind: 0, code: 41 }), "~");
        assert_eq!(key_label(Key { kind: 3, code: 13 }), "Alt+=");
        assert_eq!(kind_of(ModifiersState::CONTROL | ModifiersState::SHIFT), kind::CTRL_SHIFT);
        assert_eq!(kind_of(ModifiersState::ALT), kind::ALT);
    }

    /// Table du jeu : 81 entrées modifiables (82 blocs `HotkeyFlag=1`, `1:16` repris à 0). Fichier réel
    /// `Ini/UI_Hotkey.ini` d'Archonia (83 entrées) : lu, connues appliquées, réécrit avec une touche changée (en
    /// mémoire, le fichier du jeu n'est pas touché), relu ; les entrées inconnues restent identiques.
    #[test]
    fn table_du_jeu() {
        let Ok(dir) = std::env::var("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(PathBuf::from(&dir)), &[]);
        let Some(defs) = assets.table("ini", "UI_1024X768.ini").ok().and_then(|t| t.section("EventMap").map(parse_event_map)) else { return };
        let entries = build_entries(&defs);
        assert_eq!(entries.len(), 81);
        assert_eq!(entries.iter().filter(|e| e.hidden).count(), 2);
        assert!(entries.iter().filter(|e| !e.hidden).all(|e| key_name(e.default.code).is_some()));
        // Sans fichier, ArchRust écrit les 83 entrées d'Archonia dans son ordre.
        let fresh = merge_file(&entries, &HotkeyTable::new(entries.clone()).applied, None);
        assert_eq!(fresh.len(), 83);
        let Ok(raw) = std::fs::read(ini_path(Path::new(&dir))) else { return };
        let plain = file_plain(&raw);
        let file = parse_file(&plain).expect("fichier d'Archonia lu");
        assert_eq!(file.len(), 83);
        let unknown: Vec<FileEntry> = file.iter().copied().filter(|f| !entries.iter().any(|e| e.default == f.default)).collect();
        assert!(ARCHONIA_CODE_KEYS.iter().all(|k| unknown.iter().any(|f| f.default == *k)));
        // Sans changement, réécriture identique à l'octet près.
        let mut t = HotkeyTable::new(entries.clone());
        t.applied = apply_file(&entries, &file);
        assert_eq!(file_text(&merge_file(&entries, &t.applied, Some(&file))), plain);
        // Une touche connue changée : seule elle diffère, inconnues et rang inchangés, 83 entrées (Archonia le relit).
        let i = entries.iter().position(|e| e.default == Key { kind: 0, code: 34 }).unwrap();
        t.applied[i] = Key { kind: 2, code: 34 };
        let out = crypt(file_text(&merge_file(&entries, &t.applied, Some(&file))).as_bytes());
        let back = parse_file(&file_plain(&out)).unwrap();
        assert_eq!(back.len(), 83);
        let changed: Vec<usize> = (0..83).filter(|&k| back[k] != file[k]).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(back[changed[0]].custom, Key { kind: 2, code: 34 });
        assert!(unknown.iter().all(|u| back.contains(u)));
        assert_eq!(apply_file(&entries, &back), t.applied);
    }
}
