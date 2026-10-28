//! Taxes des régions (AgpmTax 0x53) et fenêtres d'AgcmUITax d'Archonia (voir `hud_tax.rs`).
//!
//! * **Intendant** « Grand Advisor » (fonction de PNJ 34, bouton `sysstr` 609 « Tax », `FUN_1406ca6a0`) : op 0
//!   (`FUN_1406341e0`) ; l'accord (op 1) n'ouvre `UI_TaxSteward` (`FUN_140634d10`) que si le joueur est le maître
//!   du château où il se trouve (`FUN_14026e500` ; ici : propriétaire connu par le module des sièges). Titre : nom
//!   du PNJ ; texte `TAX_INTRO` avec la région du joueur traduite.
//! * **Consultation** (op 3, `FUN_140634e70`) : « Control tax rate » (mode 1) ou « Tax Ledger » (mode 0) ; la
//!   réponse (`FUN_140634320`, résultat 0) ferme l'intendant et ouvre `UI_TaxModify` ou `UI_TaxView`.
//! * **Changement** (op 2, `FUN_1406325f0`) : un entier de 0 à 50 par région, sinon boîte `TAX_MODIFY_F_EX_RANGE`
//!   sans envoi ; confirmation `TAX_MODIFY_CON_QUE` + une ligne `TAX_MODIFY_CON_REG` par région ; réponse
//!   (`FUN_140634430`) : boîte `TAX_MODIFY_SUCCESS` (table mise à jour), `TAX_MODIFY_F_NOT_TIME`,
//!   `TAX_MODIFY_F_EX_RANGE` (`TAX_MODIFY_F_NOT_OWNER` manque à `uimessage.txt` : rien).
//! * **Registre** `UI_TaxView` : 4 régions (nom, taux, recette), 5ᵉ ligne : tribut de 20 % à l'ArchLord (château de
//!   la fiche 0 des sièges, « -20 », « -(somme × 20 × 0,01) » ou « 0 » ; `FUN_1406340f0`, `FUN_140633270`), absente
//!   au château de l'ArchLord ; recettes des régions, recette du château, date du dernier versement
//!   (`%Y-%m-%d %H:%M:%S`, heure locale).
//! * Les trois fenêtres se ferment au-delà de 149 unités du point d'ouverture (`FUN_140634550`).
//! * **Taux d'une région** (op 4, à tous ; ops 2 et 3) : table des régions par nom **traduit** ; le taux du joueur
//!   (`GetTaxRatio`, `FUN_14030f890` → `FUN_14026df80`) est celui de sa région (`RegionTemplate.ini` →
//!   `RegionGlossary.txt` de la langue du client, nom coréen à défaut), 0 si elle n'est pas taxée. Il s'applique
//!   aux prix (`prix + prix × taux / 100`) des achats aux PNJ, de la réparation, du coiffeur, des portails, de
//!   l'instructeur, de la production et de la création de guilde ([`taxed`]).

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, TaxView};
use crate::net::NetLink;
use crate::zones::Zones;
use archrust_netclient::tax::{self, result, CastleTax, RegionTax, TaxCommand, TaxEvent};
use archrust_netclient::{Command, WorldEvent};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};

/// Déplacement au-delà duquel les fenêtres se ferment (`FUN_140634550`).
const CLOSE_MOVE: f32 = 149.0;
const REG: [&str; 5] = ["CTL_ST_REG1", "CTL_ST_REG2", "CTL_ST_REG3", "CTL_ST_REG4", "CTL_ST_REG5"];
const EDIT: [&str; 4] = ["CTL_ED_TAX1", "CTL_ED_TAX2", "CTL_ED_TAX3", "CTL_ED_TAX4"];
const RATIO: [&str; 5] = ["CTL_ST_RATIO1", "CTL_ST_RATIO2", "CTL_ST_RATIO3", "CTL_ST_RATIO4", "CTL_ST_RATIO5"];
const INCOME: [&str; 5] = ["CTL_ST_INCOME1", "CTL_ST_INCOME2", "CTL_ST_INCOME3", "CTL_ST_INCOME4", "CTL_ST_INCOME5"];

/// Taux de taxe de la région du joueur, revu à chaque image (`App::tax_frame`).
static RATE: AtomicI32 = AtomicI32::new(0);

/// Taux de taxe (%) de la région où se tient le joueur.
pub(super) fn rate() -> i32 {
    RATE.load(Ordering::Relaxed)
}

