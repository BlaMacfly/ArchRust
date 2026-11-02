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
//!   prix + prix × taxe / 100 (taux de la région du joueur, voir `app_tax.rs`) ;
//! * « Purchase » (`FUN_1406bdf40`) : une sélection, or ≥ total, niveau ≥ niveau requis (coiffure seule : le
//!   sien ; les deux : le plus petit ; visage seul : 0, incohérence d'Archonia gardée) → `UI_CustomizeBuyConfirm`
//!   (« Cost : %d Gold ») ; « Confirm » envoie l'op 2 (-1 pour ce qui n'est pas choisi) et ferme les fenêtres ;
//! * AGPMCHARACTER op 1 champs 20/21 ≥ 0 : tête rechargée pour tout personnage vu ; pour soi, messages jaunes
//!   `Customize_FaceChangeFormat` / `Customize_HairChangeFormat` avec le nom pris dans les listes de la dernière
//!   ouverture (`FUN_1406bd820`).
//!
//! Aperçu (`AgcmCustomizeRender`, contrôle `CONTROL_4835` de type 16 « CustomizePreview », 209 × 370) : un
//! **rendu hors écran** d'un clone du modèle du joueur, pas le personnage du monde (qui ne change qu'après
//! l'achat) :
//! * raster 209 × 370 (`FUN_1406f89a0`), fond noir transparent (`FUN_14038d3b0`, alpha 0), caméra à part
//!   (fenêtre de vue 0,5 × 0,5·370/209, `FUN_1406f9dc0`) ; clone du clump du modèle sans l'équipement porté,
//!   première animation du modèle (attente) ; visage et coiffure essayés posés sur ce clone (`FUN_1406f9710`,
//!   `FUN_1406f9a30`, appelés par `CustomizeSelectFace` / `CustomizeSelectHair`) ;
//! * caméra (`FUN_1406f9430`) : `CUSTOMIZE_REVIEW` de `charactertemplatecustomize.ini` = deux points `proche`
//!   et `loin` (x, y, z ; x inutilisé) ; distance `d` (au départ `loin.z`) ; angle `θ` (au départ 0) ;
//!   œil = (d·sin θ, y, d·cos θ) autour du modèle, regard horizontal sur son axe, hauteur
//!   `y = proche.y + |d − proche.z| / |loin.z − proche.z| × (loin.y − proche.y)` ;
//! * boutons tenus (`FUN_1406f8d50`, 0,12 par ms) : rotation gauche `θ += 0,12·dt`, droite `θ −= 0,12·dt`
//!   (degrés), zoom avant `d −= 0,12·dt` jusqu'à `proche.z`, arrière `d += 0,12·dt` jusqu'à `loin.z` ;
//! * lumières : ambiante 0,22, directionnelle 0,7, dirigée vers `−(sin(θ + 45°), 0,5, cos(θ + 45°))`.
//! Le rendu est fait chaque image tant que la fenêtre est ouverte (`AgcmUICustomize` OnIdle `FUN_1406be590`).

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
    super::tax_glue::taxed(price)
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
    /// Tête essayée (aperçu hors écran), boutons de rotation et de zoom tenus cette image.
    preview: Option<Head>,
    rotate: f32,
    zoom: f32,
    /// Aperçu hors écran (`AgcmCustomizeRender`) : raster, modèle cloné, caméra.
    render: Option<PreviewRender>,
}

/// Raster de l'aperçu (`FUN_1406f89a0` : 209 × 370).
pub const PREVIEW_SIZE: (u32, u32) = (209, 370);
/// Vitesse de rotation (degrés) et de zoom (unités) par milliseconde (`FUN_1406f8d50`).
const PREVIEW_STEP_PER_MS: f32 = 0.12;
/// Lumières de l'aperçu (`FUN_1406f8d50`, sans `color.txt`) : ambiante et directionnelle.
const PREVIEW_AMBIENT: f32 = 0.22;
const PREVIEW_DIRECTIONAL: f32 = 0.7;
/// Plans de coupe de la caméra de l'aperçu : choix d'ArchRust (valeurs passées en registres flottants
/// à `FUN_14038d2b0` / `FUN_14038d2e0`, non relevées).
const PREVIEW_NEAR: f32 = 1.0;
const PREVIEW_FAR: f32 = 10_000.0;

