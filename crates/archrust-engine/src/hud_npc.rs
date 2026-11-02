//! Fenêtres des PNJ d'Archonia : dialogue (`UI_NPCDialog`), boutique (`UI_NPCTrade`), banque
//! (`UI_Bank`), quêtes d'un PNJ (`QuestAccept_UI`) et journal des quêtes (`QuestMain_UI`).
//!
//! Fonds, boutons et grilles viennent d'`UI_1024X768.ini` ; le texte et les listes sont dessinés
//! dans les zones prévues. Les demandes reviennent au moteur en [`NpcUi`] (voir `app_npc.rs`).

use super::items_ui::ItemDrag;
use super::*;

/// Demande d'une fenêtre de PNJ.
#[derive(Debug, Clone, PartialEq)]
pub enum NpcUi {
    CloseDialog,
    /// Bouton du dialogue : fonction du PNJ (9 boutique, 20 banque, 27 quêtes…).
    Function(i32),
    CloseShop,
    /// Achat (clic droit : un exemplaire ; Maj + clic droit : quantité à choisir).
    Buy { iid: i32, ask_count: bool },
    /// Objet du sac lâché sur la boutique : vente.
    Sell(i32),
    CloseBank,
    /// Objet lâché sur une case de la banque (du sac : dépôt ; de la banque : déplacement).
    BankDrop { drag: ItemDrag, grid: [i16; 3] },
    /// Clic droit sur un objet de la banque : retrait vers le sac.
    BankTake(i32),
    /// Bouton « Déposer » ou « Retirer » de l'argent.
    BankMoney { deposit: bool },
    /// Bouton « Agrandir » (`Item_RequestBuyBankSlot`) : fenêtre d'achat d'une page.
    BankBuyRequest,
    CloseQuests,
    SelectQuest(i32),
    QuestAccept(i32),
    QuestComplete(i32),
    QuestAbandon(i32),
    /// Clic sur une récompense au choix : rang parmi les objets choisissables, à partir de 0.
    QuestSelectReward(usize),
}

/// Dialogue ouvert.
pub struct DialogView {
    pub npc_name: String,
    pub text: String,
    /// Fonctions proposées (fonction, libellé).
    pub buttons: Vec<(i32, String)>,
    /// Dernier bouton (`StaticText_Exit`).
    pub exit_label: String,
}

/// Objet de la boutique.
pub struct ShopEntry {
    pub iid: i32,
    pub tid: i32,
    pub price: i64,
    pub count: i32,
    pub grid: [i16; 3],
    pub name: String,
    /// Infobulle : nom puis « Prix d'achat : » et les prix non nuls (or, Shrine Coins, charisme), comme
    /// Archonia (FUN_1405479c0) ; vide : le nom seul.
    pub tip: Vec<(String, Color32)>,
}

pub struct ShopView<'a> {
    pub npc_name: &'a str,
    pub items: &'a [ShopEntry],
    pub loading: bool,
    /// Première ligne affichée (défilement).
    pub first_row: &'a mut i16,
}

pub struct BankView<'a> {
    /// Objets en banque (statut 100) et leur nom.
    pub items: &'a [(ItemRecord, String)],
    pub money: Option<i64>,
    /// Pages ouvertes (la première toujours).
    pub pages: i16,
    pub page: &'a mut i16,
    /// `IsActiveBuyBankSlot` : moins de 3 pages en plus et la suivante en vente contre de l'or.
    pub can_buy: bool,
}

/// État d'une quête dans une liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestState {
    /// Proposée par le PNJ.
    Available,
    /// Au journal, objectifs en cours.
    InProgress,
    /// Au journal, objectifs remplis : à rendre.
    Ready,
    /// Déjà terminée.
    Done,
}

pub struct QuestEntry {
    pub tid: i32,
    pub name: String,
    pub state: QuestState,
}

pub struct QuestView<'a> {
    /// Fenêtre d'un PNJ (`QuestAccept_UI`) ou journal (`QuestMain_UI`).
    pub journal: bool,
    pub title: String,
    pub entries: &'a [QuestEntry],
    pub selected: Option<i32>,
    pub text: String,
    /// Libellé du bouton de fin (`CONTROL_8149`, « QusetAcceptLeftButton_Display2 », FUN_1406559a0) :
    /// sysstr 432 pour une quête proposée, 431 sinon.
    pub left_label: String,
    /// Récompense au choix retenue (rang parmi les objets choisissables).
    pub reward_choice: Option<usize>,
}

