//! Liaison entre le moteur et les AddOns Lua (sous-module de `app`) : chargement à l'entrée en
//! jeu, état du monde transmis chaque image, commandes des AddOns, CVars appliquées au rendu,
//! discussion filtrée, clavier et souris.

use super::*;
use crate::addon_ui::{AddonUi, AddonUiOutput};
use archrust_addons::{CameraInfo, ItemInfo, ItemTemplateInfo, MonsterTemplateInfo, UnitInfo};
use winit::keyboard::ModifiersState;

/// Alias façon World of Warcraft de quelques fenêtres natives d'Archonia : chaque fenêtre est une
/// frame du nom de sa section dans `UI_1024X768.ini` (`MiniMap`, `UI_CharInfo`…), ces noms en plus.
const NATIVE_ALIASES: &[(&str, &[&str])] = &[
    ("MiniMap", &["Minimap", "MinimapCluster"]),
    ("UI_CharInfo", &["PlayerFrame", "MainMenuBar"]),
    ("UI_TargetInfo", &["TargetFrame"]),
    ("UI_Party", &["PartyFrame"]),
    ("UI_Inventory", &["ContainerFrame1"]),
    ("Character_Status", &["CharacterFrame"]),
    ("UI_Skill", &["SpellBookFrame"]),
    ("QuestMain_UI", &["QuestLogFrame"]),
    ("UI_WORLD_MAP", &["WorldMapFrame"]),
    ("Chat_Input_Normal", &["ChatFrame1EditBox"]),
];

impl App {
    /// Fenêtres natives et AddOns, une fois par image : les réglages voulus par les AddOns (ancres,
    /// `Show`/`Hide`, `SetAlpha`) passent au HUD, puis la place d'origine de chaque fenêtre passe aux
    /// AddOns pour les frames qu'ils n'ont pas déplacées.
    pub(super) fn sync_native_frames(&mut self, ctx: &egui::Context) {
        if !self.addons.is_loaded() {
            crate::hud::set_native_overrides(HashMap::new());
            return;
        }
        let screen = ctx.screen_rect();
        // Repère des AddOns : origine en bas à gauche de l'écran.
        let to_addon = |r: egui::Rect| archrust_addons::Rect {
            left: r.min.x - screen.min.x,
            bottom: screen.max.y - r.max.y,
            width: r.width(),
            height: r.height(),
        };
        let defaults: Vec<(String, archrust_addons::Rect)> = crate::hud::native_default_rects(ctx, &mut self.hud, &self.assets)
            .into_iter()
            .map(|(n, r)| (n, to_addon(r)))
            .collect();
        let by_name: HashMap<&str, archrust_addons::Rect> = defaults.iter().map(|(n, r)| (n.as_str(), *r)).collect();
        let mut overrides = HashMap::new();
        for s in self.addons.native_states() {
            let Some(d) = by_name.get(s.name.as_str()) else { continue };
            let offset = s.rect.map_or(egui::Vec2::ZERO, |r| egui::vec2(r.left - d.left, d.bottom - r.bottom));
            if s.shown && s.alpha >= 0.999 && offset.length() < 0.5 {
                continue;
            }
            overrides.insert(s.name, crate::hud::NativeOverride { offset, shown: s.shown, alpha: s.alpha });
        }
        crate::hud::set_native_overrides(overrides);
        self.addons.set_native_defaults(&defaults);
    }

    /// Place d'origine des fenêtres natives donnée aux AddOns avant leur chargement, pour que leurs
    /// tailles soient connues dès le corps des fichiers Lua.
    fn native_defaults_before_load(&mut self) {
        let Some(ctx) = self.gpu.as_ref().map(|g| g.egui_state.egui_ctx().clone()) else { return };
        let screen = ctx.screen_rect();
        let defaults: Vec<(String, archrust_addons::Rect)> = crate::hud::native_default_rects(&ctx, &mut self.hud, &self.assets)
            .into_iter()
            .map(|(n, r)| (n, archrust_addons::Rect { left: r.min.x - screen.min.x, bottom: screen.max.y - r.max.y, width: r.width(), height: r.height() }))
            .collect();
        self.addons.set_native_defaults(&defaults);
    }
}

/// CVars du moteur que les AddOns peuvent lire et modifier (`GetCVar` / `SetCVar`).
pub const CVARS: &[(&str, &str, &str)] = &[
    (
        "skyColor",
        "0.35 0.45 0.6",
        "couleur du ciel (fond), « r g b » entre 0 et 1",
    ),
    (
        "cameraFov",
        "55",
        "champ de vision vertical de la caméra, en degrés",
    ),
    (
        "cameraFarClip",
        "200000",
        "distance maximale de rendu de la caméra",
    ),
    (
        "objectDrawDistance",
        "25000",
        "distance de tracé des objets du monde",
    ),
    (
        "lightDirection",
        "0.4 0.8 0.3",
        "direction de la lumière du soleil « x y z »",
    ),
    (
        "ambientColor",
        "0.45 0.45 0.5",
        "lumière ambiante « r g b »",
    ),
    ("uiScale", "1", "échelle de toute l'interface (0,5 à 3)"),
    (
        "nameplateDistance",
        "6000",
        "distance indicative des plaques de nom (lue par les AddOns)",
    ),
    (
        "loginMusic",
        crate::audio::DEFAULT_LOGIN_MUSIC,
        "musique des écrans de connexion, de sélection et de création (chemin client ou patch ; vide = silence)",
    ),
    (
        "worldMusic",
        crate::audio::DEFAULT_WORLD_MUSIC,
        "musique en jeu quand la région du joueur n'en indique pas (RegionTemplate.ini → skyset.ini)",
    ),
    (
        "worldAmbience",
        crate::audio::DEFAULT_WORLD_AMBIENCE,
        "ambiance en jeu (sound/stream/*.mp3 ; vide = aucune)",
    ),
];

