//! Liaison réseau : fait tourner `archrust-netclient` sur un fil tokio et échange avec la boucle
//! de rendu par canaux.
//!
//! Ordre : serveur de connexion joint (liste des mondes, clé de hachage) → compte, premier monde
//! annoncé, personnages → sélection et création → entrée dans le monde (panneau de connexion
//! d'ArchRust comme `--connect`).
//! Retour à la sélection des personnages (« Select Character » d'`UI_EXIT`, AgpmReturnToLogin) : la boucle de
//! jeu rend la clé de l'op 5, la liaison rejoint le serveur de connexion (`LoginClient::return_to_selection`)
//! et reprend aux écrans de sélection et de création, sans relancer le client.

use archrust_netclient::login::WorldInfo;
use archrust_netclient::{
    CharacterSummary, Command, LoginClient, NetError, WorldClient, WorldEvent,
};
use archrust_netclient::world::SessionEnd;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;

/// Identifiants saisis au panneau de connexion d'ArchRust (ou donnés par `--connect`).
#[derive(Debug, Clone)]
pub struct Credentials {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// Personnage à jouer directement (`--connect …/Nom`) ; `None` = choix à l'écran de
    /// sélection, après la liste des personnages du compte.
    pub character: Option<String>,
    pub create_if_missing: bool,
    /// Langue des textes du serveur (Options), annoncée au `HELLO` des deux connexions.
    pub language: Option<String>,
}

/// Numéro de demande des `SET_LANGUAGE` envoyés depuis les Options ; les AddOns emploient les petits
/// numéros (file de `archrust-addons`), ce qui évite toute confusion dans les `RESULT`.
pub const LANGUAGE_REQUEST_ID: u32 = 0x4000_0001;

/// Commandes des écrans de sélection et de création, comme les boutons d'Archonia
/// (`LG_LoginMainSelectButtonClick`, `…CreateButtonClick`, `…DeleteButtonClick`,
/// `LG_CharacterCreateOK`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginCommand {
    /// Entrer dans le monde avec ce personnage du compte.
    Enter(String),
    /// Ouvrir l'écran de création : le serveur envoie ses modèles de base.
    OpenCreation,
    /// Créer un personnage depuis un modèle de base.
    Create {
        tid: i32,
        name: String,
        face: i32,
        hair: i32,
    },
    /// Supprimer un personnage du compte.
    Delete(String),
}

/// Événements de la liaison vers le moteur.
#[derive(Debug)]
pub enum NetEvent {
    Status(String),
    Error(String),
    /// Mondes annoncés par le serveur de connexion (le premier est joint).
    Worlds(Vec<WorldInfo>),
    /// Compte refusé par le serveur : la liaison s'arrête.
    AuthRefused(String),
    Characters(Vec<CharacterSummary>),
    /// Modèles de base de l'écran de création.
    CreationTemplates(Vec<CharacterSummary>),
    CharacterCreated(CharacterSummary),
    CharacterDeleted(String),
    /// Refus du serveur sans fin de connexion (nom pris, création impossible…).
    Refused(String),
    /// Création refusée par le serveur : nom demandé et code `LOGIN_RESULT`.
    CreateRefused { name: String, code: i64 },
    World(WorldEvent),
}

pub struct NetLink {
    pub events: std_mpsc::Receiver<NetEvent>,
    pub commands: mpsc::Sender<Command>,
    choices: mpsc::Sender<LoginCommand>,
    _thread: std::thread::JoinHandle<()>,
}

impl NetLink {
    /// Lance la connexion complète (login puis monde) sur un fil dédié.
    pub fn start(creds: Credentials) -> Self {
        let (ev_tx, ev_rx) = std_mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(64);
        let (choice_tx, choice_rx) = mpsc::channel::<LoginCommand>(8);
        let thread = std::thread::Builder::new()
            .name("archrust-net".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ev_tx.send(NetEvent::Error(format!("tokio : {e}")));
                        return;
                    }
                };
                rt.block_on(async move {
                    if let Err(e) = session(creds, &ev_tx, cmd_rx, choice_rx).await {
                        let _ = ev_tx.send(NetEvent::Error(e.to_string()));
                    }
                });
            })
            .expect("fil réseau");
        NetLink {
            events: ev_rx,
            commands: cmd_tx,
            choices: choice_tx,
            _thread: thread,
        }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.commands.try_send(cmd);
    }

    /// Transmet une commande des écrans de sélection et de création.
    pub fn choose(&self, choice: LoginCommand) {
        let _ = self.choices.try_send(choice);
    }
}

