//! Courrier côté moteur. Protocole : `archrust_netclient::mail` (AgpmMailBox 0x2D) ; fenêtres d'Archonia :
//! `hud_mail.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIMailBox, sha256 e9c7609b…) :
//! * ouverture : bouton « Mailbox » du menu Communauté et touche U (`Hotkey=0:22`) ; le client ne demande
//!   jamais la liste : le serveur pousse chaque courrier par une op 0, insérée en tête (30 au plus, le plus
//!   ancien évincé), refusée si le destinataire n'est pas exactement le nom du joueur ou si l'ID est connu ;
//!   chaque op 0 affiche `YouHaveMail` (`FUN_140638990`) ;
//! * liste (`FUN_14063b380`) : objet, date `%Y-%m-%d %H:%M:%S` en heure locale (0 → 1900-01-01 00:00:00),
//!   icônes lu / non lu et pièce jointe ; « Delete » seulement pour un courrier lu, sans confirmation (op 1,
//!   retiré à la réponse 0) ;
//! * lecture (`FUN_140638c30`) : `MailRead`, drapeau lu posé localement, op 2 à chaque ouverture ;
//!   « Save Item » (`FUN_140639f40`) : confirmation `MailBoxItem_SaveConfirm`, fermeture de la lecture, op 4 ;
//!   à la réponse 0, drapeau 0x10, pièce remise à zéro, `MAIL_ITEM_SAVED` ;
//! * écriture (`FUN_14063b2e0`, `FUN_140639270`) : seulement avec un « Ordinary Letter Paper » (3601) dans le
//!   sac ; destinataire (blancs finaux retirés) et objet de 1 à 32 octets, texte de moins de 1000 octets, pas à
//!   soi-même ; un objet du sac au plus, refusé s'il est interdit (papiers, 4609 à 4623) ou hors du sac
//!   (`FUN_1402ece60`) ; la fenêtre se ferme à l'envoi ; réponse 0 : `MAIL_WRITE_SUCCESS` et case vidée,
//!   sinon `MAIL_WRITE_FAIL` ; « Cancel » : confirmation `MAIL_CLOSE_CONFIRM` ;
//! * réponse (`FUN_140639de0`) : destinataire = expéditeur, objet = `[RE]` + objet tronqué à 46 octets.
//!
//! Écarts : les objets liés et payants (codes 7 et 10 d'Archonia, état d'objet inconnu d'ArchRust) ne sont
//! refusés que par le serveur ; ni sons ni infobulle de la pièce jointe.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, MailBoxView, MailReadView, MailRow, MailUi, MailWriteView, MAIL_ROWS};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::mail::{attachment_check, flag, reply_subject, Mail, MailCommand, MailEvent, BODY_MAX, MAX_MAILS, NAME_MAX, PAPER_TID, SUBJECT_MAX};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use std::collections::HashMap;

/// Lettre en cours d'écriture.
#[derive(Debug, Default)]
struct Draft {
    to: String,
    subject: String,
    body: String,
}

#[derive(Default)]
pub struct MailGlue {
    /// Courriers, le plus récent en tête.
    pub mails: Vec<Mail>,
    pub show_box: bool,
    first: usize,
    /// `MailWrite` ouverte.
    write: Option<Draft>,
    /// Objet joint (IID), gardé jusqu'à l'envoi réussi ou l'annulation.
    attached: Option<i32>,
    /// `MailRead` ouverte sur ce courrier.
    reading: Option<i32>,
    confirm_cancel: bool,
    confirm_save: Option<i32>,
    messages: Option<HashMap<String, String>>,
}

impl MailGlue {
    fn text(&mut self, assets: &Assets, key: &str) -> String {
        let messages = self.messages.get_or_insert_with(|| {
            super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
        });
        messages.get(key).cloned().unwrap_or_default()
    }

    /// Boîte aux lettres ouverte ou fermée (`MAIL_LIST_OPEN` redessine seulement la liste).
    pub fn toggle(&mut self) {
        self.show_box = !self.show_box;
    }

