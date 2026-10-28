//! Coiffeur (« Stylist », fonction de PNJ 29) et apparence des personnages côté moteur. Protocole :
//! `archrust_netclient::customize` ; fenêtres d'Archonia : `hud_customize.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * bouton « Change Appearance » (sysstr 334) du dialogue → op 0 ; accord (op 1, `FUN_1406bd4b0`) : monté →
//!   `CustomizeNeedDismountRide`, métamorphosé → `CustomizeNeedRestoreTransform`, sinon `UI_Customizing` ;
//! * listes (`FUN_1406bdbe0`) : lignes de `charactercustomizelist.txt` du modèle du joueur, `Case ≠ 2`, sans le
//!   visage ni la coiffure actuels ; aucune sélection au départ ; position retenue, fenêtre fermée au-delà de
//!   149 unités (`FUN_1406bda30`) ;
//! * textes (`FUN_1406be6a0`, `FUN_1406be930`, `FUN_1406bebc0`) : « Level : n » en rouge (16711680) si le niveau
//!   manque, sinon 16773320 ; « Cost : n Gold » en rouge si l'or porté manque ; total de la coiffure et du visage ;
//!   prix + prix × taxe / 100 (taxe 0 : ArchCore n'en a pas) ;
//! * « Purchase » (`FUN_1406bdf40`) : une sélection, or ≥ total, niveau ≥ niveau requis (coiffure seule : le
//!   sien ; les deux : le plus petit ; visage seul : 0, incohérence d'Archonia gardée) → `UI_CustomizeBuyConfirm`
//!   (« Cost : %d Gold ») ; « Confirm » envoie l'op 2 (-1 pour ce qui n'est pas choisi) et ferme les fenêtres ;
//! * AGPMCHARACTER op 1 champs 20/21 ≥ 0 : tête rechargée pour tout personnage vu ; pour soi, messages jaunes
//!   `Customize_FaceChangeFormat` / `Customize_HairChangeFormat` avec le nom pris dans les listes de la dernière
//!   ouverture (`FUN_1406bd820`).
//!
//! Écart : ArchRust ne sait pas dessiner un personnage dans une fenêtre ; l'aperçu (`CustomizePreview`) applique
//! la sélection au modèle du joueur dans le monde (rendu à la fermeture), et les boutons de rotation et de zoom
//! tournent et rapprochent la caméra autour de lui.

use super::App;
use crate::assets::Assets;
use crate::hud::{CustomizeUi, CustomizeView, HudState, CUSTOMIZE_LIST_ROWS};
use crate::net::NetLink;
use crate::scene::{entity_meshes_with_equipment, Head};
use archrust_netclient::customize::{CustomizeCommand, CustomizeEvent};
use archrust_netclient::{Command, WorldEvent};
use glam::{Mat4, Vec3};

/// Déplacement au-delà duquel la fenêtre se ferme (`FUN_1406bda30`).
const CLOSE_MOVE: f32 = 149.0;
/// Taux de taxe de la région : ArchCore n'en applique pas.
const TAX_PERCENT: i64 = 0;
const RED: &str = "16711680";
const CREAM: &str = "16773320";

/// Ligne de `charactercustomizelist.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 1 coiffure, 2 visage.
    pub kind: i32,
    pub number: i32,
    pub name: String,
    /// 0 création et coiffeur, 1 coiffeur, 2 création.
    pub case: i32,
    pub tid: i32,
    pub level: i32,
    pub price: i64,
}

