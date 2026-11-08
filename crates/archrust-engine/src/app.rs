//! Boucle principale : fenêtre winit, rendu, interface, AddOns et réseau.

use crate::animation::{self, Animator};
use crate::assets::Assets;
use crate::audio::Audio;
use crate::camera::OrbitCamera;
use crate::login_settings::{self, LoginSettings, SceneCamera};
use crate::net::{Credentials, LoginCommand, NetEvent, NetLink};
use crate::objects::WorldObjects;
use crate::render::{DrawItem, Mesh, Renderer};
use crate::scene::{
    box_mesh, entity_meshes, entity_meshes_with_equipment, ground_mesh, item_name_for_tid,
    model_meshes, turn_towards, Entity, Head,
};
use crate::options::ClientOptions;
use crate::ui::{self, UiActions, UiState};
use crate::world::Terrain;
use crate::zones::{GameClock, ZoneAudio, Zones};
use anyhow::Result;
use archrust_addons::{AddonManager, HostCommand};
use archrust_netclient::{Command, WorldEvent};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[path = "app_addons.rs"]
mod addon_glue;

#[path = "app_animator_retention.rs"]
mod animator_retention;

#[path = "app_party.rs"]
mod party_glue;

#[path = "app_players.rs"]
mod players_glue;

#[path = "app_combat.rs"]
mod combat_glue;

#[path = "app_items.rs"]
mod items_glue;

#[path = "app_npc.rs"]
pub(crate) mod npc_glue;

#[path = "app_skills.rs"]
mod skills_glue;

#[path = "app_mastery.rs"]
mod mastery_glue;

#[path = "app_portal.rs"]
pub(crate) mod portal_glue;
#[path = "app_reconnect.rs"]
mod reconnect_glue;

#[path = "app_guild.rs"]
mod guild_glue;

#[path = "app_pvp.rs"]
mod pvp_glue;

#[path = "app_boards.rs"]
mod boards_glue;

#[path = "app_convert.rs"]
mod convert_glue;

#[path = "app_refinery.rs"]
mod refinery_glue;
#[path = "app_gacha.rs"]
mod gacha_glue;

#[path = "app_product.rs"]
mod product_glue;

#[path = "app_options.rs"]
mod options_glue;

#[path = "app_buddy.rs"]
mod buddy_glue;

#[path = "app_mail.rs"]
mod mail_glue;

#[path = "app_trade.rs"]
mod trade_glue;

#[path = "app_repair.rs"]
mod repair_glue;

#[path = "app_ride.rs"]
mod ride_glue;

#[path = "app_auction.rs"]
mod auction_glue;

#[path = "app_itemset.rs"]
mod itemset_glue;

#[path = "app_nature.rs"]
mod nature_glue;

#[path = "app_chat.rs"]
mod chat_glue;

#[path = "app_customize.rs"]
mod customize_glue;

#[path = "app_wanted.rs"]
mod wanted_glue;

#[path = "app_summons.rs"]
mod summons_glue;

#[path = "app_shrine.rs"]
mod shrine_glue;

#[path = "app_boss.rs"]
mod boss_glue;

#[path = "app_siege.rs"]
mod siege_glue;

#[path = "app_archlord.rs"]
mod archlord_glue;

#[path = "app_tax.rs"]
mod tax_glue;

#[path = "app_morph.rs"]
mod morph_glue;

#[path = "app_assets.rs"]
mod assets_glue;

#[path = "app_cursor.rs"]
mod cursor_glue;

#[path = "app_worldmap.rs"]
mod worldmap_glue;

#[path = "app_effects.rs"]
mod effects_glue;

#[path = "app_stones.rs"]
mod stones_glue;

#[path = "app_necklace.rs"]
mod necklace_glue;

#[path = "app_consumables.rs"]
mod consumables_glue;

#[path = "app_social.rs"]
mod social_glue;

#[path = "app_sounds.rs"]
mod sounds_glue;

#[path = "app_hotkey.rs"]
mod hotkey_glue;

#[path = "app_petbag.rs"]
mod petbag_glue;

#[path = "app_help.rs"]
mod help_glue;

#[path = "app_titles.rs"]
mod titles_glue;

#[path = "app_return_login.rs"]
mod return_login_glue;

#[path = "app_camera.rs"]
mod camera_glue;

#[path = "app_decals.rs"]
mod decals_glue;

#[path = "app_groundcast.rs"]
mod groundcast_glue;

#[path = "app_chattabs.rs"]
pub(crate) mod chattabs_glue;

#[path = "app_shadow.rs"]
mod shadow_glue;

#[path = "app_balloon.rs"]
mod balloon_glue;

#[path = "app_capture.rs"]
mod capture_glue;

/// Configuration du client.
#[derive(Debug, Clone)]
pub struct Config {
    /// Dossier `Game` du client ; `None` = mode sans client (patchs et substituts seulement).
    pub game_dir: Option<PathBuf>,
    pub addons_dir: PathBuf,
    /// Dossiers de patchs : un dossier avec `patch.toml` (ex. `free-assets/`) ou un dossier de patchs.
    pub patch_dirs: Vec<PathBuf>,
    /// Dossier des données du serveur donné par `--server-data` ; `None` : `ARCHRUST_SERVER_DATA`, puis
    /// `server-data/` à côté de `realmlist.wtf` (voir [`crate::server_data`]).
    pub server_data: Option<PathBuf>,
    pub saved_dir: PathBuf,
    pub title: String,
    /// Modèle à afficher au démarrage pour vérifier le rendu (ex. `a0000005.ecl`), facultatif.
    pub preview_model: Option<String>,
    /// Connexion automatique au démarrage : `utilisateur:motdepasse@hôte:port/personnage`.
    pub auto_connect: Option<String>,
    /// Capture d'écran automatique : fichier PNG et délai en secondes, puis le client se ferme.
    pub screenshot: Option<(PathBuf, f32)>,
    /// Commande(s) de discussion (`/goto …`, `/hello`) exécutée(s) dès l'entrée en jeu, séparées par `;`.
    pub exec_on_enter: Option<String>,
    /// Commandes des écrans de sélection et de création (tests), séparées par `;` et jouées
    /// toutes les 1,5 s une fois la scène prête : `créer`, `race Orc`, `classe Berserker`,
    /// `visage 3`, `coiffure 5`, `tourner 90`, `zoom`, `nom Toto`, `valider`, `annuler`,
    /// `choisir 2`, `supprimer Toto`, `entrer`.
    pub login_exec: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Saisie du compte : panneau d'ArchRust (`ui::login_panel`).
    Login,
    Connecting,
    /// Authentifié : choix du personnage du compte (la liaison attend `NetLink::choose`).
    CharacterSelect,
    /// Écran de création (modèles de base reçus du serveur).
    CharacterCreate,
    InWorld,
}

struct Gpu {
    window: Arc<Window>,
    renderer: Renderer,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    ground: Arc<Mesh>,
    marker: Arc<Mesh>,
    player_marker: Arc<Mesh>,
}