/// État de la liaison, gardé dans `App`.
#[derive(Default)]
pub struct AddonGlue {
    pub ui: AddonUi,
    /// L'interface des AddOns a capté la souris à l'image précédente.
    pub pointer_captured: bool,
    pub modifiers: ModifiersState,
    /// Signature de l'inventaire déjà transmis (évite de le reconstruire à chaque image).
    inventory_sig: Option<u64>,
    /// Catalogue des objets pour les AddOns, lu une fois dans `ItemTemplateAll.ini`.
    item_templates: Option<std::sync::Arc<Vec<ItemTemplateInfo>>>,
    /// Dernière valeur appliquée de `uiScale`.
    ui_scale: Option<f32>,
    /// Extensions ArchRust négociées avec le serveur monde (gardées pour chaque rechargement).
    pub extensions: Option<archrust_addons::ExtInfo>,
}

fn unit_of(e: &Entity, moving: bool) -> UnitInfo {
    UnitInfo {
        cid: e.cid,
        tid: e.tid,
        name: e.name.clone(),
        level: 1,
        hp: e.hp.map(i64::from),
        hp_max: None,
        mp: None,
        mp_max: None,
        position: e.position.to_array(),
        facing: e.turn_y,
        moving,
    }
}

/// Catalogue des objets du client pour les AddOns, trié par TID : nom, icône (colonne `TEXTURE`, texture `l%07x`
/// comme les icônes du HUD), niveau requis (seconde colonne `Level`, bloc `RestrictFactor`), `Type`, `Kind`, `Part`,
/// taille de pile (`MaxStackableCount` si `IsStackable`).
/// Colonnes numériques d'`itemdatatable.txt` données dans `stats` (`<E>` : `ELEMENTS`), noms exacts de l'en-tête.
const ITEM_STATS: [&str; 24] = ["Physical_Min_Damage", "Physical_Max_Damage", "Physical_Defense_Point", "PDR", "BlockRate", "ATK Speed", "ATK Range", "Physical_ATKRate", "Magic_ATKRate", "CON_Bonus_Point", "STR_Bonus_Point", "DEX_Bonus_Point", "INT_Bonus_Point", "Wis_Bonus_Point", "HP", "SP", "MP", "Hp_Buff", "Mp_Buff", "Attack_Buff", "Defense_Buff", "Run_Buff", "Rank", "DUR"];
const ELEMENTS: [&str; 8] = ["Magic", "Water", "Fire", "Earth", "Air", "Poison", "Ice", "Linghtning"];

/// Ligne d'`itemdatatable.txt` utile à la fiche d'un objet.
#[derive(Debug, Clone, Default, PartialEq)]
struct ItemRow {
    classes: Vec<i64>,
    races: Vec<i64>,
    min_socket: Option<i64>,
    max_socket: Option<i64>,
    stats: Vec<(String, f64)>,
}

/// Lignes d'`itemdatatable.txt` par TID (la première d'un TID compte). Les noms de colonnes sont comparés sans leurs blancs.
fn parse_item_rows(text: &str) -> HashMap<i32, ItemRow> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let Some(tid_col) = col("TID") else { return HashMap::new() };
    let (class, race, min, max) = (col("Class"), col("Race"), col("MinSocket"), col("MaxSocket"));
    let mut names: Vec<String> = ITEM_STATS.iter().map(|s| s.to_string()).collect();
    for e in ELEMENTS {
        names.extend([format!("{e}_Min_Damage"), format!("{e}_Max_Damage"), format!("{e}_Defense_Point"), format!("{e}_Defense_Rate")]);
    }
    names.sort();
    names.dedup();
    let stat_cols: Vec<(usize, &str)> = names.iter().filter_map(|n| Some((col(n)?, header[col(n)?]))).collect();
    let mut out = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let Some(tid) = f.get(tid_col).and_then(|t| t.trim().parse::<i32>().ok()) else { continue };
        let cell = |c: Option<usize>| c.and_then(|c| f.get(c)).map(|v| v.trim()).filter(|v| !v.is_empty());
        let list = |c: Option<usize>| cell(c).map_or(Vec::new(), |v| v.split(';').filter_map(|x| x.trim().parse::<i64>().ok()).collect());
        let int = |c: Option<usize>| cell(c).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i64);
        let stats = stat_cols.iter().filter_map(|(c, n)| Some(((*n).to_string(), cell(Some(*c))?.parse::<f64>().ok().filter(|v| *v != 0.0)?))).collect();
        out.entry(tid).or_insert(ItemRow { classes: list(class), races: list(race), min_socket: int(min), max_socket: int(max), stats });
    }
    out
}

