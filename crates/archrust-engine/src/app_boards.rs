//! Panneaux au-dessus des personnages, côté moteur : nom et guilde de chaque personnage visible
//! (`AgcmIDBoard`), chiffres de dégâts (`AgcmDmgBoard`) et mentions (`AgcmImoticonBoard`) des
//! coups qui touchent le joueur ou qu'il porte. Calculs dans [`crate::boards`] ; ici l'état du
//! jeu (entités, gabarits, guildes) et le dessin egui.
//!
//! Hauteur de l'ancre : colonne `HEIGHT` de `charactertemplateclient.ini` (plus le
//! `RIDER_HEIGHT` de la monture à cheval, voir `app_ride.rs`), décalée de `DEPTH` vers l'avant.

use super::App;
use crate::assets::Assets;
use crate::boards::{self, DmgBoard, DmgKind, GuildView, IdKind, Mention, MentionKind, Quad};
use crate::hud::HudState;
use archrust_netclient::world::ActionFactorPoints;
use egui::{Color32, FontFamily, FontId, Pos2, Rect};
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

/// Échelle des panneaux. Archonia prend le maximum de 1 et d'un facteur du moteur (`+0x82c` de
/// l'objet global `0x140f8a1c0`) qui n'a pas été identifié : ArchRust garde 1 (écart).
const SCALE: f32 = 1.0;
/// Famille egui de la police des noms (police n° 0 d'AgcmFont).
const ID_FAMILY: &str = "archonia-id";
/// Fond du nom d'un membre du groupe quand l'option « Détails du groupe » est cochée
/// (`board+0x464` = 1), couleur de police `0xAABBGGRR`.
const PARTY_FRAME: u32 = 0xBF8B_3F3F;
/// Au plus autant de panneaux de dégâts et de mentions vivants (Archonia en alloue 1000).
const MAX_BOARDS: usize = 64;

#[derive(Default)]
pub struct BoardsGlue {
    dmg: Vec<(i32, DmgBoard, Instant)>,
    mentions: Vec<(i32, Mention, Instant)>,
    /// `HEIGHT` et `DEPTH` par TID (`charactertemplateclient.ini`).
    templates: HashMap<i32, (f32, f32)>,
    /// TID des PNJ placés (`ini/npc.ini`), chargés au premier besoin.
    npcs: Option<HashSet<i32>>,
    /// Police « Georgia » du système (`None` pas encore cherchée, `Some(None)` absente).
    font: Option<Option<Arc<Vec<u8>>>>,
    rng: u32,
    /// Nom caché par le décor ou le terrain, par CID, et instant du dernier test (voir
    /// `App::boards_occluded`).
    occluded: HashMap<i32, (bool, Instant)>,
}

/// Intervalle entre deux tests de visibilité du nom d'un même personnage.
const OCCLUSION_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Nom (et guilde) d'un personnage à dessiner.
struct IdLine {
    anchor: Pos2,
    name: String,
    color: u32,
    guild: Option<(String, u32)>,
    /// Membre du groupe (option « Détails du groupe ») : nom sur un cadre.
    party: bool,
    /// Titre équipé (`board+0x501`, couleur `board+0x57c`, voir `app_titles.rs`) : ligne entre le nom et la
    /// guilde.
    title: Option<(String, Color32)>,
    /// Rang de charisme rangé à la création (`board+0x104` = rang − 1, `+0x108` = nom, `FUN_140563f90`) ;
    /// `FUN_1408270a0` ne le dessine pas : gardé sans dessin, comme Archonia.
    #[allow(dead_code)]
    charisma: Option<(i32, String)>,
}

