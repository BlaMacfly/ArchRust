//! Groupes côté moteur : état tenu depuis les paquets 0x08 / 0x09 (`archrust_netclient::party`),
//! messages dans la discussion, commandes `/inviter`…, fenêtre du groupe et fenêtre d'invitation.
//!
//! Commandes (alias anglais entre parenthèses) :
//! `/inviter [nom]` (`/invite`, sans nom : la cible), `/accepter` (`/accept`), `/refuser`
//! (`/decline`), `/quittergroupe` (`/leave`), `/exclure nom` (`/kick`, `/uninvite`), `/chef nom`
//! (`/promote`), `/groupe` (`/party`) pour afficher la fenêtre.
//!
//! Rappel de groupe (Summoning Orb, usage 15) : op 15 reçue d'un membre du groupe → `UI_ConfirmRecall`
//! (`PartyOpenRequestRecallUI`, son `U_WIN_SE2.wav`, texte `Party_Request_Recall` d'uimessage avec le nom du
//! lanceur, FUN_1405e3710). « Accept » (`CONTROL_24138`, FUN_1405e2db0) ferme l'échange, la boutique, la banque et
//! les portails, envoie l'annulation d'échange (0x07 op 5) puis l'op 16, et affiche `PartyAcceptRecall` ;
//! « Ignore » (`CONTROL_32003`, FUN_1405e4750) envoie l'op 17 et affiche `PartyRejectRecall`. Le lanceur reçoit
//! l'op 16 ou 17 (champ 1 = membre) : `PartyTargetAcceptRecall` / `PartyTargetRejectRecall` ; l'op 4 d'AGPMITEM
//! de l'orbe affiche `PartyUseRecallSuccess` (voir `app_consumables.rs`).

use super::App;
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::party::{
    PartyCommand, PartyNotice, PartyState, EXP_BY_DAMAGE, EXP_BY_LEVEL, ITEM_DAMAGE,
    ITEM_FREE, ITEM_SEQUENCE,
};
use archrust_netclient::{Command, WorldEvent};
use std::collections::HashMap;

/// PV et niveau connus d'un personnage (fiches et mises à jour CHARACTER, actions 0x41).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemberStats {
    pub level: Option<i32>,
    pub hp: Option<i32>,
    pub max_hp: Option<i32>,
}

#[derive(Default)]
pub struct PartyGlue {
    pub state: PartyState,
    /// Boîte d'information (`Action=4:0` des événements `PartyRecv*`).
    info: Option<String>,
    /// Messages des événements d'interface (`[EventMap]`).
    events: Option<HashMap<String, String>>,
    /// Menus et options des fenêtres de groupe d'Archonia.
    pub frames_ui: crate::hud::PartyUiState,
    /// Cadre cliqué à cibler (lu par le moteur après `panel`).
    pub pending_select: Option<i32>,
    /// Caractéristiques connues par CID (membres ou non : un joueur peut le devenir).
    pub stats: HashMap<i32, MemberStats>,
    /// Rappel accepté : fenêtres d'échange, de boutique, de banque et de portail à fermer (lu par le moteur).
    pub recall_close_windows: bool,
}

/// Envoie une demande de groupe et l'annonce dans la discussion. Fonction libre : appelée depuis la
/// fermeture egui, qui n'emprunte que certains champs de l'application.
fn send(net: Option<&NetLink>, glue: &mut PartyGlue, ui: &mut UiState, cmd: PartyCommand) {
    let _ = ui;
    let Some(net) = net else {
        log::info!("[groupe] hors ligne : {cmd:?} non envoyé");
        return;
    };
    // Archonia n'annonce pas ses propres demandes.
    if let PartyCommand::Accept(c) | PartyCommand::Reject(c) = &cmd {
        glue.state.take_invitation(*c);
    }
    net.send(Command::Party(cmd));
}