/// Table du client (colonnes `Type`, `Number`, `Sell Name`, `Case`, `CharacterTID`, `UseLevel`, `Price(Money)`).
pub fn parse_table(text: &str) -> Vec<Entry> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return Vec::new() };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    let col = |n: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(n));
    let (ty, number, name, case, tid, level, price) = (col("Type"), col("Number"), col("Sell Name"), col("Case"), col("CharacterTID"), col("UseLevel"), col("Price(Money)"));
    let mut out = Vec::new();
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let get = |c: Option<usize>| c.and_then(|i| cells.get(i)).map(|v| v.trim()).unwrap_or("");
        let num = |c: Option<usize>| get(c).parse::<i64>().unwrap_or(0);
        let kind = match get(ty) {
            t if t.eq_ignore_ascii_case("Hair") => 1,
            t if t.eq_ignore_ascii_case("Face") => 2,
            _ => continue,
        };
        out.push(Entry { kind, number: num(number) as i32, name: get(name).to_string(), case: num(case) as i32, tid: num(tid) as i32, level: num(level) as i32, price: num(price) });
    }
    out
}

/// Lignes du coiffeur pour un modèle (`Case ≠ 2`), sans l'apparence actuelle : coiffures puis visages.
pub fn lists_for(table: &[Entry], tid: i32, head: Head) -> (Vec<Entry>, Vec<Entry>) {
    let pick = |kind: i32, current: i32| table.iter().filter(|e| e.tid == tid && e.case != 2 && e.kind == kind && e.number != current).cloned().collect::<Vec<_>>();
    (pick(1, head.hair), pick(2, head.face))
}

fn taxed(price: i64) -> i64 {
    price + price * TAX_PERCENT / 100
}

/// Total et niveau requis d'une sélection (`FUN_1406bdf40`).
pub fn requirement(hair: Option<&Entry>, face: Option<&Entry>) -> (i64, i32) {
    let mut total = 0;
    let mut level = 0;
    if let Some(h) = hair {
        total += taxed(h.price);
        level = h.level;
    }
    if let Some(f) = face {
        total += taxed(f.price);
        if f.level <= level {
            level = f.level;
        }
    }
    (total, level)
}

struct Open {
    npc: i32,
    at: Vec3,
    hairs: Vec<Entry>,
    faces: Vec<Entry>,
    hair: Option<usize>,
    face: Option<usize>,
    first_hair: usize,
    first_face: usize,
    /// Apparence réelle à l'ouverture (rendue à la fermeture de l'aperçu).
    original: Head,
}

#[derive(Default)]
pub struct CustomizeGlue {
    table: Option<Vec<Entry>>,
    open: Option<Open>,
    /// `UI_CustomizeBuyConfirm` ouverte.
    confirm: bool,
    info: Option<String>,
    /// Le joueur est métamorphosé (octet d'état, bit 0).
    transformed: bool,
    /// Listes de la dernière ouverture (noms des messages de changement).
    last_lists: (Vec<Entry>, Vec<Entry>),
    /// Tête à montrer sur le modèle du joueur (aperçu), rotation et zoom de l'image.
    preview: Option<Head>,
    rotate: f32,
    zoom: f32,
}

impl CustomizeGlue {
    fn table(&mut self, assets: &Assets) -> &[Entry] {
        self.table.get_or_insert_with(|| parse_table(&super::npc_glue::client_text_in(assets, "ini", "charactercustomizelist.txt")))
    }

    fn close(&mut self) {
        if let Some(o) = self.open.take() {
            self.preview = Some(o.original);
        }
        self.confirm = false;
    }
}

fn send(net: Option<&NetLink>, cmd: CustomizeCommand) {
    if let Some(net) = net {
        net.send(Command::Customize(cmd));
    }
}

/// Texte d'une sélection (`FUN_1406be6a0` / `FUN_1406be930`).
fn entry_text(hud: &mut HudState, assets: &Assets, e: &Entry, level: i32, money: i64) -> String {
    let msg = |hud: &mut HudState, k: &str| crate::hud::ui_message(hud, assets, k);
    let lc = if e.level <= level { CREAM } else { RED };
    let price = taxed(e.price);
    let pc = if price <= money { CREAM } else { RED };
    format!("{} : <C{lc}>{}<C{CREAM}>\n{} : <C{pc}>{price} <C{CREAM}>{}", msg(hud, "Customize_UseLevel"), e.level, msg(hud, "Customize_PriceMoney"), msg(hud, "Money_Name"))
}