/// Prix avec la taxe de la région du joueur (`prix + prix × taux / 100`, division entière).
pub(super) fn taxed(price: i64) -> i64 {
    tax::taxed(price, rate())
}

/// Château affiché par `UI_TaxModify` / `UI_TaxView` (réponse à l'op 3).
#[derive(Debug, Clone, Default)]
struct Ledger {
    castle: CastleTax,
    /// Noms des régions du château, dans l'ordre reçu.
    regions: Vec<String>,
}

#[derive(Default)]
pub struct TaxGlue {
    /// Nom coréen → nom traduit (`RegionGlossary.txt`).
    glossary: Option<HashMap<String, String>>,
    /// Dernière région du joueur et son nom traduit.
    here: Option<(u16, String)>,
    /// Régions reçues (ops 2, 3, 4), par nom traduit.
    regions: HashMap<String, RegionTax>,
    ledger: Option<Ledger>,
    /// Intendant ouvert : nom du PNJ, région du joueur traduite, point d'ouverture.
    npc_name: String,
    region_label: String,
    origin: Option<Vec3>,
    /// Château où se tient le joueur (fiche des sièges) et nom de la fiche 0 (Rensphere), à l'ouverture.
    castle: String,
    castle_index: usize,
    archlord_castle: String,
    show_steward: bool,
    show_modify: bool,
    show_view: bool,
    /// Mode demandé par le dernier op 3 (vrai : `UI_TaxModify`).
    want_modify: bool,
    edits: [String; tax::REGIONS],
    /// Taux à confirmer (`TAX_MODIFY_CON_QUE`).
    confirm: Option<(String, Vec<(String, i32)>)>,
    boxes: Vec<String>,
}

impl TaxGlue {
    fn glossary(&mut self, assets: &Assets) -> &HashMap<String, String> {
        self.glossary.get_or_insert_with(|| super::portal_glue::load_glossary(assets))
    }

    /// Nom traduit d'une région de `RegionTemplate.ini` (`FUN_14048d970`), retenu pour la dernière région vue.
    fn region_name(&mut self, assets: &Assets, region: u16) -> Option<String> {
        if let Some((r, n)) = &self.here {
            if *r == region {
                return Some(n.clone());
            }
        }
        let korean = Zones::region_name(assets, region)?;
        let name = self.glossary(assets).get(korean.trim()).cloned().unwrap_or(korean);
        self.here = Some((region, name.clone()));
        Some(name)
    }

    /// Région reçue : créée ou mise à jour, taux négatif ramené à 0 (`FUN_14026dc80`).
    fn store(&mut self, r: &RegionTax) {
        if r.name.is_empty() {
            return;
        }
        let e = self.regions.entry(r.name.clone()).or_insert_with(|| RegionTax { name: r.name.clone(), ..Default::default() });
        if let Some(i) = r.income {
            e.income = Some(i);
        }
        if let Some(t) = r.ratio {
            e.ratio = Some(t.max(0));
        }
    }

    /// Taux d'une région connue par son nom traduit (0 sinon).
    fn ratio_of(&self, name: &str) -> i32 {
        self.regions.get(name).and_then(|r| r.ratio).unwrap_or(0).max(0)
    }

    fn close_all(&mut self) {
        self.show_steward = false;
        self.show_modify = false;
        self.show_view = false;
        self.confirm = None;
    }

    fn fill_edits(&mut self) {
        let names = self.ledger.as_ref().map(|l| l.regions.clone()).unwrap_or_default();
        for (i, e) in self.edits.iter_mut().enumerate() {
            *e = names.get(i).map(|n| self.regions.get(n).and_then(|r| r.ratio).unwrap_or(0).to_string()).unwrap_or_default();
        }
    }
}

/// Taux saisis (`FUN_1406325f0`) : un entier de 0 à 50 par région, sinon `None`.
pub fn parse_ratios(names: &[String], edits: &[String]) -> Option<Vec<(String, i32)>> {
    names
        .iter()
        .take(tax::REGIONS)
        .enumerate()
        .map(|(i, n)| {
            let v: i32 = edits.get(i)?.trim().parse().ok()?;
            (0..=tax::MAX_RATIO).contains(&v).then(|| (n.clone(), v))
        })
        .collect()
}

/// `TAX_MODIFY_CON_REG` (« [ %s ] Tax Rate: %3d ___ ») d'une région.
pub fn confirm_line(template: &str, name: &str, ratio: i32) -> String {
    super::pvp_glue::fill(&template.replacen("%3d", &format!("{ratio:3}"), 1), &[name.to_string()])
}