/// Ce que la fermeture egui dessine, calculé avant l'emprunt du GPU.
#[derive(Default)]
pub struct BoardsView {
    ids: Vec<IdLine>,
    digits: Vec<(Rect, Rect, Color32)>,
    mentions: Vec<(&'static str, Vec<Quad>)>,
}

/// Valeur d'une colonne nommée d'une table positionnelle (`[]` = en-tête).
fn template_column(assets: &Assets, tid: i32, column: &str) -> Option<f32> {
    let table = assets.table("ini", "charactertemplateclient.ini").ok()?;
    let key = table
        .section("")?
        .entries
        .iter()
        .find(|(_, v)| v.eq_ignore_ascii_case(column))
        .map(|(k, _)| k.clone())?;
    table.section(&tid.to_string())?.get(&key)?.trim().parse().ok()
}

impl App {
    /// Nom du personnage `cid` (au-dessus de la tête, `head`) caché, vu de `eye`, par le relief ou un
    /// objet du décor. Testé tous les `OCCLUSION_EVERY` par personnage ; entre deux tests, le dernier
    /// résultat.
    fn boards_occluded(&mut self, cid: i32, eye: Vec3, head: Vec3) -> bool {
        let now = Instant::now();
        if let Some((hidden, at)) = self.boards.occluded.get(&cid) {
            if now.duration_since(*at) < OCCLUSION_EVERY {
                return *hidden;
            }
        }
        let len = eye.distance(head);
        let by_ground = self.terrain.raycast(eye, head - eye, len).is_some_and(|p| p.distance(eye) < len - 50.0);
        let hidden = by_ground || self.objects.segment_blocked(&self.assets, eye, head, 40.0);
        self.boards.occluded.insert(cid, (hidden, now));
        if self.boards.occluded.len() > 512 {
            self.boards.occluded.retain(|_, (_, t)| now.duration_since(*t) < std::time::Duration::from_secs(5));
        }
        hidden
    }

    fn boards_template(&mut self, tid: i32) -> (f32, f32) {
        let assets = &self.assets;
        *self.boards.templates.entry(tid).or_insert_with(|| {
            (template_column(assets, tid, "HEIGHT").unwrap_or(0.0), template_column(assets, tid, "DEPTH").unwrap_or(0.0))
        })
    }

    /// Coup reçu (0x41) : chiffres et mentions si le joueur porte ou reçoit le coup
    /// (`FUN_1408202a0`), au-dessus de la cible.
    pub(super) fn boards_on_hit(&mut self, actor: i32, target: i32, result: Option<i8>, factors: Option<&ActionFactorPoints>, damage: Option<i32>) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if actor != me && target != me {
            return;
        }
        let on_me = target == me;
        let (value, color) = match factors.map(|f| boards::hit_value_and_color(f, on_me)) {
            Some((v, c)) if v > 0 => (v, c),
            _ => (damage.unwrap_or(0), if on_me { boards::argb(0xFFBB_B5F3) } else { Color32::WHITE }),
        };
        let now = Instant::now();
        let kind = match result {
            Some(5) | Some(0xd) => Some(DmgKind::Normal),
            Some(6) => Some(DmgKind::Critical),
            _ => None,
        };
        if let Some(board) = kind.and_then(|k| DmgBoard::new(k, value, color, on_me)) {
            self.boards.dmg.push((target, board, now));
        }
        if let Some(m) = result.and_then(MentionKind::from_result) {
            self.boards.mentions.push((target, Mention::new(m), now));
        }
        let excess = self.boards.dmg.len().saturating_sub(MAX_BOARDS);
        self.boards.dmg.drain(..excess);
        let excess = self.boards.mentions.len().saturating_sub(MAX_BOARDS);
        self.boards.mentions.drain(..excess);
    }

