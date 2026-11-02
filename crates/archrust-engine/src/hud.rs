//! HUD en jeu d'après l'interface d'Archonia : barre du bas (PV, PM, expérience, niveau, raccourcis,
//! menus), minicarte et icônes des améliorations actives.
//!
//! **Données du client** (vérifiées sur Archonia.exe sha256 e9c7609b… et ses fichiers) :
//! * fenêtres et contrôles : `ini/UI_1024X768.ini` (AgcmUIManager2). Chaque contrôle suit sa ligne
//!   `CtrlParent=…` et se place relativement à ce parent ; `CtrlType` 1 = image/texte, 3 = bouton,
//!   8 = grille, 9 = jauge (`Bar=…:主_Character <max> <courant>`), 16 = minicarte ; `Texture=i:nom`
//!   (`texture/ui`), dessinée au pixel près depuis son coin haut-gauche (pas d'étirement) ;
//! * positions en 1920 × 1080 : `ini/UIPosition_1920x1080.ini`, `UIPos=1:x,y` (UI_CharInfo 448,789,
//!   MiniMap 1746,0) ;
//! * minicarte : `texture/minimap/map%04d%c.png` (FUN_140702310), quart `a`…`d` d'une tuile de 16 × 16
//!   secteurs, 193 px pour 8 × 8 secteurs, 0,0037695 px par unité ; origine du quart
//!   (FUN_140703bf0/…c30) : x0 = (tuile / 100 − 25) × 16 × 6400 (+ 51 200 si quart & 1),
//!   z0 = (tuile % 100 − 25) × 16 × 6400 (+ 51 200 si quart / 2) ; raster de 237 px centré sur le
//!   joueur affiché dans le contrôle de 148 px, masqué par `Main_Minimap_Alpha.png` ;
//! * expérience par niveau : `ini/LevelUpExp.txt` (`level\texp`) ;
//! * icônes des compétences : `ini/SKILLTEMPLATE.ini`, `SMALL_SKILL_TEXTURE` (Q…) sinon
//!   `SKILL_TEXTURE` (P…), dans `texture/skill`.
//!
//! **Choix d'ArchRust** : la rangée des améliorations est posée sous la minicarte (aucune fenêtre du
//! fichier d'interface ne la décrit) ; la barre de raccourcis reste vide tant que le serveur n'envoie
//! pas son contenu (module UI_STATUS 0x14).

use crate::assets::Assets;
use archrust_data::ini::IniTable;
use archrust_netclient::world::{CharacterStats, ItemRecord};
use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "hud_items.rs"]
mod items_ui;
pub use items_ui::{draw_character, draw_drag, EquippedItem, ItemDrag, ItemUi};

#[path = "hud_npc.rs"]
mod npc_ui;

#[path = "hud_skills.rs"]
mod skills_ui;

#[path = "hud_mastery.rs"]
mod mastery_ui;
pub use mastery_ui::{draw_tree, SkillTab, TreeSlot, TreeView};

#[path = "hud_portal.rs"]
mod portal_ui;

#[path = "hud_guild.rs"]
mod guild_ui;

#[path = "hud_dialogs.rs"]
mod dialogs_ui;

#[path = "hud_party.rs"]
mod party_ui;

#[path = "hud_menu.rs"]
mod menu_ui;

#[path = "hud_login.rs"]
mod login_ui;

#[path = "hud_options.rs"]
mod options_ui;

#[path = "hud_convert.rs"]
mod convert_ui;
pub use convert_ui::{colored_text, draw_convert, draw_convert_confirm, item_tooltip, item_tooltip_at, ConvertTab, ConvertUi, ConvertView};

#[path = "hud_refinery.rs"]
mod refinery_ui;
pub use refinery_ui::{draw_cooldown, draw_recipes, draw_refinery, RecipeUi, RecipeView, RefineryUi, RefineryView};

#[path = "hud_repair.rs"]
mod repair_ui;
pub use repair_ui::{draw_equip_status, draw_repair, RepairUi, RepairView};

#[path = "hud_product.rs"]
mod product_ui;
pub use product_ui::{draw_compose, draw_pdskill, draw_pdskill_buy, ComposeUi, ComposeView, PdBuyEntry, PdBuyView, PdSkillEntry, PdSkillUi};

#[path = "hud_buddy.rs"]
mod buddy_ui;
pub use buddy_ui::{draw_add_name, draw_ban_list, draw_buddy_list, draw_target_menu, BanListView, BuddyListView, BuddyUi, TargetMenuUi, LIST_ROWS as BUDDY_LIST_ROWS};

#[path = "hud_mail.rs"]
mod mail_ui;
pub use mail_ui::{draw_mail_read, draw_mail_write, draw_mailbox, MailBoxView, MailReadView, MailRow, MailUi, MailWriteView, MAIL_ROWS};

#[path = "hud_trade.rs"]
mod trade_ui;
pub use trade_ui::{draw_private_trade, draw_trade_money, draw_trade_request, draw_trade_yes_no, TradeSlot, TradeUi, TradeView};

#[path = "hud_auction.rs"]
mod auction_ui;
pub use auction_ui::{draw_auction, draw_create_sale, draw_sales_box, AuctionSort, AuctionUi, AuctionView, BoardLine, CreateSaleUi, CreateSaleView, SalesBoxUi, SalesBoxView, SalesLine, TreeLine, AUCTION_BOARD_ROWS, AUCTION_TREE_ROWS, SALES_BOX_ROWS};

#[path = "hud_itemset.rs"]
mod itemset_ui;
pub use itemset_ui::{draw_switch_editor, SwitchUi, SwitchView};

#[path = "hud_chat.rs"]
mod chat_ui;
pub use chat_ui::{draw_chat_room, draw_create_room, draw_invite_received, draw_prompt, ChatRoomUi, ChatRoomView};

#[path = "hud_customize.rs"]
mod customize_ui;
pub use customize_ui::{draw_customize_confirm, draw_customizing, CustomizeUi, CustomizeView, LIST_ROWS as CUSTOMIZE_LIST_ROWS};

#[path = "hud_wanted.rs"]
mod wanted_ui;
pub use wanted_ui::{draw_arrange, draw_system_grid_icon, draw_wanted_alarm, draw_wanted_board, ArrangeView, WantedBoardView, WantedRow, WantedUi};

#[path = "hud_summons.rs"]
mod summons_ui;
pub use summons_ui::{draw_summons_menu, SummonsUi};

#[path = "hud_shrine.rs"]
mod shrine_ui;
pub use shrine_ui::{draw_shrine_hp, draw_shrine_info, minimap_shrine_button, ShrineIcon, ShrineInfoView, ShrineMapEntry, ShrineRow, ShrineUi, SHRINE_LIST_ROWS};

#[path = "hud_titles.rs"]
mod titles_ui;
pub use titles_ui::{draw_title_all, draw_title_current, draw_title_mini, TitleAllView, TitleDetail, TitleListView, TitleRow, TitleUi, TitleWindow, TITLE_PAGE_ROWS, TITLE_SHORT_ROWS};

#[path = "hud_consumables.rs"]
mod consumables_ui;
pub use consumables_ui::{draw_resurrect_town_confirm, draw_resurrection_orb, draw_skill_init_confirm};

#[path = "hud_siege.rs"]
mod siege_ui;
pub use siege_ui::{draw_carve_message, draw_resurrection_siege, draw_siege_window, SiegeGuildRow, SiegeUi, SiegeView, SIEGE_LIST_ROWS};

#[path = "hud_tax.rs"]
mod tax_ui;
pub use tax_ui::{draw_announce, draw_tax_window, TaxView};

#[path = "hud_boss.rs"]
mod boss_ui;
pub use boss_ui::{draw_boss, draw_boss_button, draw_boss_dps, BossDetail, BossEntry, BossUi, BossView, DpsRow, BOSS_LIST_ROWS};

#[path = "hud_worldmap.rs"]
mod worldmap_ui;
pub use worldmap_ui::{draw_world_map, map_to_world, world_to_map, WorldMapUi, WorldMapView};

#[path = "hud_status.rs"]
mod status_ui;

#[path = "hud_hotkey.rs"]
mod hotkey_ui;
pub use hotkey_ui::{draw_exit, draw_hotkey, ExitUi, HotkeyRow, HotkeyUi, HotkeyView};

#[path = "hud_help.rs"]
mod help_ui;
pub use help_ui::{draw_help, draw_tips, parse_tab_table, read_tab_table, tip_segments, HelpEntry, HelpState, TipUi};

#[path = "hud_social.rs"]
mod social_ui;
pub use social_ui::{draw_shrine_buy_confirm, draw_social_action, ShrineBuyUi, ShrineBuyView, SocialUi, SocialView};

#[path = "hud_petbag.rs"]
mod petbag_ui;
pub use petbag_ui::{draw_cash_inventory, draw_sub_inventory, CashInvenView, CashLine, PetBagUi, SubInvenView, CASH_ROWS};
pub use options_ui::{draw_options, OptionsEdit, OptionsOut};
pub use login_ui::{draw_char_create, draw_char_select, draw_id_password, draw_select_server, CharSlotView, CreateUi, LoginUi, SelectUi};
pub use menu_ui::{draw_main_menu, main_menu_open, toggle_options_menu, MenuUi};
pub use party_ui::{draw_party, party_face, PartyFrame, PartyFrames, PartyUi, PartyUiState};
pub use dialogs_ui::{info_box, confirm_box, draw_bank_money, draw_buy_count, draw_dialog, draw_drop_money, draw_split, ui_message, DialogOut};
pub use guild_ui::{draw_battle_texts, draw_guild_warehouse, draw_guild_window, draw_gware_money, draw_message_box, GridCell, GuildEdit, GuildRow, GuildUi, GuildView, GwareItem, GwareView, ListSpec, PageColors};
pub use portal_ui::{draw_buy_bank, draw_loading, draw_reconnect_hold, draw_remission, draw_teleport, BuyBankView, PortalUi, RemissionUi, RemissionView, TeleportRow, TeleportView};
pub use skills_ui::{draw_quickbar, draw_skill_book, draw_trainer, BookEntry, BookView, Shortcut, SkillUi, SlotView, TrainerEntry, TrainerView, EXTRA_BELTS, QUICK_SLOTS};
pub use npc_ui::{draw_bank, draw_npc_dialog, draw_quests, draw_shop, BankView, DialogView, NpcUi, QuestEntry, QuestState, QuestView, ShopEntry, ShopView};