/// Tribut à l'ArchLord (ligne 5 de `UI_TaxView`) : « -(somme × 20 × 0,01) », « 0 » sans recette.
pub fn tribute_text(sum: i64) -> String {
    let t = (sum as f64 * tax::ARCHLORD_TRIBUTE as f64 * 0.01) as i64;
    if t != 0 {
        format!("-{t}")
    } else {
        "0".into()
    }
}

fn msg(hud: &mut HudState, assets: &Assets, key: &str) -> String {
    crate::hud::ui_message(hud, assets, key)
}

fn send(net: Option<&NetLink>, c: TaxCommand) {
    if let Some(net) = net {
        net.send(Command::Tax(c));
    }
}

/// Fenêtres, confirmation et boîtes (passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut TaxGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let fill = super::pvp_glue::fill;
    let mut out: Vec<String> = Vec::new();
    let castle = glue.castle.clone();

    if glue.show_steward {
        let view = TaxView {
            texts: vec![("CTL_ST_CAPTION", glue.npc_name.clone()), ("CTL_ED_INTRO", fill(&msg(hud, assets, "TAX_INTRO"), &[glue.region_label.clone()]))],
            ..Default::default()
        };
        crate::hud::draw_tax_window(ctx, hud, assets, "UI_TaxSteward", view, &mut out);
        for c in out.drain(..) {
            match c.as_str() {
                "CTL_BTN_SYSCLOSE" => glue.close_all(),
                // `TAX_MODIFY_RATIO` / `TAX_VIEW_INCOME` : op 3 avec le nom du château seul.
                "CTL_BTN_MODIFY" | "CTL_BTN_VIEW" => {
                    glue.want_modify = c == "CTL_BTN_MODIFY";
                    send(net, TaxCommand::View(castle.clone()));
                }
                _ => {}
            }
        }
    }

    let ledger = glue.ledger.clone().unwrap_or_default();
    if glue.show_modify {
        let mut texts = vec![("CTL_ST_CAPTION", fill(&msg(hud, assets, "TAX_MODIFY_CAPTION"), &[ledger.castle.name.clone()]))];
        for (i, n) in ledger.regions.iter().take(tax::REGIONS).enumerate() {
            texts.push((REG[i], n.clone()));
        }
        let shown = ledger.regions.len().min(tax::REGIONS);
        let edits: Vec<(&str, &mut String)> = EDIT.iter().copied().zip(glue.edits.iter_mut()).take(shown).collect();
        crate::hud::draw_tax_window(ctx, hud, assets, "UI_TaxModify", TaxView { texts, edits }, &mut out);
        for c in out.drain(..) {
            match c.as_str() {
                "CTL_BTN_CONFIRM" if glue.confirm.is_none() => match parse_ratios(&ledger.regions, &glue.edits) {
                    None => glue.boxes.push(msg(hud, assets, "TAX_MODIFY_F_EX_RANGE")),
                    Some(list) => {
                        let line = msg(hud, assets, "TAX_MODIFY_CON_REG");
                        let mut text = format!("{}___", msg(hud, assets, "TAX_MODIFY_CON_QUE"));
                        for (n, v) in &list {
                            text.push_str(&confirm_line(&line, n, *v));
                        }
                        glue.confirm = Some((text, list));
                    }
                },
                "CTL_BTN_RESET" => {
                    for e in glue.edits.iter_mut().take(shown) {
                        *e = "0".into();
                    }
                }
                // `TAX_BACK` : retour à l'intendant.
                "CTL_BTN_BACK" | "CTL_BTN_SYSCLOSE" => {
                    glue.show_modify = false;
                    glue.confirm = None;
                    glue.show_steward = true;
                }
                _ => {}
            }
        }
    }

    if glue.show_view {
        let mut texts = vec![("CTL_ST_CAPTION", fill(&msg(hud, assets, "TAX_VIEW_CAPTION"), &[ledger.castle.name.clone()]))];
        let mut sum = 0i64;
        for (i, n) in ledger.regions.iter().take(tax::REGIONS).enumerate() {
            let r = glue.regions.get(n).cloned().unwrap_or_default();
            sum += r.income.unwrap_or(0);
            texts.push((REG[i], n.clone()));
            texts.push((RATIO[i], r.ratio.unwrap_or(0).to_string()));
            texts.push((INCOME[i], r.income.unwrap_or(0).to_string()));
        }
        // Tribut à l'ArchLord, sauf au château de l'ArchLord.
        if glue.castle_index != 0 && !glue.archlord_castle.is_empty() {
            texts.push((REG[4], glue.archlord_castle.clone()));
            texts.push((RATIO[4], format!("-{}", tax::ARCHLORD_TRIBUTE)));
            texts.push((INCOME[4], tribute_text(sum)));
        }
        texts.push(("CTL_ST_REC_INCOME", sum.to_string()));
        texts.push(("CTL_ST_TOT_INCOME", ledger.castle.income.unwrap_or(0).to_string()));
        if let Some(t) = ledger.castle.transferred.filter(|t| *t > 0) {
            texts.push(("CTL_ST_TR_DATE", super::mail_glue::format_date(t as i64)));
        }
        crate::hud::draw_tax_window(ctx, hud, assets, "UI_TaxView", TaxView { texts, ..Default::default() }, &mut out);
        for c in out.drain(..) {
            if matches!(c.as_str(), "CTL_BTN_BACK" | "CTL_BTN_SYSCLOSE") {
                glue.show_view = false;
                glue.show_steward = true;
            }
        }
    }

    if let Some((text, list)) = glue.confirm.clone() {
        match crate::hud::confirm_box(ctx, hud, assets, "archonia-tax-confirm", &text) {
            Some(true) => {
                glue.confirm = None;
                send(net, TaxCommand::Modify { castle: ledger.castle.name.clone(), regions: list });
            }
            Some(false) => glue.confirm = None,
            None => {}
        }
    }

    if let Some(text) = glue.boxes.first().cloned() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.boxes.remove(0);
        }
    }
}