impl App {
    /// Noms et PV des personnages, relevés dans tous les événements (avant leur traitement).
    pub(super) fn party_observe(&mut self, ev: &WorldEvent) {
        let concerned = match ev {
            WorldEvent::SelfCharacter { cid, .. } | WorldEvent::CharacterUpdated { cid, .. } => Some(*cid),
            WorldEvent::CharacterAction { target_cid, .. } => Some(*target_cid),
            _ => None,
        };
        self.party_learn(ev);
        // PV d'un membre : les AddOns voient tout de suite le changement (`UnitHealth("party1")`).
        if concerned.is_some_and(|c| self.party.state.contains(c)) {
            self.party_addons_sync();
        }
    }

    fn party_learn(&mut self, ev: &WorldEvent) {
        let glue = &mut self.party;
        match ev {
            WorldEvent::SelfCharacter { cid, name, stats, .. } => {
                glue.state.learn_name(*cid, name);
                if let Some(s) = stats {
                    let e = glue.stats.entry(*cid).or_default();
                    e.level = s.level.or(e.level);
                    e.hp = s.hp.or(e.hp);
                    e.max_hp = s.max_hp.or(e.max_hp);
                }
            }
            WorldEvent::CharacterUpdated { cid, factors, stats, .. } => {
                let hp = stats.as_ref().and_then(|s| s.hp).or(factors.as_ref().and_then(|f| f.hp));
                if hp.is_some() || stats.is_some() {
                    let e = glue.stats.entry(*cid).or_default();
                    e.hp = hp.or(e.hp);
                    if let Some(s) = stats {
                        e.level = s.level.or(e.level);
                        e.max_hp = s.max_hp.or(e.max_hp);
                    }
                }
            }
            WorldEvent::CharacterSeen(c) => glue.state.learn_name(c.cid, &c.name),
            WorldEvent::CharacterAction { target_cid, target_hp, factors, .. } => {
                if let Some(hp) = factors.as_ref().and_then(|f| f.hp).or(*target_hp) {
                    if let Some(e) = glue.stats.get_mut(target_cid) {
                        e.hp = Some(hp);
                    }
                }
            }
            _ => {}
        }
    }