    /// Ferme les fenêtres ouvertes (Échap) ; vrai s'il y en avait.
    pub fn close_windows(&mut self) -> bool {
        let open = self.show_box || self.write.is_some() || self.reading.is_some();
        self.show_box = false;
        self.write = None;
        self.reading = None;
        open
    }

    /// Op 0 : ajout en tête, refusé pour un autre destinataire ou un ID connu ; liste pleine : le plus ancien
    /// part (`FUN_1402ec500`).
    pub fn add(&mut self, mail: Mail, me: &str) -> bool {
        if mail.id == 0 || mail.to != me || self.mails.iter().any(|m| m.id == mail.id) {
            return false;
        }
        if self.mails.len() >= MAX_MAILS {
            self.mails.pop();
        }
        self.mails.insert(0, mail);
        true
    }
}

/// Papiers à lettre ordinaires du sac (statut 2, quantité de pile).
fn letter_papers(items: &HashMap<i32, ItemRecord>) -> i32 {
    items.values().filter(|i| i.status == Some(2) && i.template_id == Some(PAPER_TID)).map(|i| i.count.unwrap_or(1).max(1)).sum()
}

/// Date en heure locale, `%Y-%m-%d %H:%M:%S` (`FUN_140496230`).
pub fn format_date(secs: i64) -> String {
    if secs <= 0 {
        return "1900-01-01 00:00:00".into();
    }
    let local = secs + local_offset(secs);
    let (days, rem) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    // Jours depuis 1970 → date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Décalage de l'heure locale à l'instant `secs` (heure d'été comprise), en secondes.
#[cfg(windows)]
fn local_offset(secs: i64) -> i64 {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime};
    let ticks = (secs + 11_644_473_600) as u64 * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    // SAFETY : structures locales initialisées, pointeurs valides le temps des appels.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        let mut back: FILETIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0 || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 || SystemTimeToFileTime(&local, &mut back) == 0 {
            return 0;
        }
        let local_ticks = ((back.dwHighDateTime as u64) << 32) | back.dwLowDateTime as u64;
        (local_ticks as i64 - ticks as i64) / 10_000_000
    }
}

#[cfg(not(windows))]
fn local_offset(_secs: i64) -> i64 {
    0
}

/// Contrôles d'envoi de `FUN_140639270` : texte nettoyé du destinataire si la lettre peut partir.
fn send_checks(d: &Draft, me: &str, papers: i32) -> Option<String> {
    let to = d.to.trim_end().to_string();
    let len = |s: &str| archrust_netclient::guild::to_cp1252(s).len();
    let ok = papers > 0
        && (1..=NAME_MAX).contains(&len(&to))
        && (1..=SUBJECT_MAX).contains(&len(&d.subject))
        && len(&d.body) <= BODY_MAX
        && to != me;
    ok.then_some(to)
}