pub(super) fn window(hud: &mut HudState, assets: &Assets, name: &str) -> Option<UiWindow> {
    hud.windows(assets).iter().find(|w| w.name == name).cloned()
}

/// Bouton d'Archonia : textures « a » (repos), « b » (survol), « c » (appui), « d » (inactif).
#[allow(clippy::too_many_arguments)]
pub(super) fn button(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, label: &str, font: f32, enabled: bool, id: impl std::hash::Hash) -> egui::Response {
    let resp = ui.interact(r, egui::Id::new(id), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
    let tex = if !enabled {
        pick("d.png")
    } else if resp.is_pointer_button_down_on() {
        pick("c.png")
    } else if resp.hovered() {
        pick("b.png")
    } else {
        pick("a.png")
    }
    .or_else(|| button_texture(c).map(str::to_string));
    match tex {
        Some(t) if hud.image(ctx, assets, &t).is_some() => control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0),
        _ => {
            painter.rect_filled(r, 3.0, if resp.hovered() && enabled { Color32::from_rgb(110, 80, 40) } else { Color32::from_rgb(70, 50, 30) });
        }
    }
    if !label.is_empty() {
        // Libellé réduit s'il dépasse le bouton (ex. « Réinitialiser les positions de l'interface »).
        let width = painter.layout_no_wrap(label.to_string(), FontId::proportional(font), Color32::WHITE).size().x;
        let font = if width > r.width() - 4.0 && width > 0.0 { (font * (r.width() - 4.0) / width).max(6.0) } else { font };
        shadow_text(painter, r.center(), Align2::CENTER_CENTER, label, font, if enabled { Color32::WHITE } else { Color32::GRAY });
    }
    resp
}

/// Zone de texte défilante dans le rectangle `r`.
fn text_box(ui: &mut egui::Ui, r: Rect, id: &str, text: &str, font: f32, color: Color32) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(r.height()).show(ui, |ui| {
            ui.set_width(r.width() - 12.0);
            ui.label(egui::RichText::new(text).size(font).color(color).strong());
        });
    });
}

/// Morceau du texte d'une quête : texte coloré ou balise `<ITEM::nombre::TID::…::choix>`.
#[derive(Debug, Clone, PartialEq)]
enum QuestPart {
    Text(String),
    Item { count: i32, tid: i32, selectable: bool },
}

/// Découpe le texte d'une quête aux balises `<ITEM::…>` (six nombres, AgcUIQuestText FUN_14088f9a0 :
/// le premier est le nombre, le deuxième le TID, le dernier 1 pour un objet à choisir).
fn quest_parts(text: &str) -> Vec<QuestPart> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<ITEM::") {
        let Some(end) = rest[i..].find('>') else { break };
        if i > 0 {
            out.push(QuestPart::Text(rest[..i].to_string()));
        }
        let v: Vec<i32> = rest[i + 7..i + end].split("::").map(|p| p.trim().parse().unwrap_or(0)).collect();
        out.push(QuestPart::Item { count: v.first().copied().unwrap_or(0), tid: v.get(1).copied().unwrap_or(0), selectable: v.get(5).copied().unwrap_or(0) != 0 });
        rest = &rest[i + end + 1..];
    }
    if !rest.is_empty() {
        out.push(QuestPart::Text(rest.to_string()));
    }
    out
}