    /// Paquets 0x08 / 0x09 : état, message, événements des AddOns.
    pub(super) fn on_party_event(&mut self, ev: WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let notice = match &ev {
            WorldEvent::Party(p) => {
                log::debug!("[groupe] 0x08 {p:?}");
                self.party.state.apply_party(p, me)
            }
            WorldEvent::PartyManagement(m) => {
                log::debug!("[groupe] 0x09 {m:?}");
                self.party.state.apply_management(m)
            }
            _ => None,
        };
        let Some(notice) = notice else { return };
        // Invitations de groupe bloquées (onglet Fonction des options) : déclinée aussitôt, sans
        // fenêtre, en attendant que le serveur applique lui-même le drapeau 2 (comme pour Archonia).
        if let PartyNotice::Invited(i) = &notice {
            if self.options.refuse.party {
                log::info!("[groupe] invitation de {} déclinée (invitations de groupe bloquées)", i.name);
                let cid = i.cid;
                send(self.net.as_ref(), &mut self.party, &mut self.ui, PartyCommand::Reject(cid));
                self.party_addons_sync();
                return;
            }
        }
        log::info!("[groupe] {}", notice.message(&self.party.state));
        // Messages de l'[EventMap] d'Archonia : boîtes `PartyRecv*` (`Action=4:0`), lignes système
        // `PartyChangeLeader`, `PartyOptionUI_RecvInviteNoLoginMember` (`Action=4:3`) ; `Option_Party_Off_User`
        // d'uimessage. Arrivée, départ d'un membre et changements de répartition : Archonia n'écrit rien
        // (aucun événement ni clé uimessage ; seuls les bonus `Party_Add_*`, non calculés ici).
        let assets = self.assets.clone();
        let mut event = |key: &str| super::portal_glue::ui_event_cached(&mut self.party.events, &assets, key);
        let (boxed, text) = match &notice {
            PartyNotice::Left => (true, event("PartyRecvLeave")),
            PartyNotice::Banished => (true, event("PartyRecvBanish")),
            PartyNotice::Rejected(_) => (true, event("PartyRecvReject")),
            PartyNotice::Accepted(_) => (true, event("PartyRecvInviteAccept")),
            PartyNotice::LeaderChanged(c) => {
                let name = self.party.state.name_of(*c);
                (false, super::guild_glue::fill_braces(&event("PartyChangeLeader"), &name))
            }
            // Réponses au rappel, chez le lanceur (`Action=4:3`, %name = `PartyRecallTargetName`).
            PartyNotice::RecallAccepted(c) | PartyNotice::RecallRejected(c) => {
                let key = if matches!(notice, PartyNotice::RecallAccepted(_)) { "PartyTargetAcceptRecall" } else { "PartyTargetRejectRecall" };
                let name = self.party.state.name_of(*c);
                (false, super::guild_glue::fill_braces(&event(key), &name))
            }
            PartyNotice::Failed { code, name } => match *code {
                archrust_netclient::party::mgmt::FAILED_OTHER_PARTY => (true, event("PartyRecvAlreadyOtherMember")),
                archrust_netclient::party::mgmt::FAILED_ALREADY_MEMBER => (true, event("PartyRecvAlreadyPartyMember")),
                archrust_netclient::party::mgmt::FAILED_FULL => (true, event("PartyRecvMemberFull")),
                archrust_netclient::party::mgmt::FAILED_NOT_LEADER => (true, event("PartyRecvNotLeader")),
                archrust_netclient::party::mgmt::FAILED_NO_LOGIN => (false, super::guild_glue::fill_braces(&event("PartyOptionUI_RecvInviteNoLoginMember"), name)),
                // FUN_1405e45f0 : uimessage `Option_Party_Off_User` (« %s bloque… ») en ligne système.
                archrust_netclient::party::mgmt::FAILED_REFUSE => (false, crate::hud::ui_message(&mut self.hud, &assets, "Option_Party_Off_User").replacen("%s", name, 1)),
                _ => (true, event("PartyRecvOperationFailed")),
            },
            _ => (false, String::new()),
        };
        if boxed {
            self.party.info = Some(text).filter(|t| !t.is_empty());
        } else {
            self.hud.push_system(&text);
        }
        // Groupe créé ou rejoint (FUN_1405e30a0) : uimessage `Party_Create_Message` puis `Party_Create_Message_2`.
        if let PartyNotice::Joined = &notice {
            for key in ["Party_Create_Message", "Party_Create_Message_2"] {
                let line = crate::hud::ui_message(&mut self.hud, &self.assets, key);
                self.hud.push_system(&line);
            }
        }
        // `PartyOpenRequestRecallUI` : `UI_ConfirmRecall` (dessinée par `panel` tant que la demande attend) et son son.
        if let PartyNotice::RecallRequested(_) = &notice {
            self.audio.play_effect("sound/ui/U_WIN_SE2.wav");
        }
        self.party_addons_sync();
        match &notice {
            PartyNotice::Joined => self.addons.fire_event("GROUP_JOINED", &[]),
            // L'exclusion (gestion op 4) suit toujours le retrait du groupe (op 4 / op 1) : `Left`.
            PartyNotice::Left => self.addons.fire_event("GROUP_LEFT", &[]),
            PartyNotice::LeaderChanged(_) => self.addons.fire_event("PARTY_LEADER_CHANGED", &[]),
            PartyNotice::Invited(i) => self.addons.fire_event_str("PARTY_INVITE_REQUEST", &[&i.name]),
            PartyNotice::ExpTypeChanged(_) | PartyNotice::ItemDivisionChanged(_) => {
                self.addons.fire_event("PARTY_LOOT_METHOD_CHANGED", &[])
            }
            _ => {}
        }
    }