/// Fenêtres de guilde d'Archonia (voir `hud_guild.rs`).
const GUILD_WINDOWS: [&str; 18] = [
    "Guild_Main",
    "Guild_Create",
    "Guild_Destroy",
    "Guild_Create_Request",
    "Guild_Create_Complete",
    "Guild_Destroy_Confirm",
    "Guild_Destroy_Complete",
    "Guild_Notice",
    "Guild_Member",
    "Guild_List",
    "Guild_MemberSize",
    "Guild_Mark",
    "Guild_MarkConfirm",
    "Guild_GBattle",
    "UI_GuildWarehouse",
    "UI_GWareConfirm",
    "UI_GuildBattleResult",
    "UI_GuildBattleMember",
];

/// Amélioration active sur le joueur.
#[derive(Debug, Clone)]
pub struct ActiveBuff {
    pub skill_tid: i32,
    /// Fin annoncée (`None` = durée inconnue ou permanente).
    pub expires: Option<Instant>,
    pub total_ms: u64,
}

/// Contrôle d'une fenêtre d'interface, position absolue dans la fenêtre (px virtuels 1920 × 1080).
#[derive(Debug, Clone, Default)]
pub struct UiControl {
    pub name: String,
    pub kind: i32,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub textures: Vec<(u32, String)>,
    pub display: Option<String>,
    pub bar: Option<String>,
    /// `Grid=…:écart x:écart y:largeur:hauteur:…`.
    pub grid: Option<[f32; 4]>,
    /// Nom de la grille, dernier élément de `Grid=` (ex. `EquipSlotGridHead`).
    pub grid_name: Option<String>,
    /// Décalage de la première case, deux premiers éléments de `Grid=`.
    pub grid_origin: Option<[f32; 2]>,
    /// `CtrlFont=0:couleur:échelle:…:alignement:…` : couleur ARGB du texte (-1 blanc) et alignement
    /// (relevés : 16 à gauche, 17 centré, 18 à droite).
    pub font: Option<(u32, i32)>,
    /// `Status2=n:rect:…:couleur:. Nom` : couleur du texte de chaque statut (0, 1, 2…).
    pub status_colors: Vec<u32>,
    /// `Button=repos:survol:appui:inactif:…` : indice de `Texture=` de chaque état (0 : aucune).
    pub button_states: Option<[u32; 4]>,
    /// Premier rappel `Action=3:Nom` (ex. `OP_ViewNameMonster`) : certains noms de contrôle se
    /// répètent dans une fenêtre, pas leurs rappels.
    pub action: Option<String>,
    /// Texte fixe du même contrôle dans le dossier de langue (`CtrlDisplay=1:…` d'`ini_fr/UI_1024X768.ini`),
    /// pour les fenêtres qui reconnaissent leurs contrôles au texte anglais de `display`.
    pub label: Option<String>,
}

impl UiControl {
    /// Texte fixe affiché : celui du dossier de langue, sinon `CtrlDisplay=1:…` ; vide sans texte fixe.
    pub fn fixed_label(&self) -> &str {
        self.label.as_deref().or_else(|| self.display.as_deref().and_then(|d| d.strip_prefix("1:"))).unwrap_or("")
    }
}

#[derive(Debug, Clone, Default)]
pub struct UiWindow {
    pub name: String,
    /// Position en 1920 × 1080 (UIPosition, index 1).
    pub pos: [f32; 2],
    /// `Pos=` du fichier d'interface (disposition 1024 × 768, `UIPos=0`) : sert à placer une
    /// fenêtre par rapport à une autre (listes des menus principaux au-dessus de leur bouton).
    pub pos0: [f32; 2],
    pub size: [f32; 2],
    pub textures: Vec<(u32, String)>,
    pub controls: Vec<UiControl>,
}

fn pair(v: &str) -> Option<[f32; 2]> {
    let (a, b) = v.split_once(',')?;
    Some([a.trim().parse().ok()?, b.trim().parse().ok()?])
}

fn texture_entry(v: &str) -> Option<(u32, String)> {
    let (i, n) = v.split_once(':')?;
    Some((i.trim().parse().ok()?, n.trim().to_string()))
}