/// `CUSTOMIZE_REVIEW` d'un modèle (`charactertemplatecustomize.ini`, six flottants séparés par `:`) : point
/// proche (visage) et point lointain (corps), `loin.z` relevé d'une unité s'il égale `proche.z`
/// (`FUN_1406f9dc0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Review {
    pub near: Vec3,
    pub far: Vec3,
}

impl Review {
    pub fn parse(value: &str) -> Review {
        let v: Vec<f32> = value.split(':').map(|s| s.trim().parse().unwrap_or(0.0)).collect();
        let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
        let near = Vec3::new(at(0), at(1), at(2));
        let mut far = Vec3::new(at(3), at(4), at(5));
        if (far.z - near.z).abs() < 1e-4 {
            far.z = near.z + 1.0;
        }
        Review { near, far }
    }

    pub fn for_tid(assets: &Assets, tid: i32) -> Review {
        let value = (|| {
            let t = assets.table("ini", "charactertemplatecustomize.ini").ok()?;
            let key = t.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("CUSTOMIZE_REVIEW")).map(|(k, _)| k.clone())?;
            t.section(&tid.to_string())?.get(&key).map(str::to_owned)
        })();
        Review::parse(value.as_deref().unwrap_or(""))
    }
}

/// Caméra de l'aperçu : angle `θ` (degrés, `+0x1768`) et distance `d` (`+0x177c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewCamera {
    pub angle: f32,
    pub distance: f32,
}

impl PreviewCamera {
    pub fn new(review: &Review) -> PreviewCamera {
        PreviewCamera { angle: 0.0, distance: review.far.z }
    }

    /// Une image (`FUN_1406f8d50`) : `rotate` −1 rotation gauche (état 1), +1 droite (état 2) ; `zoom` +1
    /// avant (état 3), −1 arrière (état 4) ; `dt_ms` écoulées. Les bornes de l'angle sont celles
    /// d'Archonia (à droite, un angle négatif devient `360 − θ`).
    pub fn step(&mut self, review: &Review, rotate: f32, zoom: f32, dt_ms: f32) {
        let s = dt_ms * PREVIEW_STEP_PER_MS;
        if rotate < 0.0 {
            self.angle += s;
            if self.angle >= 360.0 {
                self.angle -= 360.0;
            }
        } else if rotate > 0.0 {
            self.angle -= s;
            if self.angle < 0.0 {
                self.angle = 360.0 - self.angle;
            }
        } else if zoom > 0.0 {
            self.distance = (self.distance - s).max(review.near.z);
        } else if zoom < 0.0 {
            self.distance = (self.distance + s).min(review.far.z);
        }
    }

    /// Œil et point visé (repère du modèle, cadre 0 à l'identité), comme `FUN_1406f9430`.
    pub fn eye_target(&self, review: &Review) -> (Vec3, Vec3) {
        let d = self.distance;
        let (s, c) = self.angle.to_radians().sin_cos();
        let t = (d - review.near.z).abs() / (review.far.z - review.near.z).abs();
        let y = t * (review.far.y - review.near.y) + review.near.y;
        (Vec3::new(d * s, y, d * c), Vec3::new(0.0, y, 0.0))
    }

    /// Direction **vers** la lumière directionnelle : opposée à `(−sin(θ + 45°), −0,5, −cos(θ + 45°))`.
    pub fn light_towards(&self) -> Vec3 {
        let (s, c) = (self.angle.to_radians() + std::f32::consts::FRAC_PI_4).sin_cos();
        Vec3::new(s, 0.5, c).normalize()
    }

    /// Vue-projection (profondeur inversée comme la scène) : fenêtre de vue 0,5 × 0,5·h/w.
    pub fn view_proj(&self, review: &Review) -> Mat4 {
        let (w, h) = (PREVIEW_SIZE.0 as f32, PREVIEW_SIZE.1 as f32);
        let fov_y = 2.0 * (0.5 * h / w).atan();
        let (eye, target) = self.eye_target(review);
        Mat4::perspective_rh(fov_y, w / h, PREVIEW_FAR, PREVIEW_NEAR) * Mat4::look_at_rh(eye, target, Vec3::Y)
    }
}

