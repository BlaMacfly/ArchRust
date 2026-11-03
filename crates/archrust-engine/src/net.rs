//! Liaison réseau : fait tourner `archrust-netclient` sur un fil tokio et échange avec la boucle
//! de rendu par canaux.
//!
//! Ordre d'Archonia : serveur de connexion joint (liste des mondes, clé de hachage) → écran
//! `UI_SelectServer` → « Connect » sur un monde (`LoginCommand::SelectWorld`) → compte, monde,
//! personnages → sélection et création → entrée dans le monde. Écran de connexion d'ArchRust (par
//! défaut) : même ordre, la liste des mondes n'apparaît que si le serveur en annonce plusieurs, sinon
//! premier monde ; `--connect` : toujours le premier monde (voir [`WorldChoice`]).
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

/// Choix du monde après la liste du serveur de connexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorldChoice {
    /// Premier monde annoncé, sans écran (`--connect`).
    #[default]
    First,
    /// Écran `UI_SelectServer` dans tous les cas (fenêtres de connexion d'Archonia).
    List,
    /// Écran `UI_SelectServer` si le serveur annonce plusieurs mondes, sinon premier monde (écran de
    /// connexion d'ArchRust).
    ListIfSeveral,
}

impl WorldChoice {
    /// La liste des mondes s'affiche-t-elle (la liaison attend alors `LoginCommand::SelectWorld`) ?
    pub fn lists(self, worlds: usize) -> bool {
        match self {
            WorldChoice::First => false,
            WorldChoice::List => true,
            WorldChoice::ListIfSeveral => worlds > 1,
        }
    }
}

/// Identifiants saisis à l'écran de connexion (panneau d'ArchRust ou `UI_LG_IDPASSWORD`).
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
    /// Monde : quand la liste s'affiche (voir [`WorldChoice::lists`]), la liaison envoie la liste des
    /// mondes et attend `LoginCommand::SelectWorld` avant d'authentifier le compte ; sinon premier monde.
    pub world_choice: WorldChoice,
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
    /// « Connect » sur un monde de `UI_SelectServer` (`LG_SelectServer`) : authentification du compte
    /// puis choix du monde.
    SelectWorld(String),
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
    /// Mondes annoncés par le serveur de connexion (`Login_Servers` d'`UI_SelectServer`).
    Worlds(Vec<WorldInfo>),
    /// Compte refusé par le serveur (`LG_IDPASSWORD_FAILD`) : la liaison s'arrête.
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
    // Écran `UI_SelectServer` : rien n'est envoyé du compte avant « Connect » sur un monde.
    let world = if creds.world_choice.lists(login.worlds.len()) {
        loop {
            match choice_rx.recv().await.ok_or_else(|| anyhow::anyhow!("connexion annulée"))? {
                LoginCommand::SelectWorld(name) => break name,
                other => log::debug!("choix ignoré avant celui du monde : {other:?}"),
            }
        }
    } else {
        login.worlds.first().map(|w| w.name.clone()).unwrap_or_else(|| "ArchCore".into())
    };
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
                    LoginCommand::SelectWorld(_) => {}
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

#[cfg(test)]
mod tests {
    use super::WorldChoice;

    #[test]
    fn liste_des_mondes() {
        // `--connect` : premier monde ; fenêtres d'Archonia : toujours la liste ; panneau d'ArchRust :
        // la liste dès deux mondes.
        assert!(!WorldChoice::First.lists(3));
        assert!(WorldChoice::List.lists(1));
        assert!(!WorldChoice::ListIfSeveral.lists(1));
        assert!(!WorldChoice::ListIfSeveral.lists(0));
        assert!(WorldChoice::ListIfSeveral.lists(2));
    }
}