/// Texte d'une quête (`CONTROL_QUEST_TEXT`, AgcUIQuestText) : texte coloré, objets en cases
/// `Quest_Slot.png` avec leur icône et leur nombre (« %d », 0xFFFF9999, en bas à droite, FUN_14088cd40) ;
/// un objet à choisir se clique et prend le cadre `Quest_ActiveBox.png` (un seul à la fois, LAB_14088c840).
#[allow(clippy::too_many_arguments)]
fn quest_text_box(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, r: Rect, id: &str, text: &str, font: f32, base: Color32, choice: Option<usize>, out: &mut Vec<NpcUi>) {
    let parts = quest_parts(text);
    let slot_tex = hud.image(ctx, assets, "Quest_Slot.png");
    let active_tex = hud.image(ctx, assets, "Quest_ActiveBox.png");
    let cell = slot_tex.map_or(36.0, |(_, size)| size[0].max(16.0)) * (font / 11.0).max(0.5);
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(r.height()).show(ui, |ui| {
            ui.set_width(r.width() - 12.0);
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
            let mut k = 0;
            let mut selectable_rank = 0usize;
            while k < parts.len() {
                match &parts[k] {
                    QuestPart::Text(t) => {
                        let mut job = egui::text::LayoutJob::default();
                        job.wrap.max_width = r.width() - 12.0;
                        for (part, color) in super::convert_ui::colored_segments(if k > 0 { t.strip_prefix('\n').unwrap_or(t) } else { t }, base) {
                            let color = if color == Color32::WHITE { base } else { color };
                            job.append(&part, 0.0, egui::TextFormat { font_id: FontId::proportional(font), color, ..Default::default() });
                        }
                        ui.label(job);
                        k += 1;
                    }
                    QuestPart::Item { .. } => {
                        // Objets consécutifs : une rangée de cases.
                        ui.horizontal_wrapped(|ui| {
                            while let Some(QuestPart::Item { count, tid, selectable }) = parts.get(k) {
                                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cell), if *selectable { egui::Sense::click() } else { egui::Sense::hover() });
                                let painter = ui.painter();
                                if let Some((tex, _)) = slot_tex {
                                    painter.image(tex, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                }
                                super::items_ui::icon(ctx, hud, assets, painter, rect.shrink(3.0 * cell / 36.0), Some(*tid), Color32::WHITE);
                                if *count != 0 {
                                    shadow_text(painter, rect.right_bottom() - Vec2::splat(3.0), Align2::RIGHT_BOTTOM, &count.to_string(), font * 0.95, Color32::from_rgb(0xff, 0x99, 0x99));
                                }
                                if *selectable {
                                    let rank = selectable_rank;
                                    selectable_rank += 1;
                                    if choice == Some(rank) {
                                        match active_tex {
                                            Some((tex, _)) => {
                                                painter.image(tex, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                            }
                                            None => {
                                                painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
                                            }
                                        }
                                    }
                                    if resp.clicked() {
                                        out.push(NpcUi::QuestSelectReward(rank));
                                    }
                                }
                                if resp.hovered() {
                                    let name = crate::scene::item_name_for_tid(assets, *tid).unwrap_or_default();
                                    if !name.is_empty() {
                                        super::item_tooltip(ctx, &[(name, Color32::WHITE)], "");
                                    }
                                }
                                k += 1;
                            }
                        });
                    }
                }
            }
        });
    });
}

/// Fenêtre déplaçable aux dimensions d'Archonia, avec son fond.
pub(super) fn frame<R>(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, w: &UiWindow, id: &str, add: impl FnOnce(&mut egui::Ui, &mut HudState, &Layout, Pos2, &egui::Painter) -> R) -> Option<R> {
    let layout = Layout::new(ctx.screen_rect());
    let size = Vec2::new(w.size[0], w.size[1]) * layout.scale;
    egui::Area::new(egui::Id::new(id))
        .default_pos(layout.window_origin(w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            match w.textures.first() {
                Some((_, tex)) if hud.image(ctx, assets, tex).is_some() => window_image(ctx, hud, assets, &painter, rect, w.size, tex),
                _ => {
                    painter.rect_filled(rect, 4.0, Color32::from_black_alpha(220));
                }
            }
            add(ui, hud, &layout, rect.min, &painter)
        })
        .inner
        .into()
}