/// Fenêtres du courrier et confirmations.
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut MailGlue, items: &HashMap<i32, ItemRecord>, me: &str, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    let papers = letter_papers(items);
    let mut out = Vec::new();
    if glue.show_box {
        let rows: Vec<MailRow> = glue.mails.iter().map(|m| MailRow { subject: m.subject.clone(), date: format_date(m.date), read: m.read(), attached: m.has_item() }).collect();
        let letters = glue.text(assets, "MAIL_LETTER_COUNT").replacen("%d", &papers.to_string(), 1);
        glue.first = glue.first.min(rows.len().saturating_sub(MAIL_ROWS));
        let view = MailBoxView { rows: &rows, first: glue.first, letters: &letters, can_write: papers > 0 };
        crate::hud::draw_mailbox(ctx, hud, assets, &view, &mut out);
    }
    if glue.write.is_some() {
        let desc = glue.text(assets, "MAIL_WRITE_DESC").replacen("%d", &papers.to_string(), 1);
        let item = glue.attached.and_then(|iid| items.get(&iid)).and_then(|i| Some((i.template_id?, i.count.unwrap_or(1).max(1))));
        if let Some(d) = glue.write.as_mut() {
            let view = MailWriteView { to: &mut d.to, subject: &mut d.subject, body: &mut d.body, item, desc: &desc, from: me };
            crate::hud::draw_mail_write(ctx, hud, assets, view, &mut out);
        }
    }
    if let Some(m) = glue.reading.and_then(|id| glue.mails.iter().find(|m| m.id == id)) {
        let item = m.has_item().then_some((m.item_tid, m.item_count));
        let view = MailReadView { from: &m.from, to: &m.to, subject: &m.subject, body: &m.body, item };
        crate::hud::draw_mail_read(ctx, hud, assets, &view, papers > 0, &mut out);
    }
    for a in out {
        match a {
            MailUi::CloseBox => glue.show_box = false,
            MailUi::Scroll(step) => glue.first = (glue.first as i32 + step).clamp(0, glue.mails.len().saturating_sub(MAIL_ROWS) as i32) as usize,
            MailUi::Write if papers > 0 => glue.write = Some(Draft::default()),
            MailUi::Write => {}
            MailUi::Read(i) => {
                if let Some(m) = glue.mails.get_mut(i) {
                    m.flags |= flag::READ;
                    glue.reading = Some(m.id);
                    if let Some(net) = net {
                        net.send(Command::Mail(MailCommand::Read(m.id)));
                    }
                }
            }
            MailUi::Delete(i) => {
                if let (Some(m), Some(net)) = (glue.mails.get(i).filter(|m| m.read()), net) {
                    net.send(Command::Mail(MailCommand::Remove(m.id)));
                }
            }
            MailUi::CancelWrite => glue.confirm_cancel = true,
            MailUi::Send => {
                let Some(d) = glue.write.as_ref() else { continue };
                let Some(to) = send_checks(d, me, papers) else { continue };
                let item = glue.attached.filter(|iid| items.get(iid).is_some_and(|i| attachment_check(i.template_id.unwrap_or(0), i.status) == 0)).unwrap_or(0);
                if let Some(net) = net {
                    net.send(Command::Mail(MailCommand::Write { from: me.to_string(), to, subject: d.subject.clone(), body: d.body.clone(), item }));
                }
                // `MAIL_CLOSE_WRITE` dès l'envoi ; la case se vide à la réponse.
                glue.write = None;
            }
            MailUi::Attach(drag) => {
                let ok = !drag.worn && !drag.bank && items.get(&drag.iid).is_some_and(|i| attachment_check(i.template_id.unwrap_or(drag.tid), i.status) == 0);
                if ok {
                    glue.attached = Some(drag.iid);
                }
            }
            MailUi::Detach => glue.attached = None,
            MailUi::CloseRead => glue.reading = None,
            MailUi::SaveItem => glue.confirm_save = glue.reading,
            MailUi::Reply => {
                if let Some(m) = glue.reading.and_then(|id| glue.mails.iter().find(|m| m.id == id)).filter(|_| papers > 0) {
                    glue.write = Some(Draft { to: m.from.clone(), subject: reply_subject(&m.subject), body: String::new() });
                }
            }
        }
    }
    if glue.confirm_cancel {
        let text = glue.text(assets, "MAIL_CLOSE_CONFIRM");
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "mail-cancel", &text) {
            glue.confirm_cancel = false;
            if yes {
                glue.write = None;
                glue.attached = None;
            }
        }
    } else if let Some(id) = glue.confirm_save {
        let text = glue.text(assets, "MailBoxItem_SaveConfirm");
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("mail-save", id), &text) {
            glue.confirm_save = None;
            if yes {
                glue.reading = None;
                if let Some(net) = net {
                    net.send(Command::Mail(MailCommand::SaveItem(id)));
                }
            }
        }
    }
    let _ = ui;
}