/// Fenêtres du coiffeur (dans la passe egui).
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut CustomizeGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let level = hud.stats.level.unwrap_or(0);
    let money = hud.stats.money.unwrap_or(0);
    if let Some(o) = glue.open.as_ref() {
        let hair = o.hair.and_then(|i| o.hairs.get(i));
        let face = o.face.and_then(|i| o.faces.get(i));
        let (total, need) = requirement(hair, face);
        let can_buy = (hair.is_some() || face.is_some()) && level >= need && money >= total;
        let hair_text = hair.map(|e| entry_text(hud, assets, e, level, money)).unwrap_or_default();
        let face_text = face.map(|e| entry_text(hud, assets, e, level, money)).unwrap_or_default();
        // `FUN_1406bebc0` : noms choisis puis total.
        let mut total_text = String::new();
        if let Some(h) = hair {
            total_text.push_str(&format!("{} : {}\n", crate::hud::ui_message(hud, assets, "Customize_HairStyle"), h.name));
        }
        if let Some(f) = face {
            total_text.push_str(&format!("{} : {}\n", crate::hud::ui_message(hud, assets, "Customize_FaceStyle"), f.name));
        }
        if total > 0 {
            let c = if total <= money { CREAM } else { RED };
            total_text.push_str(&format!("{} : <C{c}>{total} <C{CREAM}>{}", crate::hud::ui_message(hud, assets, "Customize_TotalPriceMoney"), crate::hud::ui_message(hud, assets, "Money_Name")));
        }
        let hairs: Vec<String> = o.hairs.iter().map(|e| e.name.clone()).collect();
        let faces: Vec<String> = o.faces.iter().map(|e| e.name.clone()).collect();
        let view = CustomizeView { hairs: &hairs, faces: &faces, hair: o.hair, face: o.face, first_hair: o.first_hair, first_face: o.first_face, hair_text: &hair_text, face_text: &face_text, total_text: &total_text, can_buy };
        let mut out = Vec::new();
        crate::hud::draw_customizing(ctx, hud, assets, &view, &mut out);
        for r in out {
            let Some(o) = glue.open.as_mut() else { break };
            match r {
                CustomizeUi::SelectHair(i) => o.hair = Some(i),
                CustomizeUi::SelectFace(i) => o.face = Some(i),
                CustomizeUi::Scroll { hair, delta } => {
                    let (first, len) = if hair { (&mut o.first_hair, o.hairs.len()) } else { (&mut o.first_face, o.faces.len()) };
                    *first = (*first as i64 + delta as i64).clamp(0, len.saturating_sub(CUSTOMIZE_LIST_ROWS) as i64) as usize;
                }
                // `CustomizeInit` : sélection effacée, aperçu sur l'apparence actuelle.
                CustomizeUi::Reset => {
                    o.hair = None;
                    o.face = None;
                }
                CustomizeUi::Buy => {
                    if can_buy {
                        glue.confirm = true;
                    }
                }
                CustomizeUi::Close => {
                    glue.close();
                    break;
                }
                CustomizeUi::Rotate(d) => glue.rotate += d,
                CustomizeUi::Zoom(d) => glue.zoom += d,
            }
        }
        if let Some(o) = glue.open.as_ref() {
            let face = o.face.and_then(|i| o.faces.get(i)).map_or(o.original.face, |e| e.number);
            let hair = o.hair.and_then(|i| o.hairs.get(i)).map_or(o.original.hair, |e| e.number);
            glue.preview = Some(Head { face, hair });
        }
        if glue.confirm {
            let text = crate::hud::ui_message(hud, assets, "Customize_BuyPriceFormat").replacen("%d", &total.to_string(), 1);
            match crate::hud::draw_customize_confirm(ctx, hud, assets, &text) {
                // `CustomizeConfirmOK` (FUN_1406bd2a0) puis fermeture des deux fenêtres, sans attendre de réponse.
                Some(true) => {
                    if let Some(o) = glue.open.as_ref() {
                        let face = o.face.and_then(|i| o.faces.get(i)).map_or(-1, |e| e.number);
                        let hair = o.hair.and_then(|i| o.hairs.get(i)).map_or(-1, |e| e.number);
                        log::info!("coiffeur : achat visage {face}, coiffure {hair} ({total} or)");
                        send(net, CustomizeCommand::Buy { npc: o.npc, face, hair });
                    }
                    glue.close();
                }
                Some(false) => glue.confirm = false,
                None => {}
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if text.is_empty() || crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
}

impl App {
    /// Métamorphose du joueur (refus du coiffeur) ; nouvelle connexion.
    pub(super) fn customize_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            WorldEvent::Transformed { cid, transformed } if *cid == me => self.customize.transformed = *transformed,
            WorldEvent::Connected => {
                self.customize.transformed = false;
                self.customize.close();
                self.customize.preview = None;
            }
            _ => {}
        }
    }

    pub(super) fn on_customize_event(&mut self, ev: CustomizeEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            CustomizeEvent::Granted { npc, cid } if cid == me || me == 0 => {
                let events = super::portal_glue::ui_events(&assets);
                if self.ride.riding(me) {
                    self.customize.info = events.get("CustomizeNeedDismountRide").cloned();
                    return;
                }
                if self.customize.transformed {
                    self.customize.info = events.get("CustomizeNeedRestoreTransform").cloned();
                    return;
                }
                let Some(p) = self.player.as_ref() else { return };
                let (tid, head, at) = (p.tid, p.head, p.position);
                let (hairs, faces) = lists_for(self.customize.table(&assets), tid, head);
                log::info!("coiffeur {npc} : {} coiffure(s), {} visage(s) pour le modèle {tid}", hairs.len(), faces.len());
                self.customize.last_lists = (hairs.clone(), faces.clone());
                self.customize.open = Some(Open { npc, at, hairs, faces, hair: None, face: None, first_hair: 0, first_face: 0, original: head });
                self.customize.confirm = false;
            }
            CustomizeEvent::Granted { .. } => {}
            CustomizeEvent::Appearance { cid, face, hair } => {
                if self.player.as_ref().is_some_and(|p| p.cid == cid) {
                    let Some(old) = self.player.as_ref().map(|p| p.head) else { return };
                    let head = Head { face: face.unwrap_or(old.face), hair: hair.unwrap_or(old.hair) };
                    // `FUN_1406bd820` : messages jaunes, nom pris dans les listes de la dernière ouverture.
                    let (hairs, faces) = &self.customize.last_lists;
                    let mut lines = Vec::new();
                    if head.face != old.face {
                        if let Some(e) = faces.iter().find(|e| e.number == head.face) {
                            lines.push(crate::hud::ui_message(&mut self.hud, &assets, "Customize_FaceChangeFormat").replacen("%s", &e.name, 1));
                        }
                    }
                    if head.hair != old.hair {
                        if let Some(e) = hairs.iter().find(|e| e.number == head.hair) {
                            lines.push(crate::hud::ui_message(&mut self.hud, &assets, "Customize_HairChangeFormat").replacen("%s", &e.name, 1));
                        }
                    }
                    for l in lines {
                        self.ui.push_chat_in(0x10, format!("|cffffff00{l}|r"));
                    }
                    if let Some(o) = self.customize.open.as_mut() {
                        o.original = head;
                    }
                    self.set_player_head(head);
                } else if let Some(index) = self.entities.iter().position(|e| e.cid == cid) {
                    let e = &mut self.entities[index];
                    let head = Head { face: face.unwrap_or(e.head.face), hair: hair.unwrap_or(e.head.hair) };
                    if head != e.head {
                        e.head = head;
                        self.reload_entity_meshes(index);
                    }
                }
            }
        }
    }

    /// Tête du modèle du joueur (aperçu ou apparence réelle), équipement porté gardé.
    fn set_player_head(&mut self, head: Head) {
        let Some(p) = self.player.as_ref() else { return };
        if p.head == head || p.cid == 0 {
            return;
        }
        let Some(gpu) = self.gpu.as_mut() else { return };
        let fallback = gpu.player_marker.clone();
        let meshes = entity_meshes_with_equipment(&mut gpu.renderer, &self.assets, p.tid, head, &p.equipment, &fallback).unwrap_or_else(|err| {
            log::warn!("apparence du joueur (TID {}) : {err:#}", p.tid);
            vec![(fallback.clone(), Mat4::IDENTITY)]
        });
        if let Some(p) = self.player.as_mut() {
            p.head = head;
            p.meshes = meshes;
        }
    }

    /// À chaque image : fermeture en s'éloignant, aperçu sur le modèle, rotation et zoom de la caméra.
    pub(super) fn customize_frame(&mut self, dt: f32) {
        if let (Some(me), Some(o)) = (self.player.as_ref().map(|p| p.position), self.customize.open.as_ref()) {
            if Vec3::new(me.x - o.at.x, 0.0, me.z - o.at.z).length() > CLOSE_MOVE {
                self.customize.close();
            }
        }
        if let Some(head) = self.customize.preview.take() {
            self.set_player_head(head);
        }
        let (rotate, zoom) = (std::mem::take(&mut self.customize.rotate), std::mem::take(&mut self.customize.zoom));
        if rotate != 0.0 {
            self.camera.rotate(rotate.signum() * 400.0 * dt, 0.0);
        }
        if zoom != 0.0 {
            self.camera.zoom(zoom.signum() * 3.0 * dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "Type\tNumber\tSell Name\tCase\tCharacterTID\tUseLevel\tPrice(Money)\tPrice(Skull)\r\n\
Hair\t0\tSpiked 1\t0\t96\t1\t0\t0\r\n\
Hair\t4\tTop Volume 1\t1\t96\t7\t1000\t0\r\n\
Hair\t9\tCreation\t2\t96\t1\t0\t0\r\n\
Face\t1\tType 2\t1\t96\t7\t3000\t0\r\n\
Face\t21\tType 22\t1\t96\t10\t3000\t0\r\n\
Face\t0\tType 1\t0\t1\t1\t0\t0\r\n";

    #[test]
    fn listes_du_coiffeur() {
        let t = parse_table(TABLE);
        assert_eq!(t.len(), 6);
        let (hairs, faces) = lists_for(&t, 96, Head { face: 1, hair: 0 });
        assert_eq!(hairs.iter().map(|e| e.number).collect::<Vec<_>>(), vec![4]);
        assert_eq!(faces.iter().map(|e| e.number).collect::<Vec<_>>(), vec![21]);
    }

    #[test]
    fn niveau_et_prix_d_archonia() {
        let t = parse_table(TABLE);
        let hair = t.iter().find(|e| e.kind == 1 && e.number == 4);
        let face = t.iter().find(|e| e.kind == 2 && e.number == 21);
        assert_eq!(requirement(hair, None), (1000, 7));
        assert_eq!(requirement(hair, face), (4000, 7));
        // Visage seul : aucun niveau exigé par l'interface d'Archonia.
        assert_eq!(requirement(None, face), (3000, 0));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : 749 lignes, chevaliers 58 coiffures / 24 visages.
    #[test]
    fn table_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let t = parse_table(&super::super::npc_glue::client_text_in(&assets, "ini", "charactercustomizelist.txt"));
        assert_eq!(t.iter().filter(|e| e.tid == 96 && e.kind == 1).count(), 58);
        assert_eq!(t.iter().filter(|e| e.tid == 96 && e.kind == 2).count(), 24);
    }
}