    /// `/panneau <résultat> [dégâts] [facteur 9-19] [moi]` : coup simulé du joueur sur sa cible (ou
    /// de la cible sur le joueur avec `moi`), pour essayer chiffres et mentions sans attendre le
    /// serveur (résultats d'Archonia : 1 raté, 5 coup, 6 critique, 7/8 esquives, 9 parade,
    /// 0xc attaque, 0xf renvoi ; facteur 9 physique, 10 magie… 19 héroïque).
    pub(super) fn boards_slash(&mut self, cmd: &str, arg: &str) -> bool {
        if cmd != "/panneau" {
            return false;
        }
        let words: Vec<&str> = arg.split_whitespace().collect();
        let on_me = words.contains(&"moi");
        let n: Vec<i32> = words.iter().filter_map(|w| w.parse().ok()).collect();
        let Some(&result) = n.first() else {
            self.ui.push_chat("[système] /panneau <résultat> [dégâts] [facteur 9-19] [moi]".to_string());
            return true;
        };
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let other = self.addons.target_cid().unwrap_or(me);
        let (actor, target) = if on_me { (other, me) } else { (me, other) };
        let value = -n.get(1).copied().unwrap_or(0).abs();
        let mut points = ActionFactorPoints::default();
        let slot = match n.get(2).copied().unwrap_or(9) {
            10 => &mut points.magic_damage,
            11 => &mut points.water_damage,
            12 => &mut points.fire_damage,
            13 => &mut points.earth_damage,
            14 => &mut points.air_damage,
            15 => &mut points.poison_damage,
            16 => &mut points.lightning_damage,
            17 => &mut points.ice_damage,
            19 => &mut points.heroic_damage,
            _ => &mut points.normal_damage,
        };
        *slot = Some(value);
        self.boards_on_hit(actor, target, Some(result as i8), Some(&points), Some(-value));
        true
    }