    /// Groupe vu par les AddOns : membres (noms, PV) et invitations en attente.
    pub(super) fn party_addons_sync(&mut self) {
        let view = self.party_view();
        let members = view
            .members
            .into_iter()
            .map(|(cid, name, s)| archrust_addons::PartyMember {
                cid,
                name,
                level: s.level.map(i64::from),
                hp: s.hp.map(i64::from),
                hp_max: s.max_hp.map(i64::from),
            })
            .collect();
        let invitations = self.party.state.invitations.iter().map(|i| i.name.clone()).collect();
        self.addons.set_party(members, invitations);
    }

    /// Rappel accepté (`CONTROL_24138` d'`UI_ConfirmRecall`) : `UI_PrivateTrade`, `UI_PrivateTradeYesNo`,
    /// `UI_NPCTrade`, `UI_Teleport` et `UI_Bank` fermées comme dans Archonia (l'échange se termine par l'op 5 du
    /// serveur qui répond à l'annulation envoyée).
    pub(super) fn party_recall_close(&mut self) {
        if std::mem::take(&mut self.party.recall_close_windows) {
            self.npc.close_windows();
            self.portal_close_list();
        }
    }

    /// Déconnexion : le serveur retire le joueur de son groupe.
    pub(super) fn party_reset(&mut self) {
        self.party.state.reset();
        self.party_addons_sync();
    }

    /// Commandes `/…` des groupes ; rend `false` si `cmd` n'en est pas une.
    pub(super) fn party_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let arg = arg.trim();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let cmd = cmd.to_lowercase();
        let command = match cmd.as_str() {
            "/inviter" | "/invite" | "/inv" => {
                if arg.is_empty() {
                    match self.party_target() {
                        Some((cid, _)) if cid != me => Some(PartyCommand::Invite { cid: Some(cid), name: None }),
                        _ => {
                            self.ui.push_chat("[groupe] usage : /inviter nom (ou sélectionnez un joueur)".to_string());
                            None
                        }
                    }
                } else {
                    Some(PartyCommand::Invite { cid: None, name: Some(arg.to_string()) })
                }
            }
            "/accepter" | "/accept" | "/refuser" | "/decline" => {
                let accept = matches!(cmd.as_str(), "/accepter" | "/accept");
                let invitation = if arg.is_empty() {
                    self.party.state.invitations.last().cloned()
                } else {
                    self.party
                        .state
                        .invitations
                        .iter()
                        .find(|i| i.name.eq_ignore_ascii_case(arg))
                        .cloned()
                };
                match invitation {
                    Some(i) if accept => Some(PartyCommand::Accept(i.cid)),
                    Some(i) => Some(PartyCommand::Reject(i.cid)),
                    None => {
                        self.ui.push_chat("[groupe] aucune invitation en attente".to_string());
                        None
                    }
                }
            }
            "/quittergroupe" | "/leave" | "/leaveparty" => {
                if self.party.state.in_party() {
                    Some(PartyCommand::Leave)
                } else {
                    self.ui.push_chat("[groupe] vous n'êtes pas dans un groupe".to_string());
                    None
                }
            }
            "/exclure" | "/kick" | "/uninvite" | "/chef" | "/promote" => {
                let kick = !matches!(cmd.as_str(), "/chef" | "/promote");
                let member = if arg.is_empty() {
                    self.party_target().map(|(c, _)| c).filter(|c| self.party.state.contains(*c))
                } else {
                    self.party.state.member_by_name(arg)
                };
                match member {
                    _ if !self.party.state.is_leader(me) => {
                        self.ui.push_chat("[groupe] seul le chef du groupe peut le faire".to_string());
                        None
                    }
                    Some(c) if c != me && kick => Some(PartyCommand::Banish(c)),
                    Some(c) if c != me => Some(PartyCommand::Delegate(c)),
                    _ => {
                        self.ui.push_chat(format!("[groupe] usage : {cmd} nom d'un membre"));
                        None
                    }
                }
            }
            "/groupe" | "/party" => {
                // Options du groupe d'Archonia (`UI_PartyOption`).
                self.party.frames_ui.options = !self.party.frames_ui.options;
                None
            }
            _ => return false,
        };
        if let Some(c) = command {
            send(self.net.as_ref(), &mut self.party, &mut self.ui, c);
        }
        true
    }

    /// Cible sélectionnée (AddOns : `TargetUnit`, `/target`) : CID et nom.
    fn party_target(&self) -> Option<(i32, String)> {
        let cid = self.addons.target_cid()?;
        let name = self
            .entities
            .iter()
            .find(|e| e.cid == cid)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| self.party.state.name_of(cid));
        Some((cid, name))
    }

    /// Données de la fenêtre du groupe, préparées avant la fermeture egui.
    pub(super) fn party_view(&self) -> PartyView {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let player_hp = self.player.as_ref().and_then(|p| p.hp);
        let members = self
            .party
            .state
            .members
            .iter()
            .map(|&cid| {
                let mut s = self.party.stats.get(&cid).copied().unwrap_or_default();
                if cid == me {
                    s.hp = player_hp.or(s.hp);
                }
                if s.hp.is_none() {
                    s.hp = self.entities.iter().find(|e| e.cid == cid).and_then(|e| e.hp);
                }
                (cid, self.party.state.name_of(cid), s)
            })
            .collect();
        // Visage de chaque membre visible : race et classe de son modèle (charactertemplate.ini).
        let templates = self.assets.table("ini", "charactertemplate.ini").ok();
        let faces = self
            .party
            .state
            .members
            .iter()
            .filter_map(|&cid| {
                let tid = if cid == me { self.player.as_ref().map(|p| p.tid) } else { self.entities.iter().find(|e| e.cid == cid).map(|e| e.tid) }?;
                let sec = templates.as_ref()?.section(&tid.to_string())?;
                let num = |k: &str| sec.get(k).and_then(|v| v.trim().parse::<i32>().ok());
                crate::hud::party_face(num("Race")?, num("Class")?).map(|f| (cid, f))
            })
            .collect();
        PartyView {
            me,
            members,
            faces,
            selected: self.addons.target_cid(),
        }
    }
}