impl App {
    /// CID connu du client : soi ou un personnage en vue (sinon le paquet est ignoré, `FUN_14026c7b0`).
    fn tax_known(&self, cid: i32) -> bool {
        self.player.as_ref().is_some_and(|p| p.cid == cid) || self.entities.iter().any(|e| e.cid == cid)
    }

    fn tax_is_me(&self, cid: i32) -> bool {
        self.player.as_ref().is_some_and(|p| p.cid == cid)
    }

    /// Paquets des taxes.
    pub(super) fn on_tax_event(&mut self, ev: TaxEvent) {
        let assets = self.assets.clone();
        match ev {
            TaxEvent::Request { .. } => {}
            TaxEvent::Granted { cid, npc } => {
                if !self.tax_is_me(cid) {
                    return;
                }
                let me = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                let (here, first) = self.siege_tax_castles();
                let Some((castle, index, owner)) = here else {
                    log::info!("taxes : intendant hors d'un château connu, fenêtre non ouverte");
                    return;
                };
                // `FUN_14026e500` : maître du château où se tient le joueur.
                if owner.as_deref() != Some(me.as_str()) {
                    log::info!("taxes : {me} n'est pas le maître de {castle} ({owner:?}), fenêtre non ouverte");
                    return;
                }
                let region = self.zone_region.as_ref().map(|z| z.region);
                let label = region.and_then(|r| self.tax.region_name(&assets, r)).unwrap_or_default();
                let npc_name = npc.and_then(|n| self.entities.iter().find(|e| e.cid == n)).map(|e| e.name.clone()).unwrap_or_default();
                let t = &mut self.tax;
                t.close_all();
                t.npc_name = npc_name;
                t.region_label = label;
                t.castle = castle;
                t.castle_index = index;
                t.archlord_castle = first.unwrap_or_default();
                t.origin = self.player.as_ref().map(|p| p.position);
                t.show_steward = true;
            }
            TaxEvent::Modified { cid, result: code, castle, regions } => {
                if !self.tax_known(cid) {
                    return;
                }
                if code == result::SUCCESS {
                    for r in &regions {
                        self.tax.store(r);
                    }
                    if let (Some(c), Some(l)) = (castle, self.tax.ledger.as_mut()) {
                        if l.castle.name == c.name || c.name.is_empty() {
                            l.castle.income = c.income.or(l.castle.income);
                            l.castle.transferred = c.transferred.or(l.castle.transferred);
                            l.castle.modified = c.modified.or(l.castle.modified);
                        }
                    }
                    self.tax.fill_edits();
                }
                if self.tax_is_me(cid) {
                    if let Some(key) = result::message(code) {
                        let t = msg(&mut self.hud, &assets, key);
                        if !t.trim().is_empty() {
                            self.tax.boxes.push(t);
                        }
                    }
                }
            }
            TaxEvent::Castle { cid, result: code, castle, regions } => {
                if !self.tax_known(cid) {
                    return;
                }
                for r in &regions {
                    self.tax.store(r);
                }
                if !self.tax_is_me(cid) || code != 0 {
                    return;
                }
                let t = &mut self.tax;
                t.ledger = Some(Ledger { castle: castle.unwrap_or_else(|| CastleTax { name: t.castle.clone(), ..Default::default() }), regions: regions.iter().map(|r| r.name.clone()).collect() });
                t.fill_edits();
                t.show_steward = false;
                t.confirm = None;
                if t.want_modify {
                    t.show_modify = true;
                    t.show_view = false;
                } else {
                    t.show_view = true;
                    t.show_modify = false;
                }
            }
            TaxEvent::Region { cid, region } => {
                if self.tax_known(cid) {
                    self.tax.store(&region);
                }
            }
        }
    }