impl UiWindow {
    /// Fenêtre `name` du fichier d'interface, placée d'après le fichier de positions 1920 × 1080.
    pub fn parse(ui: &IniTable, positions: Option<&IniTable>, name: &str) -> Option<UiWindow> {
        let sec = ui.section(name)?;
        let mut w = UiWindow {
            name: name.to_string(),
            ..Default::default()
        };
        let mut controls: Vec<UiControl> = Vec::new();
        let mut abs: HashMap<String, [f32; 2]> = HashMap::new();
        let mut parents: Vec<Option<String>> = Vec::new();
        let mut pending_parent: Option<String> = None;
        let mut window_pos1: Option<[f32; 2]> = None;
        for (k, v) in &sec.entries {
            match k.as_str() {
                "CtrlParent" => pending_parent = Some(v.trim().to_string()),
                "CtrlType" => {
                    controls.push(UiControl {
                        kind: v
                            .split(':')
                            .next()
                            .and_then(|t| t.trim().parse().ok())
                            .unwrap_or(0),
                        ..Default::default()
                    });
                    parents.push(pending_parent.take());
                }
                _ => {
                    let target = controls.last_mut();
                    match (k.as_str(), target) {
                        ("CtrlName", Some(c)) => c.name = v.trim().to_string(),
                        ("Pos", Some(c)) => c.pos = pair(v).unwrap_or_default(),
                        ("Size", Some(c)) => c.size = pair(v).unwrap_or_default(),
                        ("Texture", Some(c)) => c.textures.extend(texture_entry(v)),
                        ("CtrlDisplay", Some(c)) => c.display = Some(v.trim().to_string()),
                        ("Bar", Some(c)) => c.bar = Some(v.trim().to_string()),
                        ("Action", Some(c)) if c.action.is_none() => c.action = v.trim().strip_prefix("3:").map(|a| a.trim().to_string()),
                        ("CtrlFont", Some(c)) => {
                            let p: Vec<&str> = v.split(':').collect();
                            if let (Some(color), Some(align)) = (p.get(1).and_then(|x| x.trim().parse::<i64>().ok()), p.get(4).and_then(|x| x.trim().parse().ok())) {
                                c.font = Some((color as u32, align));
                            }
                        }
                        ("Button", Some(c)) => {
                            let p: Vec<u32> = v.split(':').take(4).filter_map(|x| x.trim().parse::<i64>().ok()).map(|x| x.max(0) as u32).collect();
                            if let [a, b, h, d] = p[..] {
                                c.button_states = Some([a, b, h, d]);
                            }
                        }
                        ("Status2", Some(c)) => {
                            if let Some(color) = v.split(':').nth(4).and_then(|x| x.trim().parse::<i64>().ok()) {
                                c.status_colors.push(color as u32);
                            }
                        }
                        ("Grid", Some(c)) => {
                            let p: Vec<f32> =
                                v.split(':').filter_map(|x| x.trim().parse().ok()).collect();
                            if p.len() >= 6 {
                                c.grid = Some([p[2], p[3], p[4], p[5]]);
                                c.grid_origin = Some([p[0], p[1]]);
                                c.grid_name = v.rsplit(':').next().map(str::trim).filter(|n| !n.is_empty() && n.parse::<f32>().is_err()).map(str::to_string);
                            }
                        }
                        ("Pos", None) => {
                            w.pos = pair(v).unwrap_or_default();
                            w.pos0 = w.pos;
                        }
                        ("Size", None) => w.size = pair(v).unwrap_or_default(),
                        ("Texture", None) => w.textures.extend(texture_entry(v)),
                        ("UIPos", None) => {
                            if let Some(rest) = v.strip_prefix("1:") {
                                window_pos1 = pair(rest);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // Positions absolues : parent déjà vu (ordre du fichier), sinon la fenêtre.
        for (c, parent) in controls.iter_mut().zip(parents) {
            let base = parent
                .and_then(|p| abs.get(&p).copied())
                .unwrap_or([0.0, 0.0]);
            c.pos = [base[0] + c.pos[0], base[1] + c.pos[1]];
            abs.insert(c.name.clone(), c.pos);
        }
        w.controls = controls;
        let from_positions = positions.and_then(|t| t.section(name)).and_then(|s| {
            s.get_all("UIPos")
                .find_map(|v| v.strip_prefix("1:").and_then(pair))
        });
        w.pos = from_positions.or(window_pos1).unwrap_or(w.pos);
        Some(w)
    }

    pub fn control(&self, name: &str) -> Option<&UiControl> {
        self.controls.iter().find(|c| c.name == name)
    }
}

/// Image d'interface téléversée : texture egui et taille de l'image d'origine du client (les UV se
/// calculent sur elle, même si un patch HD fournit une version agrandie).
struct UiImage {
    tex: egui::TextureHandle,
    orig: [f32; 2],
}

/// Minicarte composée sur le CPU.
#[derive(Default)]
struct MinimapState {
    tex: Option<egui::TextureHandle>,
    drawn_at: Option<(i32, i32, i32)>,
    tiles: HashMap<String, Option<Arc<crate::assets::RgbaImage>>>,
    mask: Option<Option<crate::assets::RgbaImage>>,
    /// Facteur de zoom d'AcUIMinimap (`+0x4e3c`, 1 au départ, FUN_1408d3920) : part du raster de
    /// 237 px affichée = facteur × 148 px ; ± 0,2 par bouton, de 0,4 à 1,6 (FUN_1408d3c80 / 3cf0).
    /// 0 : pas encore réglé (lu comme 1).
    scale: f32,
    /// Carte masquée (`Minimap_Map_Toggle`, FUN_1407015a0 : fond `Main_Minimap_Base_hide.png`).
    hidden: bool,
}

/// Demande des boutons de la minicarte traitée par le moteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimapUi {
    /// `Minimap_CameraView_Click` : mode de caméra suivant.
    CameraView,
    /// `WorldMap_Toggle` (`CONTROL_MINIMAP_WORLDOPEN`) : carte du monde.
    WorldMap,
}

/// Repère d'AgcmMinimap (FUN_1408d5490) : type de la liste `+0x1b88` et position du monde.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapIcon {
    /// 0 PNJ (`Main_Minimap_Pos_NPC0`, 10 px), 1 quête proposée (`…_NPC2`, 16 px), 2 quête à rendre
    /// (`…_NPC1`, 16 px), 3 / 4 objets de siège (`…_Siege0` 17 px, `…_Siege1` 20 px).
    pub kind: u8,
    pub pos: Vec3,
}

impl MinimapIcon {
    /// Image et côté (px du raster) du type (FUN_1408d5490).
    pub fn image(&self) -> (&'static str, f32) {
        match self.kind {
            1 => ("Main_Minimap_Pos_NPC2.png", 16.0),
            2 => ("Main_Minimap_Pos_NPC1.png", 16.0),
            3 => ("Main_Minimap_Pos_Siege0.png", 17.0),
            4 => ("Main_Minimap_Pos_Siege1.png", 20.0),
            _ => ("Main_Minimap_Pos_NPC0.png", 10.0),
        }
    }
}

/// Facteur de zoom suivant d'un bouton de la minicarte : « + » retire 0,2 (pas sous 0,4), « − » ajoute
/// 0,2 (pas au-delà de 1,6), en dixièmes entiers comme FUN_1408d3c80 / FUN_1408d3cf0.
pub fn minimap_zoom(scale: f32, plus: bool) -> f32 {
    let tenths = (scale * 10.0).round() as i32;
    let next = if plus { tenths - 2 } else { tenths + 2 };
    if (plus && next > 3) || (!plus && next < 17) {
        next as f32 * 0.1
    } else {
        scale
    }
}

#[derive(Default)]
pub struct HudState {
    /// Dernières caractéristiques connues du joueur (champs fusionnés au fil des paquets).
    pub stats: CharacterStats,
    pub buffs: Vec<ActiveBuff>,
    windows: Option<Vec<UiWindow>>,
    images: HashMap<String, Option<UiImage>>,
    exp_table: Option<HashMap<i32, u64>>,
    skill_icons: Option<HashMap<i32, String>>,
    minimap: MinimapState,
    /// Sac affiché dans la fenêtre d'inventaire (onglets « Bag 1 » à « Bag 5 »).
    inventory_layer: i16,
    /// Clic sur l'or de l'inventaire : `UI_DropMoneyFromInventory` à ouvrir (lu par `app_items.rs`).
    pub drop_money_request: bool,
    item_icons: Option<HashMap<i32, String>>,
    /// Messages système récents (texte, arrivée), affichés en orange à droite comme Archonia.
    system_messages: Vec<(String, Instant)>,
    /// Plus haut PV vu par CID : le serveur n'envoie pas le maximum des monstres.
    max_hp_seen: HashMap<i32, i32>,
    /// Phrases de `uimessage.txt` (voir `hud_dialogs.rs`), chargées au premier besoin.
    ui_messages: Option<HashMap<String, String>>,
    /// Liste ouverte des menus principaux (`UI_MainMenu*`, voir `hud_menu.rs`).
    main_menu: Option<String>,
    /// Choix des menus principaux, traités par le moteur à l'image suivante.
    pub menu_requests: Vec<MenuUi>,
    /// Onglet des options (0 GFX, 1 Son, 2 Affichage, 3 Fonction, 4 Avancé) et défilement de la
    /// liste « Avancé » (voir `hud_options.rs`).
    pub options_tab: u8,
    options_scroll: u16,
    /// Infobulle de chaque objet du joueur (IID → lignes colorées), préparée par le moteur avant la
    /// passe egui (voir `app_convert.rs`) ; sans entrée, le nom seul.
    pub item_tips: HashMap<i32, Vec<(String, Color32)>>,
    /// Pourcentage de durabilité des objets du joueur qui en ont une (IID → 0 à 100, voir
    /// `app_repair.rs`) : marque des cases à 0 % (porté) ou à 20 % et moins (sac).
    pub durability: HashMap<i32, i32>,
    /// Boutons de la minicarte cliqués (caméra, carte du monde), traités par le moteur.
    pub minimap_requests: Vec<MinimapUi>,
}

/// Cible du joueur affichée dans `UI_TargetInfo`.
#[derive(Debug, Clone)]
pub struct HudTarget {
    pub cid: i32,
    pub name: String,
    pub hp: Option<i32>,
    pub is_player_character: bool,
}

/// Données de la scène utiles au HUD.
pub struct HudScene {
    /// Cible choisie (clic, `/target`, AddOns).
    pub target: Option<HudTarget>,
    pub player: Option<Vec3>,
    /// Direction de la caméra au sol (x, z), pour le cône de la minicarte.
    pub camera_forward: Vec3,
    /// Repères de la minicarte (PNJ, quêtes), voir [`MinimapIcon`].
    pub minimap_icons: Vec<MinimapIcon>,
}

/// Pixels de minicarte par unité du monde (FUN_1408d5220).
const MINIMAP_PX_PER_UNIT: f32 = 0.003_769_531_1;
/// Côté du raster de minicarte d'Archonia (largeur du contrôle × 1,6, `DAT_140d8dca0`) ; le zoom en
/// affiche `facteur × 148` px dans le contrôle de 148 px.
const MINIMAP_RASTER: f32 = 237.0;
/// Largeur du contrôle de carte (`MiniMap1`, type 16) dans `UI_1024X768.ini`.
const MINIMAP_VIEW: f32 = 148.0;

/// Côté (px du raster) de la part affichée de la minicarte pour un facteur de zoom.
fn minimap_shown(scale: f32) -> f32 {
    let s = if scale > 0.0 { scale } else { 1.0 };
    (s * MINIMAP_VIEW).min(MINIMAP_RASTER)
}

impl HudState {
    /// Oublie les images d'interface et la minicarte déjà chargées : elles sont relues à leur
    /// prochain affichage (packs rechargés en cours de jeu).
    pub fn forget_images(&mut self) {
        self.images.clear();
        self.minimap = MinimapState::default();
    }

    pub fn merge_stats(&mut self, s: &CharacterStats) {
        let st = &mut self.stats;
        st.level = s.level.or(st.level);
        st.hp = s.hp.or(st.hp);
        st.mp = s.mp.or(st.mp);
        st.max_hp = s.max_hp.or(st.max_hp);
        st.max_mp = s.max_mp.or(st.max_mp);
        st.exp = s.exp.or(st.exp);
        st.money = s.money.or(st.money);
        st.bank_money = s.bank_money.or(st.bank_money);
        st.cash = s.cash.or(st.cash);
        st.bank_pages = s.bank_pages.or(st.bank_pages);
        st.skill_points = s.skill_points.or(st.skill_points);
        st.heroic_points = s.heroic_points.or(st.heroic_points);
        // Facteurs détaillés (fenêtre `Character_Status`, voir `hud_status.rs`) et minuteries PvP.
        st.factors.extend(s.factors.iter().map(|(k, v)| (*k, *v)));
        st.murderer = s.murderer.or(st.murderer);
        st.criminal_left = s.criminal_left.or(st.criminal_left);
        st.murderer_left = s.murderer_left.or(st.murderer_left);
        st.tid = s.tid.or(st.tid);
        st.properties.extend(s.properties.iter().map(|(k, v)| (*k, *v)));
        st.pvp = s.pvp.or(st.pvp);
    }

    /// Valeurs des opérations 34 à 36 d'AGPMCHARACTER (ignorés, ténacités, dégâts dans la durée).
    pub fn set_character_properties(&mut self, op: i8, values: &[i32]) {
        for (i, v) in values.iter().enumerate() {
            self.stats.properties.insert((op, i as u8), *v);
        }
    }

    pub fn add_buff(&mut self, skill_tid: i32, duration_ms: u64) {
        let expires =
            (duration_ms > 0).then(|| Instant::now() + Duration::from_millis(duration_ms));
        self.buffs.retain(|b| b.skill_tid != skill_tid);
        self.buffs.push(ActiveBuff {
            skill_tid,
            expires,
            total_ms: duration_ms,
        });
    }

    /// Message système reçu (AGPMSYSTEMMESSAGE) : affiché quelques secondes à droite de l'écran.
    pub fn push_system(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.system_messages.push((text.to_string(), Instant::now()));
        let excess = self.system_messages.len().saturating_sub(8);
        self.system_messages.drain(..excess);
    }

    /// PV maximum d'un personnage quand le serveur le donne (facteurs de la vue d'un monstre).
    pub fn note_max_hp(&mut self, cid: i32, max_hp: i32) {
        self.max_hp_seen.insert(cid, max_hp.max(1));
    }

    /// Sac affiché dans la fenêtre d’inventaire (0 à 4).
    pub fn set_inventory_layer(&mut self, layer: i16) {
        self.inventory_layer = layer.clamp(0, 4);
    }

    pub fn remove_buff(&mut self, skill_tid: i32) {
        self.buffs.retain(|b| b.skill_tid != skill_tid);
    }

    /// Retire les améliorations échues (le serveur envoie aussi un retrait, mais on n'attend pas).
    pub fn expire_buffs(&mut self) {
        let now = Instant::now();
        self.buffs.retain(|b| b.expires.is_none_or(|t| t > now));
    }

    /// Part de l'expérience du niveau courant, entre 0 et 1, si le niveau et la table sont connus.
    pub fn exp_ratio(&mut self, assets: &Assets) -> Option<f32> {
        let level = self.stats.level?;
        let exp = self.stats.exp?;
        let table = self.exp_table.get_or_insert_with(|| load_exp_table(assets));
        let needed = *table.get(&level)?;
        (needed > 0).then(|| (exp as f64 / needed as f64).clamp(0.0, 1.0) as f32)
    }

    fn windows(&mut self, assets: &Assets) -> &[UiWindow] {
        self.windows.get_or_insert_with(|| {
            let ui = assets.table("ini", "UI_1024X768.ini").ok();
            let pos = assets.table("ini", "UIPosition_1920x1080.ini").ok();
            let Some(ui) = ui else {
                log::warn!("HUD : ini/UI_1024X768.ini illisible");
                return Vec::new();
            };
            let mut list: Vec<UiWindow> = ["UI_CharInfo", "MiniMap", "UI_Inventory", "UI_TargetInfo", "Chat_Input_Normal", "UI_Resurrection", "Character_Status", "UI_NPCDialog", "UI_NPCTrade", "UI_Bank", "QuestAccept_UI", "QuestMain_UI", "UI_Skill", "UI_SkillBuy", "UI_Teleport", "UI_BuyBank", "UI_LG_Loading", "UI_SplitItem", "UI_NPCTrade_BuyStackCount", "UI_BankConfirm", "UI_Party", "UI_SelfCharacterMenu", "UI_TargetCharacterMenu", "UI_PartyOption", "UI_LG_CHARSELECT", "UI_ConfirmDeleteChar", "UI_LG_CHARCREATE", "Chat_Input_Private", "UI_ReconnectHold", "UI_Remission", "UI_ConfirmRecall", "UI_SHRINEITEM_BUY_CONFIRM"]
                .iter()
                .chain(GUILD_WINDOWS.iter())
                .chain(menu_ui::MENU_WINDOWS.iter())
                .chain(options_ui::OPTION_WINDOWS.iter())
                .chain(convert_ui::CONVERT_WINDOWS.iter())
                .chain(refinery_ui::REFINERY_WINDOWS.iter())
                .chain(repair_ui::REPAIR_WINDOWS.iter())
                .chain(product_ui::PRODUCT_WINDOWS.iter())
                .chain(buddy_ui::BUDDY_WINDOWS.iter())
                .chain(mail_ui::MAIL_WINDOWS.iter())
                .chain(trade_ui::TRADE_WINDOWS.iter())
                .chain(auction_ui::AUCTION_WINDOWS.iter())
                .chain(itemset_ui::ITEMSET_WINDOWS.iter())
                .chain(chat_ui::CHAT_WINDOWS.iter())
                .chain(customize_ui::CUSTOMIZE_WINDOWS.iter())
                .chain(wanted_ui::WANTED_WINDOWS.iter())
                .chain(summons_ui::SUMMONS_WINDOWS.iter())
                .chain(shrine_ui::SHRINE_WINDOWS.iter())
                .chain(titles_ui::TITLE_WINDOWS.iter())
                .chain(boss_ui::BOSS_WINDOWS.iter())
                .chain(siege_ui::SIEGE_WINDOWS.iter())
                .chain(consumables_ui::CONSUMABLE_WINDOWS.iter())
                .chain(tax_ui::TAX_WINDOWS.iter())
                .chain(worldmap_ui::WORLDMAP_WINDOWS.iter())
                .chain(skills_ui::QUICKBELT_WINDOWS.iter())
                .chain(hotkey_ui::HOTKEY_WINDOWS.iter())
                .chain(help_ui::HELP_WINDOWS.iter())
                .chain(login_ui::LOGIN_WINDOWS.iter())
                .chain(mastery_ui::MASTERY_WINDOWS.iter())
                .chain([mastery_ui::SCION_ARCHLORD_WINDOW].iter())
                .chain(social_ui::SOCIAL_WINDOWS.iter())
                .chain(petbag_ui::PETBAG_WINDOWS.iter())
                .filter_map(|n| UiWindow::parse(&ui, pos.as_deref(), n))
                .collect();
            // Textes fixes du dossier de langue (`ini_fr/UI_1024X768.ini`…, mêmes contrôles) pour les
            // fenêtres qui les affichent tels quels ; rien à remplacer en anglais (`ini`).
            let lang = crate::assets::text_dir();
            if let Some(fr) = (lang != "ini").then(|| assets.table(lang, "UI_1024X768.ini").ok()).flatten() {
                for w in list.iter_mut().filter(|w| ["Character_Status", "UI_WORLD_MAP", "UI_Teleport", "UI_BuyBank", "UI_Bank", "UI_SplitItem", "UI_NPCTrade_BuyStackCount", "UI_BankConfirm", "UI_Party", "UI_SelfCharacterMenu", "UI_TargetCharacterMenu", "UI_PartyOption", "UI_LG_CHARSELECT", "UI_ConfirmDeleteChar", "UI_LG_CHARCREATE", "UI_ReconnectHold", "UI_Remission", "UI_ConfirmRecall", "UI_SHRINEITEM_BUY_CONFIRM"].contains(&w.name.as_str()) || GUILD_WINDOWS.contains(&w.name.as_str()) || menu_ui::MENU_WINDOWS.contains(&w.name.as_str()) || options_ui::OPTION_WINDOWS.contains(&w.name.as_str()) || convert_ui::CONVERT_WINDOWS.contains(&w.name.as_str()) || refinery_ui::REFINERY_WINDOWS.contains(&w.name.as_str()) || repair_ui::REPAIR_WINDOWS.contains(&w.name.as_str()) || product_ui::PRODUCT_WINDOWS.contains(&w.name.as_str()) || buddy_ui::BUDDY_WINDOWS.contains(&w.name.as_str()) || mail_ui::MAIL_WINDOWS.contains(&w.name.as_str()) || trade_ui::TRADE_WINDOWS.contains(&w.name.as_str()) || auction_ui::AUCTION_WINDOWS.contains(&w.name.as_str()) || chat_ui::CHAT_WINDOWS.contains(&w.name.as_str()) || customize_ui::CUSTOMIZE_WINDOWS.contains(&w.name.as_str()) || wanted_ui::WANTED_WINDOWS.contains(&w.name.as_str()) || summons_ui::SUMMONS_WINDOWS.contains(&w.name.as_str()) || shrine_ui::SHRINE_WINDOWS.contains(&w.name.as_str()) || boss_ui::BOSS_WINDOWS.contains(&w.name.as_str()) || siege_ui::SIEGE_WINDOWS.contains(&w.name.as_str()) || tax_ui::TAX_WINDOWS.contains(&w.name.as_str())) {
                    let Some(fw) = UiWindow::parse(&fr, None, &w.name) else { continue };
                    for c in &mut w.controls {
                        if let Some(d) = fw.control(&c.name).and_then(|x| x.display.clone()) {
                            c.display = Some(d);
                        }
                    }
                }
                // Autres fenêtres : texte fixe traduit à part (`label`), `display` reste l'anglais qui sert
                // à reconnaître les contrôles (onglets des compétences, « 1:Close »…).
                for w in list.iter_mut() {
                    let Some(fw) = UiWindow::parse(&fr, None, &w.name) else { continue };
                    for (i, c) in w.controls.iter_mut().enumerate() {
                        if c.display.as_deref().is_some_and(|d| d.starts_with("1:")) {
                            // Même rang d'abord : certains noms de contrôle se répètent dans une fenêtre.
                            let x = fw.controls.get(i).filter(|x| x.name == c.name).or_else(|| fw.control(&c.name));
                            c.label = x.and_then(|x| x.display.as_deref()?.strip_prefix("1:").map(str::to_string));
                        }
                    }
                }
            }
            log::info!("HUD : {} fenêtre(s) d'interface Archonia", list.len());
            list
        })
    }

    pub(crate) fn image(
        &mut self,
        ctx: &egui::Context,
        assets: &Assets,
        name: &str,
    ) -> Option<(egui::TextureId, [f32; 2])> {
        let key = name.to_ascii_lowercase();
        if !self.images.contains_key(&key) {
            let loaded = load_ui_image(ctx, assets, &key);
            self.images.insert(key.clone(), loaded);
        }
        self.images
            .get(&key)?
            .as_ref()
            .map(|i| (i.tex.id(), i.orig))
    }

    fn skill_icon(&mut self, assets: &Assets, tid: i32) -> Option<String> {
        let map = self
            .skill_icons
            .get_or_insert_with(|| load_skill_icons(assets));
        map.get(&tid).cloned()
    }
}

fn load_ui_image(ctx: &egui::Context, assets: &Assets, name: &str) -> Option<UiImage> {
    let img = match assets.image("texture/ui", name) {
        Ok(i) => i,
        Err(e) => {
            log::debug!("HUD : texture/ui/{name} : {e:#}");
            return None;
        }
    };
    // Taille d'origine : en-tête PNG de l'entrée du client (un patch HD peut l'agrandir).
    let orig = assets
        .read_packed("texture/ui", name)
        .ok()
        .filter(|b| b.len() >= 24 && b.starts_with(b"\x89PNG"))
        .map(|b| {
            let w = u32::from_be_bytes(b[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(b[20..24].try_into().unwrap());
            [w as f32, h as f32]
        })
        .unwrap_or([img.width as f32, img.height as f32]);
    Some(UiImage {
        tex: upload(ctx, &format!("hud-{name}"), &img),
        orig,
    })
}

fn upload(ctx: &egui::Context, name: &str, img: &crate::assets::RgbaImage) -> egui::TextureHandle {
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [img.width as usize, img.height as usize],
        &img.data,
    );
    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
}

fn load_exp_table(assets: &Assets) -> HashMap<i32, u64> {
    // En clair (données du serveur) ou chiffrée (client).
    let text = assets.read_text("ini", "LevelUpExp.txt").unwrap_or_default();
    let map: HashMap<i32, u64> = text
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((
                it.next()?.trim().parse().ok()?,
                it.next()?.trim().parse().ok()?,
            ))
        })
        .collect();
    log::info!(
        "HUD : expérience de {} niveau(x) (LevelUpExp.txt)",
        map.len()
    );
    map
}

fn load_skill_icons(assets: &Assets) -> HashMap<i32, String> {
    let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") else {
        return HashMap::new();
    };
    let Some(header) = t.section("") else {
        return HashMap::new();
    };
    let col = |name: &str| {
        header
            .entries
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone())
    };
    let (small, big) = (col("SMALL_SKILL_TEXTURE"), col("SKILL_TEXTURE"));
    t.sections
        .iter()
        .filter_map(|s| {
            let tid: i32 = s.name.parse().ok()?;
            let icon = small
                .as_deref()
                .and_then(|c| s.get(c))
                .or_else(|| big.as_deref().and_then(|c| s.get(c)))?;
            Some((tid, icon.trim().to_ascii_lowercase()))
        })
        .collect()
}

/// Passage des coordonnées virtuelles 1920 × 1080 à l'écran : échelle uniforme, fenêtre ancrée au
/// bord (ou au centre) le plus proche, comme les positions d'Archonia collées aux bords.
#[derive(Clone, Copy)]
struct Layout {
    screen: Rect,
    scale: f32,
}

impl Layout {
    fn new(screen: Rect) -> Layout {
        let scale = (screen.width() / 1920.0)
            .min(screen.height() / 1080.0)
            .clamp(0.4, 2.0);
        Layout { screen, scale }
    }