/// Monstres de `CharacterDataTable.txt` (bit 0x4 de `Type`, voir `character_types` d'`app_combat.rs`) : TID et niveau de `base`,
/// nom de `local` (table de la langue du client, sans quoi celui de `base`).
fn parse_monsters(base: &str, local: &str) -> Vec<MonsterTemplateInfo> {
    fn columns(line: &str) -> Vec<&str> {
        line.split('\t').map(str::trim).collect()
    }
    let names: HashMap<i32, String> = {
        let mut lines = local.lines();
        let header = columns(lines.next().unwrap_or_default());
        let at = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
        match (at("TID"), at("Name")) {
            (Some(t), Some(n)) => lines.filter_map(|l| { let f = columns(l); Some((f.get(t)?.parse().ok()?, f.get(n)?.to_string())) }).collect(),
            _ => HashMap::new(),
        }
    };
    let mut lines = base.lines();
    let header = columns(lines.next().unwrap_or_default());
    let at = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (Some(tid), Some(name), Some(ty), Some(lvl)) = (at("TID"), at("Name"), at("Type"), at("LVL")) else { return Vec::new() };
    let mut out: Vec<MonsterTemplateInfo> = lines
        .filter_map(|l| {
            let f = columns(l);
            let t: i32 = f.get(tid)?.parse().ok()?;
            (f.get(ty)?.parse::<i64>().ok()? & 4 != 0).then(|| MonsterTemplateInfo {
                tid: t,
                name: names.get(&t).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| f.get(name).unwrap_or(&"").to_string()),
                level: f.get(lvl).and_then(|v| v.parse::<f64>().ok()).map_or(0, |v| v as i64),
            })
        })
        .collect();
    out.sort_by_key(|m| m.tid);
    out.dedup_by_key(|m| m.tid);
    out
}

fn monster_templates(assets: &Assets) -> Vec<MonsterTemplateInfo> {
    parse_monsters(&super::npc_glue::client_text_in(assets, "ini", "CharacterDataTable.txt"), &super::npc_glue::client_text(assets, "CharacterDataTable.txt"))
}

fn item_templates(assets: &Assets) -> Vec<ItemTemplateInfo> {
    let Ok(table) = assets.table("ini", "ItemTemplateAll.ini") else {
        return Vec::new();
    };
    // Fiche d'`itemdatatable.txt` : celle du serveur (`server-data/`) si elle existe, qui dit aussi ce que le serveur connaît.
    let server_text = assets.server_text("ini", "itemdatatable.txt");
    let on_server = server_text.is_some();
    let rows_text = server_text.unwrap_or_else(|| super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt"));
    let rows = parse_item_rows(&rows_text);
    let rows_read = !rows.is_empty();
    let Some(header) = table.section("") else {
        return Vec::new();
    };
    let column = |n: &str| header.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(n)).map(|(k, _)| k.clone());
    let [name, kind_type, kind, part, level, texture, stackable, max_stack] =
        ["Name", "Type", "Kind", "Part", "Level", "TEXTURE", "IsStackable", "MaxStackableCount"].map(column);
    let mut list: Vec<ItemTemplateInfo> = table
        .sections
        .iter()
        .filter_map(|s| {
            let tid: i32 = s.name.parse().ok()?;
            let get = |c: &Option<String>| c.as_ref().and_then(|c| s.get(c));
            let num = |v: Option<&str>| v.and_then(|v| v.trim().parse::<f64>().ok()).map_or(0, |v| v as i64);
            let icon = num(get(&texture));
            let stack = if num(get(&stackable)) != 0 { num(get(&max_stack)).max(1) } else { 1 };
            let row = rows.get(&tid);
            Some(ItemTemplateInfo {
                tid,
                name: get(&name).unwrap_or_default().trim().to_string(),
                icon: if icon > 0 { format!("l{:07x}", icon as u32) } else { String::new() },
                level: level.as_ref().map_or(0, |c| num(s.get_all(c).nth(1))),
                item_type: num(get(&kind_type)),
                kind: num(get(&kind)),
                part: num(get(&part)),
                stack,
                server: on_server.then(|| rows.contains_key(&tid)),
                classes: rows_read.then(|| row.map_or(Vec::new(), |r| r.classes.clone())),
                races: rows_read.then(|| row.map_or(Vec::new(), |r| r.races.clone())),
                min_socket: row.and_then(|r| r.min_socket),
                max_socket: row.and_then(|r| r.max_socket),
                stats: row.map_or(Vec::new(), |r| r.stats.clone()),
            })
        })
        .collect();
    list.sort_by_key(|t| t.tid);
    list
}

fn floats(v: &[f32], n: usize) -> Option<Vec<f32>> {
    (v.len() >= n).then(|| v[..n].to_vec())
}