    /// Taux de la région du joueur ; fermeture des fenêtres en s'éloignant.
    pub(super) fn tax_frame(&mut self) {
        let assets = self.assets.clone();
        let ratio = match self.zone_region.as_ref().map(|z| z.region) {
            Some(r) if !self.tax.regions.is_empty() => self.tax.region_name(&assets, r).map_or(0, |n| self.tax.ratio_of(&n)),
            _ => 0,
        };
        RATE.store(ratio, Ordering::Relaxed);
        let t = &mut self.tax;
        if t.show_steward || t.show_modify || t.show_view {
            let pos = self.player.as_ref().map(|p| p.position);
            if let (Some(o), Some(p)) = (t.origin, pos) {
                if o.distance(p) > CLOSE_MOVE {
                    t.close_all();
                }
            }
        }
    }

    /// Remises à zéro.
    pub(super) fn tax_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Disconnected(_) | WorldEvent::Connected) {
            let glossary = self.tax.glossary.take();
            self.tax = TaxGlue { glossary, ..Default::default() };
            RATE.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saisie_et_textes() {
        let names: Vec<String> = vec!["A".into(), "B".into()];
        assert_eq!(parse_ratios(&names, &["5".into(), "50".into()]), Some(vec![("A".into(), 5), ("B".into(), 50)]));
        assert_eq!(parse_ratios(&names, &["5".into(), "51".into()]), None);
        assert_eq!(parse_ratios(&names, &["-1".into(), "0".into()]), None);
        assert_eq!(parse_ratios(&names, &["".into(), "0".into()]), None);
        assert_eq!(confirm_line("[ %s ] Tax Rate: %3d ___", "Inflame", 7), "[ Inflame ] Tax Rate:   7 ___");
        assert_eq!((tribute_text(1000), tribute_text(4), tribute_text(0)), ("-200".to_string(), "0".to_string(), "0".to_string()));
        let mut g = TaxGlue::default();
        g.store(&RegionTax { name: "A".into(), income: Some(10), ratio: Some(-3) });
        g.store(&RegionTax { name: "A".into(), income: None, ratio: None });
        assert_eq!((g.ratio_of("A"), g.regions["A"].income, g.ratio_of("Z")), (0, Some(10), 0));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : fenêtres d'Archonia et textes.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        for (w, c) in [("UI_TaxSteward", "CTL_ED_INTRO"), ("UI_TaxSteward", "CTL_BTN_VIEW"), ("UI_TaxModify", "CTL_ED_TAX4"), ("UI_TaxModify", "CTL_BTN_RESET"), ("UI_TaxView", "CTL_ST_INCOME5"), ("UI_TaxView", "CTL_ST_TR_DATE")] {
            let win = crate::hud::UiWindow::parse(&ui, None, w).unwrap_or_else(|| panic!("{w}"));
            assert!(win.control(c).is_some(), "{w} {c}");
        }
        for id in [609, 630, 642, 650, 656, 672] {
            assert!(super::super::npc_glue::sysstr(&assets, id).is_some_and(|t| !t.is_empty()), "{id}");
        }
        let mut g = TaxGlue::default();
        assert!(!g.glossary(&assets).is_empty());
        // Région du château d'Inflame (80) : nom coréen trouvé dans le glossaire de la langue du client.
        let korean = Zones::region_name(&assets, 80).unwrap();
        assert!(g.glossary(&assets).contains_key(korean.trim()), "{korean}");
        assert!(g.region_name(&assets, 80).is_some_and(|n| !n.is_empty() && n != korean));
    }
}