    fn window_origin(&self, w: &UiWindow) -> Pos2 {
        let axis = |pos: f32, size: f32, virt: f32, real_min: f32, real: f32| {
            let center = pos + size / 2.0;
            if center < virt / 3.0 {
                real_min + pos * self.scale
            } else if center > virt * 2.0 / 3.0 {
                real_min + real - (virt - pos) * self.scale
            } else {
                real_min + real / 2.0 + (pos - virt / 2.0) * self.scale
            }
        };
        Pos2::new(
            axis(
                w.pos[0],
                w.size[0],
                1920.0,
                self.screen.left(),
                self.screen.width(),
            ),
            axis(
                w.pos[1],
                w.size[1],
                1080.0,
                self.screen.top(),
                self.screen.height(),
            ),
        )
    }

    fn rect(&self, origin: Pos2, pos: [f32; 2], size: [f32; 2]) -> Rect {
        Rect::from_min_size(
            origin + Vec2::new(pos[0], pos[1]) * self.scale,
            Vec2::new(size[0], size[1]) * self.scale,
        )
    }
}

fn shadow_text(p: &egui::Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    let font = FontId::proportional(size);
    p.text(
        pos + Vec2::splat(1.0),
        align,
        text,
        font.clone(),
        Color32::from_black_alpha(220),
    );
    p.text(pos, align, text, font, color);
}

/// Place occupée par le HUD à l'écran, pour que les autres panneaux l'évitent : largeur de la
/// minicarte (coin haut droit) et hauteur de la barre du bas (jauges comprises), en points egui.
pub fn reserved(ctx: &egui::Context) -> (f32, f32) {
    let s = Layout::new(ctx.screen_rect()).scale;
    // MiniMap : 172 px de large ; UI_CharInfo : de la jauge PV/PM (y 202) au bas (291).
    (172.0 * s + 8.0, (291.0 - 202.0) * s + 6.0)
}

/// Icônes des objets : colonne `TEXTURE` de `ItemTemplateAll.ini`, nom `l%07x` (hexadécimal,
/// vérifié sur le pack `texture/item` : au-delà de 255, seuls les noms hexadécimaux existent).
fn load_item_icons(assets: &Assets) -> HashMap<i32, String> {
    let Ok(t) = assets.table("ini", "ItemTemplateAll.ini") else {
        return HashMap::new();
    };
    let Some(header) = t.section("") else {
        return HashMap::new();
    };
    let Some(col) = header
        .entries
        .iter()
        .find(|(_, v)| v.eq_ignore_ascii_case("TEXTURE"))
        .map(|(k, _)| k.clone())
    else {
        return HashMap::new();
    };
    t.sections
        .iter()
        .filter_map(|s| {
            let tid: i32 = s.name.parse().ok()?;
            let n: u32 = s.get(&col)?.trim().parse().ok()?;
            (n > 0).then(|| (tid, format!("l{n:07x}")))
        })
        .collect()
}

/// Texture egui d'une image de pack chargée par nom de texture (`texture/item`, `texture/skill`…).
fn texture_icon(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    name: &str,
) -> Option<egui::TextureId> {
    let key = format!("icon:{name}");
    if !hud.images.contains_key(&key) {
        let loaded = assets.texture(name).ok().map(|img| UiImage {
            orig: [img.width as f32, img.height as f32],
            tex: upload(ctx, &format!("hud-{key}"), &img),
        });
        hud.images.insert(key.clone(), loaded);
    }
    hud.images
        .get(&key)
        .and_then(|i| i.as_ref())
        .map(|i| i.tex.id())
}

/// Argent avec séparateur de milliers.
pub(crate) fn format_money(v: i64) -> String {
    let digits = v.abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Fenêtre d'inventaire d'Archonia (`UI_Inventory`) : fond `INV_BASE.png`, cinq sacs en onglets
/// (`INV_TAB` / `INV_TAB_SEL`), grille de 5 × 5 cases de 50 px (`Grid=0:0:2:2:50:50`), argent,
/// boutons. Les objets viennent des paquets AgpmItem (statut 2, case `[sac, ligne, colonne]`).
/// Fenêtre déplaçable ; les boutons autres que la fermeture et les onglets ne font encore rien.
/// Les objets se déplacent à la souris, s'utilisent ou se portent au clic droit (voir `hud_items.rs`).
pub fn draw_inventory(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    open: &mut bool,
    items: &[(ItemRecord, String)],
    out: &mut Vec<ItemUi>,
) {
    if !*open {
        return;
    }
    let Some(w) = hud
        .windows(assets)
        .iter()
        .find(|w| w.name == "UI_Inventory")
        .cloned()
    else {
        return;
    };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let money = hud.stats.money;
    egui::Area::new(egui::Id::new("archonia-inventory"))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let origin = rect.min;
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            let font = (11.0 * s).max(9.0);
            for c in &w.controls {
                let r = layout.rect(origin, c.pos, c.size);
                let label = c
                    .display
                    .as_deref()
                    .and_then(|d| d.strip_prefix("1:"))
                    .unwrap_or("");
                if let Some(layer) = c
                    .name
                    .strip_prefix("CONTROL_Inven_Layer")
                    .and_then(|n| n.parse::<i16>().ok())
                {
                    // Onglet de sac : texture 2 si choisi, sinon 1.
                    let selected = layer == hud.inventory_layer;
                    let tex = c
                        .textures
                        .iter()
                        .find(|(i, _)| *i == if selected { 2 } else { 1 })
                        .or(c.textures.first())
                        .map(|(_, n)| n.clone());
                    if let Some(tex) = tex {
                        control_image(ctx, hud, assets, &painter, r, c.size, &tex, 1.0);
                    }
                    let color = if selected {
                        Color32::from_rgb(255, 230, 160)
                    } else {
                        Color32::from_rgb(190, 190, 190)
                    };
                    shadow_text(
                        &painter,
                        r.center(),
                        Align2::CENTER_CENTER,
                        label,
                        font,
                        color,
                    );
                    if ui
                        .interact(r, egui::Id::new(("inv-tab", layer)), egui::Sense::click())
                        .clicked()
                    {
                        hud.inventory_layer = layer;
                    }
                    continue;
                }
                match c.kind {
                    1 => {
                        if c.display
                            .as_deref()
                            .is_some_and(|d| d.ends_with("InventoryMoney"))
                        {
                            let text = format_money(money.unwrap_or(0));
                            shadow_text(
                                &painter,
                                r.right_center(),
                                Align2::RIGHT_CENTER,
                                &text,
                                font,
                                Color32::from_rgb(255, 220, 120),
                            );
                            // `INV_OpenDropMoney` : débranché dans Archonia (`Inventory_OpenDropMoneyUI` vide, aucune
                            // action sur `InventoryMoney`) ; ArchRust l'ouvre au clic sur l'or, comme le jeu d'origine.
                            if ui.interact(r, egui::Id::new("inv-money"), egui::Sense::click()).clicked() {
                                hud.drop_money_request = true;
                            }
                        } else if !label.is_empty() {
                            shadow_text(
                                &painter,
                                r.left_center(),
                                Align2::LEFT_CENTER,
                                label,
                                font * 1.1,
                                Color32::from_rgb(235, 225, 200),
                            );
                        }
                    }
                    3 => {
                        let resp = ui.interact(
                            r,
                            egui::Id::new(("inv-btn", c.name.as_str())),
                            egui::Sense::click(),
                        );
                        if let Some(tex) = button_texture(c).map(str::to_string) {
                            control_image(ctx, hud, assets, &painter, r, c.size, &tex, 1.0);
                        }
                        if !label.is_empty() {
                            shadow_text(
                                &painter,
                                r.center(),
                                Align2::CENTER_CENTER,
                                label,
                                font * 0.95,
                                Color32::WHITE,
                            );
                        }
                        if resp.hovered() {
                            painter.rect_filled(r, 2.0, Color32::from_white_alpha(20));
                        }
                        if c.name == "CONTROL_Inventory_Close_Button" {
                            if resp.clicked() {
                                *open = false;
                            }
                        } else if c.name == "CONTROL_AuctionButton" {
                            // « Sell » : boîte de ventes de la maison des ventes (`AC_SalesBox2Open`).
                            if resp.clicked() {
                                out.push(ItemUi::SalesBox);
                            }
                        } else if c.name == "CONTROL_LoneyButton" {
                            // « Chantra Bag » : inventaire Cash (`CInven_UIOpen`, voir `app_petbag.rs`).
                            if resp.clicked() {
                                out.push(ItemUi::CashInventory);
                            }
                        } else {
                            resp.on_hover_text("Pas encore disponible dans ArchRust");
                        }
                    }
                    8 if c.name == "CONTROL_Inventory_Grid" => {
                        items_ui::bag_grid(ctx, ui, hud, assets, &painter, &layout, origin, c, items, font, out);
                    }
                    _ => {}
                }
            }
        });
}