    /// Positions à l'écran de tous les panneaux de l'image.
    pub(super) fn boards_view(&mut self) -> BoardsView {
        let mut view = BoardsView::default();
        let now = Instant::now();
        self.boards.dmg.retain(|(_, b, born)| now.duration_since(*born).as_secs_f32() * 1000.0 <= b.life_ms());
        self.boards.mentions.retain(|(_, m, born)| now.duration_since(*born).as_millis() as i32 <= m.kind.life_ms());
        let Some(gpu) = &self.gpu else { return view };
        let ppp = gpu.window.scale_factor() as f32;
        let (w, h) = (gpu.renderer.config.width as f32 / ppp, gpu.renderer.config.height as f32 / ppp);
        let vp = self.camera.view_proj(gpu.renderer.aspect());
        let to_screen = move |p: Vec3| {
            let c = vp * p.extend(1.0);
            (c.w > 1e-3 && c.z / c.w <= 1.0).then(|| Pos2::new(((c.x / c.w) + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };
        if self.boards.npcs.is_none() {
            let set = self
                .assets
                .table("ini", "npc.ini")
                .map(|t| t.sections.iter().filter_map(|s| s.get("TID")?.trim().parse().ok()).collect())
                .unwrap_or_default();
            self.boards.npcs = Some(set);
        }
        // Personnages : (CID, TID, nom, position, orientation, joueur ?).
        let chars: Vec<(i32, i32, String, Vec3, f32, bool)> = self
            .player
            .iter()
            .map(|p| (p.cid, p.tid, p.name.clone(), p.position, p.turn_y, true))
            .chain(self.entities.iter().map(|e| (e.cid, e.tid, e.name.clone(), e.position, e.turn_y, e.is_player)))
            .collect();
        // Couleur des joueurs premiers attaquants ou meurtriers (voir `app_pvp.rs`).
        let pvp_colors: HashMap<i32, u32> = chars.iter().filter(|c| c.5).filter_map(|c| Some((c.0, self.pvp_name_color(c.0)?))).collect();
        let mut feet: HashMap<i32, (Vec3, f32)> = HashMap::new();
        let state = &self.guild.state;
        let my_guild = state.info.as_ref().map(|i| i.name.clone());
        let guilds: HashMap<i32, String> = state.links.clone();
        let ranks: HashMap<String, i32> = state.members.iter().map(|m| (m.name.to_lowercase(), m.rank)).collect();
        // Onglet Affichage des options : noms par sorte (`board+0x100` d'Archonia : 1 soi, 2 monstre,
        // 8 autre joueur ; les PNJ toujours), la cible gardant le sien comme Archonia le rallume au survol.
        let display = self.options.display.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let target = self.addons.target_cid();
        let party: HashSet<i32> = if display.name_party { self.party.state.members.iter().copied().filter(|&c| c != me).collect() } else { HashSet::new() };
        let (char_distance, eye) = (self.options.video.char_distance(), self.camera.eye());
        for (cid, tid, name, pos, turn, player) in chars {
            let (height, depth) = self.boards_template(tid);
            // À cheval : `RIDER_HEIGHT` de la monture en plus (voir `app_ride.rs`).
            let height = height + self.ride_board_height(cid);
            let forward = Vec3::new(turn.to_radians().sin(), 0.0, turn.to_radians().cos());
            let foot = pos + forward * depth;
            feet.insert(cid, (foot, height));
            if name.is_empty() {
                continue;
            }
            let Some(anchor) = to_screen(foot + Vec3::Y * height) else { continue };
            let kind = if player {
                IdKind::Player
            } else if self.boards.npcs.as_ref().is_some_and(|n| n.contains(&tid)) {
                IdKind::Npc
            } else {
                IdKind::Monster
            };
            let shown = match kind {
                _ if Some(cid) == target => true,
                IdKind::Player if cid == me => display.name_self,
                IdKind::Player => display.name_others,
                IdKind::Monster => display.name_monster,
                IdKind::Npc => true,
            };
            // Personnage au-delà de la distance `CharView` (non tracé) : pas de nom non plus.
            if !shown || (cid != me && char_distance.is_some_and(|d| pos.distance(eye) > d)) {
                continue;
            }
            // Nom caché par le décor (murs, toits) ou le relief entre la caméra et lui : pas dessiné.
            if cid != me && self.boards_occluded(cid, eye, foot + Vec3::Y * height) {
                continue;
            }
            let color = pvp_colors.get(&cid).copied().unwrap_or_else(|| boards::name_color(kind, None));
            let guild = guilds.get(&cid).filter(|g| !g.is_empty() && display.name_guild).map(|g| {
                let gv = GuildView { my_guild: my_guild.as_deref(), rank_in_mine: ranks.get(&name.to_lowercase()).copied(), allies: &[] };
                (boards::guild_text(g), boards::guild_color(color, g, &gv))
            });
            // Titre (op 9) des personnages joueurs ; rang de charisme connu pour le joueur seulement (CHA).
            let title = if player { self.titles.over_head_title(&name) } else { None };
            let charisma = if cid == me { self.titles.my_charisma_rank() } else { None };
            view.ids.push(IdLine { anchor, name, color, guild, party: party.contains(&cid), title, charisma });
        }
        for (cid, board, born) in &self.boards.dmg {
            let Some((foot, height)) = feet.get(cid) else { continue };
            let age = now.duration_since(*born).as_secs_f32() * 1000.0;
            if let Some(anchor) = to_screen(*foot + Vec3::Y * board.lift(*height, age)) {
                view.digits.extend(board.quads(anchor, age, SCALE));
            }
        }
        let rng = &mut self.boards.rng;
        let mut rand = || {
            *rng = rng.wrapping_mul(214_013).wrapping_add(2_531_011);
            (*rng >> 16) & 0x7fff
        };
        for (cid, m, born) in self.boards.mentions.iter_mut() {
            let Some((foot, height)) = feet.get(cid) else { continue };
            let age = now.duration_since(*born).as_millis() as i32;
            if let Some(anchor) = to_screen(*foot + Vec3::Y * (*height + boards::MENTION_LIFT)) {
                view.mentions.push((m.kind.image(), m.quads(anchor, age, &mut rand)));
            }
        }
        view
    }
}

/// Famille de la police des noms : « Georgia » du système (Archonia la demande à Windows par son
/// nom), sinon la police par défaut d'egui (écart).
fn id_family(ctx: &egui::Context, glue: &mut BoardsGlue) -> FontFamily {
    let font = glue.font.get_or_insert_with(|| {
        let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
        match std::fs::read(dir.join("Fonts").join("georgia.ttf")) {
            Ok(bytes) => Some(Arc::new(bytes)),
            Err(e) => {
                log::warn!("panneaux : police {} introuvable ({e}), police par défaut", boards::ID_FONT);
                None
            }
        }
    });
    let Some(bytes) = font else { return FontFamily::Proportional };
    let family = FontFamily::Name(ID_FAMILY.into());
    if ctx.fonts(|f| f.families().contains(&family)) {
        return family;
    }
    // Ajoutée (pour l'image suivante) dès qu'elle manque : `set_fonts` (polices des AddOns) remplace tout.
    ctx.add_font(egui::epaint::text::FontInsert::new(
        ID_FAMILY,
        egui::FontData::from_owned(bytes.as_ref().clone()),
        vec![egui::epaint::text::InsertFontFamily { family: family.clone(), priority: egui::epaint::text::FontPriority::Highest }],
    ));
    FontFamily::Proportional
}

fn paint_quads(painter: &egui::Painter, tex: egui::TextureId, quads: &[Quad]) {
    let mut mesh = egui::Mesh::with_texture(tex);
    for q in quads {
        let base = mesh.vertices.len() as u32;
        for i in 0..4 {
            mesh.vertices.push(egui::epaint::Vertex { pos: q.pos[i], uv: q.uv[i], color: q.color });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Dessin egui, sous les fenêtres du HUD : noms et guildes, chiffres, mentions.
pub fn panel(ctx: &egui::Context, glue: &mut BoardsGlue, view: &BoardsView, hud: &mut HudState, assets: &Assets) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("archrust-boards")));
    let font = FontId::new(boards::ID_FONT_SIZE * SCALE, id_family(ctx, glue));
    let shadow = Color32::from_black_alpha(boards::SHADOW_ALPHA);
    for line in &view.ids {
        let name = painter.layout_no_wrap(line.name.clone(), font.clone(), boards::font_color(line.color));
        let guild = line.guild.as_ref().map(|(text, color)| painter.layout_no_wrap(text.clone(), font.clone(), boards::font_color(*color)));
        let height = name.size().y;
        let (at, mut guild_at) = boards::id_lines(line.anchor, name.size().x, guild.as_ref().map(|g| g.size().x), height, SCALE);
        // Titre sur la ligne 1 (haut à `ancre − 2h − 6`), la guilde passe à la ligne 2 (`ancre − 2h − 20`,
        // `FUN_1408270a0` : lignes +0xF0…+0xFC).
        let title = line.title.as_ref().map(|(text, color)| painter.layout_no_wrap(text.clone(), font.clone(), *color));
        let title_at = title.as_ref().map(|t| Pos2::new((line.anchor.x - t.size().x * 0.5 * SCALE).trunc(), at.y - height - 6.0 * SCALE));
        if title.is_some() {
            guild_at = guild_at.map(|g| Pos2::new(g.x, at.y - height - 20.0 * SCALE));
        }
        if line.party {
            // Cadre `0xBF8B3F3F` (`FUN_140825a20`) derrière le nom d'un membre du groupe.
            let c = boards::font_color(PARTY_FRAME);
            let fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (PARTY_FRAME >> 24) as u8);
            painter.rect_filled(Rect::from_min_size(at, name.size()).expand(1.0), 0.0, fill);
        }
        for (galley, pos) in [(Some(name), Some(at)), (title, title_at), (guild, guild_at)] {
            let (Some(galley), Some(pos)) = (galley, pos) else { continue };
            painter.galley_with_override_text_color(pos + egui::vec2(1.0, 1.0), galley.clone(), shadow);
            painter.galley(pos, galley, Color32::WHITE);
        }
    }
    if !view.digits.is_empty() {
        if let Some((tex, _)) = hud.image(ctx, assets, "DM_NUMBER.png") {
            let quads: Vec<Quad> = view
                .digits
                .iter()
                .map(|(r, uv, c)| Quad {
                    pos: [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()],
                    uv: [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()],
                    color: *c,
                })
                .collect();
            paint_quads(&painter, tex, &quads);
        }
    }
    for (image, quads) in &view.mentions {
        if let Some((tex, _)) = hud.image(ctx, assets, image) {
            paint_quads(&painter, tex, quads);
        }
    }
}