/// Aperçu hors écran : raster et texture egui (gardés d'une ouverture à l'autre), modèle cloné.
struct PreviewRender {
    target: crate::render::OffscreenTarget,
    texture: egui::TextureId,
    model: Option<PreviewModel>,
}

struct PreviewModel {
    tid: i32,
    head: Head,
    review: Review,
    camera: PreviewCamera,
    meshes: Vec<(std::sync::Arc<crate::render::Mesh>, Mat4)>,
    animator: Option<crate::animation::Animator>,
}

impl CustomizeGlue {
    fn table(&mut self, assets: &Assets) -> &[Entry] {
        self.table.get_or_insert_with(|| parse_table(&super::npc_glue::client_text_in(assets, "ini", "charactercustomizelist.txt")))
    }

    fn close(&mut self) {
        // Le modèle de l'aperçu est libéré (`FUN_1406f87c0` à la fermeture) ; le raster reste.
        self.open = None;
        self.preview = None;
        if let Some(r) = self.render.as_mut() {
            r.model = None;
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
        let view = CustomizeView { hairs: &hairs, faces: &faces, hair: o.hair, face: o.face, first_hair: o.first_hair, first_face: o.first_face, hair_text: &hair_text, face_text: &face_text, total_text: &total_text, can_buy, preview: glue.render.as_ref().filter(|r| r.model.is_some()).map(|r| r.texture) };
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
                // Métamorphosé ou évolué (`+0x600` / `+0x604`, voir `app_morph.rs`).
                if self.customize.transformed || self.morph_changed_form() {
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

    /// À chaque image : fermeture en s'éloignant ; aperçu hors écran (modèle cloné avec la tête essayée,
    /// rotation et zoom de sa caméra, animation d'attente), voir l'en-tête du module.
    pub(super) fn customize_frame(&mut self, dt: f32) {
        if let (Some(me), Some(o)) = (self.player.as_ref().map(|p| p.position), self.customize.open.as_ref()) {
            if Vec3::new(me.x - o.at.x, 0.0, me.z - o.at.z).length() > CLOSE_MOVE {
                self.customize.close();
            }
        }
        let (rotate, zoom) = (std::mem::take(&mut self.customize.rotate), std::mem::take(&mut self.customize.zoom));
        let (Some(head), Some(tid)) = (self.customize.preview.filter(|_| self.customize.open.is_some()), self.player.as_ref().map(|p| p.tid)) else { return };
        let assets = self.assets.clone();
        let Some(gpu) = self.gpu.as_mut() else { return };
        let render = self.customize.render.get_or_insert_with(|| {
            let target = gpu.renderer.create_offscreen(PREVIEW_SIZE.0, PREVIEW_SIZE.1);
            let texture = gpu.egui_renderer.register_native_texture(&gpu.renderer.device, &target.view, wgpu::FilterMode::Linear);
            PreviewRender { target, texture, model: None }
        });
        // Nouveau visage ou nouvelle coiffure : clone reconstruit, caméra gardée.
        if render.model.as_ref().is_none_or(|m| m.tid != tid || m.head != head) {
            let camera = render.model.as_ref().filter(|m| m.tid == tid).map(|m| m.camera);
            let fallback = gpu.player_marker.clone();
            let meshes = crate::scene::entity_meshes(&mut gpu.renderer, &assets, tid, head, &fallback).unwrap_or_else(|err| {
                log::warn!("aperçu du coiffeur (TID {tid}) : {err:#}");
                vec![(fallback.clone(), Mat4::IDENTITY)]
            });
            let animator = crate::animation::Animator::new(&assets, tid, &meshes, 1, 0, true);
            let review = Review::for_tid(&assets, tid);
            render.model = Some(PreviewModel { tid, head, review, camera: camera.unwrap_or_else(|| PreviewCamera::new(&review)), meshes, animator });
        }
        let Some(m) = render.model.as_mut() else { return };
        m.camera.step(&m.review, rotate, zoom, dt * 1000.0);
        if let Some(a) = m.animator.as_mut() {
            a.update(&assets, &gpu.renderer, dt, false, false, 30.0);
        }
        let items: Vec<crate::render::DrawItem> = m
            .meshes
            .iter()
            .filter_map(|(mesh, local)| {
                let model = crate::worn::piece_model(mesh, Mat4::IDENTITY, *local, false, m.animator.as_ref())?;
                Some(crate::render::DrawItem { mesh: mesh.clone(), model, tint: [1.0; 4], character: true })
            })
            .collect();
        let (eye, _) = m.camera.eye_target(&m.review);
        let light = (m.camera.light_towards(), [PREVIEW_AMBIENT, PREVIEW_AMBIENT, PREVIEW_AMBIENT, 1.0], [PREVIEW_DIRECTIONAL, PREVIEW_DIRECTIONAL, PREVIEW_DIRECTIONAL, 1.0]);
        gpu.renderer.render_offscreen(&render.target, m.camera.view_proj(&m.review), eye, &items, light, wgpu::Color::TRANSPARENT);
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

    /// Caméra de l'aperçu (`FUN_1406f8d50`, `FUN_1406f9430`) avec le `CUSTOMIZE_REVIEW` du chevalier (TID 96).
    #[test]
    fn camera_de_l_apercu() {
        let r = Review::parse("0:162.100693:35.700043:0:75.999413:139.799347");
        assert_eq!((r.near.y, r.near.z, r.far.z), (162.100693, 35.700043, 139.799347));
        // Points confondus : `loin.z` relevé d'une unité.
        assert_eq!(Review::parse("0:0:0:0:0:0").far.z, 1.0);
        let mut c = PreviewCamera::new(&r);
        assert_eq!(c.distance, r.far.z);
        // Au départ : devant le modèle (+Z), à la hauteur du point lointain, regard horizontal.
        let (eye, target) = c.eye_target(&r);
        assert!((eye - Vec3::new(0.0, r.far.y, r.far.z)).length() < 1e-3 && target == Vec3::new(0.0, r.far.y, 0.0));
        // Rotation gauche tenue 500 ms : 60° ; droite 1 s depuis 60° : −60° → 360 − (−60) = 420 (Archonia).
        c.step(&r, -1.0, 0.0, 500.0);
        assert!((c.angle - 60.0).abs() < 1e-3);
        c.step(&r, 1.0, 0.0, 1000.0);
        assert!((c.angle - 420.0).abs() < 1e-3);
        // Zoom avant jusqu'au point proche (hauteur du visage), arrière jusqu'au point lointain.
        c.step(&r, 0.0, 1.0, 10_000.0);
        assert_eq!(c.distance, r.near.z);
        assert!((c.eye_target(&r).0.y - r.near.y).abs() < 1e-3);
        c.step(&r, 0.0, -1.0, 100.0);
        assert!((c.distance - (r.near.z + 12.0)).abs() < 1e-3);
        c.step(&r, 0.0, -1.0, 10_000.0);
        assert_eq!(c.distance, r.far.z);
        // Lumière à 45° de la caméra, plongeante (vers la lumière : y > 0).
        let l = PreviewCamera { angle: 0.0, distance: 1.0 }.light_towards();
        assert!((l - Vec3::new(1.0, 0.70710677, 1.0).normalize()).length() < 1e-5);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : 749 lignes, chevaliers 58 coiffures / 24 visages.
    #[test]
    fn table_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let t = parse_table(&super::super::npc_glue::client_text_in(&assets, "ini", "charactercustomizelist.txt"));
        assert_eq!(t.iter().filter(|e| e.tid == 96 && e.kind == 1).count(), 58);
        assert_eq!(t.iter().filter(|e| e.tid == 96 && e.kind == 2).count(), 24);
        let r = Review::for_tid(&assets, 96);
        assert!((r.near.y - 162.1).abs() < 0.01 && (r.far.z - 139.8).abs() < 0.01, "{r:?}");
    }
}