/// Dessine le HUD en jeu.
pub fn draw(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, scene: &HudScene) {
    hud.expire_buffs();
    let layout = Layout::new(ctx.screen_rect());
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("archonia-hud"),
    ));
    let windows: Vec<UiWindow> = hud.windows(assets).to_vec();
    for w in &windows {
        match w.name.as_str() {
            "UI_CharInfo" => draw_charinfo(ctx, hud, assets, &painter, &layout, w),
            "MiniMap" => draw_minimap(ctx, hud, assets, &painter, &layout, w, scene),
            "UI_TargetInfo" => {
                if let Some(t) = &scene.target {
                    draw_target(ctx, hud, assets, &painter, &layout, w, t);
                }
            }
            _ => {}
        }
    }
    draw_buffs(ctx, hud, assets, &painter, &layout, &windows);
    draw_system_messages(hud, &painter, &layout);
}

/// Barre de saisie de la discussion placée comme dans Archonia.
pub struct ChatBar {
    /// Fenêtre `Chat_Input_Normal` à l'écran.
    pub window: Rect,
    /// Contrôle `Input_Message` (type 16, ChattingEdit), où se pose la zone de saisie.
    pub input: Rect,
    /// Champ du destinataire (`Input_ID`) de la barre de chuchotement.
    pub id: Option<Rect>,
    pub scale: f32,
    /// Onglets de la discussion d'Archonia (`MainChat_Tab_a/b/c.png`, FUN_1407a30a0) : texture et
    /// taille d'origine, repos / survol / choisi.
    pub tabs: [Option<(egui::TextureId, [f32; 2])>; 3],
}

/// Dessine `Chat_Input_Normal` (395 × 25, en bas à gauche en 1920 × 1080) et rend la place de sa
/// zone de saisie. Les lignes de discussion d'Archonia ne sont pas une fenêtre du fichier
/// d'interface : le client les dessine au-dessus de cette barre (voir `ui::chat_panel`).
pub fn chat_bar(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, private: bool) -> Option<ChatBar> {
    let normal = hud
        .windows(assets)
        .iter()
        .find(|w| w.name == "Chat_Input_Normal")
        .cloned()?;
    // Barre de chuchotement `Chat_Input_Private` (382 × 24, `Chat_Input_Prv.png`) : même
    // `UIPos` que la barre normale dans le fichier d'interface, posée au même endroit.
    let w = if private {
        hud.windows(assets).iter().find(|w| w.name == "Chat_Input_Private").cloned().unwrap_or_else(|| normal.clone())
    } else {
        normal.clone()
    };
    let layout = Layout::new(ctx.screen_rect());
    let origin = layout.window_origin(&normal);
    let window = layout.rect(origin, [0.0, 0.0], w.size);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("archonia-chat-bar"),
    ));
    if let Some((_, tex)) = w.textures.first() {
        control_image(ctx, hud, assets, &painter, window, w.size, tex, 1.0);
    }
    let input = w
        .control("Input_Message")
        .map(|c| layout.rect(origin, c.pos, c.size))
        .unwrap_or(window);
    let tabs = ["MainChat_Tab_a.png", "MainChat_Tab_b.png", "MainChat_Tab_c.png"].map(|t| hud.image(ctx, assets, t));
    let id = (w.name == "Chat_Input_Private").then(|| w.control("Input_ID").map(|c| layout.rect(origin, c.pos, c.size))).flatten();
    Some(ChatBar {
        window,
        input,
        id,
        scale: layout.scale,
        tabs,
    })
}