pub struct App {
    config: Config,
    assets: Arc<Assets>,
    addons: AddonManager,
    gpu: Option<Gpu>,
    ui: UiState,
    /// Fond de l'écran de connexion (`None` = pas encore chargé, `Some(None)` = illisible).
    login_screen: Option<Option<egui::TextureHandle>>,
    /// La connexion en cours passe par l'écran de sélection des personnages.
    choosing_character: bool,
    /// `ini/LoginSettings.txt` (`None` = pas encore lu, `Some(None)` = absent).
    login_settings: Option<Option<LoginSettings>>,
    /// Signature de la scène de connexion construite (phase, personnages ou création).
    scene_signature: Option<String>,
    /// Point autour duquel charger le terrain de la scène de connexion (`None` hors scène).
    scene_focus: Option<Vec3>,
    /// Hauteur du sol (objet) sous chaque personnage de la scène, par CID local.
    scene_floor: HashMap<i32, f32>,
    camera: OrbitCamera,
    phase: Phase,
    net: Option<NetLink>,
    entities: Vec<Entity>,
    items: HashMap<i32, archrust_netclient::world::ItemRecord>,
    player: Option<Entity>,
    /// Animateurs par CID (avec l'identité des maillages pour lesquels ils ont été construits) ;
    /// `None` = entité sans maillage skinné ou sans animation connue.
    animators: HashMap<i32, (usize, Option<Animator>)>,
    player_animator: Option<(usize, Option<Animator>)>,
    /// État du HUD Archonia : caractéristiques du joueur, améliorations actives, textures.
    hud: crate::hud::HudState,
    /// Cibles prises au Tab récemment (voir [`crate::targeting`]).
    tab_history: crate::targeting::TabHistory,
    /// TID des PNJ placés (`ini/npc.ini`), que le Tab ne cible pas ; chargé au premier appui.
    npc_tids: Option<std::collections::HashSet<i32>>,
    preview: Vec<(Arc<Mesh>, Mat4)>,
    terrain: Terrain,
    /// Objets statiques du monde (`ini/obj%05d.ini`).
    objects: WorldObjects,
    /// Le joueur est en marche vers sa destination (un paquet d'arrêt sera envoyé à l'arrivée).
    walking: bool,
    started: Instant,
    screenshot_done: bool,
    last_frame: Instant,
    fps: f32,
    right_drag: bool,
    middle_drag: bool,
    last_cursor: Option<(f64, f64)>,
    keys: [bool; 4],
    exit: bool,
    /// Instant où lancer `--exec` : un peu après l'entrée en jeu, pour que les paquets CHARACTER
    /// déjà reçus (qui remettent la destination du joueur sur sa position) soient traités avant.
    exec_at: Option<Instant>,
    /// Commandes `--exec` restantes : `/attendre <s>` met la suite en pause, `/capture <fichier>` enregistre
    /// une capture sans quitter (séances de test pilotées).
    exec_queue: Vec<String>,
    /// File des commandes `--login-exec` restantes et instant de la prochaine.
    login_exec: Vec<String>,
    login_exec_at: Option<Instant>,
    /// Liaison avec les AddOns Lua (interface, souris, CVars), voir `app_addons.rs`.
    addon_glue: addon_glue::AddonGlue,
    /// Musique, ambiance et effets (voir `audio.rs`).
    audio: Audio,
    /// Régions du monde (grilles `.amf`) pour la musique de zone.
    zones: Zones,
    /// Options persistées (`saved-variables/options.toml`) et drapeau « à enregistrer ».
    options: ClientOptions,
    options_dirty: bool,
    /// Packs de ressources tels que le joueur les règle dans les Options (copie de ceux d'`assets`,
    /// enregistrée dans `packs.toml` et appliquée au prochain démarrage).
    packs: archrust_patch::PatchStack,
    /// Musique imposée par un AddOn (`PlayMusic`) : `Some(Some(chemin))` ; `Some(None)` = coupée ;
    /// `None` = musique automatique (écrans ou région).
    music_override: Option<Option<String>>,
    /// Prochaine recherche de la région du joueur (toutes les secondes) et dernière région trouvée
    /// (musique et ambiance de l'heure courante).
    zone_check_at: Option<Instant>,
    zone_region: Option<ZoneAudio>,
    /// Heure du jeu reçue du serveur (`AGPMTIMER`), 12 h ×6 d'Archonia sinon.
    game_clock: GameClock,
    /// Ciel, météo et musique du gabarit, voir `app_nature.rs`.
    nature: nature_glue::NatureGlue,
    /// Effets d'Archonia joués (voir `app_effects.rs`).
    effects: effects_glue::EffectsGlue,
    /// Groupe du joueur (paquets 0x08 / 0x09), voir `app_party.rs`.
    party: party_glue::PartyGlue,
    /// Combat, mort, résurrection et butin, voir `app_combat.rs`.
    combat: combat_glue::CombatGlue,
    /// Inventaire actif, équipement, fenêtre du personnage, voir `app_items.rs`.
    item_ui: items_glue::ItemsGlue,
    /// Dialogue, boutique, banque, quêtes des PNJ, voir `app_npc.rs`.
    npc: npc_glue::NpcGlue,
    /// Compétences : livre, instructeurs, barre de raccourcis, voir `app_skills.rs`.
    skills: skills_glue::SkillsGlue,
    /// Portails : liste des destinations, écran de chargement, voir `app_portal.rs`.
    portal: portal_glue::PortalGlue,
    /// Reconnexion automatique en cours (AgcmReconnect).
    reconnect: reconnect_glue::ReconnectGlue,
    /// Guilde : fenêtres d'Archonia, invitations, discussion de guilde, voir `app_guild.rs`.
    guild: guild_glue::GuildGlue,
    /// Combat entre joueurs : ennemis, messages, statut criminel et de meurtrier, voir `app_pvp.rs`.
    pvp: pvp_glue::PvpGlue,
    /// Panneaux au-dessus des personnages : noms, guildes, dégâts, voir `app_boards.rs`.
    boards: boards_glue::BoardsGlue,
    /// Forgeron et état des objets convertis (renforcement, sockets, pierres), voir `app_convert.rs`.
    convert: convert_glue::ConvertGlue,
    /// Effets des armes serties (pierres d'esprit, runes, renforcement), voir `app_stones.rs`.
    stones: stones_glue::StonesGlue,
    necklace: necklace_glue::NecklaceGlue,
    /// Sphère d'alchimie : grille, recettes, prix et barre d'attente, voir `app_refinery.rs`.
    refinery: refinery_glue::RefineryGlue,
    /// Socles de gacha (« Fortune Gate ») : fenêtre `UI_Gacha`, bande, voir `app_gacha.rs`.
    gacha: gacha_glue::GachaGlue,
    /// Production : récolte, cuisine, alchimie, PNJ de production, voir `app_product.rs`.
    product: product_glue::ProductGlue,
    /// Amis et ignorés : listes, fenêtres, `IgnoreList.txt`, voir `app_buddy.rs`.
    buddy: buddy_glue::BuddyGlue,
    /// Courrier : boîte aux lettres, lecture, écriture, voir `app_mail.rs`.
    mail: mail_glue::MailGlue,
    /// Échange entre joueurs : demande, fenêtre d'échange, voir `app_trade.rs`.
    trade: trade_glue::TradeGlue,
    /// Usure et réparation chez les forgerons, voir `app_repair.rs`.
    repair: repair_glue::RepairGlue,
    /// Montures : modèles, cavaliers, refus à cheval, voir `app_ride.rs`.
    ride: ride_glue::RideGlue,
    /// Maison des ventes : planche, boîte de ventes, mise en vente, voir `app_auction.rs`.
    auction: auction_glue::AuctionGlue,
    /// Commutateur d'équipement (`UI_SwitchEditor`, `ItemSwitch_*.dat`), voir `app_itemset.rs`.
    itemset: itemset_glue::ItemSetGlue,
    /// Discussion d'Archonia et salons (`UI_ChatRoom*`), voir `app_chat.rs`.
    chat: chat_glue::ChatGlue,
    /// Coiffeur (`UI_Customizing`) et apparence des personnages, voir `app_customize.rs`.
    customize: customize_glue::CustomizeGlue,
    /// Avis de recherche (`UI_WantedCriminal`, `UI_CriminalArrange*`), voir `app_wanted.rs`.
    wanted: wanted_glue::WantedGlue,
    /// Invocations : registre, menus `UI_TargetSummonsMenu*`, familiers, voir `app_summons.rs`.
    summons: summons_glue::SummonsGlue,
    /// Combats de Shrine (0x62) et annonces du Battleground (0x58), voir `app_shrine.rs`.
    shrine: shrine_glue::ShrineGlue,
    /// Fenêtre des boss et compteur de dégâts (0x64), voir `app_boss.rs`.
    boss: boss_glue::BossGlue,
    /// Guerres de siège (0x49 / 0x50) et tableau des guerres (0x61), voir `app_siege.rs`.
    siege: siege_glue::SiegeGlue,
    /// Titre d'ArchLord (0x55), voir `app_archlord.rs`.
    archlord: archlord_glue::ArchlordGlue,
    /// Taxes des régions et intendants (0x53), voir `app_tax.rs`.
    tax: tax_glue::TaxGlue,
    /// Métamorphose et évolution du Dragon Scion : modèles, icônes d'annulation, refus, voir `app_morph.rs`.
    morph: morph_glue::MorphGlue,
    /// Orbe de résurrection, Gem of origin, oubli chez le maître, messages des objets, voir `app_consumables.rs`.
    consumables: consumables_glue::ConsumablesGlue,
    /// Actions sociales (`UI_SocialAction`, AGPMCHARACTER op 16, `SOCIAL_PC.txt`), voir `app_social.rs`.
    social: social_glue::SocialGlue,
    /// Sons des personnages : animations, pas, compétences (voir `app_sounds.rs`).
    sounds: sounds_glue::SoundsGlue,
    /// Touches d'Archonia (`[EventMap]`, `Ini/UI_Hotkey.ini`, `UI_Hotkey`) et fenêtre `UI_EXIT`, voir `app_hotkey.rs`.
    hotkey: hotkey_glue::HotkeyGlue,
    /// Inventaire Cash (`UI_CashInven`) et sac du familier (`UI_SubInventory`), voir `app_petbag.rs`.
    petbag: petbag_glue::PetBagGlue,
    /// Aide (`UI_Help`) et astuces (`UI_TIPS`), voir `app_help.rs`.
    help: help_glue::HelpGlue,
    /// Titres (0x60) : `UI_TitleAll`, `UI_TitleCurrent`, `UI_TitleMini`, titres au-dessus des têtes, voir `app_titles.rs`.
    titles: titles_glue::TitlesGlue,
    /// Surveillance des dossiers de packs (`assets/`, `patches/`), voir `app_assets.rs`.
    assets_watch: assets_glue::AssetsWatch,
    /// Curseurs de la souris d'Archonia, voir `app_cursor.rs`.
    cursor: cursor_glue::CursorGlue,
    /// Carte du monde (`UI_WORLD_MAP`, touche M), voir `app_worldmap.rs`.
    worldmap: worldmap_glue::WorldMapGlue,
    /// Retour à la sélection des personnages (« Select Character » d'`UI_EXIT`), voir `app_return_login.rs`.
    return_login: return_login_glue::ReturnLoginGlue,
    /// Caméra d'Archonia : `CamSetting.xml`, touches, modes F10 / F11 / F12, voir `app_camera.rs`.
    cam: camera_glue::CameraGlue,
    /// Ombres rondes sous les personnages et repère de destination au sol, voir `app_decals.rs`.
    decals: decals_glue::DecalsGlue,
    /// Ciblage au sol des compétences de zone (voir `app_groundcast.rs`).
    ground_cast: groundcast_glue::GroundCastGlue,
    /// Menus et options des onglets de la discussion (voir `app_chattabs.rs`).
    chat_tabs_ui: chattabs_glue::ChatTabsGlue,
    /// Ombres projetées des objets sur le sol (AgcmShadow2), voir `app_shadow.rs`.
    shadow2: shadow_glue::ShadowGlue,
    /// Bulles de discussion au-dessus des têtes (AgcmTextBoard), voir `app_balloon.rs`.
    balloons: balloon_glue::BalloonGlue,
    /// Capture d'écran (Impr. écran → `screen\SC….bmp`), voir `app_capture.rs`.
    capture: capture_glue::CaptureGlue,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        // Packs de ressources, avec les préférences du joueur (`saved-variables/packs.toml`).
        let pack_settings = archrust_patch::PackSettings::load(&config.saved_dir);
        // Données du serveur (`server-data/`) : tables de valeurs lues avant les packs et le client.
        let server_data = crate::server_data::find_dir(config.server_data.as_deref(), config.game_dir.as_deref());
        let assets = Arc::new(Assets::new(config.game_dir.clone(), &config.patch_dirs).with_pack_settings(&pack_settings).with_server_data(server_data.as_deref()));
        let packs = assets.pack_stack();
        // Comme sous WoW, les AddOns se chargent à l'entrée en jeu (voir `enter_world_addons`).
        let addons = Self::new_addon_manager(&config)?;
        log::info!(
            "{} AddOn(s) trouvé(s) dans {}",
            AddonManager::discover(&config.addons_dir).len(),
            config.addons_dir.display()
        );
        // Options persistées, puis périphérique audio (muet s'il n'y en a pas).
        let options = crate::options::load(&config.saved_dir);
        crate::assets::set_text_language(options.language.as_deref());
        let audio = Audio::new(
            assets.clone(),
            Some(config.addons_dir.clone()),
            options.audio.clone(),
        );
        // Serveur de connexion : `realmlist.wtf` (à côté de l'exécutable, dossier courant, client).
        let mut ui = UiState::default();
        // Compte enregistré (case « Enregistrer le compte »).
        if let Some(saved) = crate::account::load(&config.saved_dir) {
            log::info!("compte enregistré : {}", saved.user);
            ui.user = saved.user;
            ui.password = saved.password;
            ui.character = saved.character;
            ui.remember = true;
        }
        match crate::realmlist::find(&crate::realmlist::search_dirs(config.game_dir.as_deref())) {
            Some(r) => {
                log::info!("serveur {}:{} ({})", r.host, r.port, r.source.display());
                ui.host = r.host;
                ui.port = r.port.to_string();
            }
            None => log::info!(
                "pas de {} : serveur {}:{} par défaut",
                crate::realmlist::FILE_NAME,
                ui.host,
                ui.port
            ),
        }
        Ok(Self {
            config,
            assets,
            addons,
            gpu: None,
            ui,
            login_screen: None,
            choosing_character: false,
            login_settings: None,
            scene_signature: None,
            scene_focus: None,
            scene_floor: HashMap::new(),
            camera: OrbitCamera::default(),
            phase: Phase::Login,
            net: None,
            entities: Vec::new(),
            items: HashMap::new(),
            player: None,
            animators: HashMap::new(),
            player_animator: None,
            hud: crate::hud::HudState::default(),
            tab_history: Default::default(),
            npc_tids: None,
            preview: Vec::new(),
            terrain: Terrain::new(options.video.sector_radius()),
            objects: WorldObjects::new(options.video.sector_radius()),
            walking: false,
            started: Instant::now(),
            screenshot_done: false,
            last_frame: Instant::now(),
            fps: 0.0,
            right_drag: false,
            middle_drag: false,
            last_cursor: None,
            keys: [false; 4],
            exit: false,
            exec_at: None,
            exec_queue: Vec::new(),
            login_exec: Vec::new(),
            login_exec_at: None,
            addon_glue: Default::default(),
            audio,
            zones: Zones::default(),
            packs,
            options,
            options_dirty: false,
            music_override: None,
            zone_check_at: None,
            zone_region: None,
            game_clock: GameClock::default(),
            nature: Default::default(),
            effects: Default::default(),
            party: Default::default(),
            combat: Default::default(),
            item_ui: Default::default(),
            npc: Default::default(),
            skills: Default::default(),
            portal: Default::default(),
            reconnect: Default::default(),
            guild: Default::default(),
            pvp: Default::default(),
            boards: Default::default(),
            convert: Default::default(),
            stones: Default::default(),
            necklace: Default::default(),
            refinery: Default::default(),
            gacha: Default::default(),
            product: Default::default(),
            buddy: Default::default(),
            mail: Default::default(),
            trade: Default::default(),
            repair: Default::default(),
            ride: Default::default(),
            auction: Default::default(),
            itemset: Default::default(),
            chat: Default::default(),
            customize: Default::default(),
            wanted: Default::default(),
            summons: Default::default(),
            shrine: Default::default(),
            boss: Default::default(),
            siege: Default::default(),
            archlord: Default::default(),
            tax: Default::default(),
            morph: Default::default(),
            consumables: Default::default(),
            social: Default::default(),
            sounds: Default::default(),
            hotkey: Default::default(),
            petbag: Default::default(),
            help: Default::default(),
            titles: Default::default(),
            assets_watch: Default::default(),
            cursor: Default::default(),
            worldmap: Default::default(),
            return_login: Default::default(),
            cam: Default::default(),
            decals: Default::default(),
            ground_cast: Default::default(),
            chat_tabs_ui: Default::default(),
            shadow2: Default::default(),
            balloons: Default::default(),
            capture: Default::default(),
        })
    }

    /// Musique et ambiance voulues selon l'écran, la région du joueur et les AddOns, puis fondus.
    fn update_audio(&mut self, dt: f32) {
        let in_world = self.phase == Phase::InWorld;
        // Région du joueur, revue toutes les secondes (et à chaque entrée en jeu).
        if in_world {
            let due = self.zone_check_at.map_or(true, |t| Instant::now() >= t);
            if due {
                self.zone_check_at = Some(Instant::now() + std::time::Duration::from_secs(1));
                let hour = self.game_clock.hour();
                let found = self.player.as_ref().and_then(|p| {
                    self.zones
                        .audio_at(&self.assets, p.position.x, p.position.z, hour)
                });
                // Journal au changement de région, de musique ou d'ambiance (donc de tranche).
                if found != self.zone_region {
                    if let Some(z) = &found {
                        log::info!(
                            "région {} ({}) à {hour} h ({}) : musiques {} ; ambiance {}",
                            z.region,
                            Zones::region_name(&self.assets, z.region).unwrap_or_default(),
                            if self.game_clock.is_set() { "heure du serveur" } else { "heure d'Archonia par défaut" },
                            if z.music.is_empty() { "aucune dans skyset.ini".to_string() } else { z.music.join(", ") },
                            if z.ambience.is_empty() {
                                "aucune dans skyset.ini".to_string()
                            } else {
                                z.ambience.join(" + ")
                            }
                        );
                    }
                }
                // Zone sûre, libre ou de combat annoncée au changement de région (voir `app_pvp.rs`).
                if let Some(region) = found.as_ref().map(|z| z.region) {
                    self.pvp_region(region as u32);
                }
                self.zone_region = found;
            }
        } else {
            self.zone_check_at = None;
            self.zone_region = None;
        }
        // En jeu, la liste de lecture du ciel (`app_nature.rs`) : morceau tiré dans le gabarit, joué
        // une fois, le suivant 10 s après la fin ; sinon la CVar `worldMusic` en boucle.
        let playlist = match &self.music_override {
            None if in_world => self.nature_music(),
            _ => None,
        };
        // Ambiances de la tranche composée du ciel (région, zone locale, météo : `app_nature.rs`).
        let nature_ambience = if in_world { self.nature_ambience_sets() } else { None };
        let cvar = |name: &str| {
            self.addons
                .cvar(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let music = match &self.music_override {
            Some(forced) => forced.clone(),
            None if in_world => cvar("worldMusic"),
            None => cvar("loginMusic"),
        };
        // Ambiances : tous les flux des `EffectIndex` de la tranche composée, joués ensemble comme
        // Archonia (`FUN_140581fd0`) ; à défaut celles de la région, sinon la CVar `worldAmbience`.
        let ambience: Vec<String> = if in_world {
            let list = match nature_ambience {
                Some(ids) => Zones::ambience_of_sets(&self.assets, &ids),
                None => self.zone_region.as_ref().map(|z| z.ambience.clone()).unwrap_or_default(),
            };
            if list.is_empty() { cvar("worldAmbience").into_iter().collect() } else { list }
        } else {
            Vec::new()
        };
        match playlist {
            Some(Some((path, generation))) => self.audio.play_music_once(Some(&path), generation),
            // Silence entre deux morceaux du ciel.
            Some(None) => self.audio.play_music(None),
            None => self.audio.play_music(music.as_deref()),
        }
        self.audio.play_ambiences(&ambience);
        // Sons des personnages, pas et sons 3D (voir `app_sounds.rs`).
        self.sounds_frame(dt);
        self.audio.update(dt);
    }

    /// Enregistre les options si elles ont changé et que l'utilisateur a lâché les curseurs.
    fn save_options_if_needed(&mut self, force: bool) {
        if !self.options_dirty {
            return;
        }
        let pointer_busy = self
            .gpu
            .as_ref()
            .map(|g| g.egui_state.egui_ctx().is_using_pointer())
            .unwrap_or(false);
        if force || !pointer_busy {
            match crate::options::save(&self.config.saved_dir, &self.options) {
                Ok(()) => self.options_dirty = false,
                Err(e) => {
                    log::warn!("options : {e:#}");
                    self.options_dirty = false;
                }
            }
        }
    }

    fn init_gpu(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let mut attrs = Window::default_attributes()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        // Icône de fenêtre : le logo officiel d'ArchRust (64×64), intégré à l'exécutable. Sous
        // Windows, winit n'en fait que la petite icône (titre) : la barre des tâches et Alt+Tab
        // lisent la grande, posée à part.
        const WINDOW_ICON_PNG: &[u8] = include_bytes!("../assets/window-icon.png");
        if let Ok(img) = image::load_from_memory(WINDOW_ICON_PNG) {
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            let rgba = img.into_raw();
            #[cfg(windows)]
            {
                use winit::platform::windows::WindowAttributesExtWindows;
                if let Ok(icon) = winit::window::Icon::from_rgba(rgba.clone(), w, h) {
                    attrs = attrs.with_taskbar_icon(Some(icon));
                }
            }
            if let Ok(icon) = winit::window::Icon::from_rgba(rgba, w, h) {
                attrs = attrs.with_window_icon(Some(icon));
            }
        }
        let window = Arc::new(event_loop.create_window(attrs)?);
        let mut renderer = Renderer::new(window.clone())?;
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1, false);
        let ground = Arc::new(ground_mesh(&renderer, 200_000.0));
        let marker = Arc::new(box_mesh(
            &renderer,
            Vec3::new(40.0, 90.0, 40.0),
            [0.9, 0.8, 0.3, 1.0],
        ));
        let player_marker = Arc::new(box_mesh(
            &renderer,
            Vec3::new(40.0, 90.0, 40.0),
            [0.3, 0.6, 1.0, 1.0],
        ));
        if let Some(model) = &self.config.preview_model {
            match model_meshes(&mut renderer, &self.assets, "character", model) {
                Ok(meshes) => {
                    self.preview = meshes;
                    log::info!("aperçu {model} : {} maillages", self.preview.len());
                    // Cadrage : près du modèle (à l'origine), décalé pour qu'il ne soit pas sous le panneau de connexion.
                    self.camera.target = Vec3::new(0.0, 100.0, 0.0);
                    self.camera.distance = 600.0;
                    self.camera.pan(0.0, 300.0);
                }
                Err(e) => log::warn!("aperçu {model} : {e:#}"),
            }
        }
        // Sans client ni connexion automatique : un personnage local (humain par défaut, TID 96) à
        // l'origine, pour voir les modèles des patchs (`[[models]]` glTF) sans serveur.
        if !self.assets.has_client() && self.config.auto_connect.is_none() && self.player.is_none()
        {
            let meshes = entity_meshes(
                &mut renderer,
                &self.assets,
                Self::LOCAL_PREVIEW_TID,
                Head::default(),
                &player_marker,
            )
            .unwrap_or_default();
            log::info!(
                "mode sans client : personnage local (TID {}) affiché à l'origine, {} maillage(s)",
                Self::LOCAL_PREVIEW_TID,
                meshes.len()
            );
            self.player = Some(Entity {
                cid: 0,
                tid: Self::LOCAL_PREVIEW_TID,
                name: "Aperçu".into(),
                hp: None,
                position: Vec3::ZERO,
                destination: Vec3::ZERO,
                turn_y: 0.0,
                head: Head::default(),
                meshes,
                equipment: Vec::new(),
                is_player: true,
                speeds: Default::default(),
            });
            // Cadrage : près du personnage, décalé pour qu'il ne soit pas sous le panneau de connexion.
            self.camera.target = Vec3::new(0.0, 100.0, 0.0);
            self.camera.distance = 600.0;
            self.camera.pan(0.0, 300.0);
        }
        self.gpu = Some(Gpu {
            window,
            renderer,
            egui_state,
            egui_renderer,
            ground,
            marker,
            player_marker,
        });
        if let Some(spec) = self.config.auto_connect.clone() {
            // utilisateur:motdepasse@hôte:port/personnage ; sans hôte (`test:test@/Testeur`), le
            // serveur du `realmlist.wtf` (déjà dans l'écran de connexion) est gardé.
            if let Some((creds, rest)) = spec.split_once('@') {
                let (user, pass) = creds.split_once(':').unwrap_or((creds, ""));
                // Sans `/personnage`, la connexion passe par l'écran de sélection.
                let (hostport, character) = match rest.split_once('/') {
                    Some((h, c)) if !c.trim().is_empty() => (h, Some(c.trim().to_string())),
                    Some((h, _)) => (h, None),
                    None => (rest, None),
                };
                self.ui.user = user.to_string();
                self.ui.password = pass.to_string();
                if !hostport.trim().is_empty() {
                    let (host, port) = hostport.split_once(':').unwrap_or((hostport, "11002"));
                    self.ui.host = host.to_string();
                    self.ui.port = port.to_string();
                }
                if let Some(name) = &character {
                    self.ui.character = name.clone();
                }
                self.start_connection(character);
            } else {
                log::warn!(
                    "--connect : forme attendue utilisateur:motdepasse@hôte:port/personnage"
                );
            }
        }
        Ok(())
    }

    /// `LoginSettings.txt`, lu une fois.
    fn login_settings(&mut self) -> Option<&LoginSettings> {
        if self.login_settings.is_none() {
            let s = login_settings::load(&self.assets);
            match &s {
                Some(s) => log::info!(
                    "scène de connexion : division {:?}, {} emplacements",
                    s.division,
                    s.select_spots.len()
                ),
                None => log::warn!("ini/LoginSettings.txt absent ou illisible"),
            }
            self.login_settings = Some(s);
        }
        self.login_settings.as_ref().and_then(|s| s.as_ref())
    }

    /// Point autour duquel charger le terrain de la scène de connexion.
    fn scene_center(&mut self) -> Option<Vec3> {
        let phase = self.phase;
        let race = self
            .ui
            .creation
            .as_ref()
            .map(|c| c.race.to_ascii_lowercase());
        let in_scene = matches!(phase, Phase::CharacterSelect | Phase::CharacterCreate)
            || (phase == Phase::Connecting && self.choosing_character);
        if !in_scene {
            return None;
        }
        let s = self.login_settings()?;
        match (phase, race) {
            (Phase::CharacterCreate, Some(r)) => s.create_spots.get(&r).map(|p| p.pos),
            _ => s.select_spots.first().map(|p| p.pos),
        }
    }

    /// Caméra fixe de la scène : sélection, création par race, zoom par race et classe.
    fn scene_camera(&mut self) -> Option<SceneCamera> {
        let phase = self.phase;
        let creation = self.ui.creation.as_ref().map(|c| {
            (
                c.race.to_ascii_lowercase(),
                c.current()
                    .map(|t| login_settings::class_kind(&t.class_name).to_ascii_lowercase())
                    .unwrap_or_default(),
                c.zoom,
            )
        });
        let s = self.login_settings()?;
        match (phase, creation) {
            (Phase::CharacterCreate, Some((race, class, zoom))) => {
                let zoomed = zoom
                    .then(|| s.camera_create_zoom.get(&(race.clone(), class)).copied())
                    .flatten();
                zoomed.or_else(|| s.camera_create.get(&race).copied())
            }
            _ => s.camera_select,
        }
    }

    /// Réapplique la caméra de la scène de connexion (après les CVars des AddOns).
    fn reapply_scene_camera(&mut self) {
        let in_scene = matches!(self.phase, Phase::CharacterSelect | Phase::CharacterCreate)
            || (self.phase == Phase::Connecting
                && self.choosing_character
                && !self.ui.characters.is_empty());
        if in_scene {
            if let Some(cam) = self.scene_camera() {
                self.apply_scene_camera(cam);
            }
        }
    }

    /// Applique la caméra fixe à la caméra orbitale : cible à 600 unités devant l'œil.
    fn apply_scene_camera(&mut self, cam: SceneCamera) {
        let forward = (cam.look_at - cam.pos).normalize_or(Vec3::X);
        let back = -forward;
        let distance = 600.0;
        self.camera.target = cam.pos + forward * distance;
        self.camera.distance = distance;
        self.camera.collision = f32::INFINITY;
        self.camera.pitch = back.y.clamp(-1.0, 1.0).asin();
        self.camera.yaw = back.z.atan2(back.x);
        let aspect = self
            .gpu
            .as_ref()
            .map(|g| g.renderer.aspect())
            .unwrap_or(16.0 / 9.0);
        self.camera.fov_y = cam.fov_y(aspect);
    }

    /// Construit ou met à jour la scène de connexion : caméra, personnages du compte aux
    /// emplacements (`CharacterSelect_Position`), personnage choisi qui avance de son
    /// `CharacterSelect_MoveOffSet`, aperçu de création (`CharacterCreate_Position`).
    /// Joue la prochaine commande `--login-exec` quand la scène est prête.
    fn run_login_exec(&mut self) {
        if self.login_exec.is_empty() {
            if let Some(list) = self.config.login_exec.take() {
                self.login_exec = list
                    .split(';')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .rev()
                    .collect();
            }
        }
        let ready = matches!(self.phase, Phase::CharacterSelect | Phase::CharacterCreate)
            && self.terrain.loaded_count() >= 9;
        if !ready || self.login_exec.is_empty() {
            return;
        }
        let now = Instant::now();
        if self.login_exec_at.is_some_and(|t| now < t) {
            return;
        }
        if self.login_exec_at.is_none() {
            self.login_exec_at = Some(now + std::time::Duration::from_millis(1500));
            return;
        }
        self.login_exec_at = Some(now + std::time::Duration::from_millis(1500));
        let Some(cmd) = self.login_exec.pop() else {
            return;
        };
        log::info!("--login-exec : {cmd}");
        let (verb, arg) = cmd.split_once(' ').unwrap_or((cmd.as_str(), ""));
        let arg = arg.trim().to_string();
        let mut actions = UiActions::default();
        match verb {
            "créer" | "creer" => actions.open_creation = true,
            "valider" => {
                if let Some(c) = &self.ui.creation {
                    if let Some(t) = c.current() {
                        actions.create_submit = Some((t.tid, c.name.clone(), c.face, c.hair));
                    }
                }
            }
            "annuler" => actions.cancel_creation = true,
            "entrer" => {
                actions.enter_character = self
                    .ui
                    .selected_character
                    .and_then(|i| self.ui.characters.get(i))
                    .map(|c| c.name.clone())
            }
            "supprimer" => actions.delete_character = Some(arg.clone()),
            "choisir" => {
                if let Ok(n) = arg.parse::<usize>() {
                    if n >= 1 && n <= self.ui.characters.len() {
                        self.ui.selected_character = Some(n - 1);
                    }
                }
            }
            _ => {
                if let Some(c) = self.ui.creation.as_mut() {
                    match verb {
                        "race" => {
                            if let Some(r) = login_settings::RACES
                                .into_iter()
                                .find(|r| r.eq_ignore_ascii_case(&arg))
                            {
                                c.select_race(r);
                            }
                        }
                        "classe" => {
                            if let Some(i) = c.templates.iter().position(|t| {
                                t.race == c.race && t.class_name.eq_ignore_ascii_case(&arg)
                            }) {
                                c.template = i;
                            }
                        }
                        "visage" => c.face = arg.parse().unwrap_or(c.face),
                        "coiffure" => c.hair = arg.parse().unwrap_or(c.hair),
                        "tourner" => c.rotate += arg.parse::<f32>().unwrap_or(0.0),
                        "zoom" => c.zoom = !c.zoom,
                        "nom" => c.name = arg.clone(),
                        _ => log::warn!("--login-exec : commande inconnue {cmd}"),
                    }
                }
            }
        }
        self.handle_actions(actions);
    }

    fn update_login_scene(&mut self) {
        self.run_login_exec();
        self.scene_focus = self.scene_center();
        let in_scene = matches!(self.phase, Phase::CharacterSelect | Phase::CharacterCreate)
            || (self.phase == Phase::Connecting
                && self.choosing_character
                && !self.ui.characters.is_empty());
        if !in_scene {
            return;
        }
        if let Some(cam) = self.scene_camera() {
            self.apply_scene_camera(cam);
        }
        let Some(settings) = self.login_settings().cloned() else {
            return;
        };
        // Signature : ce qui impose de recharger les modèles.
        let signature = match (self.phase, &self.ui.creation) {
            (Phase::CharacterCreate, Some(c)) => format!(
                "creation:{}:{}:{}",
                c.current().map(|t| t.tid).unwrap_or(0),
                c.face,
                c.hair
            ),
            _ => format!(
                "select:{}",
                self.ui
                    .characters
                    .iter()
                    .map(|c| format!("{}/{}/{}/{}", c.name, c.tid, c.face, c.hair))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        };
        if self.scene_signature.as_deref() != Some(signature.as_str()) {
            self.scene_signature = Some(signature);
            self.scene_floor.clear();
            self.build_login_scene(&settings);
        }
        // Sol réel sous chaque personnage : le dallage (objet) de la scène, sous la hauteur de
        // `LoginSettings.txt` ; calculé dès que les objets du secteur sont lus.
        for e in self.entities.iter_mut().filter(|e| e.cid < 0) {
            if !self.scene_floor.contains_key(&e.cid) {
                if let Some(h) = self.objects.floor_height(
                    &self.assets,
                    e.position.x,
                    e.position.z,
                    e.position.y + 50.0,
                ) {
                    log::debug!("scène : sol à {h:.1} sous {}", e.name);
                    self.scene_floor.insert(e.cid, h);
                }
            }
            if let Some(h) = self.scene_floor.get(&e.cid) {
                e.position.y = *h;
                e.destination.y = *h;
            }
        }
        // Rotation de l'aperçu de création ; pas en avant du personnage choisi.
        if self.phase == Phase::CharacterCreate {
            let (race, rotate) = self
                .ui
                .creation
                .as_ref()
                .map(|c| (c.race.to_ascii_lowercase(), c.rotate))
                .unwrap_or_default();
            if let (Some(spot), Some(e)) = (
                settings.create_spots.get(&race),
                self.entities
                    .iter_mut()
                    .find(|e| e.cid == Self::CREATION_CID),
            ) {
                e.turn_y = spot.rotate + rotate;
            }
            return;
        }
        let selected = self.ui.selected_character;
        for (i, ch) in self
            .ui
            .characters
            .iter()
            .enumerate()
            .take(settings.select_spots.len())
        {
            let spot = settings.select_spots[i];
            let Some(e) = self
                .entities
                .iter_mut()
                .find(|e| e.cid == Self::slot_cid(i))
            else {
                continue;
            };
            let (race, class) = login_settings::template_identity(&self.assets, ch.tid);
            let offset = settings
                .move_offsets
                .get(&(
                    race.to_ascii_lowercase(),
                    login_settings::class_kind(&class).to_ascii_lowercase(),
                ))
                .copied()
                .unwrap_or(0.0);
            let rad = spot.rotate.to_radians();
            let forward = Vec3::new(rad.sin(), 0.0, rad.cos());
            let target = if selected == Some(i) {
                spot.pos + forward * offset
            } else {
                spot.pos
            };
            e.destination = Vec3::new(target.x, e.position.y, target.z);
            if !Self::is_moving(e) {
                e.turn_y = spot.rotate;
            }
        }
    }

    const CREATION_CID: i32 = -100;

    /// CID local d'un emplacement de la scène de sélection (négatif : jamais un CID serveur).
    fn slot_cid(slot: usize) -> i32 {
        -1 - slot as i32
    }

    fn build_login_scene(&mut self, settings: &LoginSettings) {
        self.entities.retain(|e| e.cid >= 0);
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let fallback = gpu.marker.clone();
        let mut spawned = Vec::new();
        if self.phase == Phase::CharacterCreate {
            if let Some(c) = &self.ui.creation {
                let spot = settings
                    .create_spots
                    .get(&c.race.to_ascii_lowercase())
                    .copied();
                if let (Some(t), Some(spot)) = (c.current(), spot) {
                    let head = Head {
                        face: c.face,
                        hair: c.hair,
                    };
                    let meshes =
                        entity_meshes(&mut gpu.renderer, &self.assets, t.tid, head, &fallback)
                            .unwrap_or_default();
                    spawned.push((
                        Self::CREATION_CID,
                        t.tid,
                        t.class_name.clone(),
                        head,
                        spot,
                        meshes,
                    ));
                }
            }
        } else {
            for (i, ch) in self
                .ui
                .characters
                .iter()
                .enumerate()
                .take(settings.select_spots.len())
            {
                let head = Head {
                    face: ch.face,
                    hair: ch.hair,
                };
                let meshes =
                    entity_meshes(&mut gpu.renderer, &self.assets, ch.tid, head, &fallback)
                        .unwrap_or_default();
                spawned.push((
                    Self::slot_cid(i),
                    ch.tid,
                    ch.name.clone(),
                    head,
                    settings.select_spots[i],
                    meshes,
                ));
            }
        }
        for (cid, tid, name, head, spot, meshes) in spawned {
            self.entities.push(Entity {
                cid,
                tid,
                name,
                hp: None,
                position: spot.pos,
                destination: spot.pos,
                turn_y: spot.rotate,
                head,
                meshes,
                equipment: Vec::new(),
                is_player: false,
                speeds: Default::default(),
            });
        }
    }

    /// Quitte la scène de connexion : personnages de la scène retirés, caméra normale.
    fn leave_login_scene(&mut self) {
        self.entities.retain(|e| e.cid >= 0);
        self.scene_signature = None;
        self.scene_floor.clear();
        // Caméra orbitale par défaut, autour du joueur s'il est déjà là.
        let target = self
            .player
            .as_ref()
            .map(|p| p.position + Vec3::new(0.0, 100.0, 0.0))
            .unwrap_or(self.camera.target);
        self.camera = OrbitCamera {
            target,
            ..OrbitCamera::default()
        };
        // Bornes et hauteurs de `CamSetting.xml` réappliquées à l'image suivante (`app_camera.rs`).
        self.camera_reset();
    }

    /// Clic dans la scène de sélection : personnage le plus proche du rayon de la souris.
    fn click_scene_character(&mut self) {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else {
            return;
        };
        let (w, h) = (
            gpu.renderer.config.width as f32,
            gpu.renderer.config.height as f32,
        );
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let ndc = glam::Vec2::new(cx as f32 / w * 2.0 - 1.0, 1.0 - cy as f32 / h * 2.0);
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        let dir = (far - near).normalize_or_zero();
        let mut best: Option<(usize, f32)> = None;
        for i in 0..self.ui.characters.len() {
            let Some(e) = self.entities.iter().find(|e| e.cid == Self::slot_cid(i)) else {
                continue;
            };
            // Distance du rayon à l'axe du personnage, à mi-hauteur.
            let center = e.position + Vec3::Y * 90.0;
            let t = (center - near).dot(dir).max(0.0);
            let d = (near + dir * t).distance(center);
            if d < 90.0 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        if let Some((i, _)) = best {
            self.ui.selected_character = Some(i);
        }
    }

    /// Lance la connexion ; `character` = entrée directe, `None` = écran de sélection.
    /// Langue choisie dans les Options : envoyée tout de suite au serveur monde (`SET_LANGUAGE`) en
    /// jeu, sinon annoncée au `HELLO` de la prochaine connexion. Fonction libre (pas de `&mut self`)
    /// pour être appelée depuis la fermeture egui, qui n'emprunte que certains champs.
    fn apply_language_option(net: Option<&NetLink>, language: Option<&str>, ui: &mut UiState, in_world: bool) {
        match (net, language) {
            (Some(net), Some(lang)) if in_world => {
                net.send(Command::SetLanguage {
                    request_id: crate::net::LANGUAGE_REQUEST_ID,
                    language: lang.to_string(),
                });
                ui.push_chat(format!("[langue] {lang} demandée au serveur ; textes du client ({}) au prochain démarrage", crate::assets::text_dir_for(Some(lang))));
            }
            (_, Some(lang)) => ui.status = format!("langue {lang} : textes du serveur à la prochaine connexion, du client au prochain démarrage"),
            (_, None) => ui.status = "langue automatique : textes du serveur à la prochaine connexion, du client (français) au prochain démarrage".into(),
        }
    }

    /// Connexion au serveur de connexion, puis au premier monde annoncé.
    fn start_connection(&mut self, character: Option<String>) {
        let port = self.ui.port.trim().parse().unwrap_or(11002);
        self.choosing_character = character.is_none();
        self.ui.characters.clear();
        self.ui.selected_character = None;
        let creds = Credentials {
            host: self.ui.host.trim().to_string(),
            port,
            user: self.ui.user.trim().to_string(),
            password: self.ui.password.clone(),
            character,
            create_if_missing: self.ui.create_if_missing,
            language: self.options.language.clone(),
        };
        self.ui.server_language = None;
        self.ui.status = "connexion…".into();
        self.net = Some(NetLink::start(creds));
        self.phase = Phase::Connecting;
    }

    fn poll_net(&mut self) {
        let Some(net) = &self.net else { return };
        let mut events = Vec::new();
        while let Ok(e) = net.events.try_recv() {
            events.push(e);
        }
        for e in events {
            match e {
                NetEvent::Status(s) => {
                    // État interne de la connexion : journal seulement (Archonia n'écrit rien).
                    log::info!("connexion : {s}");
                    self.ui.status = s.clone();
                }
                NetEvent::Error(e) => {
                    // Erreur affichée sur la ligne d'état du panneau de connexion.
                    log::warn!("connexion : erreur : {e}");
                    self.ui.status = format!("erreur : {e}");
                    self.ui.characters.clear();
                    self.ui.creation = None;
                    self.leave_login_scene();
                    self.phase = Phase::Login;
                    self.net = None;
                    return;
                }
                NetEvent::Worlds(w) => {
                    log::info!("mondes : {}", w.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", "));
                    // Monde joint : le premier annoncé (titre d'`UI_EXIT`).
                    self.ui.selected_world = w.first().map(|x| x.name.clone());
                }
                // Compte refusé : le panneau d'ArchRust garde les champs et affiche l'erreur.
                NetEvent::AuthRefused(why) => {
                    log::info!("compte refusé : {why}");
                    self.ui.status = format!("erreur : {why}");
                    self.phase = Phase::Login;
                    self.net = None;
                    return;
                }
                NetEvent::Characters(c) => {
                    for ch in &c {
                        self.ui.push_chat(format!(
                            "[système] personnage {} niveau {} (cid {})",
                            ch.name, ch.level, ch.cid
                        ));
                    }
                    // Présélection : le dernier personnage joué, sinon le premier.
                    self.ui.selected_character = c
                        .iter()
                        .position(|ch| ch.name.eq_ignore_ascii_case(self.ui.character.trim()))
                        .or(if c.is_empty() { None } else { Some(0) });
                    self.ui.characters = c;
                    if self.choosing_character {
                        self.phase = Phase::CharacterSelect;
                        self.ui.status.clear();
                    }
                }
                NetEvent::CreationTemplates(bases) => {
                    let templates: Vec<ui::CreationTemplate> = bases
                        .iter()
                        .map(|b| {
                            let (race, class_name) =
                                login_settings::template_identity(&self.assets, b.tid);
                            let (face_count, hair_count) =
                                crate::scene::head_counts(&self.assets, b.tid);
                            ui::CreationTemplate {
                                tid: b.tid,
                                class_name: if class_name.is_empty() {
                                    b.name.clone()
                                } else {
                                    class_name
                                },
                                race,
                                face_count,
                                hair_count,
                            }
                        })
                        .collect();
                    log::info!("création : {} modèle(s) de base", templates.len());
                    let mut creation = ui::CreationUi {
                        templates,
                        ..Default::default()
                    };
                    if let Some(first) = creation.templates.first().map(|t| t.race) {
                        creation.select_race(first);
                    }
                    self.ui.creation = Some(creation);
                    self.ui.status.clear();
                    self.phase = Phase::CharacterCreate;
                }
                NetEvent::CharacterCreated(c) => {
                    log::info!("personnage créé : {} (TID {})", c.name, c.tid);
                    self.ui.character = c.name.clone();
                    self.ui.characters.push(c);
                    self.ui.selected_character = Some(self.ui.characters.len() - 1);
                    self.ui.creation = None;
                    self.ui.status.clear();
                    self.phase = Phase::CharacterSelect;
                }
                NetEvent::CharacterDeleted(name) => {
                    self.ui.characters.retain(|c| c.name != name);
                    self.ui.selected_character = if self.ui.characters.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.ui.status = format!("{name} supprimé");
                }
                NetEvent::Refused(msg) => {
                    self.ui.status = format!("refusé : {msg}");
                }
                NetEvent::CreateRefused { name, code } => {
                    log::info!("création de {name} refusée (résultat {code})");
                    self.ui.status.clear();
                    // Messages de l'[EventMap] d'Archonia : LG_CHAR_CREATE_FAILD_SAME_NAME pour le
                    // résultat 7, sinon LG_CHAR_CREATE_FAILD_INVALID_NAME.
                    self.ui.alert = Some(match code {
                        7 => "There is already a character with that name.".to_string(),
                        _ => "This character name is not allowed. Please choose another name".to_string(),
                    });
                }
                NetEvent::World(ev) => self.on_world_event(ev),
            }
        }
    }

    fn on_world_event(&mut self, ev: WorldEvent) {
        // Discussion d'un ignoré à « Block Chat » : écartée (voir `app_buddy.rs`).
        if self.buddy_blocks(&ev) {
            return;
        }
        // Noms et PV des membres de groupe possibles (voir `app_party.rs`).
        self.party_observe(&ev);
        // Morts, vues des monstres, objets retirés du sol (voir `app_combat.rs`).
        self.combat_observe(&ev);
        // Effets : montée de niveau, apprentissage, compétences, coups (voir `app_effects.rs`).
        self.effects_observe(&ev);
        // Sons des compétences et des coups portés (voir `app_sounds.rs`).
        self.sounds_observe(&ev);
        // Statut criminel et points de meurtrier (voir `app_pvp.rs`).
        self.pvp_observe(&ev);
        self.portal_observe(&ev);
        // Fonctions des PNJ en vue (voir `app_npc.rs`).
        self.npc_observe(&ev);
        // Compétences de production et lancement d'une récolte (voir `app_product.rs`).
        self.product_observe(&ev);
        // Métamorphose, refus du serveur, sorties de vue pour les montures (voir `app_ride.rs`).
        self.ride_observe(&ev);
        // Salons, coiffeur, avis : remis à zéro à chaque connexion (métamorphose pour le coiffeur).
        self.chat_observe(&ev);
        self.customize_observe(&ev);
        self.wanted_observe(&ev);
        // Invocations : remises à zéro, attaches des personnages sortis de la vue (voir `app_summons.rs`).
        self.summons_observe(&ev);
        // Shrines : métamorphose du joueur, remises à zéro (voir `app_shrine.rs`).
        self.shrine_observe(&ev);
        // Titres : remises à zéro (voir `app_titles.rs`).
        self.titles_observe(&ev);
        // Siège : remises à zéro (voir `app_siege.rs`).
        self.siege_observe(&ev);
        // ArchLord et taxes : remises à zéro (voir `app_archlord.rs`, `app_tax.rs`).
        self.archlord_observe(&ev);
        self.tax_observe(&ev);
        // Métamorphose : remises à zéro, effet d'évolution, compte à rebours des potions (voir `app_morph.rs`).
        self.morph_observe(&ev);
        // Consommables : remises à zéro, messages et recharge de l'op 4, `ReverseOrbError` (voir `app_consumables.rs`).
        self.consumables_observe(&ev);
        // Derniers coups du joueur, pour le délai du retour à la sélection (voir `app_return_login.rs`).
        self.return_login_observe(&ev);
        match ev {
            WorldEvent::Connected => {
                // Nouvelle connexion au monde : les instantanés d'objets qui suivent repartent de zéro.
                log::debug!(target: "archrust_buffs", "hud_reset raison=connexion avant={}", self.hud.buffs.len());
                self.items.clear();
                self.hud.buffs.clear();
                // Amis, ignorés et courriers renvoyés par le serveur.
                self.buddy_reset();
                self.mail_reset();
                self.trade_reset();
                self.auction_reset();
                self.itemset_reset();
            }
            WorldEvent::SelfCharacter {
                cid,
                tid,
                name,
                position,
                face,
                hair,
                stats,
            } => {
                // Fiche d'un autre joueur qui entre dans la vue : une entité (voir `app_players.rs`).
                if self.is_other_player(cid) {
                    self.other_player_seen(players_glue::OtherPlayer {
                        cid,
                        tid,
                        name,
                        position,
                        face,
                        hair,
                        stats,
                    });
                    return;
                }
                // Caractéristiques du joueur pour le HUD (avant les retours anticipés ci-dessous).
                if let Some(stats) = &stats {
                    if self.player.as_ref().is_none_or(|p| p.cid == 0 || p.cid == cid) {
                        self.hud.merge_stats(stats);
                    }
                }
                // Les paquets CHARACTER ultérieurs (mises à jour partielles) n'ont pas toujours le TID :
                // on garde le modèle déjà chargé, et on ignore ceux d'un autre CID.
                // Le personnage local du mode sans client (cid 0) est remplacé par le vrai.
                let mut head = Head {
                    face: face.unwrap_or(0),
                    hair: hair.unwrap_or(0),
                };
                let mut tid = tid;
                if let Some(p) = self.player.as_mut().filter(|p| p.cid != 0) {
                    if p.cid != cid {
                        return;
                    }
                    head = Head {
                        face: face.unwrap_or(p.head.face),
                        hair: hair.unwrap_or(p.head.hair),
                    };
                    if tid == 0 || tid == p.tid {
                        if !name.is_empty() {
                            p.name = name.clone();
                        }
                        if head == p.head {
                            return;
                        }
                        // Même modèle, autre visage ou coiffure : on recharge.
                        tid = p.tid;
                    }
                }
                let name = if name.is_empty() {
                    self.player
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_default()
                } else {
                    name
                };
                let fallback = self.gpu.as_ref().map(|g| g.player_marker.clone());
                let meshes = match (&mut self.gpu, fallback) {
                    (Some(g), Some(fb)) => {
                        entity_meshes(&mut g.renderer, &self.assets, tid, head, &fb)
                            .unwrap_or_default()
                    }
                    _ => Vec::new(),
                };
                let pos = self
                    .player
                    .as_ref()
                    .map(|p| p.position)
                    .unwrap_or(Vec3::ZERO);
                let hp = self.player.as_ref().and_then(|p| p.hp);
                let turn_y = self.player.as_ref().map_or(0.0, |p| p.turn_y);
                let speeds = self.player.as_ref().map(|p| p.speeds).unwrap_or_default();
                log::info!(
                    "joueur {name} (cid {cid}, tid {tid}) : {} maillage(s)",
                    meshes.len()
                );
                self.player = Some(Entity {
                    cid,
                    tid,
                    name: name.clone(),
                    hp,
                    position: pos,
                    destination: pos,
                    turn_y,
                    head,
                    meshes,
                    equipment: Vec::new(),
                    is_player: true,
                    speeds,
                });
                self.sync_addons_world();
            }
            WorldEvent::CharacterUpdated {
                cid,
                position,
                destination,
                factors,
                stats,
                ..
            } => {
                let is_player = self.player.as_ref().is_some_and(|p| p.cid == cid);
                if let (Some(stats), true) = (&stats, is_player) {
                    self.hud.merge_stats(stats);
                }
                let hp = factors.and_then(|f| f.hp).or(stats.as_ref().and_then(|s| s.hp));
                let (walk, run) = stats.as_ref().map_or((None, None), |s| (s.walk_speed, s.run_speed));
                if let Some(player) = self.player.as_mut().filter(|p| p.cid == cid) {
                    player.speeds.learn(walk, run);
                    if let Some(position) = position {
                        player.position = position;
                    }
                    if let Some(destination) = destination {
                        player.destination = destination;
                    }
                    if hp.is_some() {
                        player.hp = hp;
                    }
                } else if let Some(entity) = self.entities.iter_mut().find(|e| e.cid == cid) {
                    entity.speeds.learn(walk, run);
                    if let Some(position) = position {
                        entity.position = position;
                    }
                    if let Some(destination) = destination {
                        entity.destination = destination;
                    }
                    if hp.is_some() {
                        entity.hp = hp;
                    }
                }
            }
            WorldEvent::CharacterRemoved { cid, for_view } => {
                if self.player.as_ref().is_some_and(|player| player.cid == cid) {
                    log::debug!(
                        "paquet retrait personnage local ignoré (cid {cid}, vue={for_view})"
                    );
                } else {
                    self.entities.retain(|entity| entity.cid != cid);
                    log::debug!("personnage {cid} retiré du monde (vue={for_view})");
                }
            }
            WorldEvent::ItemChanged(item) => {
                // Grille d'échange entre joueurs (statut 6) : objets de l'autre à part (voir `app_trade.rs`).
                if self.trade_item(&item) {
                    return;
                }
                // Objet porté par un autre joueur : son équipement, pas l'inventaire.
                if self.other_player_item(&item) {
                    return;
                }
                // Métamorphosé ou évolué : déplacement refusé par FUN_1401bca10 ignoré (voir `app_morph.rs`).
                if self.morph_ignores_item(&item) {
                    return;
                }
                // Sac du familier indisponible : objet du statut 13 ignoré (voir `app_petbag.rs`).
                if self.petbag_ignores(&item) {
                    return;
                }
                if item.operation == 1
                    || item
                        .status
                        // 53 : entrepôt de guilde (voir `app_guild.rs`) ; 13 sac du familier, 51 inventaire
                        // Cash (voir `app_petbag.rs`).
                        .is_some_and(|status| !matches!(status, 2 | 3 | 13 | 51 | 53 | 100))
                {
                    self.items.remove(&item.id);
                } else {
                    let current = self.items.entry(item.id).or_insert_with(|| item.clone());
                    current.operation = item.operation;
                    if item.template_id.is_some() {
                        current.template_id = item.template_id;
                    }
                    if item.owner_cid.is_some() {
                        current.owner_cid = item.owner_cid;
                    }
                    if item.count.is_some() {
                        current.count = item.count;
                    }
                    if item.status.is_some() {
                        current.status = item.status;
                    }
                    if item.grid.is_some() {
                        current.grid = item.grid;
                    }
                }
            }
            WorldEvent::ItemsSnapshot(items) => {
                // Les objets de chaque OPT_VIEW op 2 s'ajoutent à ceux déjà reçus : ArchCore (comme
                // le serveur d'origine) envoie un premier paquet plein puis un second au champ vide,
                // qui ne doit pas vider l'inventaire.
                self.items.extend(
                    items
                        .into_iter()
                        .filter(|item| matches!(item.status, Some(2 | 3 | 51 | 100)))
                        .map(|item| (item.id, item)),
                );
            }
            WorldEvent::LoadingPosition(p) => {
                if let Some(pl) = &mut self.player {
                    pl.position = p;
                    pl.destination = p;
                }
                self.camera.target = p + Vec3::new(0.0, 100.0, 0.0);
            }
            // Heure du monde, météo et effets de nature (voir `app_nature.rs`).
            WorldEvent::Nature(ev) => self.nature_event(ev),
            WorldEvent::CharacterSeen(c) => {
                if let Some(index) = self.entities.iter().position(|e| e.cid == c.cid) {
                    // Métamorphosé : l'équipement suit sans être redessiné (voir `app_morph.rs`).
                    if self.entities[index].equipment != c.equipment && self.morph_frozen_model(c.cid) {
                        self.entities[index].equipment = c.equipment;
                    } else if self.entities[index].equipment != c.equipment {
                        if let Some(gpu) = self.gpu.as_mut() {
                            let fallback = gpu.marker.clone();
                            let head = self.entities[index].head;
                            if let Ok(meshes) = entity_meshes_with_equipment(
                                &mut gpu.renderer,
                                &self.assets,
                                c.tid,
                                head,
                                &c.equipment,
                                &fallback,
                            ) {
                                self.entities[index].meshes = meshes;
                                self.entities[index].equipment = c.equipment;
                            }
                        }
                    }
                    return;
                }
                let mut speeds = crate::scene::Speeds::default();
                speeds.learn(c.stats.as_ref().and_then(|s| s.walk_speed), c.stats.as_ref().and_then(|s| s.run_speed));
                let fallback = self.gpu.as_ref().map(|g| g.marker.clone());
                // Chaque entité en vue reçoit son modèle (plus de limite : au-delà de 64, les
                // créatures étaient remplacées par la boîte de repère). La boîte ne reste qu'en
                // cas d'échec du chargement.
                let meshes = match (&mut self.gpu, fallback) {
                    (Some(g), Some(fb)) => {
                        let started = Instant::now();
                        let meshes = entity_meshes_with_equipment(
                            &mut g.renderer,
                            &self.assets,
                            c.tid,
                            Head::default(),
                            &c.equipment,
                            &fb,
                        )
                        .unwrap_or_else(|e| {
                            log::warn!("TID {} : {e:#}", c.tid);
                            vec![(fb.clone(), Mat4::IDENTITY)]
                        });
                        log::debug!(
                            "modèle de {} (TID {}) en {:.1} ms",
                            c.name,
                            c.tid,
                            started.elapsed().as_secs_f32() * 1000.0
                        );
                        meshes
                    }
                    (_, Some(fb)) => vec![(fb, Mat4::IDENTITY)],
                    _ => Vec::new(),
                };
                self.entities.push(Entity {
                    cid: c.cid,
                    tid: c.tid,
                    name: c.name,
                    hp: None,
                    position: c.position,
                    destination: c.destination,
                    // Orientation de la vue ; sinon, vers la destination s'il marche déjà.
                    turn_y: c
                        .turn_y
                        .or_else(|| turn_towards(c.position, c.destination))
                        .unwrap_or(0.0),
                    head: Head::default(),
                    meshes,
                    equipment: c.equipment,
                    is_player: false,
                    speeds,
                });
            }
            WorldEvent::CharacterMoved {
                cid,
                position,
                destination,
                fast,
            } => {
                if let Some(e) = self.entities.iter_mut().find(|e| e.cid == cid) {
                    e.position = position;
                    e.destination = destination;
                    e.speeds.fast = fast;
                } else if self.player.as_ref().map(|p| p.cid == cid).unwrap_or(false) {
                    if let Some(p) = &mut self.player {
                        // Le joueur se déplace toujours à la course (`send_move`) : un déplacement imposé
                        // par le serveur (téléportation, arrêt) ne change pas son allure.
                        p.position = position;
                        p.destination = destination;
                    }
                }
            }
            // Coups (0x41) : PV, messages, chiffres, animations, voir `app_combat.rs`.
            ev @ WorldEvent::CharacterAction { .. } => self.on_combat_action(ev),
            // Butin au sol, ramassage, montée de niveau.
            ev @ (WorldEvent::GroundItem(_) | WorldEvent::PickupResult { .. } | WorldEvent::LevelUp { .. }) => {
                self.on_combat_event(ev)
            }
            // Objet utilisé, point de retour d'un parchemin (voir `app_items.rs`).
            ev @ (WorldEvent::ItemUsed { .. } | WorldEvent::ReturnPoint { .. }) => self.on_item_event(ev),
            // Dialogue, boutique, banque, quêtes (voir `app_npc.rs`).
            WorldEvent::Npc(ev) => self.on_npc_event(ev),
            // Compétences : liste, niveau, lancement, instructeur (voir `app_skills.rs`).
            WorldEvent::Skill(ev) => self.on_skill_event(ev),
            // Portails : accord, départ, chargement fini (voir `app_portal.rs`).
            WorldEvent::Teleport(ev) => self.on_teleport_event(ev),
            // Métamorphose : relevée par `portal_observe` (refus des portails).
            WorldEvent::Transformed { .. } => {}
            // Métamorphose et évolution (AGPMCHARACTER ops 11, 12, 26, 27 ; AGPMITEM ops 19, 20, 27), voir `app_morph.rs`.
            WorldEvent::Morph(ev) => self.on_morph_event(ev),
            // Orbe, échecs d'objet, « skill init », oubli chez le maître (voir `app_consumables.rs`).
            WorldEvent::Consumable(ev) => self.on_consumable_event(ev),
            // Actions sociales : animation et son de `SOCIAL_PC.txt` (voir `app_social.rs`).
            WorldEvent::Social(ev) => self.on_social_event(ev),
            // Reconnexion automatique (0x63), voir `app_reconnect.rs`.
            WorldEvent::Reconnecting => self.reconnect_event(true),
            WorldEvent::Reconnected => self.reconnect_event(false),
            // Guildes (0x15 / 0x39), voir `app_guild.rs`.
            WorldEvent::Guild(ev) => self.on_guild_event(ev),
            WorldEvent::UiStatus(st) => self.on_ui_status(st),
            WorldEvent::Admin(a) => self.on_admin_level(a),
            WorldEvent::Pvp(ev) => self.on_pvp(ev),
            WorldEvent::Remission(ev) => self.on_remission(ev),
            WorldEvent::Convert(ev) => self.on_convert_event(ev),
            WorldEvent::Refinery(ev) => self.on_refinery_event(ev),
            WorldEvent::Gacha(ev) => self.on_gacha_event(ev),
            WorldEvent::Product(ev) => self.on_product_event(ev),
            // Amis et ignorés (0x2B), courrier (0x2D) : voir `app_buddy.rs`, `app_mail.rs`.
            WorldEvent::Buddy(ev) => self.on_buddy_event(ev),
            WorldEvent::Mail(ev) => self.on_mail_event(ev),
            // Échange entre joueurs (0x07), voir `app_trade.rs`.
            WorldEvent::Trade(ev) => self.on_trade_event(ev),
            // Réparation et durabilité (0x34, AGPMITEM champ 10), montures (0x4D) : voir `app_repair.rs`,
            // `app_ride.rs`.
            WorldEvent::Repair(ev) => self.on_repair_event(ev),
            WorldEvent::Ride(ev) => self.on_ride_event(ev),
            // Maison des ventes (0x0B), commutateur d'équipement (0x6A) : voir `app_auction.rs`, `app_itemset.rs`.
            WorldEvent::Auction(ev) => self.on_auction_event(ev),
            WorldEvent::ItemSet(ev) => self.on_itemset_event(ev),
            // Salons (0x2C), coiffeur (0x43), avis de recherche (0x46) : voir `app_chat.rs`, `app_customize.rs`,
            // `app_wanted.rs`.
            WorldEvent::Channel(ev) => self.on_channel_event(ev),
            WorldEvent::Customize(ev) => self.on_customize_event(ev),
            WorldEvent::Wanted(ev) => self.on_wanted_event(ev),
            // Invocations (0x4F) et état spécial des personnages : voir `app_summons.rs`.
            WorldEvent::Summons(ev) => self.on_summons_event(ev),
            // Shrines (0x62), Battleground (0x58), fenêtre des boss (0x64) : voir `app_shrine.rs`, `app_boss.rs`.
            WorldEvent::Shrine(ev) => self.on_shrine_event(ev),
            WorldEvent::Battleground(ev) => self.on_battleground(ev),
            WorldEvent::Boss(ev) => self.on_boss_event(ev),
            // Guerres de siège (0x49 / 0x50) et tableau des guerres (0x61) : voir `app_siege.rs`.
            WorldEvent::Siege(ev) => self.on_siege_event(ev),
            // Titre d'ArchLord (0x55) et taxes des régions (0x53) : voir `app_archlord.rs`, `app_tax.rs`.
            WorldEvent::Archlord(ev) => self.on_archlord_event(ev),
            WorldEvent::Tax(ev) => self.on_tax_event(ev),
            // Mini Cash et ramassage automatique (AGPMITEM ops 26, 30) : voir `app_petbag.rs`.
            WorldEvent::PetBag(ev) => petbag_glue::on_event(&mut self.petbag, ev),
            // Titres (0x60) : voir `app_titles.rs`.
            WorldEvent::Title(p) => self.on_title_event(p),
            // Messages système réduits à un code (invocations : `app_summons.rs` ; banque agrandie… :
            // `app_npc.rs`).
            WorldEvent::SystemCode(code) => {
                if !self.summons_system_code(code) {
                    self.npc_system_code(code);
                }
            }
            WorldEvent::Chat {
                from,
                to,
                message,
                channel,
                ..
            } if channel == 5 || channel == 6 => {
                // Chuchotement : le serveur renvoie aussi à l'émetteur son propre message.
                let me = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                if !me.is_empty() && from == me && !to.is_empty() {
                    self.addon_chat_received("CHAT_MSG_WHISPER_INFORM", &message, &to);
                } else {
                    self.ui.last_whisper = Some(from.clone());
                    self.addon_chat_received("CHAT_MSG_WHISPER", &message, &from);
                }
            }
            WorldEvent::Chat {
                from_cid,
                from,
                message,
                channel,
                ..
            } => {
                // Bulle au-dessus de la tête (discussion normale, `VH_BALOONCHAT`), voir `app_balloon.rs`.
                self.balloon_chat(from_cid, channel, &message);
                // Types AGPMCHATTING et cris de zone (0x4C) : classe, couleur et format d'Archonia
                // (FUN_140511e20), filtres des AddOns, événement CHAT_MSG_* (voir `app_chat.rs`).
                self.chat_received(channel, &from, &message);
            }
            WorldEvent::SystemMessage(m) => {
                self.hud.push_system(&m);
                self.addon_chat_received("CHAT_MSG_SYSTEM", &m, "");
            }
            WorldEvent::EnteredWorld => {
                self.leave_login_scene();
                self.phase = Phase::InWorld;
                self.ui.status = "en jeu".into();
                // Chargement des AddOns (comme sous WoW), PLAYER_LOGIN, PLAYER_ENTERING_WORLD.
                self.enter_world_addons();
                // Refus et casque d'`options.toml` (onglets Fonction et Affichage des options).
                self.options_enter_world();
                // Courrier non lu déjà reçu : `YouHaveMail` (voir `app_mail.rs`).
                self.mail_enter_world();
                log::info!("{} entités en vue à l'entrée en jeu", self.entities.len());
                if let Some(cmds) = &self.config.exec_on_enter {
                    self.exec_queue = cmds.split(';').map(str::trim).filter(|c| !c.is_empty()).map(String::from).collect();
                    self.exec_at = Some(Instant::now() + std::time::Duration::from_millis(1500));
                }
            }
            WorldEvent::BuffAdded(buff) => {
                let me = self.player.as_ref().map(|p| p.cid);
                log::debug!(target: "archrust_buffs", "moteur_ajout porteur={} joueur={me:?} id={} tid={} duree_ms={} modele_lanceur={} accepte_hud={}", buff.owner, buff.skill_id, buff.skill_tid, buff.duration_ms, buff.caster_tid, me == Some(buff.owner));
                if self.player.as_ref().is_some_and(|p| p.cid == buff.owner) {
                    log::info!("amélioration ajoutée : compétence TID {} ({} ms)", buff.skill_tid, buff.duration_ms);
                    self.hud.add_buff(buff.skill_tid, buff.duration_ms);
                }
            }
            WorldEvent::BuffRemoved { owner, skill_tid } => {
                let me = self.player.as_ref().map(|p| p.cid);
                log::debug!(target: "archrust_buffs", "moteur_retrait porteur={owner} joueur={me:?} tid={skill_tid} accepte_hud={}", me == Some(owner));
                if self.player.as_ref().is_some_and(|p| p.cid == owner) {
                    self.hud.remove_buff(skill_tid);
                }
            }
            WorldEvent::CharacterProperties { op, values } => self.hud.set_character_properties(op, &values),
            WorldEvent::Unhandled { packet_type, op } => {
                log::debug!("paquet non géré 0x{packet_type:02x} op {op:?}")
            }
            // Extensions ArchRust (0xAC) : accord et messages d'AddOn, voir `app_addons.rs`.
            ev @ (WorldEvent::ExtensionsReady(_)
            | WorldEvent::AddonMessage { .. }
            | WorldEvent::AddonMessageFailed { .. }
            | WorldEvent::LanguageChanged { .. }
            | WorldEvent::Texts { .. }) => self.addon_world_event(ev),
            // Groupes (0x08 / 0x09), voir `app_party.rs`.
            ev @ (WorldEvent::Party(_) | WorldEvent::PartyManagement(_)) => self.on_party_event(ev),
            // Retour à la sélection des personnages : refus ou départ du monde (voir `app_return_login.rs`).
            WorldEvent::ReturnToLogin(e) => self.return_login_event(e),
            // Compte connecté ailleurs (AGPMCHARACTER op 8, AgcmUILogin `0x1405a6350`) : boîte `LG_DUPLICATE_ACCOUNT`,
            // dessinée par l'écran de connexion après le `Disconnected` qui suit.
            WorldEvent::DisconnectedByAnotherUser => {
                let text = portal_glue::ui_event_text(&self.assets, "LG_DUPLICATE_ACCOUNT");
                self.ui.alert = Some(text).filter(|t| !t.is_empty());
            }
            WorldEvent::Disconnected(why) => {
                // Reconnexion échouée : boîte d'Archonia (`DisconnectedByServer`, `Action=4:0`), dessinée
                // par l'écran de connexion ; sinon rien d'affiché (journal).
                if let Some(text) = self.reconnect_failed() {
                    self.ui.alert = Some(text);
                }
                self.party_reset();
                self.guild_reset();
                self.buddy_reset();
                self.mail_reset();
                self.trade_reset();
                self.auction_reset();
                self.itemset_reset();
                log::info!("déconnecté : {why}");
                self.ui.status = format!("déconnecté : {why}");
                self.phase = Phase::Login;
                self.net = None;
                self.entities.clear();
                if let Some(player) = &mut self.player {
                    player.hp = None;
                }
                self.leave_world_addons();
            }
        }
    }

    fn handle_actions(&mut self, actions: UiActions) {
        if actions.connect && self.phase == Phase::Login {
            if self.ui.remember {
                let account = crate::account::SavedAccount {
                    user: self.ui.user.trim().to_string(),
                    password: self.ui.password.clone(),
                    character: self.ui.character.trim().to_string(),
                };
                if let Err(e) = crate::account::save(&self.config.saved_dir, &account) {
                    log::warn!("enregistrement du compte : {e:#}");
                    self.ui.status = format!("compte non enregistré : {e}");
                }
            } else {
                crate::account::forget(&self.config.saved_dir);
            }
            self.start_connection(None);
        }
        if self.phase == Phase::CharacterCreate {
            if let Some((tid, name, face, hair)) = actions.create_submit {
                self.ui.status = format!("création de {name}…");
                if let Some(net) = &self.net {
                    net.choose(LoginCommand::Create {
                        tid,
                        name,
                        face,
                        hair,
                    });
                }
            } else if actions.cancel_creation {
                self.ui.creation = None;
                self.ui.status.clear();
                self.phase = Phase::CharacterSelect;
            }
        }
        if self.phase == Phase::CharacterSelect {
            if actions.open_creation {
                self.ui.status = "écran de création…".into();
                if let Some(net) = &self.net {
                    net.choose(LoginCommand::OpenCreation);
                }
            }
            if let Some(name) = actions.delete_character.clone() {
                if let Some(net) = &self.net {
                    net.choose(LoginCommand::Delete(name));
                }
            }
            if let Some(name) = actions.enter_character.clone() {
                let choice = LoginCommand::Enter(name.clone());
                self.ui.character = name.clone();
                if self.ui.remember {
                    let account = crate::account::SavedAccount {
                        user: self.ui.user.trim().to_string(),
                        password: self.ui.password.clone(),
                        character: name.clone(),
                    };
                    if let Err(e) = crate::account::save(&self.config.saved_dir, &account) {
                        log::warn!("enregistrement du compte : {e:#}");
                    }
                }
                if let Some(net) = &self.net {
                    net.choose(choice);
                }
                self.phase = Phase::Connecting;
            } else if actions.back_to_login {
                // Fermer la liaison : la session réseau voit son canal de choix fermé et s'arrête.
                self.net = None;
                self.ui.characters.clear();
                self.ui.status.clear();
                self.leave_login_scene();
                self.phase = Phase::Login;
            }
        }
        if let Some(text) = actions.chat {
            if text.starts_with('/') {
                if !self.addons.run_slash(&text) {
                    self.builtin_slash(&text);
                }
            } else {
                // Passe par `SendChatMessage` des AddOns (surchargeable, accrochable).
                self.send_typed_chat(text);
            }
        }
        if let Some(src) = actions.console {
            if let Err(e) = self.addons.run_script(&src) {
                self.ui.push_chat(format!("[lua] {e}"));
            }
        }
        for id in actions.clicked_frames {
            self.addons.click(id, "LeftButton");
        }
    }

    fn builtin_slash(&mut self, text: &str) {
        let (cmd, arg) = text.split_once(' ').unwrap_or((text, ""));
        match cmd {
            "/reload" | "/rl" => self.reload_addons(),
            // Fenêtre Options du client (son, langue, packs de ressources), comme la touche Échap ;
            // « /options » appartient déjà à l'AddOn ArchAce.
            // `/reglages [gfx|son|affichage|fonction|avance]` : options, sur l'onglet demandé.
            "/reglages" | "/réglages" => {
                let tab = ["gfx", "son", "affichage", "fonction", "avance"].iter().position(|t| arg.trim().eq_ignore_ascii_case(t));
                match tab {
                    Some(t) => {
                        self.hud.options_tab = t as u8;
                        self.ui.show_options = true;
                    }
                    None => self.ui.show_options = !self.ui.show_options,
                }
            }
            // /effet <EID> [cid] : joue un jeu d'effets (voir `app_effects.rs`).
            "/effet" | "/effect" => {
                let line = self.effects_slash(arg);
                self.ui.push_chat(format!("[système] {line}"));
            }
            // /music <chemin> | off | auto : musique forcée (comme PlayMusic / StopMusic des AddOns).
            "/music" => {
                let arg = arg.trim();
                let line = match arg {
                    "" => format!(
                        "[système] musique : {} ; ambiance : {} ; heure du jeu : {} h ({}) ; usage : /music <chemin>|off|auto",
                        self.audio.current_music().unwrap_or("aucune"),
                        self.audio.current_ambience().unwrap_or("aucune"),
                        self.game_clock.hour(),
                        if self.game_clock.is_set() { "serveur" } else { "locale" }
                    ),
                    "auto" => {
                        self.music_override = None;
                        "[système] musique automatique".to_string()
                    }
                    "off" | "stop" => {
                        self.music_override = Some(None);
                        "[système] musique coupée".to_string()
                    }
                    path => {
                        self.music_override = Some(Some(path.to_string()));
                        format!("[système] musique : {path}")
                    }
                };
                self.ui.push_chat(line);
            }
            // /sound <chemin> : joue un effet (sound/ui/x.wav, sd0000.wav, fichier d'AddOn).
            "/sound" => {
                let arg = arg.trim();
                let line = if arg.is_empty() {
                    "[système] usage : /sound <chemin>".to_string()
                } else if self.audio.play_effect(arg) {
                    format!("[système] son : {arg}")
                } else {
                    format!("[système] son {arg} non joué (introuvable, muet ou sans sortie audio)")
                };
                self.ui.push_chat(line);
            }
            "/goto" => {
                let parts: Vec<f32> = arg
                    .split([' ', ','])
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if parts.len() >= 3 {
                    let mut dest = Vec3::new(parts[0], parts[1], parts[2]);
                    if let Some(h) = Self::ground_height(&self.terrain, &self.objects, &self.assets, dest.x, dest.z) {
                        dest.y = h;
                    }
                    log::debug!(
                        "/goto {dest:?} (joueur présent : {}, réseau : {})",
                        self.player.is_some(),
                        self.net.is_some()
                    );
                    if let (Some(net), Some(p)) = (&self.net, &mut self.player) {
                        net.send(Command::MoveTo {
                            position: p.position,
                            destination: dest,
                        });
                        p.destination = dest;
                        self.walking = true;
                    }
                } else {
                    self.ui
                        .push_chat("[système] usage : /goto x y z".to_string());
                }
            }
            // /sac [1-5] : ouvre ou ferme l’inventaire, sur le sac donné.
            "/sac" => match arg.trim().parse::<i16>() {
                Ok(n) if (1..=5).contains(&n) => {
                    self.hud.set_inventory_layer(n - 1);
                    self.ui.show_inventory = true;
                }
                _ => self.ui.show_inventory = !self.ui.show_inventory,
            },
            "/buff" => {
                // /buff <TID de compétence> [secondes] : amélioration locale, pour essayer le HUD
                // (les vraies arrivent du serveur par AGPMSKILL op 4). /buff sans argument : efface.
                let parts: Vec<u64> = arg.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                match parts.as_slice() {
                    [tid, rest @ ..] => {
                        let secs = rest.first().copied().unwrap_or(60);
                        self.hud.add_buff(*tid as i32, secs * 1000);
                        self.ui.push_chat(format!("[système] amélioration locale : compétence {tid}, {secs} s"));
                    }
                    [] => {
                        self.hud.buffs.clear();
                        self.ui.push_chat("[système] améliorations locales effacées".to_string());
                    }
                }
            }
            "/cam" => {
                // /cam distance [lacet°] [tangage°] : cadre la caméra orbitale (tests et captures).
                let parts: Vec<f32> = arg
                    .split([' ', ','])
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                match parts.as_slice() {
                    [d, rest @ ..] => {
                        self.camera.distance = d.clamp(100.0, 50_000.0);
                        if let Some(yaw) = rest.first() {
                            self.camera.yaw = yaw.to_radians();
                        }
                        if let Some(pitch) = rest.get(1) {
                            self.camera.pitch = pitch.to_radians().clamp(-1.2, 1.5);
                        }
                        self.ui.push_chat(format!(
                            "[système] caméra : distance {:.0}, lacet {:.0}°, tangage {:.0}°",
                            self.camera.distance,
                            self.camera.yaw.to_degrees(),
                            self.camera.pitch.to_degrees()
                        ));
                    }
                    _ => self.ui.push_chat(
                        "[système] usage : /cam distance [lacet°] [tangage°]".to_string(),
                    ),
                }
            }
            "/quit" => self.exit = true,
            // Combat : /attaquer, /ramasser, /ressusciter (voir `app_combat.rs`).
            _ if self.combat_slash(cmd, arg) => {}
            // Oubli des compétences chez le maître : /oublier ; /go, /nick, /merchant, /safekeeper (objets d'usage 24 et 26)
            // envoyés au serveur (voir `app_consumables.rs`).
            _ if self.consumables_slash(cmd, text) => {}
            // Actions sociales : /greet, /clap… /sit down (titres de `SocialAction.txt`, voir `app_social.rs`).
            _ if self.social_slash(cmd, arg) => {}
            // Objets : /perso, /utiliser, /porter, /ranger, /detruire, /retour (voir `app_items.rs`).
            _ if self.items_slash(cmd, arg) => {}
            // PNJ : /parler, /quetes, /prendrequete, /abandonner (voir `app_npc.rs`).
            _ if self.npc_slash(cmd, arg) => {}
            // Portails : /portail, /portail n (voir `app_portal.rs`).
            _ if self.portal_slash(cmd, arg) => {}
            // Essai de la reconnexion : /coupure (voir `app_reconnect.rs`).
            _ if self.reconnect_slash(cmd) => {}
            // Réparation : /reparation, /areparer, /reparer, /durabilite (voir `app_repair.rs`) ; montures :
            // /monture (voir `app_ride.rs`).
            _ if self.repair_slash(cmd, arg) => {}
            _ if self.ride_slash(cmd) => {}
            // Sphère d'alchimie : /sphere, /grille, /raffiner, /repeter, /recette (voir `app_refinery.rs`).
            _ if self.refinery_slash(cmd, arg) => {}
            // Gacha : /gacha, /gacha start, /gacha fermer (voir `app_gacha.rs`).
            _ if self.gacha_slash(cmd, arg) => {}
            // Production : /production, /recettes (voir `app_product.rs`).
            _ if self.product_slash(cmd) => {}
            // Amis, ignorés, courrier : /amis, /ignores, /courrier (voir `app_buddy.rs`, `app_mail.rs`).
            _ if self.buddy_slash(cmd) => {}
            _ if self.mail_slash(cmd) => {}
            // Maison des ventes : /ventes ; commutateur : /commutateur, /switch N, /saveset N.
            _ if self.auction_slash(cmd) => {}
            _ if self.itemset_slash(cmd, arg) => {}
            // Compétences : /competences, /lancer (voir `app_skills.rs`).
            _ if self.skills_slash(cmd, arg) => {}
            // Groupes : /inviter, /accepter, /refuser, /quittergroupe, /exclure, /chef, /groupe.
            _ if self.party_slash(cmd, arg) => {}
            // Discussion /G, /P, /N et guilde : /guilde, /ginviter, /gquitter… (voir `app_guild.rs`).
            _ if self.guild_slash(cmd, arg) => {}
            // Salons : /join, /leave ; /trouver (voir `app_chat.rs`).
            _ if self.chat_slash(cmd, arg) => {}
            // Panneaux au-dessus des personnages : /panneau (coup simulé, voir `app_boards.rs`).
            _ if self.boards_slash(cmd, arg) => {}
            // Forgeron : /ameliorer, /catalyseur, /onglet, /convertir, /infobulle (voir `app_convert.rs`).
            _ if self.convert_slash(cmd, arg) => {}
            // Ciel et météo : /ciel (état), /effetnature <id|-1> (local, voir `app_nature.rs`).
            _ if self.nature_slash(cmd, arg) => {}
            // Comme Archonia : une commande inconnue du client part au serveur dans la discussion,
            // qui traite les commandes MJ (`/create`, `/money`, `/move`… d'ArchCore) ou répond.
            _ if self.net.is_some() => {
                if let Some(net) = &self.net {
                    net.send(Command::Say(text.to_string()));
                }
            }
            _ => self
                .ui
                .push_chat(format!("[système] commande inconnue : {cmd}")),
        }
    }

    /// TID du personnage local affiché en mode sans client (humain par défaut, `a0000060.ecl`).
    const LOCAL_PREVIEW_TID: i32 = 96;

    /// Rayon du personnage contre les segments de blocage des `.amf` : Archonia prend celui du
    /// modèle de personnage (`+0x3fa0`), qu'aucune table du client ne renseigne ; 0 ici.
    const CHARACTER_RADIUS: f32 = 0.0;

    /// Hauteur du sol sous (x, z) comme Archonia (`FUN_140528910`) : terrain, ou modèle de
    /// collision d'un objet praticable (escalier, pont, dallage) s'il est plus haut.
    fn ground_height(terrain: &Terrain, objects: &WorldObjects, assets: &Assets, x: f32, z: f32) -> Option<f32> {
        let t = terrain.height_at(x, z)?;
        Some(objects.ridable_height(assets, x, z).map_or(t, |o| o.max(t)))
    }

    /// Avance une entité vers sa destination et la pose sur le sol. Pour le joueur (`blocking`),
    /// le pas s'arrête devant une cellule bloquée ou un segment du `.amf` (`FUN_14016ecd0` →
    /// `FUN_140196ab0`) : la destination devient le point d'arrêt.
    fn step_entity(terrain: &Terrain, objects: &WorldObjects, assets: &Assets, e: &mut Entity, dt: f32, blocking: bool) {
        let to = Vec3::new(
            e.destination.x - e.position.x,
            0.0,
            e.destination.z - e.position.z,
        );
        let dist = to.length();
        if dist > 1.0 {
            // Archonia tourne le personnage vers sa destination à chaque déplacement
            // (MoveCharacter, `FUN_140168070`) et garde l'angle à l'arrêt.
            if let Some(turn) = turn_towards(e.position, e.destination) {
                e.turn_y = turn;
            }
            let step = (e.speeds.units_per_second() * dt).min(dist);
            let next = e.position + to / dist * step;
            // Un départ dans une cellule bloquée (position imposée par le serveur) ne fige pas le
            // joueur : il peut en sortir.
            let stop = (blocking && !terrain.blocked(e.position.x, e.position.z))
                .then(|| terrain.clip_move(e.position, next, Self::CHARACTER_RADIUS))
                .filter(|c| (c.x - next.x).abs() > 0.01 || (c.z - next.z).abs() > 0.01);
            match stop {
                Some(c) => {
                    e.position = Vec3::new(c.x, e.position.y, c.z);
                    e.destination = e.position;
                }
                None => e.position = next,
            }
        }
        // Les personnages de la scène de connexion (CID négatifs) gardent la hauteur de
        // `LoginSettings.txt` : ils se tiennent sur le sol d'un bâtiment (salle du trône de la
        // division 1817), pas sur le terrain extérieur, plus bas.
        if e.cid < 0 {
            return;
        }
        if let Some(h) = Self::ground_height(terrain, objects, assets, e.position.x, e.position.z) {
            e.position.y = h;
        }
    }

    /// Simulation locale : déplacements, hauteur du sol, caméra qui suit le joueur.
    fn simulate(&mut self, dt: f32) {
        for e in self.entities.iter_mut() {
            Self::step_entity(&self.terrain, &self.objects, &self.assets, e, dt, false);
        }
        if let Some(p) = &mut self.player {
            let before = p.position;
            Self::step_entity(&self.terrain, &self.objects, &self.assets, p, dt, true);
            // Suivi, hauteur visée, touches et collision de la caméra : `camera_frame` (`app_camera.rs`).
            self.cam.moved |= before != p.position;
            let arrived = (p.destination.x - p.position.x).abs() <= 1.0
                && (p.destination.z - p.position.z).abs() <= 1.0;
            if self.walking && arrived {
                self.walking = false;
                if let Some(net) = &self.net {
                    net.send(Command::Stop {
                        position: p.position,
                    });
                }
                log::debug!("arrivé en {:?}", p.position);
            }
        }
        self.camera_frame(dt);
        // `--exec` : plusieurs commandes possibles, séparées par `;` (ex. `/cam 300 90 10; /goto x y z`) ;
        // `/attendre 5` attend 5 s avant la suite, `/capture x.png` enregistre une capture et continue.
        if self.exec_at.is_some_and(|t| Instant::now() >= t) {
            self.exec_at = None;
            while !self.exec_queue.is_empty() {
                let cmd = self.exec_queue.remove(0);
                let (head, arg) = cmd.split_once(' ').map(|(h, a)| (h, a.trim())).unwrap_or((cmd.as_str(), ""));
                match head {
                    "/attendre" | "/wait" => {
                        let secs = arg.replace(',', ".").parse::<f32>().unwrap_or(1.0).max(0.0);
                        self.exec_at = Some(Instant::now() + std::time::Duration::from_secs_f32(secs));
                        break;
                    }
                    "/capture" if !arg.is_empty() => {
                        log::info!("--exec : capture {arg}");
                        self.addon_glue.ui.screenshot = Some(PathBuf::from(arg));
                        if let Some(g) = &mut self.gpu {
                            g.renderer.capture_next = true;
                        }
                        // La capture part à la prochaine image : la suite attend un peu.
                        self.exec_at = Some(Instant::now() + std::time::Duration::from_millis(300));
                        break;
                    }
                    _ => {
                        log::info!("--exec : {cmd}");
                        self.handle_actions(UiActions {
                            chat: Some(cmd.clone()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        self.animate(dt);
    }

    /// Distance au-delà de laquelle les personnages ne sont plus animés (unités ArchLord).
    const ANIMATION_RANGE: f32 = 8000.0;

    fn is_moving(e: &Entity) -> bool {
        (e.destination.x - e.position.x).abs() > 1.0 || (e.destination.z - e.position.z).abs() > 1.0
    }

    /// Animation des personnages : attente, marche ou course selon le déplacement, sommets réécrits à
    /// à chaque image pour le joueur et à moins de 2 000 unités du joueur ou du point visé ;
    /// ailleurs, 30 Hz au plus dans le rayon existant. Les animateurs sont reconstruits quand les
    /// maillages d'une entité changent (équipement, modèle) ou que l'ensemble d'animations change
    /// (arme, monture : `worn::anim_equip`) ; un ensemble −1 laisse l'animation courante.
    fn animate(&mut self, dt: f32) {
        let Some(gpu) = &self.gpu else { return };
        let focus = self.camera.target;
        // Personnage hors du champ : le temps de son clip avance, sa pose n'est pas réécrite (rien n'en est
        // visible : les ombres projetées ne portent que sur le décor) ; elle l'est dès son retour dans le champ.
        let frustum = crate::render::Frustum::new(self.camera.render_view_proj(gpu.renderer.aspect()));
        let seed = |cid: i32| (cid as u32).wrapping_mul(0x9e37_79b9) | 1;
        let stale = |slot: Option<&(usize, Option<Animator>)>, sig: usize, equip: i32| {
            slot.is_none_or(|(s, a)| *s != sig || (equip >= 0 && a.as_ref().is_some_and(|a| a.equip != equip)))
        };
        if let Some(p) = &self.player {
            let sig = animation::signature(&p.meshes);
            let current = self.player_animator.as_ref().and_then(|(_, a)| a.as_ref()).map_or(0, |a| a.equip);
            let equip = crate::worn::anim_equip_for(&self.assets, p.tid, true, self.ride.riding(p.cid), &p.equipment, current);
            if stale(self.player_animator.as_ref(), sig, equip) {
                let a = Animator::new(&self.assets, p.tid, &p.meshes, seed(p.cid), equip, true);
                if let Some(a) = &a {
                    log::info!("joueur TID {} animé, ensemble {equip} ({:?})", p.tid, a.clip_name());
                }
                self.player_animator = Some((sig, a));
            }
            if let Some((_, Some(a))) = &mut self.player_animator {
                // Seuil de pose nul : suit chaque image sans arrondi de période à 60/144 Hz.
                a.update(&self.assets, &gpu.renderer, dt, Self::is_moving(p), p.speeds.fast || p.speeds.riding, f32::INFINITY);
            }
        }
        animator_retention::retain_present(&mut self.animators, &self.entities, |e| e.cid);
        for e in &self.entities {
            let sig = animation::signature(&e.meshes);
            let current = self.animators.get(&e.cid).and_then(|(_, a)| a.as_ref()).map_or(0, |a| a.equip);
            let equip = crate::worn::anim_equip_for(&self.assets, e.tid, e.is_player, self.ride.riding(e.cid), &e.equipment, current);
            if stale(self.animators.get(&e.cid), sig, equip) {
                self.animators.insert(e.cid, (sig, Animator::new(&self.assets, e.tid, &e.meshes, seed(e.cid), equip, e.is_player)));
            }
            let distance2 = e.position.distance_squared(focus);
            let close = distance2 < 2000.0 * 2000.0
                || self.player.as_ref().is_some_and(|p| e.position.distance_squared(p.position) < 2000.0 * 2000.0);
            let near = close || distance2 < Self::ANIMATION_RANGE * Self::ANIMATION_RANGE;
            if let (Some((_, Some(a))), true) = (self.animators.get_mut(&e.cid), near) {
                let shown = frustum.sphere(e.position + Vec3::Y * 100.0, 400.0);
                let hz = if !shown { 0.0 } else if close { f32::INFINITY } else { 30.0 };
                a.update(&self.assets, &gpu.renderer, dt, Self::is_moving(e), e.speeds.fast || e.speeds.riding, hz);
            }
        }
    }

    /// Ensemble d'animations courant du personnage `cid` (0 sans animateur).
    pub(super) fn anim_equip_of(&self, cid: i32) -> i32 {
        let slot = if self.player.as_ref().is_some_and(|p| p.cid == cid) { self.player_animator.as_ref() } else { self.animators.get(&cid) };
        slot.and_then(|(_, a)| a.as_ref()).map_or(0, |a| a.equip)
    }

    /// Clic gauche au sol : rayon caméra → terrain, destination du joueur et paquet 0x40.
    /// Personnage sous le curseur (le plus proche de la caméra), d'après sa silhouette projetée :
    /// des pieds à environ 1,8 m au-dessus, sur une largeur de la moitié de cette hauteur.
    /// Échap : ferme la console, puis les fenêtres ouvertes ; sans rien à fermer, ouvre le menu
    /// Options d'Archonia (`UI_MainMenuOptions`) en jeu, la fenêtre des options ailleurs. Les
    /// options, les menus et les boîtes egui se ferment d'eux-mêmes sur la touche.
    fn escape_key(&mut self) {
        if self.ui.console_open {
            self.ui.console_open = false;
            return;
        }
        // Ciblage au sol annulé (voir `app_groundcast.rs`).
        if self.groundcast_escape() {
            return;
        }
        // Boîtes et fenêtres d'`UI_Hotkey` / `UI_EXIT`, puis `UI_Help` (voir `app_hotkey.rs`, `app_help.rs`).
        if self.hotkey_escape() || self.help.close_windows() {
            return;
        }
        if self.ui.show_options || crate::hud::main_menu_open(&self.hud) {
            return;
        }
        if self.phase == Phase::InWorld {
            let mut closed = self.npc.close_windows();
            closed |= self.product.close_windows();
            closed |= self.buddy.close_windows();
            closed |= self.mail.close_windows();
            closed |= self.auction.close_windows();
            closed |= self.itemset.close_windows();
            for open in [&mut self.ui.show_inventory, &mut self.item_ui.show_character, &mut self.skills.show_book, &mut self.worldmap.open] {
                closed |= std::mem::take(open);
            }
            if closed {
                return;
            }
        }
        self.ui.escape_open = true;
    }

    /// Choix des menus principaux d'Archonia (`hud_menu.rs`) : mêmes effets que les raccourcis.
    fn main_menu_frame(&mut self) {
        for m in std::mem::take(&mut self.hud.menu_requests) {
            match m {
                crate::hud::MenuUi::Quests => {
                    self.npc_slash("/quetes", "");
                }
                crate::hud::MenuUi::Inventory => self.ui.show_inventory = !self.ui.show_inventory,
                crate::hud::MenuUi::Character => self.item_ui.show_character = !self.item_ui.show_character,
                crate::hud::MenuUi::Skills => self.skills.show_book = !self.skills.show_book,
                crate::hud::MenuUi::ProductSkills => self.product.show_skills = !self.product.show_skills,
                crate::hud::MenuUi::Guild => self.guild_toggle(),
                crate::hud::MenuUi::Party => self.party.frames_ui.options = !self.party.frames_ui.options,
                crate::hud::MenuUi::Emotes => self.social.toggle(),
                crate::hud::MenuUi::Friends => self.buddy.toggle_friends(),
                crate::hud::MenuUi::IgnoreList => self.buddy.toggle_bans(),
                crate::hud::MenuUi::Mailbox => self.mail.toggle(),
                crate::hud::MenuUi::CreateChannel => self.chat.open_create(),
                crate::hud::MenuUi::JoinChannel => self.chat.open_enter(),
                crate::hud::MenuUi::Options => self.ui.show_options = !self.ui.show_options,
                crate::hud::MenuUi::Help => self.help.toggle(),
                crate::hud::MenuUi::Exit => self.hotkey_toggle_exit(),
                crate::hud::MenuUi::CashInventory => self.petbag.toggle_cash(),
            }
        }
    }

    /// Tab / Maj+Tab : monstre suivant ou précédent (voir [`crate::targeting`]). Sont exclus
    /// les joueurs, les PNJ de `npc.ini`, les morts (PV connus à 0) et ce qui est au-delà de
    /// [`crate::targeting::TAB_RANGE`].
    fn tab_target(&mut self, reverse: bool) {
        let (Some(gpu), Some(player)) = (&self.gpu, &self.player) else {
            return;
        };
        let assets = &self.assets;
        let npcs = self.npc_tids.get_or_insert_with(|| {
            let Ok(t) = assets.table("ini", "npc.ini") else {
                return Default::default();
            };
            t.sections
                .iter()
                .filter_map(|s| s.get("TID")?.trim().parse().ok())
                .collect()
        });
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let vp = self.camera.view_proj(gpu.renderer.aspect());
        let to_screen = |p: Vec3| {
            let c = vp * p.extend(1.0);
            (c.w > 1e-3).then(|| (((c.x / c.w) + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };
        let cands: Vec<_> = self
            .entities
            .iter()
            .filter(|e| !e.is_player && e.cid != player.cid && !npcs.contains(&e.tid))
            // Familiers non sélectionnables (voir `app_summons.rs`).
            .filter(|e| !self.summons.book.is_pet(e.cid))
            .filter(|e| e.hp.is_none_or(|hp| hp > 0))
            .filter_map(|e| {
                let dist = (e.position - player.position).length();
                (dist <= crate::targeting::TAB_RANGE).then(|| {
                    // Le serveur ne dit pas encore qui attaque le joueur : pas de bonus « m'attaque ».
                    crate::targeting::Candidate::new(e.cid, to_screen(e.position + Vec3::new(0.0, 100.0, 0.0)), (w, h), dist, false)
                })
            })
            .collect();
        let current = self.addons.target_cid();
        if let Some(cid) = self.tab_history.cycle(cands, current, reverse, std::time::Instant::now()) {
            self.addons.set_target(Some(cid));
        }
    }

    fn entity_under_cursor(&self) -> Option<i32> {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else {
            return None;
        };
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let vp = self.camera.view_proj(gpu.renderer.aspect());
        let to_screen = |p: Vec3| {
            let c = vp * p.extend(1.0);
            (c.w > 1e-3).then(|| (((c.x / c.w) + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h, c.w))
        };
        let (cx, cy) = (cx as f32, cy as f32);
        self.entities
            .iter()
            .filter(|e| self.summons_selectable(e.cid))
            .filter_map(|e| {
                let (fx, fy, depth) = to_screen(e.position)?;
                let (_, hy, _) = to_screen(e.position + Vec3::new(0.0, 180.0, 0.0))?;
                let height = (fy - hy).abs().max(24.0);
                let inside = (cx - fx).abs() <= (height * 0.3).max(14.0) && cy <= fy + 4.0 && cy >= fy - height;
                inside.then_some((e.cid, depth))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(cid, _)| cid)
    }

    /// Point du sol sous le curseur : terrain, ou objet praticable (marche d'escalier, pont) plus proche.
    pub(super) fn cursor_ground_point(&self) -> Option<Vec3> {
        let (gpu, (cx, cy)) = (self.gpu.as_ref()?, self.last_cursor?);
        let (w, h) = (
            gpu.renderer.config.width as f32,
            gpu.renderer.config.height as f32,
        );
        let ndc = Vec3::new(
            (cx as f32 / w) * 2.0 - 1.0,
            1.0 - (cy as f32 / h) * 2.0,
            0.0,
        );
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let near = inv.project_point3(ndc);
        let far = inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        let dir = (far - near).normalize_or_zero();
        let ground = self.terrain.raycast(near, dir, self.camera.far);
        let max = ground.map_or(self.camera.far, |g| g.distance(near));
        match self.objects.ridable_raycast(&self.assets, near, dir, max) {
            Some(t) => Some(near + dir * t),
            None => ground,
        }
    }

    fn click_ground(&mut self) {
        // Ciblage au sol en cours : tout se joue au relâchement (voir `app_groundcast.rs`).
        if self.groundcast_active() {
            return;
        }
        // Mort, objet au sol, attaque de la cible déjà choisie (voir `app_combat.rs`).
        if self.combat_left_click() {
            return;
        }
        // Clic sur un personnage : il devient la cible, comme dans Archonia, sans déplacement.
        if let Some(cid) = self.entity_under_cursor() {
            self.addons.set_target(Some(cid));
            return;
        }
        // Clic sur un portail du décor : demande au serveur (voir `app_portal.rs`).
        if self.portal_left_click() {
            return;
        }
        // Clic sur une sphère d'alchimie du décor (voir `app_refinery.rs`).
        if self.refinery_left_click() {
            return;
        }
        // Clic sur un socle de gacha du décor (voir `app_gacha.rs`).
        if self.gacha_left_click() {
            return;
        }
        // Point cliqué : terrain, ou objet praticable.
        let Some(hit) = self.cursor_ground_point() else { return };
        let Some(player) = self.player.as_ref() else { return };
        // Archonia refuse une destination dans une cellule bloquée (`FUN_140168070`) et arrête
        // le trajet devant la première rencontrée (`FUN_140196ab0`).
        if self.terrain.blocked(hit.x, hit.z) {
            log::debug!("destination {hit:?} bloquée");
            return;
        }
        let from = player.position;
        let mut hit = if self.terrain.blocked(from.x, from.z) { hit } else { self.terrain.clip_move(from, hit, Self::CHARACTER_RADIUS) };
        if let Some(h) = Self::ground_height(&self.terrain, &self.objects, &self.assets, hit.x, hit.z) {
            hit.y = h;
        }
        if self.skills_defer_move(hit) { return; }
        if let Some(p) = &mut self.player {
            p.destination = hit;
        }
        // Le joueur se déplace : l'attaque en cours et le suivi s'arrêtent.
        self.combat_interrupt();
        self.buddy.follow = None;
        self.walking = true;
        log::debug!("destination {:?} → {:?}", from, hit);
        if let Some(net) = &self.net {
            net.send(Command::MoveTo {
                position: from,
                destination: hit,
            });
        }
    }

    fn frame(&mut self) -> Result<()> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        self.fps = if dt > 0.0 {
            (self.fps * 0.9) + (1.0 / dt as f32) * 0.1
        } else {
            self.fps
        };
        self.poll_net();
        self.main_menu_frame();
        // Fenêtres `UI_EXIT` et `UI_Hotkey`, minuterie des astuces (voir `app_hotkey.rs`, `app_help.rs`).
        self.hotkey_frame();
        self.help_frame();
        // Combat : PV des vues, morts, marche vers un objet à ramasser.
        self.combat_frame();
        // Équipement porté affiché sur le modèle du joueur (voir `app_items.rs`).
        self.items_frame();
        // Approche d'un PNJ, fenêtres fermées en s'éloignant (voir `app_npc.rs`).
        self.npc_frame();
        // Infobulles des objets, fenêtre du forgeron fermée en s'éloignant (voir `app_convert.rs`).
        self.convert_frame();
        // Durabilité : cases, ligne « Repair Cost » des infobulles, réparation fermée en s'éloignant
        // (voir `app_repair.rs`).
        self.repair_frame();
        // Portails des tuiles proches, liste fermée en s'éloignant (voir `app_portal.rs`).
        self.portal_frame();
        self.refinery_frame();
        // Socles de gacha, bande de `UI_Gacha` (voir `app_gacha.rs`).
        self.gacha_window_frame();
        // Compétences lancées depuis `UI_PDSkill`, PNJ de production (voir `app_product.rs`).
        self.product_frame();
        self.guild_frame();
        // Demandes des fenêtres de compétences, barre du personnage (voir `app_skills.rs`).
        self.skills_frame();
        // Bouton du marchand d'objets : op 9 ou refus (voir `app_auction.rs`).
        self.auction_request();
        // Coiffeur : fermeture en s'éloignant, aperçu, caméra ; avis : bouton de la fenêtre de mort, rang.
        self.customize_frame(dt as f32);
        self.wanted_frame();
        // Droits de l'ArchLord ; taux de taxe de la région du joueur, fenêtres de l'intendant fermées en
        // s'éloignant (voir `app_archlord.rs`, `app_tax.rs`).
        self.archlord_frame();
        self.morph_frame();
        self.consumables_frame();
        // Rappel accepté à l'image précédente : boutique, banque et portail fermés (voir `app_party.rs`).
        self.party_recall_close();
        self.tax_frame();
        // Titres : joueur pour les contrôles, `UI_TitleAll` fermée en s'éloignant (voir `app_titles.rs`).
        self.titles_frame();
        // Curseur d'Archonia selon ce qui est sous la souris (voir `app_cursor.rs`).
        self.cursor_update();
        // Carte du monde : zone, carte parente, téléportation des MJ (voir `app_worldmap.rs`).
        self.worldmap_frame();

        // Déplacement clavier (ZQSD / WASD) de la caméra.
        let speed = (self.camera.distance * 0.8 * dt as f32).max(1.0);
        if self.keys[0] {
            self.camera.pan(speed, 0.0);
        }
        if self.keys[1] {
            self.camera.pan(-speed, 0.0);
        }
        if self.keys[2] {
            self.camera.pan(0.0, -speed);
        }
        if self.keys[3] {
            self.camera.pan(0.0, speed);
        }
        self.update_login_scene();
        self.simulate(dt as f32);
        // Montures : modèles, animations, bassin où s'assoit le cavalier (voir `app_ride.rs`).
        self.ride_frame(dt as f32);

        // AddOns : état du monde, OnUpdate et minuteurs, commandes, CVars.
        self.sync_addons_frame();
        self.addons.update(dt);
        self.apply_addon_commands();
        self.apply_addon_cvars();
        // La scène de connexion garde sa caméra fixe (`LoginSettings.txt`), CVars comprises.
        self.reapply_scene_camera();
        let object_distance = self.object_draw_distance();
        // Heure, ciel, brouillard, lumières et météo (`app_nature.rs`), après les CVars.
        self.update_nature(dt as f32);
        // Effets des armes serties (`app_stones.rs`), puis effets d'Archonia : sons et triangles de l'image
        // (voir `app_effects.rs`).
        self.stones_frame();
        self.necklace_frame();
        self.effects_frame();
        // Repère du ciblage au sol (voir `app_groundcast.rs`), ombres rondes et repère de destination,
        // ajoutés aux lots d'effets (voir `app_decals.rs`).
        self.groundcast_frame();
        self.buddy_target_menu_frame();
        self.decals_frame();
        // Ombres projetées des objets sur le sol (voir `app_shadow.rs`).
        self.shadow_frame();
        // Mode de fenêtre, résolution, V-Sync et gamma des options (voir `app_capture.rs`).
        self.display_frame();
        self.update_audio(dt as f32);
        // Packs déposés dans assets/ ou patches/ en cours de jeu (`app_assets.rs`).
        self.watch_packs();
        self.save_options_if_needed(false);
        // Fenêtre du groupe (`app_party.rs`) : membres, PV, cible.
        let party_view = self.party_view();
        // Combat : objets au sol, fenêtre de résurrection.
        let combat_view = self.combat_view();
        // Noms, guildes, chiffres de dégâts et mentions au-dessus des personnages (`app_boards.rs`).
        let boards_view = self.boards_view();
        // Bulles de discussion (`app_balloon.rs`).
        let balloon_view = self.balloon_view();
        // Repères des PNJ de la minicarte (`app_camera.rs`).
        let minimap_icons = self.minimap_icons();
        let loot_items = self.combat_draw_items();
        // Objets portés (fenêtre du personnage, voir `app_items.rs`).
        let equipped = self.items_equipped();
        let player_name = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        // Fenêtres des PNJ (voir `app_npc.rs`).
        let npc_views = self.npc_views();
        // Compétences et barre de raccourcis (voir `app_skills.rs`).
        let skill_views = self.skills_views();
        // Siège : joueur et résurrection de siège (voir `app_siege.rs`).
        let siege_me = self.siege_me();
        // Échange entre joueurs : entrée « Trade » pour le joueur ciblé, refus (voir `app_trade.rs`).
        let trade_frame = {
            let me = self.player.as_ref().map_or(0, |p| p.cid);
            let target = self.addons.target_cid().filter(|c| *c != me && self.entities.iter().any(|e| e.cid == *c && e.is_player));
            self.trade_frame(target)
        };

        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        let raw_input = gpu.egui_state.take_egui_input(&gpu.window);
        let ctx = gpu.egui_state.egui_ctx().clone();
        if let Some(colors) = self.addons.take_ui_colors() {
            self.addon_glue.ui.apply_ui_colors(&ctx, &colors);
        }
        let mut addon_out = crate::addon_ui::AddonUiOutput::default();
        let mut actions = UiActions::default();
        let phase = self.phase;
        let busy = phase == Phase::Connecting;
        let choosing = self.choosing_character;
        // La scène 3D remplace l'image une fois son terrain chargé.
        let scene_ready = self.terrain.loaded_count() >= 9;
        let player_info = self
            .player
            .as_ref()
            .map(|p| (p.name.clone(), p.position.to_array(), p.hp));
        let ents: Vec<(String, [f32; 3], Option<i32>)> = self
            .entities
            .iter()
            .map(|e| (e.name.clone(), e.position.to_array(), e.hp))
            .collect();
        let inventory: Vec<_> = self
            .items
            .values()
            .cloned()
            .map(|item| {
                let name = item
                    .template_id
                    .and_then(|tid| item_name_for_tid(&self.assets, tid))
                    .unwrap_or_else(|| {
                        format!("Objet TID {}", item.template_id.unwrap_or_default())
                    });
                (item, name)
            })
            .collect();
        let login_screen = self
            .login_screen
            .get_or_insert_with(|| ui::load_login_screen(&ctx))
            .clone();
        let mut item_requests = Vec::new();
        let full_output = ctx.run(raw_input, |ctx| {
            if phase != Phase::InWorld {
                // Écran de connexion : image plein écran, panneau de connexion ou de sélection
                // du personnage, console seule.
                let in_scene = phase == Phase::CharacterSelect
                    || phase == Phase::CharacterCreate
                    || (busy && choosing && !self.ui.characters.is_empty());
                if !in_scene || !scene_ready {
                    if let Some(tex) = &login_screen {
                        ui::login_background(ctx, tex);
                    }
                }
                // Boîte « OK » d'Archonia (refus de création…), au-dessus des écrans de connexion.
                if let Some(text) = self.ui.alert.clone() {
                    if crate::hud::info_box(ctx, &mut self.hud, &self.assets, &text) {
                        self.ui.alert = None;
                    }
                }
                if phase == Phase::CharacterCreate {
                    // Écran de création d'Archonia (`UI_LG_CHARCREATE`, voir `hud_login.rs`).
                    let status = self.ui.status.clone();
                    if let Some(c) = self.ui.creation.as_mut() {
                        let mut requests = Vec::new();
                        crate::hud::draw_char_create(ctx, &mut self.hud, &self.assets, c, busy, &status, &mut requests);
                        for r in requests {
                            match r {
                                crate::hud::CreateUi::Submit(tid, name, face, hair) => actions.create_submit = Some((tid, name, face, hair)),
                                crate::hud::CreateUi::Cancel => actions.cancel_creation = true,
                            }
                        }
                    }
                } else if in_scene {
                    // Écran de sélection d'Archonia (`UI_LG_CHARSELECT`, voir `hud_login.rs`).
                    let slots: Vec<crate::hud::CharSlotView> = self.ui.characters.iter().map(|c| crate::hud::CharSlotView { name: c.name.clone(), level: c.level }).collect();
                    let mut requests = Vec::new();
                    let status = self.ui.status.clone();
                    crate::hud::draw_char_select(ctx, &mut self.hud, &self.assets, &slots, self.ui.selected_character, busy, &status, self.ui.delete_confirm.is_some(), &mut requests);
                    let chosen = self.ui.selected_character.and_then(|i| self.ui.characters.get(i)).map(|c| c.name.clone());
                    for r in requests {
                        match r {
                            crate::hud::SelectUi::Select(i) => self.ui.selected_character = Some(i),
                            crate::hud::SelectUi::Enter => actions.enter_character = chosen.clone(),
                            crate::hud::SelectUi::Create => actions.open_creation = true,
                            crate::hud::SelectUi::Delete => self.ui.delete_confirm = chosen.clone(),
                            crate::hud::SelectUi::Back => actions.back_to_login = true,
                            crate::hud::SelectUi::Quit => self.exit = true,
                            crate::hud::SelectUi::ConfirmDelete(yes) => {
                                if yes {
                                    actions.delete_character = self.ui.delete_confirm.clone();
                                }
                                self.ui.delete_confirm = None;
                            }
                        }
                    }
                } else {
                    // Saisie du compte : panneau d'ArchRust.
                    ui::login_panel(ctx, &mut self.ui, busy, &mut actions);
                }
            } else {
                // HUD d'Archonia (barre du bas, minicarte, améliorations), sous les panneaux.
                let target = self.addons.target_cid().and_then(|cid| {
                    self.entities
                        .iter()
                        .find(|e| e.cid == cid)
                        .or(self.player.as_ref().filter(|p| p.cid == cid))
                        .map(|e| crate::hud::HudTarget { cid, name: e.name.clone(), hp: e.hp, is_player_character: e.is_player })
                });
                let hud_scene = crate::hud::HudScene {
                    minimap_icons: minimap_icons.clone(),
                    target,
                    player: self.player.as_ref().map(|p| p.position),
                    camera_forward: {
                        let f = self.camera.target - self.camera.eye();
                        Vec3::new(f.x, 0.0, f.z).normalize_or_zero()
                    },
                };
                // Panneaux d'Archonia au-dessus des personnages, sous le HUD.
                boards_glue::panel(ctx, &mut self.boards, &boards_view, &mut self.hud, &self.assets);
                balloon_glue::panel(ctx, &balloon_view, &mut self.hud, &self.assets);
                crate::hud::draw(ctx, &mut self.hud, &self.assets, &hud_scene);
                // Les panneaux natifs sont des frames pilotables par les AddOns
                // (`ChatFrame1:Hide()`, `ArchRustInfoFrame:Hide()`…).
                if self.addons.builtin_frame("ChatFrame1").0 {
                    let bar = crate::hud::chat_bar(ctx, &mut self.hud, &self.assets, self.ui.whisper_target.is_some());
                    ui::chat_panel(ctx, &mut self.ui, &mut actions, bar);
                }
                // Onglets de la discussion : menus et options (voir `app_chattabs.rs`).
                chattabs_glue::panel(ctx, &mut self.chat_tabs_ui, &player_name, &self.config.saved_dir, &mut self.hud, &self.assets, &mut self.ui);
                // `/fps` : chiffre seul, en haut à gauche (place d'Archonia non relevée).
                if self.ui.show_fps {
                    ctx.debug_painter().text(egui::Pos2::new(4.0, 4.0), egui::Align2::LEFT_TOP, format!("{:.0}", self.fps), egui::FontId::proportional(13.0), egui::Color32::WHITE);
                }
                // Tableau de bord de développement (egui, pas une fenêtre d'Archonia) : seulement avec
                // la console Lua ouverte (touche `).
                if self.ui.console_open && self.addons.builtin_frame("ArchRustInfoFrame").0 {
                    ui::info_panel(
                        ctx,
                        &mut self.ui,
                        self.fps,
                        player_info.as_ref().map(|(n, p, hp)| (n.as_str(), *p, *hp)),
                        &ents,
                        &self.addons,
                        self.terrain.loaded_count(),
                    );
                }
                // Boutons et menus principaux d'Archonia (à la place du bouton egui « Inventaire »).
                if self.addons.builtin_frame("ArchRustInventoryButton").0 {
                    crate::hud::draw_main_menu(ctx, &mut self.hud, &self.assets);
                }
                // Fenêtre d'inventaire d'Archonia (UI_Inventory).
                crate::hud::draw_inventory(ctx, &mut self.hud, &self.assets, &mut self.ui.show_inventory, &inventory, &mut item_requests);
                // Fenêtre du personnage d'Archonia (Character_Status) et demandes sur les objets.
                crate::hud::draw_character(ctx, &mut self.hud, &self.assets, &mut self.item_ui.show_character, &player_name, &equipped, &mut item_requests);
                // Barre de raccourcis avant les fenêtres d'objets : un objet lâché dessus y reste.
                skills_glue::panel(ctx, &mut self.skills, &skill_views, &mut self.hud, &self.assets);
                // Carte du monde (touche M) et clic des MJ sur les cartes (voir `app_worldmap.rs`).
                if phase == Phase::InWorld {
                    worldmap_glue::panel(ctx, &mut self.worldmap, self.player.as_ref().map(|p| p.position), &mut self.hud, &self.assets);
                }
                // Entrepôt de guilde avant la banque : un objet de l'entrepôt lâché dans le sac y est repris.
                // Objet de la grille d'échange lâché dans le sac : repris (voir `app_trade.rs`).
                // Inventaire Cash et sac du familier avant eux : bouton « Chantra Bag », clic droit (`app_petbag.rs`).
                if phase == Phase::InWorld {
                    let me = petbag_glue::Me { world: self.hotkey.world.as_deref(), name: &player_name };
                    petbag_glue::panel(ctx, &mut self.petbag, &mut item_requests, &self.items, &me, &mut self.hud, &self.assets, self.net.as_ref());
                }
                trade_glue::item_requests(&self.trade, &mut item_requests, self.net.as_ref());
                guild_glue::item_requests(&self.guild, &mut item_requests, self.net.as_ref());
                npc_glue::panel(ctx, &mut self.npc, &npc_views, &mut item_requests, &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Fenêtre de rémission du pardonneur (voir `app_pvp.rs`).
                let me = self.player.as_ref().map_or(0, |p| p.cid);
                pvp_glue::remission_panel(ctx, &mut self.pvp, &self.items, me, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Forgeron et pierre d'esprit lâchée sur un objet du sac.
                convert_glue::panel(ctx, &mut self.convert, &mut item_requests, &self.items, &self.auction.item_flags, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Sphère d'alchimie : clic droit sur un objet du sac pour le poser dans la grille.
                refinery_glue::panel(ctx, &mut self.refinery, &mut item_requests, &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Gacha (`UI_Gacha`) et ses messages.
                gacha_glue::panel(ctx, &mut self.gacha, &mut self.hud, &self.assets, self.net.as_ref());
                // Réparation (`UI_Repair`) et silhouette d'usure (`UI_EquipStatus`).
                repair_glue::panel(ctx, &mut self.repair, &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Potions refusées à cheval ou métamorphosé.
                ride_glue::item_requests(&mut self.ride, &self.items, &mut item_requests, &mut self.hud, &self.assets, &mut self.ui);
                // Passeports métamorphosé, objet d'oubli des compétences évolué (voir `app_morph.rs`).
                // Orbe et Gem of origin (refus du mort d'abord, confirmation), voir `app_consumables.rs`.
                let convert_states = &self.convert.states;
                consumables_glue::item_requests(&mut self.consumables, &self.items, &|iid| convert_states.get(&iid).map_or(0, |c| c.filled.len()), &mut item_requests, &mut self.hud, &self.assets, &mut self.ui);
                morph_glue::item_requests(&mut self.morph, &self.items, &self.auction.item_flags, &mut item_requests, &mut self.hud, &self.assets, &mut self.ui);
                // Fabrication, PNJ et compétences de production, barre de la récolte.
                product_glue::panel(ctx, &mut self.product, &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Amis, ignorés, menu d'un joueur ciblé (voir `app_buddy.rs`) ; courrier (voir `app_mail.rs`).
                let player_target = self.addons.target_cid().filter(|c| *c != me).and_then(|cid| self.entities.iter().find(|e| e.cid == cid && e.is_player)).map(|e| (e.cid, e.name.clone()));
                // Entrée « Trade » du menu et fenêtres de l'échange (voir `app_trade.rs`).
                self.buddy.trade_menu = trade_frame.menu;
                buddy_glue::panel(ctx, &mut self.buddy, player_target, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                trade_glue::panel(ctx, &mut self.trade, &trade_frame, self.buddy.trade_chosen.take(), &self.items, (&self.auction.item_flags, &self.petbag.cash), &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                mail_glue::panel(ctx, &mut self.mail, &self.items, &player_name, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Maison des ventes (« Sell » de l'inventaire compris) et commutateur d'équipement.
                let player_pos = self.player.as_ref().map(|p| p.position);
                auction_glue::panel(ctx, &mut self.auction, &mut item_requests, &self.items, &player_name, player_pos, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                let owner = self.player.as_ref().filter(|p| p.cid != 0 && !p.name.is_empty()).map(|p| p.name.clone());
                itemset_glue::panel(ctx, &mut self.itemset, &self.items, owner.as_deref(), &self.config.saved_dir, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                items_glue::panel(ctx, &mut self.item_ui, std::mem::take(&mut item_requests), &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Groupe : bouton, fenêtre des membres, invitations reçues.
                party_glue::panel(ctx, &mut self.party, &party_view, self.net.as_ref(), &mut self.ui, &mut self.hud, &self.assets);
                if let Some(cid) = self.party.pending_select.take() {
                    self.addons.set_target(Some(cid));
                }
                // Actions sociales (`UI_SocialAction`, voir `app_social.rs`).
                social_glue::panel(ctx, &mut self.social, &mut self.hud, &self.assets, self.net.as_ref());
                combat_glue::panel(ctx, &mut self.combat, &combat_view, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Bouton de l'orbe dans `UI_Resurrection`, confirmations de l'oubli des compétences.
                consumables_glue::panel(ctx, &mut self.consumables, &self.items, &mut self.hud, &self.assets, self.net.as_ref());
                // Liste des destinations d'un portail, écran de chargement du trajet.
                portal_glue::panel(ctx, &mut self.portal, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                reconnect_glue::panel(ctx, &mut self.reconnect, &mut self.hud, &self.assets);
                // Fenêtres de guilde et boîte OK / Annuler.
                guild_glue::panel(ctx, &mut self.guild, &mut self.hud, &self.assets, &player_name);
                // Salons de discussion, coiffeur, avis de recherche.
                chat_glue::panel(ctx, &mut self.chat, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui, &player_name);
                customize_glue::panel(ctx, &mut self.customize, &mut self.hud, &self.assets, self.net.as_ref());
                wanted_glue::panel(ctx, &mut self.wanted, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Icônes d'annulation de la métamorphose et de l'évolution (`UI_SystemMessageGrid`).
                morph_glue::panel(ctx, &mut self.morph, self.wanted.alarm_shown(), &mut self.hud, &self.assets);
                // Menu d'une invocation ciblée (`UI_TargetSummonsMenu*`, voir `app_summons.rs`).
                summons_glue::panel(ctx, &mut self.summons, self.addons.target_cid(), me, &mut self.hud, &self.assets, self.net.as_ref());
                // Shrines (`UI_Shrine_info`, jauge, messages du centre) et fenêtre des boss.
                shrine_glue::panel(ctx, &mut self.shrine, &mut self.hud, &self.assets, self.net.as_ref());
                // Titres (`UI_TitleAll`, `UI_TitleCurrent`, `UI_TitleMini`, boîtes).
                titles_glue::panel(ctx, &mut self.titles, &mut self.hud, &self.assets, self.net.as_ref());
                boss_glue::panel(ctx, &mut self.boss, &self.game_clock, &self.config.saved_dir, &mut self.hud, &self.assets, self.net.as_ref());
                // Guerres de siège : fenêtres d'AgcmUISiegeWar, résurrection de siège, `UI_WarInfo`.
                siege_glue::panel(ctx, &mut self.siege, &siege_me, &self.items, &mut self.hud, &self.assets, self.net.as_ref(), &mut self.ui);
                // Annonces de l'ArchLord ; fenêtres de l'intendant (`UI_TaxSteward`, `UI_TaxModify`, `UI_TaxView`).
                archlord_glue::panel(ctx, &mut self.archlord, &mut self.hud, &self.assets);
                tax_glue::panel(ctx, &mut self.tax, &mut self.hud, &self.assets, self.net.as_ref());
                // Aide et astuces (`UI_Help`, `UI_TIPS`) ; sortie et touches (`UI_EXIT`, `UI_Hotkey`) au-dessus.
                help_glue::panel(ctx, &mut self.help, &mut self.hud, &self.assets);
                hotkey_glue::panel(ctx, &mut self.hotkey, &mut self.hud, &self.assets);
                // Comme dans WoW, l'interface des AddOns n'apparaît qu'en jeu.
                self.addon_glue
                    .ui
                    .draw(ctx, &self.addons, &self.assets, &mut addon_out);
            }
            ui::console_panel(ctx, &mut self.ui, &mut actions);
            let status = ui::AudioStatus {
                active: self.audio.is_active(),
                music: self.audio.current_music().map(str::to_string),
                ambience: self.audio.current_ambience().map(str::to_string),
                region: self.zone_region.as_ref().map(|z| {
                    (
                        z.region,
                        Zones::region_name(&self.assets, z.region).unwrap_or_default(),
                    )
                }),
            };
            // AddOns des Options : liste relue à l'ouverture et après chaque changement.
            if self.ui.show_options && self.ui.addon_list.is_empty() {
                let off = archrust_addons::manager::disabled_addons(&self.config.saved_dir);
                self.ui.addon_list = archrust_addons::AddonManager::discover(&self.config.addons_dir)
                    .into_iter()
                    .map(|m| {
                        let on = !off.contains(&m.name.to_ascii_lowercase());
                        (m.name, m.title, on)
                    })
                    .collect();
            }
            let language_before = self.options.language.clone();
            let options_before = self.options.clone();
            let mut pack_action = ui::PackAction::None;
            if ui::options_panel(
                ctx,
                &mut self.ui,
                &mut self.options,
                &status,
                &mut self.packs,
                &mut pack_action,
                &mut self.hud,
                &self.assets,
            ) {
                self.audio.set_settings(&self.options.audio);
                self.options_dirty = true;
                Self::options_changed(&self.options, &options_before, &mut self.terrain, &mut self.objects, self.net.as_ref(), phase == Phase::InWorld);
                if self.options.language != language_before {
                    Self::apply_language_option(
                        self.net.as_ref(),
                        self.options.language.as_deref(),
                        &mut self.ui,
                        phase == Phase::InWorld,
                    );
                }
            }
            // Échap sans fenêtre à fermer (voir `escape_key`).
            if std::mem::take(&mut self.ui.escape_open) {
                if phase == Phase::InWorld {
                    // `EXIT_Open_Window` : `UI_EXIT` (voir `app_hotkey.rs`).
                    self.hotkey.exit = !self.hotkey.exit;
                } else {
                    self.ui.show_options = true;
                }
            }
            // AddOn activé ou désactivé dans les Options : enregistré, puis interface rechargée en jeu.
            if let Some((name, on)) = self.ui.addon_toggle.take() {
                match archrust_addons::manager::set_addon_enabled(&self.config.saved_dir, &name, on) {
                    Ok(()) => {
                        self.ui.addon_list.clear();
                        if phase == Phase::InWorld {
                            actions.chat = Some("/reload".into());
                        } else {
                            self.ui.status = format!("AddOn {name} {} : appliqué à l'entrée en jeu", if on { "activé" } else { "désactivé" });
                        }
                    }
                    Err(e) => log::error!("AddOns.txt : {e}"),
                }
            }
            match pack_action {
                ui::PackAction::None => {}
                ui::PackAction::Changed => match self.packs.settings().save(&self.config.saved_dir) {
                    Ok(()) => self.ui.packs_restart = true,
                    Err(e) => log::warn!("packs.toml : {e}"),
                },
                ui::PackAction::Reset => match archrust_patch::PackSettings::reset(&self.config.saved_dir) {
                    Ok(()) => {
                        self.packs.apply_settings(&archrust_patch::PackSettings::default());
                        self.ui.packs_restart = true;
                    }
                    Err(e) => log::warn!("packs.toml : {e}"),
                },
            }
        });
        let egui_cursor = full_output.platform_output.cursor_icon;
        gpu.egui_state
            .handle_platform_output(&gpu.window, full_output.platform_output);
        self.cursor.apply(&gpu.window, egui_cursor);
        let paint_jobs = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.renderer.config.width, gpu.renderer.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };
        for (id, delta) in &full_output.textures_delta.set {
            gpu.egui_renderer
                .update_texture(&gpu.renderer.device, &gpu.renderer.queue, *id, delta);
        }

        // Scène 3D : terrain autour du joueur (ou de la caméra), puis entités.
        let scene_center = if self.phase == Phase::InWorld {
            None
        } else {
            self.scene_focus
        };
        if self.phase == Phase::InWorld || self.player.is_some() || scene_center.is_some() {
            let center = scene_center.unwrap_or_else(|| {
                self.player
                    .as_ref()
                    .map(|p| p.position)
                    .unwrap_or(self.camera.target)
            });
            self.terrain.update(&mut gpu.renderer, &self.assets, center);
            self.objects.update(&mut gpu.renderer, &self.assets, center);
        }
        let ground = self.terrain.ground_meshes();
        let mut items: Vec<DrawItem> = self.terrain.draw_items();
        items.extend(self.objects.draw_items(self.camera.eye(), object_distance));
        if items.is_empty() && ground.is_empty() {
            items.push(DrawItem {
                mesh: gpu.ground.clone(),
                model: Mat4::from_translation(Vec3::new(
                    self.camera.target.x,
                    0.0,
                    self.camera.target.z,
                )),
                tint: [1.0; 4],
                character: false,
            });
        }
        // L'aperçu (`--preview`) est posé à l'origine, comme le personnage local du mode sans client.
        for (mesh, local) in &self.preview {
            items.push(DrawItem {
                mesh: mesh.clone(),
                model: *local,
                tint: [1.0; 4],
                character: true,
            });
        }
        if let Some(p) = &self.player {
            // À cheval, le cavalier est posé sur sa monture (voir `app_ride.rs`).
            let base = self.ride.rider_matrix(p.cid, p.model_matrix());
            // Objets en main : nœud 101/102 de la pose courante (voir `worn`).
            let (riding, animator) = (self.ride.riding(p.cid), self.player_animator.as_ref().and_then(|(_, a)| a.as_ref()));
            for (mesh, local) in &p.meshes {
                let Some(model) = crate::worn::piece_model(mesh, base, *local, riding, animator) else { continue };
                items.push(DrawItem {
                    mesh: mesh.clone(),
                    model,
                    tint: self.addons.unit_tint(p.cid).unwrap_or([1.0; 4]),
                    character: true,
                });
            }
        }
        // Personnages au-delà de la distance `CharView` des options : non tracés.
        let char_distance = self.options.video.char_distance();
        let char_eye = self.camera.eye();
        for e in &self.entities {
            if char_distance.is_some_and(|d| e.position.distance(char_eye) > d) {
                continue;
            }
            let base = self.ride.rider_matrix(e.cid, e.model_matrix());
            let (riding, animator) = (self.ride.riding(e.cid), self.animators.get(&e.cid).and_then(|(_, a)| a.as_ref()));
            for (mesh, local) in &e.meshes {
                let Some(model) = crate::worn::piece_model(mesh, base, *local, riding, animator) else { continue };
                items.push(DrawItem {
                    mesh: mesh.clone(),
                    model,
                    tint: self.addons.unit_tint(e.cid).unwrap_or([1.0; 4]),
                    character: true,
                });
            }
        }
        // Montures (personnages locaux d'AgcmRide).
        items.extend(self.ride.draw_items(self.entities.iter().chain(self.player.as_ref()), char_distance, char_eye));
        items.extend(loot_items);
        let view_proj = self.camera.render_view_proj(gpu.renderer.aspect());
        // Pièces hors du champ de la caméra : ni envoyées ni dessinées (voir `render_frustum.rs`).
        let frustum = crate::render::Frustum::new(view_proj);
        items.retain(|item| frustum.keeps(item));
        let eye = self.camera.eye();
        if let Some((_, after)) = &self.config.screenshot {
            if !self.screenshot_done && self.started.elapsed().as_secs_f32() >= *after {
                gpu.renderer.capture_next = true;
            }
        }
        let egui_renderer = &mut gpu.egui_renderer;
        gpu.renderer.render(
            view_proj,
            eye,
            &ground,
            &items,
            |device, queue, encoder, view| {
                egui_renderer.update_buffers(device, queue, encoder, &paint_jobs, &screen);
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let mut pass = pass.forget_lifetime();
                egui_renderer.render(&mut pass, &paint_jobs, &screen);
            },
        )?;
        for id in &full_output.textures_delta.free {
            gpu.egui_renderer.free_texture(id);
        }
        if let Some((w, h, rgba)) = gpu.renderer.take_capture() {
            if let Some(path) = self.addon_glue.ui.screenshot.take() {
                // `Screenshot()` d'un AddOn : le client continue.
                match image::RgbaImage::from_raw(w, h, rgba).map(|img| img.save(&path)) {
                    Some(Ok(())) => self
                        .ui
                        .push_chat(format!("[système] capture : {}", path.display())),
                    other => log::error!("capture : {other:?}"),
                }
            } else if let Some((path, _)) = &self.config.screenshot {
                match image::RgbaImage::from_raw(w, h, rgba).map(|img| img.save(path)) {
                    Some(Ok(())) => log::info!(
                        "capture enregistrée : {} ({w}x{h}, {} secteurs, {} entités)",
                        path.display(),
                        self.terrain.loaded_count(),
                        self.entities.len()
                    ),
                    other => log::error!("capture : {other:?}"),
                }
                self.screenshot_done = true;
                self.exit = true;
            }
        }
        self.after_addon_ui(addon_out);
        self.handle_actions(actions);
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_none() {
            if let Err(e) = self.init_gpu(event_loop) {
                log::error!("initialisation graphique : {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let consumed = if let Some(gpu) = &mut self.gpu {
            gpu.egui_state.on_window_event(&gpu.window, &event).consumed
        } else {
            false
        };
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_addons();
                self.save_options_if_needed(true);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                // Curseurs d'Archonia, créés une fois (voir `app_cursor.rs`).
                self.cursor_load(event_loop);
                if let Err(e) = self.frame() {
                    log::error!("rendu : {e:#}");
                }
                if self.exit {
                    self.shutdown_addons();
                    self.save_options_if_needed(true);
                    event_loop.exit();
                }
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if !consumed || state == ElementState::Released =>
            {
                // Un clic sur une frame d'AddOn ne traverse pas vers le monde.
                let on_addon = self.addon_glue.pointer_captured;
                match button {
                    MouseButton::Right => {
                        self.right_drag = state == ElementState::Pressed && !on_addon;
                        // Clic droit sans glisser : attaque ou ramassage (voir `app_combat.rs`).
                        if self.phase == Phase::InWorld && (!on_addon || state == ElementState::Released) {
                            self.combat_right_button(state == ElementState::Pressed);
                        }
                    }
                    MouseButton::Middle => {
                        self.middle_drag = state == ElementState::Pressed && !on_addon
                    }
                    MouseButton::Left
                        if state == ElementState::Pressed
                            && self.phase == Phase::InWorld
                            && !on_addon =>
                    {
                        self.click_ground()
                    }
                    // Relâché sur le sol : compétence au sol (voir `app_groundcast.rs`).
                    MouseButton::Left
                        if state == ElementState::Released
                            && self.phase == Phase::InWorld
                            && !consumed
                            && !on_addon =>
                    {
                        self.groundcast_release()
                    }
                    MouseButton::Left
                        if state == ElementState::Pressed
                            && self.phase == Phase::CharacterSelect =>
                    {
                        self.click_scene_character()
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((lx, ly)) = self.last_cursor {
                    let (dx, dy) = ((position.x - lx) as f32, (position.y - ly) as f32);
                    if self.right_drag {
                        self.camera.rotate(dx, dy);
                    } else if self.middle_drag {
                        self.cam.panned = true;
                        self.camera.pan(
                            dy * self.camera.distance * 0.002,
                            -dx * self.camera.distance * 0.002,
                        );
                    }
                }
                self.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. }
                if !consumed && !self.addon_glue.pointer_captured =>
            {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.zoom(d);
            }
            WindowEvent::ModifiersChanged(m) => self.addon_glue.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if !consumed => {
                let down = event.state == ElementState::Pressed;
                // Raccourcis et OnKeyDown des AddOns (`ArchRust.SetBinding`) d'abord.
                if let PhysicalKey::Code(code) = event.physical_key {
                    if !event.repeat && self.addon_key(code, down) {
                        return;
                    }
                }
                // Table des touches d'Archonia (`[EventMap]` d'`UI_1024X768.ini`, `Ini/UI_Hotkey.ini`, voir
                // `app_hotkey.rs`) : saisie d'`UI_Hotkey`, déplacements et fonctions branchées.
                if let PhysicalKey::Code(code) = event.physical_key {
                    if self.hotkey_key(code, down, event.repeat) {
                        return;
                    }
                    // Touches de caméra d'Archonia (flèches, pavé − / +, F10 / F11 / F12), voir `app_camera.rs`.
                    if self.camera_key(code, down) {
                        return;
                    }
                }
                // Touches fixes (`HotkeyFlag=0`) et commandes propres à ArchRust, hors de la table.
                match event.physical_key {
                    // Impr. écran : capture `screen\SC%y%m%d-%H%M%S.bmp` (voir `app_capture.rs`) ; Windows
                    // n'envoie que le relâchement de cette touche.
                    PhysicalKey::Code(KeyCode::PrintScreen) if !down => self.capture_request(),
                    // Y : actions sociales (`Hotkey=0:21` « Emotes », `CBSocialActionToggleUI`, voir `app_social.rs`).
                    PhysicalKey::Code(KeyCode::KeyY) if down && !event.repeat && self.phase == Phase::InWorld => self.social.toggle(),
                    // Ctrl+M : `UI_SkillMacro` (`Hotkey=2:50`, ajout d'Archonia), ici le commutateur d'équipement (voir `app_itemset.rs`).
                    PhysicalKey::Code(KeyCode::KeyM) if down && !event.repeat && self.phase == Phase::InWorld && self.addon_glue.modifiers.control_key() => self.itemset.toggle(),
                    // Maj+` : console d'ArchRust (` est « Switch quick slot up », `Hotkey=0:41` ; `1:41` est libre).
                    PhysicalKey::Code(KeyCode::Backquote) if down && !event.repeat && self.addon_glue.modifiers.shift_key() => {
                        self.ui.console_open = !self.ui.console_open
                    }
                    // Espace : ramasser (commande d'ArchRust ; « Collect item » d'Archonia est écarté) — `app_combat.rs`.
                    PhysicalKey::Code(code @ KeyCode::Space) if down && !event.repeat && self.phase == Phase::InWorld => {
                        self.combat_key(code);
                    }
                    // Échap (`Hotkey=0:1`) : `Close_UI_Window` puis `EXIT_Open_Window` (voir `escape_key`).
                    PhysicalKey::Code(KeyCode::Escape) if down && !event.repeat => self.escape_key(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}

/// Lance le client.
pub fn run(config: Config) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(config)?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