async fn session(
    creds: Credentials,
    ev: &std_mpsc::Sender<NetEvent>,
    cmd_rx: mpsc::Receiver<Command>,
    mut choice_rx: mpsc::Receiver<LoginCommand>,
) -> anyhow::Result<()> {
    let status = |s: String| {
        let _ = ev.send(NetEvent::Status(s));
    };
    status(format!("connexion à {}:{}…", creds.host, creds.port));
    let mut login = LoginClient::connect_with(&creds.host, creds.port, creds.language.as_deref()).await?;
    let _ = ev.send(NetEvent::Worlds(login.worlds.clone()));
    // Premier monde annoncé.
    let world = login.worlds.first().map(|w| w.name.clone()).unwrap_or_else(|| "ArchCore".into());
    status("authentification…".into());
    match login.authenticate(&creds.user, &creds.password).await {
        Ok(()) => {}
        Err(NetError::Server(msg)) => {
            let _ = ev.send(NetEvent::AuthRefused(msg));
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    login.select_world(&world).await?;
    let chars = login.list_characters().await?;
    let _ = ev.send(NetEvent::Characters(chars));
    let mut cmd_rx = cmd_rx;
    // Entrée directe (`--connect …/Nom`) la première fois seulement : après un retour à la sélection des
    // personnages, le choix se fait à l'écran.
    let mut direct = creds.character.clone();
    loop {
        let known = |login: &LoginClient, name: &str| login.names.iter().any(|n| n.eq_ignore_ascii_case(name));
        let character = match direct.take() {
            // Entrée directe (`--connect`) : création (humain, TID 96) si le personnage manque.
            Some(name) if known(&login, &name) => name,
            Some(name) if creds.create_if_missing => {
                status(format!("création de {name}…"));
                login.enter_creation().await?;
                login.create_character(&name, 96, 1, 2).await?;
                name
            }
            Some(name) => anyhow::bail!("personnage {name} inconnu sur ce compte"),
            // Écrans de sélection et de création : on suit les commandes jusqu'à l'entrée en jeu.
            None => loop {
                let cmd = choice_rx
                    .recv()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("connexion annulée"))?;
                match cmd {
                    LoginCommand::Enter(name) => break name,
                    LoginCommand::OpenCreation => {
                        let bases = login.enter_creation().await?;
                        let _ = ev.send(NetEvent::CreationTemplates(bases));
                    }
                    LoginCommand::Create {
                        tid,
                        name,
                        face,
                        hair,
                    } => match login.create_character(&name, tid, hair, face).await {
                        Ok(created) => {
                            let _ = ev.send(NetEvent::CharacterCreated(created));
                        }
                        Err(NetError::LoginResult(code)) => {
                            let _ = ev.send(NetEvent::CreateRefused { name, code });
                        }
                        Err(NetError::Server(msg)) => {
                            let _ = ev.send(NetEvent::Refused(format!("{name} : {msg}")));
                        }
                        Err(e) => return Err(e.into()),
                    },
                    LoginCommand::Delete(name) => {
                        login.delete_character(&name).await?;
                        let _ = ev.send(NetEvent::CharacterDeleted(name));
                    }
                }
            },
        };
        status("entrée dans le monde…".into());
        let ticket = login.enter_world(&character, &world).await?;
        login.close().await;
        // Commandes laissées par la partie précédente (retour à la sélection) : sans objet dans la nouvelle.
        while cmd_rx.try_recv().is_ok() {}
        let mut wc = WorldClient::connect(ticket).await?;
        // Identifiants renvoyés par une reconnexion automatique (AgcmReconnect).
        wc.account = creds.user.clone();
        wc.password = creds.password.clone();
        for e in wc.enter().await? {
            let _ = ev.send(NetEvent::World(e));
        }
        status("en jeu".into());
        let (wtx, mut wrx) = mpsc::channel::<WorldEvent>(256);
        let forward = {
            let ev = ev.clone();
            async move {
                while let Some(e) = wrx.recv().await {
                    if ev.send(NetEvent::World(e)).is_err() {
                        break;
                    }
                }
            }
        };
        let ((end, commands), ()) = tokio::join!(wc.run_until(wtx, cmd_rx), forward);
        cmd_rx = commands;
        // Retour à la sélection des personnages (AgpmReturnToLogin op 5) : serveur de connexion de l'op 5, ou
        // celui du départ ; op 6, clé de hachage, compte, op 7, puis liste des personnages (`return_login`).
        let SessionEnd::ReturnToLogin(back) = end else {
            return Ok(());
        };
        status("retour à la sélection des personnages…".into());
        let (host, port) = back.address.clone().unwrap_or_else(|| (creds.host.clone(), creds.port));
        let (again, chars) = LoginClient::return_to_selection(
            &host,
            port,
            creds.language.as_deref(),
            &creds.user,
            &creds.password,
            back.key,
            &world,
        )
        .await?;
        login = again;
        let _ = ev.send(NetEvent::Characters(chars));
    }
}