/// Nom WoW d'une touche (`A`, `F1`, `ESCAPE`…), précédé de `ALT-`, `CTRL-`, `SHIFT-`.
pub fn wow_key_name(code: KeyCode, mods: ModifiersState) -> Option<String> {
    use KeyCode::*;
    let base: String = match code {
        KeyA => "A".into(),
        KeyB => "B".into(),
        KeyC => "C".into(),
        KeyD => "D".into(),
        KeyE => "E".into(),
        KeyF => "F".into(),
        KeyG => "G".into(),
        KeyH => "H".into(),
        KeyI => "I".into(),
        KeyJ => "J".into(),
        KeyK => "K".into(),
        KeyL => "L".into(),
        KeyM => "M".into(),
        KeyN => "N".into(),
        KeyO => "O".into(),
        KeyP => "P".into(),
        KeyQ => "Q".into(),
        KeyR => "R".into(),
        KeyS => "S".into(),
        KeyT => "T".into(),
        KeyU => "U".into(),
        KeyV => "V".into(),
        KeyW => "W".into(),
        KeyX => "X".into(),
        KeyY => "Y".into(),
        KeyZ => "Z".into(),
        Digit0 => "0".into(),
        Digit1 => "1".into(),
        Digit2 => "2".into(),
        Digit3 => "3".into(),
        Digit4 => "4".into(),
        Digit5 => "5".into(),
        Digit6 => "6".into(),
        Digit7 => "7".into(),
        Digit8 => "8".into(),
        Digit9 => "9".into(),
        F1 => "F1".into(),
        F2 => "F2".into(),
        F3 => "F3".into(),
        F4 => "F4".into(),
        F5 => "F5".into(),
        F6 => "F6".into(),
        F7 => "F7".into(),
        F8 => "F8".into(),
        F9 => "F9".into(),
        F10 => "F10".into(),
        F11 => "F11".into(),
        F12 => "F12".into(),
        Escape => "ESCAPE".into(),
        Enter | NumpadEnter => "ENTER".into(),
        Space => "SPACE".into(),
        Tab => "TAB".into(),
        Backspace => "BACKSPACE".into(),
        Delete => "DELETE".into(),
        Insert => "INSERT".into(),
        Home => "HOME".into(),
        End => "END".into(),
        PageUp => "PAGEUP".into(),
        PageDown => "PAGEDOWN".into(),
        ArrowUp => "UP".into(),
        ArrowDown => "DOWN".into(),
        ArrowLeft => "LEFT".into(),
        ArrowRight => "RIGHT".into(),
        Minus => "-".into(),
        Equal => "=".into(),
        Numpad0 => "NUMPAD0".into(),
        Numpad1 => "NUMPAD1".into(),
        Numpad2 => "NUMPAD2".into(),
        Numpad3 => "NUMPAD3".into(),
        Numpad4 => "NUMPAD4".into(),
        Numpad5 => "NUMPAD5".into(),
        Numpad6 => "NUMPAD6".into(),
        Numpad7 => "NUMPAD7".into(),
        Numpad8 => "NUMPAD8".into(),
        Numpad9 => "NUMPAD9".into(),
        _ => return None,
    };
    let mut name = String::new();
    if mods.alt_key() {
        name.push_str("ALT-");
    }
    if mods.control_key() {
        name.push_str("CTRL-");
    }
    if mods.shift_key() {
        name.push_str("SHIFT-");
    }
    name.push_str(&base);
    Some(name)
}

impl App {
    /// Nouvel état des AddOns (sans AddOn chargé) avec les CVars du moteur.
    pub(super) fn new_addon_manager(config: &Config) -> Result<AddonManager> {
        let m =
            AddonManager::new(&config.saved_dir).map_err(|e| anyhow::anyhow!("AddOns : {e}"))?;
        m.set_host(|h| {
            h.realm = "ArchCore".into();
            h.locale = crate::assets::wow_locale().into();
        });
        for (name, default, _) in CVARS {
            m.register_cvar(name, default);
        }
        Ok(m)
    }

    /// Joueur, unités en vue et inventaire → AddOns (événements UNIT_*, NAME_PLATE_*, BAG_UPDATE…).
    pub(super) fn sync_addons_world(&mut self) {
        let player = self
            .player
            .as_ref()
            .map(|p| unit_of(p, self.walking || Self::is_moving(p)))
            .map(|mut u| {
                // Caractéristiques du joueur : facteurs du serveur (PV et PM maximum, niveau).
                let s = &self.hud.stats;
                u.level = s.level.map_or(1, i64::from);
                u.hp = s.hp.map(i64::from).or(u.hp);
                u.hp_max = s.max_hp.map(i64::from);
                u.mp = s.mp.map(i64::from);
                u.mp_max = s.max_mp.map(i64::from);
                u
            });
        let units: Vec<UnitInfo> = self
            .entities
            .iter()
            .map(|e| unit_of(e, Self::is_moving(e)))
            .collect();
        // Inventaire : reconstruit seulement s'il a changé.
        let sig = self.items.values().fold(self.items.len() as u64, |acc, i| {
            acc.wrapping_mul(31)
                .wrapping_add(i.id as u64)
                .wrapping_add((i.count.unwrap_or(0) as u64) << 20)
                .wrapping_add((i.status.unwrap_or(0) as u64) << 40)
                .wrapping_add(i.grid.map_or(0, |g| {
                    (g[0] as i64 * 10_000 + g[1] as i64 * 100 + g[2] as i64) as u64
                }))
        });
        let inventory = (self.addon_glue.inventory_sig != Some(sig)).then(|| {
            self.addon_glue.inventory_sig = Some(sig);
            let mut v: Vec<ItemInfo> = self
                .items
                .values()
                .map(|i| ItemInfo {
                    id: i.id,
                    tid: i.template_id,
                    name: i
                        .template_id
                        .and_then(|tid| item_name_for_tid(&self.assets, tid))
                        .unwrap_or_else(|| format!("Objet {}", i.template_id.unwrap_or_default())),
                    count: i.count,
                    status: i.status.map(i32::from),
                    grid: i.grid.map(|g| g.map(i32::from)),
                })
                .collect();
            v.sort_by_key(|i| i.id);
            v
        });
        self.addons.sync_world(player, units, inventory);
    }