/// Fiche de la cible (`UI_TargetInfo`, 339 × 264 en haut au centre) : icône d'attribut, cadre de
/// jauge `TargetInfo_HP_Bar_Off`, icône `TargetInfo_On`, jauge `TargetInfo_HP_Bar_On` et nom
/// (`CONTROL_TargetName`). Le maximum de PV est le plus haut PV vu pour ce CID (le serveur ne
/// l'envoie pas) ; sans PV connus, la jauge est pleine.
fn draw_target(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, w: &UiWindow, t: &HudTarget) {
    let origin = layout.window_origin(w);
    let s = layout.scale;
    let ratio = match t.hp {
        Some(hp) => {
            let max = hud.max_hp_seen.entry(t.cid).or_insert(hp.max(1));
            *max = (*max).max(hp).max(1);
            (hp.max(0) as f32 / *max as f32).clamp(0.0, 1.0)
        }
        None => 1.0,
    };
    for c in &w.controls {
        let rect = layout.rect(origin, c.pos, c.size);
        match (c.kind, c.name.as_str()) {
            (2, "CONTROL_TargetName") => {
                shadow_text(painter, rect.center(), Align2::CENTER_CENTER, &t.name, (11.0 * s).max(9.0), Color32::from_rgb(255, 235, 170));
            }
            (9, _) => {
                if let Some((_, tex)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, rect, c.size, &tex.clone(), ratio);
                }
            }
            (1, "CONTROL_10624") => {
                // Icône de droite : 2 = TargetInfo_On (cible présente).
                let tex = c.textures.iter().find(|(i, _)| *i == 2).or(c.textures.first()).map(|(_, n)| n.clone());
                if let Some(tex) = tex {
                    control_image(ctx, hud, assets, painter, rect, c.size, &tex, 1.0);
                }
            }
            (1, _) => {
                if let Some((_, tex)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, rect, c.size, &tex.clone(), 1.0);
                }
            }
            _ => {}
        }
    }
    if t.hp.is_some() {
        if let Some(bar) = w.controls.iter().find(|c| c.kind == 9) {
            let r = layout.rect(origin, bar.pos, bar.size);
            shadow_text(painter, r.center() + Vec2::new(0.0, 10.0 * s), Align2::CENTER_TOP, &format!("{} %", (ratio * 100.0).round() as i32), (10.0 * s).max(9.0), Color32::WHITE);
        }
    }
}

/// Messages système en orange, alignés à droite au-dessus de la barre du bas, comme les
/// « [Système] … » de la capture de référence d'Archonia (dessinés par le code du client, pas par
/// une fenêtre du fichier d'interface). Chacun reste 10 s puis s'efface en 1 s.
fn draw_system_messages(hud: &mut HudState, painter: &egui::Painter, layout: &Layout) {
    let now = Instant::now();
    hud.system_messages.retain(|(_, t)| now.duration_since(*t) < Duration::from_secs(11));
    if hud.system_messages.is_empty() {
        return;
    }
    let s = layout.scale;
    let size = (13.0 * s).max(10.0);
    let right = layout.screen.right() - 14.0 * s;
    let bottom = layout.screen.top() + 800.0 / 1080.0 * layout.screen.height();
    for (i, (text, at)) in hud.system_messages.iter().rev().enumerate() {
        let age = now.duration_since(*at).as_secs_f32();
        let alpha = if age > 10.0 { (11.0 - age).clamp(0.0, 1.0) } else { 1.0 };
        let y = bottom - i as f32 * (size + 4.0);
        let color = Color32::from_rgba_unmultiplied(255, 170, 40, (alpha * 255.0) as u8);
        let font = FontId::proportional(size);
        painter.text(Pos2::new(right + 1.0, y + 1.0), Align2::RIGHT_BOTTOM, format!("[Système] {text}"), font.clone(), Color32::from_black_alpha((alpha * 200.0) as u8));
        painter.text(Pos2::new(right, y), Align2::RIGHT_BOTTOM, format!("[Système] {text}"), font, color);
    }
}

/// Image d'un contrôle avec UV au pixel près : `size` (px d'origine du contrôle) sur l'image `orig`.
fn control_image(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    painter: &egui::Painter,
    rect: Rect,
    size: [f32; 2],
    name: &str,
    fraction: f32,
) {
    let Some((tex, orig)) = hud.image(ctx, assets, name) else {
        return;
    };
    let f = fraction.clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    let uv_max = Pos2::new(
        (size[0] / orig[0]).min(1.0) * f,
        (size[1] / orig[1]).min(1.0),
    );
    let draw = Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, rect.height()));
    painter.image(
        tex,
        draw,
        Rect::from_min_max(Pos2::ZERO, uv_max),
        Color32::WHITE,
    );
}

/// Fond d'une fenêtre : l'image d'origine posée au pixel près depuis le coin haut gauche, jamais
/// étirée. Certaines fenêtres sont un peu plus grandes que leur fond (`Character_Status` 570 × 500
/// pour `CS_Back.png` 553 × 487, `MailBox` 350 pour 346, `UI_Auction` 1197 pour 1191) : étiré à la
/// taille de la fenêtre, le fond décalait ses cases et sa croix de fermeture par rapport aux
/// contrôles (cadre de survol décalé, croix « hors de son emplacement »).
fn window_image(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, rect: Rect, size: [f32; 2], name: &str) {
    let Some((tex, orig)) = hud.image(ctx, assets, name) else {
        return;
    };
    let part = [(orig[0] / size[0].max(1.0)).min(1.0), (orig[1] / size[1].max(1.0)).min(1.0)];
    let uv_max = Pos2::new((size[0] / orig[0]).min(1.0), (size[1] / orig[1]).min(1.0));
    let draw = Rect::from_min_size(rect.min, Vec2::new(rect.width() * part[0], rect.height() * part[1]));
    painter.image(tex, draw, Rect::from_min_max(Pos2::ZERO, uv_max), Color32::WHITE);
}

/// Texture « au repos » d'un bouton : celle en `…a.png`, sinon la première.
fn button_texture(c: &UiControl) -> Option<&str> {
    c.textures
        .iter()
        .find(|(_, n)| n.to_ascii_lowercase().ends_with("a.png"))
        .or(c.textures.first())
        .map(|(_, n)| n.as_str())
}

fn draw_charinfo(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    painter: &egui::Painter,
    layout: &Layout,
    w: &UiWindow,
) {
    let origin = layout.window_origin(w);
    let s = layout.scale;
    let st = hud.stats.clone();
    let ratio = |cur: Option<i32>, max: Option<i32>| match (cur, max) {
        (Some(c), Some(m)) if m > 0 => (c as f32 / m as f32).clamp(0.0, 1.0),
        (Some(_), None) => 1.0,
        _ => 0.0,
    };
    let exp_ratio = hud.exp_ratio(assets);
    for c in &w.controls {
        let rect = layout.rect(origin, c.pos, c.size);
        match c.kind {
            // Images et textes.
            1 => {
                if let Some((_, tex)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, rect, c.size, tex, 1.0);
                }
                let Some(display) = &c.display else { continue };
                let size = (rect.height() * 0.95).clamp(9.0, 22.0);
                if display.ends_with("HP_Bar") {
                    let text = format!(
                        "{} / {}",
                        st.hp.unwrap_or(0),
                        st.max_hp.or(st.hp).unwrap_or(0)
                    );
                    shadow_text(
                        painter,
                        rect.center(),
                        Align2::CENTER_CENTER,
                        &text,
                        size,
                        Color32::WHITE,
                    );
                } else if display.ends_with("MP_Bar") {
                    let text = format!(
                        "{} / {}",
                        st.mp.unwrap_or(0),
                        st.max_mp.or(st.mp).unwrap_or(0)
                    );
                    shadow_text(
                        painter,
                        rect.center(),
                        Align2::CENTER_CENTER,
                        &text,
                        size,
                        Color32::WHITE,
                    );
                } else if display.ends_with("현재_레벨") {
                    let text = st.level.map(|l| l.to_string()).unwrap_or_default();
                    shadow_text(
                        painter,
                        rect.center(),
                        Align2::CENTER_CENTER,
                        &text,
                        size * 1.1,
                        Color32::from_rgb(255, 230, 160),
                    );
                } else if display.ends_with("백분율") {
                    let text = exp_ratio
                        .map(|r| format!("EXP {:.2}%", r * 100.0))
                        .unwrap_or_else(|| "EXP".into());
                    shadow_text(
                        painter,
                        rect.left_center(),
                        Align2::LEFT_CENTER,
                        &text,
                        size.max(9.0 * s),
                        Color32::from_rgb(220, 230, 200),
                    );
                } else if let Some(label) = display.strip_prefix("1:") {
                    // Libellés fixes : touches des raccourcis et des potions.
                    shadow_text(
                        painter,
                        rect.left_top() + Vec2::new(2.0, 1.0) * s,
                        Align2::LEFT_TOP,
                        label,
                        (11.0 * s).max(9.0),
                        Color32::from_rgb(230, 230, 230),
                    );
                }
            }
            // Boutons : texture au repos.
            3 => {
                if let Some(tex) = button_texture(c) {
                    control_image(
                        ctx,
                        hud,
                        assets,
                        painter,
                        rect,
                        c.size,
                        &tex.to_string(),
                        1.0,
                    );
                }
            }
            // Jauges : PV, PM, expérience.
            9 => {
                let Some((_, tex)) = c.textures.first() else {
                    continue;
                };
                let bar = c.bar.as_deref().unwrap_or("");
                let f = if bar.ends_with(" HP") {
                    ratio(st.hp, st.max_hp)
                } else if bar.ends_with(" MP") {
                    ratio(st.mp, st.max_mp)
                } else if bar.contains("경험치") {
                    exp_ratio.unwrap_or(0.0)
                } else {
                    0.0
                };
                control_image(ctx, hud, assets, painter, rect, c.size, &tex.clone(), f);
            }
            _ => {}
        }
    }
}