impl App {
    /// Paquet AgpmMailBox (rappels d'Archonia : le CID doit être celui du joueur).
    pub(super) fn on_mail_event(&mut self, ev: MailEvent) {
        let assets = self.assets.clone();
        let (me_cid, me) = self.player.as_ref().map_or((0, String::new()), |p| (p.cid, p.name.clone()));
        let glue = &mut self.mail;
        match ev {
            MailEvent::Added { cid, mail } if cid == me_cid => {
                if glue.add(mail, &me) {
                    self.ui.push_chat(glue.text(&assets, "YouHaveMail"));
                }
            }
            MailEvent::Removed { cid, id, result: 0 } if cid == me_cid => {
                if glue.reading == Some(id) {
                    glue.reading = None;
                }
                glue.mails.retain(|m| m.id != id);
            }
            MailEvent::Written { cid, result } if cid == me_cid => {
                if result == 0 {
                    glue.attached = None;
                    self.ui.push_chat(glue.text(&assets, "MAIL_WRITE_SUCCESS"));
                } else {
                    self.ui.push_chat(glue.text(&assets, "MAIL_WRITE_FAIL"));
                }
            }
            MailEvent::ItemSaved { cid, id, result: 0 } if cid == me_cid => {
                if let Some(m) = glue.mails.iter_mut().find(|m| m.id == id) {
                    m.flags |= flag::ITEM_SAVED;
                    m.item_tid = 0;
                    m.item_count = 0;
                }
                self.ui.push_chat(glue.text(&assets, "MAIL_ITEM_SAVED"));
            }
            MailEvent::RemoveAll { cid } if cid == me_cid => {
                glue.mails.clear();
                glue.reading = None;
            }
            _ => {}
        }
    }

    /// Fin du chargement : `YouHaveMail` s'il reste un courrier non lu (`FUN_14063a810`).
    pub(super) fn mail_enter_world(&mut self) {
        if self.mail.mails.iter().any(|m| !m.read()) {
            let assets = self.assets.clone();
            let line = self.mail.text(&assets, "YouHaveMail");
            self.ui.push_chat(line);
        }
    }

    /// Connexion au monde perdue : courriers oubliés (le serveur les renvoie).
    pub(super) fn mail_reset(&mut self) {
        let show = self.mail.show_box;
        self.mail = MailGlue { show_box: show, ..Default::default() };
    }

    /// `/courrier` (`/mail`) : boîte aux lettres.
    pub(super) fn mail_slash(&mut self, cmd: &str) -> bool {
        if matches!(cmd, "/courrier" | "/mail") {
            self.mail.toggle();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(id: i32, to: &str) -> Mail {
        Mail { id, to: to.into(), ..Default::default() }
    }

    #[test]
    fn liste_de_trente_au_plus() {
        let mut g = MailGlue::default();
        assert!(!g.add(mail(1, "autre"), "Moi"));
        assert!(!g.add(mail(1, "moi"), "Moi"), "nom comparé avec la casse");
        assert!(!g.add(mail(0, "Moi"), "Moi"));
        for id in 1..=31 {
            assert!(g.add(mail(id, "Moi"), "Moi"));
        }
        assert!(!g.add(mail(5, "Moi"), "Moi"));
        assert_eq!(g.mails.len(), 30);
        // En tête le plus récent, le plus ancien (1) évincé.
        assert_eq!((g.mails[0].id, g.mails[29].id), (31, 2));
    }

    #[test]
    fn controles_d_envoi() {
        let d = Draft { to: "Toi  ".into(), subject: "Objet".into(), body: "Texte".into() };
        assert_eq!(send_checks(&d, "Moi", 1).as_deref(), Some("Toi"));
        assert_eq!(send_checks(&d, "Moi", 0), None);
        assert_eq!(send_checks(&d, "Toi", 1), None);
        let long = Draft { subject: "x".repeat(33), ..Draft { to: "Toi".into(), subject: String::new(), body: String::new() } };
        assert_eq!(send_checks(&long, "Moi", 1), None);
        let empty = Draft { to: "Toi".into(), subject: String::new(), body: String::new() };
        assert_eq!(send_checks(&empty, "Moi", 1), None);
        let big = Draft { to: "Toi".into(), subject: "s".into(), body: "b".repeat(1000) };
        assert_eq!(send_checks(&big, "Moi", 1), None);
    }

    #[test]
    fn date_affichee() {
        assert_eq!(format_date(0), "1900-01-01 00:00:00");
        let d = format_date(1_700_000_000);
        assert_eq!(d.len(), 19);
        // 2023-11-14 22:13:20 UTC, à un fuseau près.
        assert!(d.starts_with("2023-11-1"), "{d}");
    }
}