    /// Caméra, matrice de projection, secteurs chargés, touches de modification → AddOns.
    pub(super) fn sync_addons_frame(&mut self) {
        let aspect = self
            .gpu
            .as_ref()
            .map_or(16.0 / 9.0, |g| g.renderer.aspect());
        let cam = CameraInfo {
            distance: self.camera.distance,
            yaw: self.camera.yaw.to_degrees(),
            pitch: self.camera.pitch.to_degrees(),
            fov: self.camera.fov_y.to_degrees(),
            target: self.camera.target.to_array(),
            eye: self.camera.eye().to_array(),
        };
        let vp = self.camera.view_proj(aspect).to_cols_array();
        let sectors = self.terrain.loaded_count();
        let fps = self.fps as f64;
        let in_world = self.phase == Phase::InWorld;
        let m = self.addon_glue.modifiers;
        self.addons.set_host(|h| {
            h.camera = cam;
            h.view_proj = vp;
            h.sectors = sectors;
            h.framerate = fps;
            h.in_world = in_world;
            h.modifiers = [m.shift_key(), m.control_key(), m.alt_key()];
        });
        if in_world && self.addons.is_loaded() {
            self.sync_addons_world();
        }
    }

    /// Entrée en jeu : chargement des AddOns (une fois), puis `PLAYER_LOGIN` et
    /// `PLAYER_ENTERING_WORLD`.
    pub(super) fn enter_world_addons(&mut self) {
        let first = !self.addons.is_loaded();
        if first {
            // Catalogue des objets (une fois par session) : avant le chargement, pour les AddOns qui le lisent tout de suite.
            let templates = self
                .addon_glue
                .item_templates
                .get_or_insert_with(|| std::sync::Arc::new(item_templates(&self.assets)))
                .clone();
            self.addons.set_host(|h| h.item_templates = templates);
            let monsters = std::sync::Arc::new(monster_templates(&self.assets));
            self.addons.set_host(|h| h.monster_templates = monsters);
            self.addon_glue.inventory_sig = None;
            self.sync_addons_frame();
            self.sync_addons_world();
            // Fenêtres natives d'Archonia, pilotables par les AddOns dès leur chargement.
            let names = crate::hud::native_window_names(&mut self.hud, &self.assets);
            let frames: Vec<(&str, &[&str])> = names
                .iter()
                .map(|n| (n.as_str(), NATIVE_ALIASES.iter().find(|(w, _)| w == n).map_or(&[][..], |(_, a)| *a)))
                .collect();
            if let Err(e) = self.addons.register_native_frames(&frames) {
                log::warn!("AddOns : fenêtres natives : {e}");
            }
            self.native_defaults_before_load();
            if let Err(e) = self.addons.load_dir(&self.config.addons_dir) {
                log::error!("AddOns : {e}");
            }
            let list = self.addons.addons();
            log::info!(
                "{} AddOn(s) chargé(s) : {}",
                list.iter().filter(|a| a.loaded).count(),
                list.iter()
                    .map(|a| format!(
                        "{}{}",
                        a.manifest.name,
                        if a.loaded { "" } else { " (erreur)" }
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            // Extensions ArchRust déjà négociées (HELLO_ACK arrive avant l'entrée en jeu).
            self.addons.set_extensions(self.addon_glue.extensions.clone());
            self.addons.fire_event("PLAYER_LOGIN", &[]);
        }
        self.addons.fire("PLAYER_ENTERING_WORLD", (first, !first));
        self.apply_addon_commands();
    }

    /// Sortie du monde : `PLAYER_LOGOUT`, sauvegarde, état Lua neuf pour la prochaine entrée.
    pub(super) fn leave_world_addons(&mut self) {
        self.music_override = None;
        self.addons.shutdown();
        match Self::new_addon_manager(&self.config) {
            Ok(m) => self.addons = m,
            Err(e) => log::error!("{e:#}"),
        }
        self.addon_glue.inventory_sig = None;
        // Déconnexion (plus de liaison réseau) : l'accord d'extensions ne vaut plus. Un `/reload`
        // le garde.
        if self.net.is_none() {
            self.addon_glue.extensions = None;
        }
    }

    /// `/reload` : comme sous WoW, `PLAYER_LOGOUT` + sauvegarde, puis rechargement complet.
    pub(super) fn reload_addons(&mut self) {
        self.leave_world_addons();
        if self.phase == Phase::InWorld {
            self.enter_world_addons();
        }
        self.ui
            .push_chat("[système] interface rechargée".to_string());
    }

    /// Fin du programme : sauvegarde des AddOns.
    pub(super) fn shutdown_addons(&mut self) {
        self.addons.shutdown();
    }

    /// Message reçu : filtres des AddOns, fenêtre de discussion, événement `CHAT_MSG_*`.
    pub(super) fn addon_chat_received(&mut self, event: &str, message: &str, from: &str) {
        let Some((msg, who)) = self.addons.filter_chat(event, message, from) else {
            return;
        };
        // Guilde et groupe : ligne d'Archonia (voir `app_guild.rs`) dans leur onglet.
        let channel = match event {
            "CHAT_MSG_GUILD" => 3,
            "CHAT_MSG_PARTY" => 4,
            "CHAT_MSG_WHISPER" => 5,
            "CHAT_MSG_WHISPER_INFORM" => 6,
            _ => 0,
        };
        let (line, mask) = match self.guild_chat_line(channel, &who, &msg) {
            Some(l) => l,
            None if event == "CHAT_MSG_SYSTEM" => (format!("[système] {msg}"), 0x10),
            None => (format!("[{}] {}", if who.is_empty() { "?" } else { &who }, msg), if event == "CHAT_MSG_WHISPER" { 0x8 } else { 0x1 }),
        };
        self.ui.push_chat_in(mask, line);
        self.addons
            .fire_event_str(event, &[message, from, "", "", from]);
    }

    /// Texte tapé dans la discussion (hors commande) : via `SendChatMessage` des AddOns si possible.
    pub(super) fn send_typed_chat(&mut self, text: String) {
        if !self.addons.send_chat(&text, "SAY") {
            self.say(text);
        }
        self.apply_addon_commands();
    }

    fn say(&mut self, message: String) {
        self.ui.push_chat(format!(
            "[{}] {}",
            self.player
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or("moi"),
            message
        ));
        if let Some(net) = &self.net {
            net.send(Command::Say(message));
        }
    }

    /// Exécute les actions demandées par les AddOns.
    pub(super) fn apply_addon_commands(&mut self) {
        for cmd in self.addons.drain_commands() {
            match cmd {
                HostCommand::Print(l) => self.ui.push_chat(l),
                HostCommand::Chat { message, kind, .. } => {
                    // Groupe, guilde et cri : préfixes `/p`, `/g`, `/s` d'Archonia (voir `chat::prefixed`).
                    let mode = match kind.to_ascii_uppercase().as_str() {
                        "PARTY" => Some(archrust_netclient::chat::chat_type::PARTY),
                        "GUILD" => Some(archrust_netclient::chat::chat_type::GUILD),
                        "YELL" => Some(archrust_netclient::chat::chat_type::WORD_BALLOON),
                        _ => None,
                    };
                    match (mode, &self.net) {
                        (Some(kind), Some(net)) => net.send(Command::Chat { kind, message }),
                        _ => {
                            if kind != "SAY" {
                                log::debug!("canal {kind} non géré par le serveur : envoyé en SAY");
                            }
                            self.say(message);
                        }
                    }
                }
                HostCommand::PlaySound(p) => {
                    // `PlaySound(1234)` façon WoW : identifiants de kits inconnus ici.
                    if p.trim().parse::<f64>().is_ok() {
                        log::debug!("son WoW n° {p} ignoré (pas de table de kits sonores)");
                    } else {
                        self.audio.play_effect(&p);
                    }
                }
                HostCommand::PlayMusic(path) => {
                    self.music_override = path.map(|p| (!p.trim().is_empty()).then_some(p));
                }
                HostCommand::ReloadUi => self.reload_addons(),
                HostCommand::Slash(line) => {
                    if !self.addons.run_slash(&line) {
                        self.builtin_slash(&line);
                    }
                }
                HostCommand::MoveTo([x, y, z]) => {
                    self.builtin_slash(&format!("/goto {x} {y} {z}"));
                }
                HostCommand::SetCamera {
                    distance,
                    yaw,
                    pitch,
                } => {
                    if let Some(d) = distance {
                        self.camera.distance = d.clamp(100.0, 50_000.0);
                    }
                    if let Some(y) = yaw {
                        self.camera.yaw = y.to_radians();
                    }
                    if let Some(p) = pitch {
                        self.camera.pitch = p.to_radians().clamp(-1.2, 1.5);
                    }
                }
                HostCommand::Screenshot(file) => {
                    let path = file
                        .filter(|f| !f.contains("..") && !f.contains(':'))
                        .map(|f| self.config.saved_dir.join("Screenshots").join(f))
                        .unwrap_or_else(|| {
                            self.config.saved_dir.join("Screenshots").join(format!(
                                "archrust-{}.png",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0)
                            ))
                        });
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    self.addon_glue.ui.screenshot = Some(path);
                    if let Some(g) = &mut self.gpu {
                        g.renderer.capture_next = true;
                    }
                }
                HostCommand::Quit => self.exit = true,
                HostCommand::AddonSend {
                    request_id,
                    prefix,
                    channel,
                    target,
                    payload,
                } => {
                    if let Some(net) = &self.net {
                        net.send(Command::AddonSend {
                            request_id,
                            prefix,
                            channel,
                            target,
                            payload,
                        });
                    }
                }
                HostCommand::RequestTexts {
                    request_id,
                    kind,
                    ids,
                } => {
                    if let Some(net) = &self.net {
                        net.send(Command::RequestTexts {
                            request_id,
                            kind,
                            ids,
                        });
                    }
                }
            }
        }
    }

    /// CVars appliquées au rendu et à l'interface.
    pub(super) fn apply_addon_cvars(&mut self) {
        let sky = self.addons.cvar_floats("skyColor");
        let fov = self.addons.cvar_f32("cameraFov");
        let far = self.addons.cvar_f32("cameraFarClip");
        let light = self.addons.cvar_floats("lightDirection");
        let ambient = self.addons.cvar_floats("ambientColor");
        let scale = self.addons.cvar_f32("uiScale");
        // Caméra et rendu : seulement en jeu (les écrans de sélection et de création ont leur
        // propre caméra, LoginSettings.txt). L'échelle de l'interface vaut partout.
        let in_world = self.phase == Phase::InWorld;
        if let Some(f) = fov.filter(|_| in_world) {
            self.camera.fov_y = f.clamp(20.0, 120.0).to_radians();
        }
        if let Some(f) = far.filter(|_| in_world) {
            self.camera.far = f.clamp(1_000.0, 2_000_000.0);
        }
        if let Some(g) = &mut self.gpu {
            if let Some(c) = floats(&sky, 3).filter(|_| in_world) {
                g.renderer.clear_color = wgpu::Color {
                    r: c[0].clamp(0.0, 1.0) as f64,
                    g: c[1].clamp(0.0, 1.0) as f64,
                    b: c[2].clamp(0.0, 1.0) as f64,
                    a: 1.0,
                };
            }
            if let Some(l) = floats(&light, 3).filter(|_| in_world) {
                let v = Vec3::new(l[0], l[1], l[2]);
                if v.length_squared() > 1e-6 {
                    g.renderer.light_dir = v;
                }
            }
            if let Some(a) = floats(&ambient, 3).filter(|_| in_world) {
                g.renderer.ambient = [a[0], a[1], a[2], 1.0];
            }
            if let Some(s) = scale.map(|s| s.clamp(0.5, 3.0)) {
                if self.addon_glue.ui_scale != Some(s) {
                    self.addon_glue.ui_scale = Some(s);
                    g.egui_state.egui_ctx().set_zoom_factor(s);
                }
            }
        }
    }

    /// Distance de tracé des objets : CVar `objectDrawDistance`, sinon la distance de la carte des options
    /// (`MapView`, onglet GFX).
    pub(super) fn object_draw_distance(&self) -> f32 {
        self.addons
            .cvar_f32("objectDrawDistance")
            .unwrap_or_else(|| self.options.video.object_distance())
            .clamp(1_000.0, 500_000.0)
    }

    /// Après l'interface : souris et EditBox des AddOns.
    pub(super) fn after_addon_ui(&mut self, out: AddonUiOutput) {
        self.addon_glue.pointer_captured = self.addons.pointer(&out.pointer);
        for (id, ev) in out.edits {
            self.addons.edit_event(id, ev);
        }
        self.apply_addon_commands();
    }

    /// Touche (hors saisie de texte) : raccourcis et `OnKeyDown` des AddOns.
    pub(super) fn addon_key(&mut self, code: KeyCode, down: bool) -> bool {
        if self.phase != Phase::InWorld || self.addons.wants_keyboard() {
            return false;
        }
        let Some(name) = wow_key_name(code, self.addon_glue.modifiers) else {
            return false;
        };
        let handled = self.addons.key(&name, down);
        self.apply_addon_commands();
        handled
    }
}

impl App {
    /// Extensions ArchRust (paquet 0xAC) : accord du serveur monde, messages d'AddOn reçus ou
    /// refusés.
    pub(super) fn addon_world_event(&mut self, ev: WorldEvent) {
        match ev {
            WorldEvent::ExtensionsReady(e) => {
                // Empreinte des tables du serveur (capacité `client-tables`) contre `server-data/` :
                // un écart est écrit dans le journal seulement.
                crate::server_data::check(self.assets.server_data(), e.tables_fingerprint.as_deref());
                if let Some(l) = &e.language {
                    self.ui.server_language = Some((l.clone(), e.languages.clone()));
                }
                let info = archrust_addons::ExtInfo {
                    software: e.software,
                    major: e.major,
                    minor: e.minor,
                    capabilities: e.capabilities,
                    max_payload: e.max_payload,
                    rate_burst: e.rate_burst,
                    rate_per_sec: e.rate_per_sec,
                    language: e.language,
                    languages: e.languages,
                };
                self.addon_glue.extensions = Some(info.clone());
                if self.addons.is_loaded() {
                    self.addons.set_extensions(Some(info));
                }
            }
            WorldEvent::LanguageChanged {
                request_id,
                language,
                languages,
            } => {
                self.ui.server_language = Some((language.clone(), languages.clone()));
                if request_id == crate::net::LANGUAGE_REQUEST_ID {
                    self.ui.push_chat(format!("[langue] le serveur applique {language}"));
                    self.ui.status = format!("langue : {language}");
                }
                if let Some(i) = self.addon_glue.extensions.as_mut() {
                    i.language = Some(language.clone());
                    i.languages = languages.clone();
                }
                if self.addons.is_loaded() {
                    self.addons.set_language(&language, &languages);
                    self.addons
                        .fire("ARCHCORE_LANGUAGE_CHANGED", (language, languages.join(";")));
                }
            }
            WorldEvent::Texts {
                request_id,
                language,
                kind,
                texts,
            } => {
                if self.addons.is_loaded() {
                    let map: std::collections::HashMap<u32, String> = texts.into_iter().collect();
                    self.addons
                        .fire("ARCHCORE_TEXTS", (request_id, language, kind, map));
                }
            }
            WorldEvent::AddonMessage {
                prefix,
                channel,
                target,
                sender_cid,
                sender,
                payload,
            } => {
                self.addons.addon_message(
                    &prefix,
                    channel,
                    target.as_deref(),
                    &sender,
                    sender_cid,
                    &payload,
                );
            }
            WorldEvent::AddonMessageFailed { request_id, result } => {
                self.addons.addon_message_failed(request_id, result);
            }
            _ => {}
        }
        self.apply_addon_commands();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiche_d_objet_et_monstres() {
        let rows = parse_item_rows("TID\tName\tClass\tRace\t MinSocket \tMaxSocket\tATK Speed\tFire_Min_Damage\tDUR\n7\tx\t1;2;4\t2\t1\t3\t60\t0\t\n8\ty\t\t\t\t\t\t1.5\t100\n7\tdoublon\t9\t\t\t\t\t\t\n");
        let a = &rows[&7];
        assert_eq!((a.classes.clone(), a.races.clone(), a.min_socket, a.max_socket), (vec![1, 2, 4], vec![2], Some(1), Some(3)));
        assert_eq!(a.stats, vec![("ATK Speed".to_string(), 60.0)], "valeurs non nulles seulement");
        let b = &rows[&8];
        assert!(b.classes.is_empty() && b.races.is_empty() && b.min_socket.is_none());
        assert_eq!(b.stats, vec![("DUR".to_string(), 100.0), ("Fire_Min_Damage".to_string(), 1.5)]);
        let base = "TID\tName\tType\tLVL\n1\tArcher\t196609\t1\n450\tDeer\t4\t3\n451\tWorm\t12\t7\n";
        let local = "TID\tName\n450\tCerf\n451\t\n";
        let m = parse_monsters(base, local);
        assert_eq!(m, vec![MonsterTemplateInfo { tid: 450, name: "Cerf".into(), level: 3 }, MonsterTemplateInfo { tid: 451, name: "Worm".into(), level: 7 }]);
    }

    /// Avec `server-data/` (`ARCHRUST_GAME_DIR`) : un objet propre à Archonia est signalé, les monstres sont lus.
    #[test]
    fn catalogue_avec_les_donnees_du_serveur() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let server = dir.join("server-data");
        if !server.is_dir() {
            return;
        }
        let assets = Assets::new(Some(dir), &[]).with_server_data(Some(&server));
        let list = item_templates(&assets);
        let archonia = list.iter().find(|t| t.tid == 15151).unwrap();
        assert_eq!(archonia.server, Some(false), "15151 n'existe que chez Archonia");
        let helm = list.iter().find(|t| t.tid == 22050).unwrap();
        assert_eq!(helm.server, Some(true));
        assert!(helm.classes.as_ref().is_some_and(|c| !c.is_empty()), "{helm:?}");
        let on = list.iter().filter(|t| t.server == Some(true)).count();
        let monsters = monster_templates(&assets);
        eprintln!("{} objets dont {on} sur le serveur, {} monstres", list.len(), monsters.len());
        assert!(on > 10_000 && monsters.len() > 100 && monsters.iter().all(|m| !m.name.is_empty()), "{on} {}", monsters.len());
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : catalogue des objets donné aux AddOns.
    #[test]
    fn catalogue_des_objets_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let list = item_templates(&assets);
        // ItemTemplateAll.ini et ItemTemplateAll2.ini (fusionnées), triées par TID.
        assert!(list.len() > 20_000, "{}", list.len());
        assert!(list.windows(2).all(|w| w[0].tid < w[1].tid));
        let bow = list.iter().find(|t| t.tid == 1).unwrap();
        assert_eq!((bow.name.as_str(), bow.category(), bow.stack), ("Short Bow", "weapon", 1));
        assert!(bow.icon.starts_with('l') && bow.icon.len() == 8, "{}", bow.icon);
        let potion = list.iter().find(|t| t.tid == 190).unwrap();
        assert_eq!(potion.category(), "usable");
        assert!(potion.stack > 1, "pile de potions : {}", potion.stack);
        let shield = list.iter().find(|t| t.name == "Lionheart Shield").unwrap();
        assert_eq!(shield.category(), "armor");
        assert!(shield.level > 1, "niveau requis : {}", shield.level);
        // L'icône est une texture du client (même nom que les icônes du HUD).
        assert!(assets.texture(&bow.icon).is_ok_and(|t| t.width > 1));
    }
}