fn draw_minimap(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    painter: &egui::Painter,
    layout: &Layout,
    w: &UiWindow,
    scene: &HudScene,
) {
    let origin = layout.window_origin(w);
    let Some(view) = w.controls.iter().find(|c| c.kind == 16) else {
        return;
    };
    let view_rect = layout.rect(origin, view.pos, view.size);
    // Fond de la fenêtre d'abord (son disque intérieur est opaque), puis la carte et les contrôles,
    // dans l'ordre d'AgcmUIManager2. Carte masquée : `Texture=2` (`Main_Minimap_Base_hide.png`),
    // ni carte ni repères (FUN_1407015a0, `Minimap_Map_OFF`).
    let hidden = hud.minimap.hidden;
    let base = if hidden { w.textures.iter().find(|(i, _)| *i == 2).or(w.textures.first()) } else { w.textures.first() };
    if let Some((_, tex)) = base.cloned() {
        control_image(
            ctx,
            hud,
            assets,
            painter,
            layout.rect(origin, [0.0, 0.0], w.size),
            w.size,
            &tex,
            1.0,
        );
    }
    if let Some(player) = scene.player.filter(|_| !hidden) {
        update_minimap(ctx, hud, assets, player, view.size[0] as usize);
        if let Some(tex) = &hud.minimap.tex {
            painter.image(
                tex.id(),
                view_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // Repères (FUN_1408d5490) : posés dans le raster à 0,0037695 px par unité, à leur taille, puis
        // le raster est réduit au contrôle avec la carte ; gardés dans le disque de la carte.
        let per_px = view_rect.width() / minimap_shown(hud.minimap.scale);
        for icon in &scene.minimap_icons {
            let d = Vec2::new(icon.pos.x - player.x, icon.pos.z - player.z) * MINIMAP_PX_PER_UNIT * per_px;
            let (image, side) = icon.image();
            let half = side * per_px / 2.0;
            if d.length() + half > view_rect.width() / 2.0 {
                continue;
            }
            if let Some((tex, _)) = hud.image(ctx, assets, image) {
                painter.image(tex, Rect::from_center_size(view_rect.center() + d, Vec2::splat(half * 2.0)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
    for c in w.controls.iter().filter(|c| c.kind == 3) {
        if let Some(tex) = button_texture(c).map(str::to_string) {
            control_image(
                ctx,
                hud,
                assets,
                painter,
                layout.rect(origin, c.pos, c.size),
                c.size,
                &tex,
                1.0,
            );
        }
    }
    minimap_buttons(ctx, hud, assets, layout, w);
    if hidden {
        return;
    }
    // Cône de la caméra puis repère du joueur, au centre.
    let center = view_rect.center();
    let f = Vec2::new(scene.camera_forward.x, scene.camera_forward.z);
    if f.length() > 1e-3 {
        if let Some((tex, orig)) = hud.image(ctx, assets, "Main_Minimap_Cursor.png") {
            let f = f.normalized();
            let right = Vec2::new(-f.y, f.x);
            let (wd, ht) = (orig[0] * layout.scale * 0.8, orig[1] * layout.scale * 0.8);
            // L'image pointe vers le haut ; son pied est le centre.
            let corners = [
                center + f * ht - right * wd / 2.0,
                center + f * ht + right * wd / 2.0,
                center + right * wd / 2.0,
                center - right * wd / 2.0,
            ];
            let uvs = [
                Pos2::new(0.0, 0.0),
                Pos2::new(1.0, 0.0),
                Pos2::new(1.0, 1.0),
                Pos2::new(0.0, 1.0),
            ];
            let mut mesh = egui::epaint::Mesh::with_texture(tex);
            for (p, uv) in corners.iter().zip(uvs) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: *p,
                    uv,
                    color: Color32::from_white_alpha(200),
                });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    if let Some((tex, orig)) = hud.image(ctx, assets, "Main_Minimap_Pos_PC.png") {
        let r = Rect::from_center_size(center, Vec2::new(orig[0], orig[1]) * layout.scale);
        painter.image(
            tex,
            r,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

/// Boutons de la minicarte reliés par leur rappel (`Action=3:`) : `Minimap_Plus_Click` /
/// `Minimap_Minus_Click` (zoom), `Minimap_Map_Toggle` (masquer), `Minimap_CameraView_Click` et
/// `WorldMap_Toggle` (demandes au moteur, [`MinimapUi`]). Textures a / b / c : repos, survol, appui.
fn minimap_buttons(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, layout: &Layout, w: &UiWindow) {
    let origin = layout.window_origin(w);
    for c in w.controls.iter().filter(|c| c.kind == 3) {
        let Some(action) = c.action.as_deref() else { continue };
        if !matches!(action, "Minimap_Plus_Click" | "Minimap_Minus_Click" | "Minimap_Map_Toggle" | "Minimap_CameraView_Click" | "WorldMap_Toggle") {
            continue;
        }
        let r = layout.rect(origin, c.pos, c.size);
        let mut clicked = false;
        egui::Area::new(egui::Id::new(("archonia-minimap-button", c.name.as_str()))).fixed_pos(r.min).order(egui::Order::Middle).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
            let suffix = if resp.is_pointer_button_down_on() { "c.png" } else if resp.hovered() { "b.png" } else { "a.png" };
            if let Some((_, t)) = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).cloned() {
                control_image(ctx, hud, assets, ui.painter(), rect, c.size, &t, 1.0);
            }
            clicked = resp.clicked();
        });
        if !clicked {
            continue;
        }
        let m = &mut hud.minimap;
        match action {
            "Minimap_Plus_Click" => m.scale = minimap_zoom(if m.scale > 0.0 { m.scale } else { 1.0 }, true),
            "Minimap_Minus_Click" => m.scale = minimap_zoom(if m.scale > 0.0 { m.scale } else { 1.0 }, false),
            "Minimap_Map_Toggle" => m.hidden = !m.hidden,
            "Minimap_CameraView_Click" => hud.minimap_requests.push(MinimapUi::CameraView),
            _ => hud.minimap_requests.push(MinimapUi::WorldMap),
        }
    }
}

/// Point du monde (x, z) sous un clic gauche sur la carte de la minicarte, pour la téléportation
/// des MJ (voir `app_worldmap.rs`) : le raster de `MINIMAP_RASTER` px centré sur le joueur couvre
/// le contrôle de type 16, comme dans `update_minimap`.
pub fn minimap_click(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, player: Vec3) -> Option<[f32; 2]> {
    let w = hud.windows(assets).iter().find(|w| w.name == "MiniMap").cloned()?;
    let view = w.controls.iter().find(|c| c.kind == 16)?;
    let layout = Layout::new(ctx.screen_rect());
    let r = layout.rect(layout.window_origin(&w), view.pos, view.size);
    let mut hit = None;
    egui::Area::new(egui::Id::new("archrust-minimap-gm")).fixed_pos(r.min).order(egui::Order::Middle).show(ctx, |ui| {
        let (_, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
        if let Some(p) = resp.clicked().then(|| resp.interact_pointer_pos()).flatten() {
            let per = minimap_shown(hud.minimap.scale) / MINIMAP_PX_PER_UNIT / r.width().max(1.0);
            let d = p - r.center();
            hit = Some([player.x + d.x * per, player.z + d.y * per]);
        }
    });
    hit
}

/// Quart de tuile, nom de fichier et coordonnées locales (px) d'un point du monde.
fn minimap_sample(x: f32, z: f32) -> (String, f32, f32) {
    let sx = (x / 6400.0).floor() as i32 + 400;
    let sz = (z / 6400.0).floor() as i32 + 400;
    let tile = (sx.div_euclid(16)) * 100 + sz.div_euclid(16);
    let x0 = ((tile / 100 - 25) * 16) as f32 * 6400.0;
    let z0 = ((tile % 100 - 25) * 16) as f32 * 6400.0;
    let qx = ((x - x0) >= 51_200.0) as i32;
    let qz = ((z - z0) >= 51_200.0) as i32;
    let q = qx + 2 * qz;
    let name = format!("map{tile:04}{}.png", (b'a' + q as u8) as char);
    let lx = (x - x0 - qx as f32 * 51_200.0) * MINIMAP_PX_PER_UNIT;
    let lz = (z - z0 - qz as f32 * 51_200.0) * MINIMAP_PX_PER_UNIT;
    (name, lx, lz)
}

fn update_minimap(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    player: Vec3,
    side: usize,
) {
    // Une image par pas de ~200 unités : inutile de recomposer à chaque image.
    let shown = minimap_shown(hud.minimap.scale);
    let key = ((player.x / 200.0) as i32, (player.z / 200.0) as i32, (shown * 10.0) as i32);
    if hud.minimap.drawn_at == Some(key) && hud.minimap.tex.is_some() {
        return;
    }
    hud.minimap.drawn_at = Some(key);
    let side = side.clamp(32, 512);
    if hud.minimap.mask.is_none() {
        hud.minimap.mask = Some(assets.image("texture/ui", "main_minimap_alpha.png").ok());
    }
    let units_per_px = shown / MINIMAP_PX_PER_UNIT / side as f32;
    let mut data = vec![0u8; side * side * 4];
    for j in 0..side {
        for i in 0..side {
            let x = player.x + (i as f32 - side as f32 / 2.0) * units_per_px;
            let z = player.z + (j as f32 - side as f32 / 2.0) * units_per_px;
            let (name, lx, lz) = minimap_sample(x, z);
            let tile = hud
                .minimap
                .tiles
                .entry(name.clone())
                .or_insert_with(|| assets.image("texture/minimap", &name).ok().map(Arc::new))
                .clone();
            let o = (j * side + i) * 4;
            let rgba = match tile {
                Some(t) if t.width > 0 && t.height > 0 => {
                    // Image de 193 px (ou agrandie par un patch) : coordonnées proportionnelles.
                    let px =
                        ((lx / 193.0) * t.width as f32).clamp(0.0, t.width as f32 - 1.0) as usize;
                    let pz =
                        ((lz / 193.0) * t.height as f32).clamp(0.0, t.height as f32 - 1.0) as usize;
                    let k = (pz * t.width as usize + px) * 4;
                    [t.data[k], t.data[k + 1], t.data[k + 2], 255]
                }
                _ => [12, 14, 18, 255],
            };
            data[o..o + 4].copy_from_slice(&rgba);
        }
    }
    // Masque circulaire d'Archonia (son alpha), à défaut un disque.
    let mask = hud.minimap.mask.clone().flatten();
    for j in 0..side {
        for i in 0..side {
            let a = match &mask {
                Some(m) if m.width > 0 => {
                    let mx = i * m.width as usize / side;
                    let my = j * m.height as usize / side;
                    m.data[(my * m.width as usize + mx) * 4 + 3]
                }
                _ => {
                    let d = Vec2::new(i as f32 - side as f32 / 2.0, j as f32 - side as f32 / 2.0)
                        .length();
                    if d < side as f32 / 2.0 {
                        255
                    } else {
                        0
                    }
                }
            };
            data[(j * side + i) * 4 + 3] = a;
        }
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &data);
    match &mut hud.minimap.tex {
        Some(t) => t.set(image, egui::TextureOptions::LINEAR),
        None => {
            hud.minimap.tex =
                Some(ctx.load_texture("hud-minimap", image, egui::TextureOptions::LINEAR))
        }
    }
}

fn draw_buffs(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    painter: &egui::Painter,
    layout: &Layout,
    windows: &[UiWindow],
) {
    if hud.buffs.is_empty() {
        return;
    }
    let s = layout.scale;
    // Sous la minicarte, de droite à gauche.
    let anchor = windows
        .iter()
        .find(|w| w.name == "MiniMap")
        .map(|w| {
            let o = layout.window_origin(w);
            Pos2::new(o.x + w.size[0] * s, o.y + w.size[1] * s + 6.0 * s)
        })
        .unwrap_or(Pos2::new(
            layout.screen.right() - 8.0,
            layout.screen.top() + 200.0 * s,
        ));
    let icon = 32.0 * s;
    let now = Instant::now();
    let buffs = hud.buffs.clone();
    for (n, b) in buffs.iter().enumerate() {
        let col = (n % 8) as f32;
        let row = (n / 8) as f32;
        let r = Rect::from_min_size(
            Pos2::new(
                anchor.x - (col + 1.0) * (icon + 4.0 * s),
                anchor.y + row * (icon + 16.0 * s),
            ),
            Vec2::splat(icon),
        );
        let icon_name = hud.skill_icon(assets, b.skill_tid);
        let tex = icon_name.as_deref().and_then(|name| {
            let key = format!("skill:{name}");
            if !hud.images.contains_key(&key) {
                let loaded = assets.texture(name).ok().map(|img| UiImage {
                    orig: [img.width as f32, img.height as f32],
                    tex: upload(ctx, &format!("hud-{key}"), &img),
                });
                hud.images.insert(key.clone(), loaded);
            }
            hud.images
                .get(&key)
                .and_then(|i| i.as_ref())
                .map(|i| i.tex.id())
        });
        match tex {
            Some(t) => {
                painter.image(
                    t,
                    r,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(r, 3.0, Color32::from_rgb(60, 70, 110));
            }
        }
        painter.rect_stroke(
            r,
            2.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(140, 160, 200)),
            egui::StrokeKind::Inside,
        );
        if let Some(end) = b.expires {
            let left = end.saturating_duration_since(now).as_secs();
            let text = if left >= 60 {
                format!("{}m", left / 60)
            } else {
                format!("{left}s")
            };
            shadow_text(
                painter,
                Pos2::new(r.center().x, r.bottom() + 1.0),
                Align2::CENTER_TOP,
                &text,
                (10.0 * s).max(9.0),
                Color32::WHITE,
            );
        }
    }
}

/// Fenêtre de résurrection d'Archonia (`UI_Resurrection`, 236 × 173, fond `Res_Base.png`) : titre et
/// bouton `CONTROL_6092` (« Revive in last visited town », action `CharCheckResurrectionToTown`,
/// textures `Common_Button_XLarge_A…D`). Les autres boutons (sur place, siège) demandent un objet ou
/// un événement que le serveur ne gère pas : ils ne sont pas dessinés. Choix d'ArchRust : la fenêtre
/// est centrée (ses `UIPos` visent d'autres définitions d'écran). `enabled` grise le bouton (demande
/// déjà envoyée). Rend `true` au clic sur le bouton.
pub fn draw_resurrection(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, enabled: bool) -> bool {
    draw_resurrection_with(ctx, hud, assets, enabled, None).0
}

/// Comme [`draw_resurrection`], avec le bouton « Wanted » (`CONTROL_10892`, `WC_ARRANGE_MENU`) quand `wanted`
/// est donné : actif si le serveur a nommé le tueur (`WC_ACTIVE_ARRANGE`, voir `app_wanted.rs`). Rend les clics
/// (résurrection au village, « Wanted »).
pub fn draw_resurrection_with(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, enabled: bool, wanted: Option<bool>) -> (bool, bool) {
    let window = hud.windows(assets).iter().find(|w| w.name == "UI_Resurrection").cloned();
    let wanted_button = wanted.and_then(|active| window.as_ref().and_then(|w| w.control("CONTROL_10892")).cloned().map(|c| (c, active)));
    let mut wanted_clicked = false;
    let size = window.as_ref().map_or([236.0, 173.0], |w| w.size);
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale * 1.25;
    let origin = layout.screen.center() - Vec2::new(size[0], size[1]) * s / 2.0 - Vec2::new(0.0, 80.0 * layout.scale);
    let rect_of = |pos: [f32; 2], sz: [f32; 2]| Rect::from_min_size(origin + Vec2::new(pos[0], pos[1]) * s, Vec2::new(sz[0], sz[1]) * s);
    let frame = rect_of([0.0, 0.0], size);
    let title_control = window.as_ref().and_then(|w| w.control("CONTROL_9227"));
    let title = title_control.map_or(([30.0, 5.0], [120.0, 13.0]), |c| (c.pos, c.size));
    // Textes fixes du dossier de langue (titre, bouton du village).
    let title_text = title_control.map(|c| c.fixed_label().to_string()).unwrap_or_default();
    let button = window.as_ref().and_then(|w| w.control("CONTROL_6092")).cloned();
    let (bpos, bsize) = button.as_ref().map_or(([19.0, 35.0], [189.0, 17.0]), |b| (b.pos, b.size));
    let brect = rect_of(bpos, [bsize[0], bsize[1]]);
    let mut clicked = false;
    egui::Area::new(egui::Id::new("archrust-resurrection"))
        .fixed_pos(frame.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(frame.size(), egui::Sense::hover());
            let painter = ui.painter();
            match window.as_ref().and_then(|w| w.textures.first()).map(|(_, t)| t.clone()) {
                Some(tex) if hud.image(ctx, assets, &tex).is_some() => control_image(ctx, hud, assets, painter, frame, size, &tex, 1.0),
                _ => {
                    painter.rect_filled(frame, 4.0, Color32::from_black_alpha(210));
                    painter.rect_stroke(frame, 4.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(160, 130, 70)), egui::StrokeKind::Inside);
                }
            }
            let t = rect_of(title.0, title.1);
            shadow_text(painter, t.left_center(), Align2::LEFT_CENTER, &title_text, (12.0 * s).max(10.0), Color32::from_rgb(255, 235, 170));
            let b = ui.interact(brect, egui::Id::new("archrust-resurrect-town"), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
            let texture = button.as_ref().and_then(|c| {
                let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
                if !enabled {
                    pick("d.png")
                } else if b.is_pointer_button_down_on() {
                    pick("c.png")
                } else if b.hovered() {
                    pick("b.png")
                } else {
                    pick("a.png")
                }
                .or_else(|| button_texture(c).map(str::to_string))
            });
            match texture {
                Some(tex) if hud.image(ctx, assets, &tex).is_some() => control_image(ctx, hud, assets, painter, brect, bsize, &tex, 1.0),
                _ => {
                    let fill = if b.hovered() && enabled { Color32::from_rgb(110, 80, 40) } else { Color32::from_rgb(70, 50, 30) };
                    painter.rect_filled(brect, 3.0, fill);
                }
            }
            let color = if enabled { Color32::WHITE } else { Color32::GRAY };
            let label = button.as_ref().map_or("", |c| c.fixed_label());
            shadow_text(painter, brect.center(), Align2::CENTER_CENTER, label, (11.0 * s).max(9.0), color);
            clicked = enabled && b.clicked();
            if let Some((c, active)) = &wanted_button {
                let r = rect_of(c.pos, c.size);
                let painter = painter.clone();
                wanted_clicked = npc_ui::button(ctx, ui, hud, assets, &painter, r, c, c.fixed_label(), (11.0 * s).max(9.0), *active, "archrust-resurrect-wanted").clicked();
            }
            let _ = response;
        });
    (clicked, wanted_clicked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_merge_keeps_known_fields() {
        let mut h = HudState::default();
        h.merge_stats(&CharacterStats {
            level: Some(3),
            hp: Some(100),
            max_hp: Some(136),
            ..Default::default()
        });
        h.merge_stats(&CharacterStats {
            hp: Some(90),
            ..Default::default()
        });
        assert_eq!(
            (h.stats.level, h.stats.hp, h.stats.max_hp),
            (Some(3), Some(90), Some(136))
        );
    }

    #[test]
    fn buffs_replace_and_remove() {
        let mut h = HudState::default();
        h.add_buff(10, 5000);
        h.add_buff(10, 9000);
        h.add_buff(11, 0);
        assert_eq!(h.buffs.len(), 2);
        h.remove_buff(10);
        assert_eq!(
            h.buffs.iter().map(|b| b.skill_tid).collect::<Vec<_>>(),
            vec![11]
        );
    }

    #[test]
    fn money_has_thousand_separators() {
        assert_eq!(format_money(0), "0");
        assert_eq!(format_money(1234567), "1 234 567");
        assert_eq!(format_money(-950), "-950");
    }

    #[test]
    fn minimap_quadrants_follow_archonia() {
        // Origine de la tuile 1719 : x0 = (17-25)*16*6400, z0 = (19-25)*16*6400.
        let x0 = -8.0 * 16.0 * 6400.0;
        let z0 = -6.0 * 16.0 * 6400.0;
        assert_eq!(minimap_sample(x0 + 10.0, z0 + 10.0).0, "map1719a.png");
        assert_eq!(minimap_sample(x0 + 60_000.0, z0 + 10.0).0, "map1719b.png");
        assert_eq!(minimap_sample(x0 + 10.0, z0 + 60_000.0).0, "map1719c.png");
        let (_, lx, lz) = minimap_sample(x0 + 51_200.0 + 25_600.0, z0 + 25_600.0);
        assert!(
            (lx - 96.5).abs() < 0.5 && (lz - 96.5).abs() < 0.5,
            "{lx} {lz}"
        );
    }

    #[test]
    fn window_controls_are_placed_relative_to_parents() {
        let ui = IniTable::parse(
            "[W]\nPos=0,519\nSize=1024,291\nUIPos=1:448,789\nCtrlType=1:.\nCtrlName=BASE\nPos=27,228\nSize=970,63\n\
             CtrlParent=BASE\nCtrlType=9:.\nCtrlName=BAR\nPos=26,55\nSize=918,6\nTexture=1:Bar.png\nBar=1:1:0:0:x y Max Cur\n",
        );
        let w = UiWindow::parse(&ui, None, "W").unwrap();
        assert_eq!(w.pos, [448.0, 789.0]);
        let bar = w.control("BAR").unwrap();
        assert_eq!(
            (bar.kind, bar.pos, bar.textures[0].1.as_str()),
            (9, [53.0, 283.0], "Bar.png")
        );
    }
}