/// Instantané pour l'affichage.
pub struct PartyView {
    pub me: i32,
    pub members: Vec<(i32, String, MemberStats)>,
    pub faces: HashMap<i32, String>,
    pub selected: Option<i32>,
}

/// Bouton « Groupe », fenêtre du groupe et fenêtre d'invitation.
pub fn panel(
    ctx: &egui::Context,
    glue: &mut PartyGlue,
    view: &PartyView,
    net: Option<&NetLink>,
    ui_state: &mut UiState,
    hud: &mut crate::hud::HudState,
    assets: &crate::assets::Assets,
) {
    let mut commands = Vec::new();
    // Boîte d'Archonia : message de l'[EventMap] `{{PartyInviterName.PartyInviter}} invites you
    // to join their party.` (`Action=4:1:-1:`).
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
    // Rappel d'un membre : `UI_ConfirmRecall`, texte `Party_Request_Recall` (« %s a active un orbe d'invocation… »).
    if let Some(request) = glue.state.recall.clone() {
        let text = crate::hud::ui_message(hud, assets, "Party_Request_Recall").replacen("%s", &request.name, 1);
        let out = crate::hud::draw_dialog(ctx, hud, assets, "UI_ConfirmRecall", |_, _, painter, c, r, font| {
            if c.name != "CONTROL_RecallEdit" {
                return false;
            }
            let galley = painter.layout(text.clone(), egui::FontId::proportional(font), egui::Color32::WHITE, r.width());
            painter.galley(egui::pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, egui::Color32::WHITE);
            true
        });
        let answer = match out.as_ref().and_then(|o| o.clicked.as_deref()) {
            Some("CONTROL_24138") => Some(true),
            Some("CONTROL_32003") => Some(false),
            _ => None,
        };
        if let Some(accept) = answer {
            glue.state.recall = None;
            let key = if accept { "PartyAcceptRecall" } else { "PartyRejectRecall" };
            let message = super::guild_glue::fill_braces(&super::portal_glue::ui_event_cached(&mut glue.events, assets, key), &request.name);
            hud.push_system(&message);
            if accept {
                // FUN_1405e2db0 : annulation d'échange (AgpmPrivateTrade op 5) avant l'op 16.
                if let Some(net) = net {
                    net.send(Command::Trade(archrust_netclient::trade::TradeCommand::Cancel));
                }
                glue.recall_close_windows = true;
                commands.push(PartyCommand::AcceptRecall(request.cid));
            } else {
                commands.push(PartyCommand::RejectRecall(request.cid));
            }
        }
    }
    for inv in glue.state.invitations.clone() {
        let text = super::guild_glue::fill_braces(&super::portal_glue::ui_event_cached(&mut glue.events, assets, "PartyRecvInvite"), &inv.name);
        match crate::hud::confirm_box(ctx, hud, assets, ("party-invitation", inv.cid), &text) {
            Some(true) => commands.push(PartyCommand::Accept(inv.cid)),
            Some(false) => commands.push(PartyCommand::Reject(inv.cid)),
            None => {}
        }
    }

    // Cadres, menus et options du groupe d'Archonia (voir `hud_party.rs`).
    let leader = glue.state.leader();
    let frame = |(cid, name, stats): &(i32, String, MemberStats)| crate::hud::PartyFrame {
        cid: *cid,
        name: name.clone(),
        level: stats.level,
        hp: stats.hp,
        max_hp: stats.max_hp,
        leader: leader == Some(*cid),
        face: view.faces.get(cid).cloned(),
    };
    let frames = crate::hud::PartyFrames {
        me: glue.state.in_party().then(|| view.members.iter().find(|m| m.0 == view.me).map(frame)).flatten(),
        others: view.members.iter().filter(|m| m.0 != view.me).map(frame).collect(),
        selected: view.selected,
        i_lead: leader == Some(view.me),
        item_division: glue.state.item_division as i32,
        exp_type: glue.state.exp_type as i32,
        item_choices: vec![(ITEM_SEQUENCE as i32, "Party_Item_Division_Sequence"), (ITEM_FREE as i32, "Party_Item_Division_Free"), (ITEM_DAMAGE as i32, "Party_Item_Division_Damage")],
        exp_choices: vec![(EXP_BY_DAMAGE as i32, "Party_Exp_Division_Damage"), (EXP_BY_LEVEL as i32, "Party_Exp_Division_Level")],
    };
    let mut requests = Vec::new();
    crate::hud::draw_party(ctx, hud, assets, &frames, &mut glue.frames_ui, &mut requests);
    for r in requests {
        match r {
            crate::hud::PartyUi::Select(cid) => glue.pending_select = Some(cid),
            crate::hud::PartyUi::Leave => commands.push(PartyCommand::Leave),
            crate::hud::PartyUi::Banish(cid) => commands.push(PartyCommand::Banish(cid)),
            crate::hud::PartyUi::Delegate(cid) => commands.push(PartyCommand::Delegate(cid)),
            crate::hud::PartyUi::Invite(name) => commands.push(PartyCommand::Invite { cid: None, name: Some(name) }),
            crate::hud::PartyUi::SetItemDivision(t) => commands.push(PartyCommand::SetItemDivision(t as i8)),
            crate::hud::PartyUi::SetExpType(t) => commands.push(PartyCommand::SetExpType(t as i8)),
        }
    }
    for c in commands {
        send(net, glue, ui_state, c);
    }
}