/// Dialogue d'un PNJ : texte, boutons des fonctions, fermeture.
pub fn draw_npc_dialog(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &DialogView, out: &mut Vec<NpcUi>) {
    let Some(w) = window(hud, assets, "UI_NPCDialog") else { return };
    frame(ctx, hud, assets, &w, "archonia-npc-dialog", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                // Titre coupé au bord de son contrôle et avant la croix : un nom long (« <Tullan> Portail
                // vers la Frontière chaotique ») passait sous la croix de fermeture.
                "CONTROL_31156" => {
                    let x_left = w.control("CONTROL_NPCDialogQuitButton").map_or(f32::INFINITY, |x| layout.rect(origin, x.pos, x.size).left() - 2.0 * s);
                    let clip = r.intersect(Rect::everything_left_of(x_left));
                    shadow_text(&painter.with_clip_rect(clip), r.left_center(), Align2::LEFT_CENTER, &view.npc_name, font, Color32::from_rgb(255, 230, 160));
                }
                "CONTROL_NPCDialogEdit" => text_box(ui, r, "npc-dialog-text", &view.text, font * 1.05, Color32::from_rgb(230, 222, 200)),
                "CONTROL_NPCDialogQuitButton" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "npc-dialog-x").clicked() {
                        out.push(NpcUi::CloseDialog);
                    }
                }
                "CONTROL_NPCDialogButton" => {
                    let mut labels: Vec<(Option<i32>, &str)> = view.buttons.iter().map(|(f, l)| (Some(*f), l.as_str())).collect();
                    labels.push((None, view.exit_label.as_str()));
                    for (i, (f, label)) in labels.into_iter().enumerate() {
                        let rr = layout.rect(origin, [c.pos[0], c.pos[1] + i as f32 * 28.0], c.size);
                        if button(ctx, ui, hud, assets, painter, rr, c, label, font, true, ("npc-dialog-fn", i)).clicked() {
                            out.push(f.map_or(NpcUi::CloseDialog, NpcUi::Function));
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Grille d'objets (boutique ou banque) : rend la case (sac, ligne, colonne) de chaque case dessinée.
fn grid_cells(c: &UiControl, layout: &Layout, origin: Pos2, page: i16, first_row: i16) -> Vec<(Rect, [i16; 3])> {
    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
    let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as i16;
    let rows = ((c.size[1] + gy) / (ch + gy)).floor().max(1.0) as i16;
    (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (row, col)))
        .map(|(row, col)| (layout.rect(origin, [c.pos[0] + col as f32 * (cw + gx), c.pos[1] + row as f32 * (ch + gy)], [cw, ch]), [page, row + first_row, col]))
        .collect()
}

/// Boutique d'un PNJ : objets et prix (clic droit : acheter ; objet du sac lâché ici : vendre).
pub fn draw_shop(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: ShopView, out: &mut Vec<NpcUi>) {
    let Some(w) = window(hud, assets, "UI_NPCTrade") else { return };
    frame(ctx, hud, assets, &w, "archonia-npc-shop", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|d| *d);
        let shift = ui.input(|i| i.modifiers.shift);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_23250" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, c.fixed_label(), font, Color32::from_rgb(255, 230, 160)),
                "CONTROL_8009" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "npc-shop-x").clicked() {
                        out.push(NpcUi::CloseShop);
                    }
                }
                "CONTROL_30471" => {
                    if button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, "npc-shop-close").clicked() {
                        out.push(NpcUi::CloseShop);
                    }
                }
                "CONTROL_20194" | "CONTROL_23924" => {
                    let up = c.name == "CONTROL_20194";
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("npc-shop-page", up)).clicked() {
                        *view.first_row = if up { (*view.first_row - 7).max(0) } else { (*view.first_row + 7).min(13) };
                    }
                }
                "CONTROL_NPCTradeGrid" => {
                    for (cell, grid) in grid_cells(c, layout, origin, 0, *view.first_row) {
                        let resp = ui.interact(cell, egui::Id::new(("npc-shop-cell", grid)), egui::Sense::click());
                        if let Some(e) = view.items.iter().find(|e| e.grid == grid) {
                            items_ui::icon(ctx, hud, assets, painter, cell, Some(e.tid), Color32::WHITE);
                            if e.count > 1 {
                                shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &e.count.to_string(), font, Color32::WHITE);
                            }
                            if resp.secondary_clicked() {
                                out.push(NpcUi::Buy { iid: e.iid, ask_count: shift });
                            }
                            if dragging.is_none() && resp.hovered() {
                                if e.tip.is_empty() {
                                    resp.clone().on_hover_text(e.name.clone());
                                } else {
                                    super::item_tooltip(ctx, &e.tip, "");
                                }
                            }
                        }
                        if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                            if !drag.worn && !drag.bank {
                                out.push(NpcUi::Sell(drag.iid));
                            }
                        }
                        let target = dragging.is_some_and(|d| !d.worn && !d.bank) && resp.contains_pointer();
                        if resp.hovered() || target {
                            let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                            painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Banque : quatre pages de 7 × 4 cases, argent, dépôt et retrait.
pub fn draw_bank(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: BankView, out: &mut Vec<NpcUi>) {
    let Some(w) = window(hud, assets, "UI_Bank") else { return };
    frame(ctx, hud, assets, &w, "archonia-bank", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|d| *d);
        let tabs = ["CONTROL_6581", "CONTROL_8957", "CONTROL_3703", "CONTROL_830530"];
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            if let Some(page) = tabs.iter().position(|t| *t == c.name).map(|p| p as i16) {
                let open = page < view.pages;
                let selected = page == *view.page;
                let tex = c.textures.iter().find(|(i, _)| *i == if !open { 3 } else if selected { 2 } else { 1 }).or(c.textures.first()).map(|(_, n)| n.clone());
                if let Some(t) = tex {
                    control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                }
                let color = if !open { Color32::GRAY } else if selected { Color32::from_rgb(255, 230, 160) } else { Color32::from_rgb(190, 190, 190) };
                shadow_text(painter, r.center(), Align2::CENTER_CENTER, &(page + 1).to_string(), font, color);
                let resp = ui.interact(r, egui::Id::new(("bank-tab", page)), if open { egui::Sense::click() } else { egui::Sense::hover() });
                if resp.clicked() {
                    *view.page = page;
                }
                continue;
            }
            match c.name.as_str() {
                "CONTROL_32725" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, c.fixed_label(), font, Color32::from_rgb(255, 230, 160)),
                "CONTROL_20738" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "bank-x").clicked() {
                        out.push(NpcUi::CloseBank);
                    }
                }
                "CONTROL_28474" => {
                    let text = view.money.map_or(String::new(), format_money);
                    shadow_text(painter, r.right_center(), Align2::RIGHT_CENTER, &text, font, Color32::from_rgb(255, 220, 120));
                }
                "CONTROL_481" | "CONTROL_20471" => {
                    let deposit = c.name == "CONTROL_481";
                    if button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font * 0.95, true, ("bank-money", deposit)).clicked() {
                        out.push(NpcUi::BankMoney { deposit });
                    }
                }
                // Agrandissement (`Item_RequestBuyBankSlot`), actif selon `IsActiveBuyBankSlot`.
                "CONTROL_26485" => {
                    let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
                    if button(ctx, ui, hud, assets, painter, r, c, label, font * 0.95, view.can_buy, "bank-expand").clicked() {
                        out.push(NpcUi::BankBuyRequest);
                    }
                }
                "CONTROL_16826" => {
                    for (cell, grid) in grid_cells(c, layout, origin, *view.page, 0) {
                        let resp = ui.interact(cell, egui::Id::new(("bank-cell", grid)), egui::Sense::click_and_drag());
                        if let Some((item, name)) = view.items.iter().find(|(i, _)| i.grid == Some(grid)) {
                            let lifted = dragging.is_some_and(|d| d.iid == item.id);
                            items_ui::icon(ctx, hud, assets, painter, cell, item.template_id, if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
                            let count = item.count.unwrap_or(1);
                            if count > 1 && !lifted {
                                shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                            }
                            if resp.dragged() {
                                resp.dnd_set_drag_payload(ItemDrag { iid: item.id, tid: item.template_id.unwrap_or(0), worn: false, bank: true });
                            }
                            if resp.secondary_clicked() {
                                out.push(NpcUi::BankTake(item.id));
                            }
                            if dragging.is_none() {
                                let label = name.clone();
                                resp.clone().on_hover_text(label);
                            }
                        }
                        if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                            if !drag.worn {
                                out.push(NpcUi::BankDrop { drag: *drag, grid });
                            }
                        }
                        let target = dragging.is_some_and(|d| !d.worn) && resp.contains_pointer();
                        if resp.hovered() || target {
                            let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                            painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Quêtes d'un PNJ ou journal : liste à gauche, texte de la quête choisie à droite, boutons.
pub fn draw_quests(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: QuestView, out: &mut Vec<NpcUi>) {
    let name = if view.journal { "QuestMain_UI" } else { "QuestAccept_UI" };
    let Some(w) = window(hud, assets, name) else { return };
    let id = if view.journal { "archonia-quest-journal" } else { "archonia-quest-npc" };
    frame(ctx, hud, assets, &w, id, |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let selected = view.entries.iter().find(|e| Some(e.tid) == view.selected);
        let state = selected.map(|e| e.state);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_16487" | "CTL_ST_TITLE" => shadow_text(painter, r.center(), Align2::CENTER_CENTER, c.fixed_label(), font * 1.1, Color32::from_rgb(255, 230, 160)),
                "CONTROL_QUEST_EXIT" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, (id, "x")).clicked() {
                        out.push(NpcUi::CloseQuests);
                    }
                }
                "CONTROL_QUEST_CLOSE" => {
                    if button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, (id, "close")).clicked() {
                        out.push(NpcUi::CloseQuests);
                    }
                }
                // Fenêtre d'un PNJ : `Quest_CompleteFromTree` (FUN_140655330), quête à rendre seulement.
                "CONTROL_8149" if !view.journal => {
                    let enabled = state == Some(QuestState::Ready);
                    if button(ctx, ui, hud, assets, painter, r, c, &view.left_label, font, enabled, (id, "complete")).clicked() {
                        if let Some(e) = selected {
                            out.push(NpcUi::QuestComplete(e.tid));
                        }
                    }
                }
                // `Quest_AcceptFromTree` (FUN_140655180), quête proposée ; à la même place que « Supprimer la
                // quete » (`CONTROL_QUEST_DEL`) : l'un ou l'autre selon la quête choisie.
                "CONTROL_QUEST_ACCEPT_NEW" if !view.journal && !matches!(state, Some(QuestState::InProgress | QuestState::Ready)) => {
                    let enabled = state == Some(QuestState::Available);
                    if button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, enabled, (id, "accept")).clicked() {
                        if let Some(e) = selected {
                            out.push(NpcUi::QuestAccept(e.tid));
                        }
                    }
                }
                // `QuestCancel_Button2` (FUN_140654f30) : confirmation `QuestCancelDlg_Open`, puis abandon.
                "CONTROL_QUEST_DEL" if view.journal || matches!(state, Some(QuestState::InProgress | QuestState::Ready)) => {
                    let enabled = matches!(state, Some(QuestState::InProgress | QuestState::Ready));
                    if button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, enabled, (id, "del")).clicked() {
                        if let Some(e) = selected {
                            out.push(NpcUi::QuestAbandon(e.tid));
                        }
                    }
                }
                // « Ajouter au journal » (`QuestDiary_AddQuest`, FUN_1406547d0 : suivi `AgcmUIQuestJournal`) et
                // « Terminer immediatement » (`Quest_CompleteInstantly`, contre des CC) : non repris, inactifs.
                "CONTROL_QUEST_SHARE" | "CONTROL_QUEST_SKIP_NEW" => {
                    button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, false, (id, c.name.as_str()));
                }
                "CONTROL_QUEST_TREE" => {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                        egui::ScrollArea::vertical().id_salt((id, "tree")).max_height(r.height()).show(ui, |ui| {
                            ui.set_width(r.width() - 24.0 * s);

                            for e in view.entries {
                                let color = match e.state {
                                    QuestState::Available => Color32::from_rgb(255, 220, 120),
                                    QuestState::InProgress => Color32::from_rgb(200, 200, 200),
                                    QuestState::Ready => Color32::from_rgb(140, 230, 120),
                                    QuestState::Done => Color32::GRAY,
                                };
                                let text = egui::RichText::new(e.name.as_str()).size(font * 1.05).color(color);
                                if ui.selectable_label(Some(e.tid) == view.selected, text).clicked() {
                                    out.push(NpcUi::SelectQuest(e.tid));
                                }
                            }
                        });
                    });
                }
                // Texte sur parchemin : codes de couleur d'Archonia, le blanc (`<HFFFFFFFF>`) en brun foncé.
                "CONTROL_QUEST_TEXT" => quest_text_box(ctx, ui, hud, assets, r.shrink(8.0 * s), &format!("{id}-text"), &view.text, font * 1.05, Color32::from_rgb(35, 22, 8), view.reward_choice, out),
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupe_aux_balises_d_objets() {
        let parts = quest_parts("a<ITEM::2::100::0::0::0::0><ITEM::1::200::0::0::0::1>\nb");
        assert_eq!(
            parts,
            vec![
                QuestPart::Text("a".into()),
                QuestPart::Item { count: 2, tid: 100, selectable: false },
                QuestPart::Item { count: 1, tid: 200, selectable: true },
                QuestPart::Text("\nb".into()),
            ]
        );
    }
}
