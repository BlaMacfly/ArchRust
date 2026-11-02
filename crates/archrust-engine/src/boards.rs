//! Panneaux d'Archonia dessinés au-dessus des personnages (`AgcmTextBoardMng`), sans AddOn :
//!
//! * panneau d'identité `AgcmIDBoard` : nom et « [guilde] », couleurs d'origine ;
//! * chiffres de dégâts `AgcmDmgBoard` : `DM_NUMBER.png` (`[Textures3]` de `ini/TextBoard.ini`) ;
//! * mentions `AgcmImoticonBoard` : `IM_miss`, `IM_critical`, `IM_block`… (`[Textures4]`).
//!
//! Tout vient d'Archonia.exe (sha256 e9c7609b…) : tables et constantes de `FUN_140561ec0`
//! (initialisation du gestionnaire), création des chiffres `FUN_140563930`, rendu `FUN_140822660`,
//! couleurs `FUN_14055fbd0`, mentions `FUN_140564850` / `FUN_140829a10`, nom `FUN_140563f90` /
//! `FUN_1408270a0` / `FUN_140825a20`, guilde `FUN_140560140` / `FUN_140560c60` / `FUN_140824b20`.
//! Détails et écarts : `docs/FORMATS.md`, section « Panneaux au-dessus des personnages ».
//!
//! Ce module ne fait que des calculs (positions à l'écran, couleurs, effets) ; le dessin egui et
//! l'état du jeu sont dans `app_boards.rs`.

use archrust_netclient::world::ActionFactorPoints;
use egui::{Color32, Pos2, Rect, Vec2};

// ---------------------------------------------------------------------------------------------
// Chiffres de dégâts (AgcmDmgBoard)
// ---------------------------------------------------------------------------------------------

/// Chiffre `d` de `DM_NUMBER.png` (256 × 32, « 1234567890 ») : u de gauche, u de droite, largeur
/// affichée (entrées de 12 octets en `+0x1c00 + 12·d` du gestionnaire). La hauteur couvre toute
/// l'image (v de 0 à 1).
pub const DIGITS: [(f32, f32, f32); 10] = [
    (0.789_062_5, 0.886_718_75, 30.000_002),
    (0.0, 0.082_031_25, 25.2),
    (0.082_031_25, 0.167_968_75, 26.4),
    (0.167_968_75, 0.253_906_25, 26.4),
    (0.253_906_25, 0.347_656_25, 28.8),
    (0.347_656_25, 0.4375, 27.6),
    (0.4375, 0.527_343_75, 27.6),
    (0.527_343_75, 0.609_375, 25.2),
    (0.609_375, 0.699_218_75, 27.6),
    (0.699_218_75, 0.789_062_5, 27.6),
];
/// Largeurs des chiffres quand la cible est le joueur (`+0x1c78`).
pub const DIGIT_WIDTHS_ON_ME: [f32; 10] = [22.8, 16.8, 20.4, 21.6, 21.6, 20.4, 21.6, 18.0, 22.8, 20.4];
/// Hauteurs des chiffres (`0x140d7f074`, et `0x140d7f070` sur le joueur).
pub const DIGIT_HEIGHT: f32 = 38.4;
pub const DIGIT_HEIGHT_ON_ME: f32 = 31.2;
/// Au-dessus de la hauteur du personnage (`0x140d3d944`), et montée des coups normaux sur leur vie
/// (`0x140d3e42c`), en unités du monde.
pub const DMG_LIFT: f32 = 15.0;
pub const DMG_RISE: f32 = 40.0;

/// Chiffres blancs (coup physique, héroïque, critique) et lavande (sur le joueur, sans dégât physique).
const WHITE: u32 = 0xFFFF_FFFF;
const ON_ME: u32 = 0xFFBB_B5F3;

/// Les deux sortes de chiffres d'Archonia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmgKind {
    /// Type 3 : coup normal (résultats 5 et 0xd), 800 ms, monte de 40 unités.
    Normal,
    /// Type 4 : coup critique (résultat 6), 700 ms, effet de zoom, toujours blanc.
    Critical,
}

/// Chiffres flottants d'un coup.
#[derive(Debug, Clone, PartialEq)]
pub struct DmgBoard {
    pub kind: DmgKind,
    /// Chiffres, poids faible d'abord (8 au plus).
    pub digits: Vec<u8>,
    pub color: Color32,
    pub on_me: bool,
    /// Décalage du premier chiffre par rapport à l'ancre (`board+0x24`, `+0x28`), en pixels.
    pub offset: Vec2,
}

impl DmgBoard {
    /// Chiffres de `value` (comme `FUN_140563930`). `None` si la valeur n'est pas positive.
    pub fn new(kind: DmgKind, value: i32, color: Color32, on_me: bool) -> Option<DmgBoard> {
        if value <= 0 {
            return None;
        }
        let on_me = on_me && kind == DmgKind::Normal;
        let mut x = if on_me { -17 } else { -29 };
        let mut digits = Vec::new();
        let mut v = value as u32;
        while digits.len() < 8 {
            let d = (v % 10) as usize;
            digits.push(d as u8);
            let w = if on_me { DIGIT_WIDTHS_ON_ME[d] } else { DIGITS[d].2 };
            x += (w * 0.5) as i32;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        let y = match kind {
            DmgKind::Normal => -55.0,
            DmgKind::Critical => -105.0,
        };
        let color = if kind == DmgKind::Critical { Color32::WHITE } else { color };
        Some(DmgBoard { kind, digits, color, on_me, offset: Vec2::new(x as f32, y) })
    }

    pub fn life_ms(&self) -> f32 {
        match self.kind {
            DmgKind::Normal => 800.0,
            DmgKind::Critical => 700.0,
        }
    }

    /// Hauteur de l'ancre au-dessus des pieds, après `age_ms` : hauteur du personnage + 15, plus
    /// la montée des coups normaux (`FUN_140822b00`).
    pub fn lift(&self, height: f32, age_ms: f32) -> f32 {
        let rise = match self.kind {
            DmgKind::Normal => age_ms / self.life_ms() * DMG_RISE,
            DmgKind::Critical => 0.0,
        };
        height + DMG_LIFT + rise
    }

    /// Rectangles à l'écran (position, UV dans `DM_NUMBER.png`, couleur) à `age_ms`, depuis
    /// l'ancre projetée ; `scale` = échelle des panneaux (au moins 1). Rendu de `FUN_140822660`.
    pub fn quads(&self, anchor: Pos2, age_ms: f32, scale: f32) -> Vec<(Rect, Rect, Color32)> {
        let r = age_ms / self.life_ms();
        let (alpha, zoom) = match self.kind {
            DmgKind::Normal => {
                let a = if r < 0.5 { 255 } else { 255 - (((r - 0.5) * 510.0) as i32 & 0xff) };
                (a, 1.0)
            }
            DmgKind::Critical => {
                if r < 0.15 {
                    ((r * 853.3 + 0.5) as i32, 1.3 - 2.0 * r)
                } else if r > 0.85 {
                    ((255.0 - (r - 0.85) * 1000.0) as i32, (r - 0.85).powi(2) * 1.5 + 1.0)
                } else {
                    (255, 1.0)
                }
            }
        };
        let z = zoom * scale.max(1.0);
        let alpha = alpha.clamp(0, 255) as u8;
        let color = Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), alpha);
        let (mut x, y, h) = match self.kind {
            DmgKind::Normal => (
                anchor.x + z * self.offset.x,
                anchor.y + z * self.offset.y,
                z * if self.on_me { DIGIT_HEIGHT_ON_ME } else { DIGIT_HEIGHT },
            ),
            DmgKind::Critical => (anchor.x + z * self.offset.x, anchor.y + z * self.offset.y * 0.5, z * DIGIT_HEIGHT),
        };
        let mut out = Vec::with_capacity(self.digits.len());
        for &d in &self.digits {
            let (u0, u1, w) = DIGITS[d as usize];
            let w = z * if self.on_me { DIGIT_WIDTHS_ON_ME[d as usize] } else { w };
            let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
            let uv = Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0));
            out.push((rect, uv, color));
            x -= w;
        }
        out
    }
}

/// Valeur et couleur des chiffres d'un coup normal (`FUN_14055fbd0`) : somme des facteurs de
/// dégâts négatifs 9 à 17 et 19 ; blanc, ou lavande sur le joueur ; le physique force le blanc ;
/// l'élément le plus fort donne sa couleur (l'héroïque, le blanc).
pub fn hit_value_and_color(points: &ActionFactorPoints, on_me: bool) -> (i32, Color32) {
    let mut color = if on_me { ON_ME } else { WHITE };
    let mut total = 0i32;
    let mut best = 1i32;
    let hurt = |v: Option<i32>| v.filter(|x| *x < 0).map(|x| -x);
    if let Some(d) = hurt(points.normal_damage) {
        total += d;
        if d > 0 {
            color = WHITE;
        }
    }
    let elements = [
        (points.magic_damage, 0xFF90_57B2),
        (points.water_damage, 0xFF32_86CD),
        (points.fire_damage, 0xFFDA_4D0A),
        (points.earth_damage, 0xFFA0_6636),
        (points.air_damage, 0xFFA2_D089),
        (points.poison_damage, 0xFF2D_D23C),
        (points.lightning_damage, 0xFFE2_BA00),
        (points.ice_damage, 0xFF89_D0CD),
        (points.heroic_damage, WHITE),
    ];
    for (value, argb) in elements {
        if let Some(d) = hurt(value) {
            total += d;
            if d > best {
                best = d;
                color = argb;
            }
        }
    }
    (total, argb(color))
}

/// Couleur ARGB (format des sommets RenderWare) vers egui.
pub fn argb(c: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

/// Couleur des polices d'Archonia (`0xAABBGGRR`, rouge dans l'octet faible) vers egui, opaque :
/// l'alpha du texte vient du panneau (`board+0x4c`, 255), pas de la couleur.
pub fn font_color(c: u32) -> Color32 {
    Color32::from_rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8)
}

// ---------------------------------------------------------------------------------------------
// Mentions (AgcmImoticonBoard)
// ---------------------------------------------------------------------------------------------

/// Taille des images `IM_*` à l'écran (`0x140d3e438`), en pixels, sans mise à l'échelle.
pub const MENTION_SIZE: f32 = 64.0;
/// Au-dessus de la hauteur du personnage (`0x140d46138`), en unités ; le rendu retire encore 35 px.
pub const MENTION_LIFT: f32 = 35.0;

/// Mention d'Archonia : type `0x212 + n`, image `IM_*` n° `n` de `[Textures4]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionKind {
    Miss,
    Critical,
    Block,
    Evade,
    Dodge,
    Reflex,
    Attack,
}

impl MentionKind {
    /// Mention d'un résultat d'action (`FUN_1408202a0`) : 1 raté, 6 critique, 7 esquive (evade),
    /// 8 esquive (dodge), 9 et 0x10 parade, 0xf renvoi, 0xc attaque.
    pub fn from_result(result: i8) -> Option<MentionKind> {
        Some(match result {
            1 => MentionKind::Miss,
            6 => MentionKind::Critical,
            7 => MentionKind::Evade,
            8 => MentionKind::Dodge,
            9 | 0x10 => MentionKind::Block,
            0xf => MentionKind::Reflex,
            0xc => MentionKind::Attack,
            _ => return None,
        })
    }

    /// Fichier de `[Textures4]` (`IM_levup.png`, n° 3, n'est jamais dessiné par Archonia).
    pub fn image(self) -> &'static str {
        match self {
            MentionKind::Miss => "IM_miss.png",
            MentionKind::Critical => "IM_critical.png",
            MentionKind::Block => "IM_block.png",
            MentionKind::Evade => "IM_evade.png",
            MentionKind::Dodge => "IM_dodge.png",
            MentionKind::Reflex => "IM_reflex.png",
            MentionKind::Attack => "IM_attack.png",
        }
    }

    pub fn life_ms(self) -> i32 {
        if self == MentionKind::Block {
            900
        } else {
            800
        }
    }

    /// Décalage vertical du coin haut (`board+0x28`).
    fn top(self) -> f32 {
        match self {
            MentionKind::Critical | MentionKind::Attack => -52.0,
            _ => -32.0,
        }
    }
}

/// Quadrilatère texturé (haut gauche, haut droit, bas droit, bas gauche : éventail RenderWare).
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub pos: [Pos2; 4],
    pub uv: [Pos2; 4],
    pub color: Color32,
}

impl Quad {
    fn rect(x: f32, y: f32, w: f32, h: f32, uv: Rect, color: Color32) -> Quad {
        Quad {
            pos: [Pos2::new(x, y), Pos2::new(x + w, y), Pos2::new(x + w, y + h), Pos2::new(x, y + h)],
            uv: [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()],
            color,
        }
    }
}

/// Mention en cours et l'état de ses effets (bris, rebond, tremblement).
#[derive(Debug, Clone, PartialEq)]
pub struct Mention {
    pub kind: MentionKind,
    /// Bris (`+0x74`, `+0x78`, `+0x7c`) : coupe en haut et en bas du morceau gauche, en pixels.
    split: Option<(f32, f32)>,
    /// Parade : état (`+0x74`), position, vitesse, borne, décalage des bandes (`+0x78` à `+0x84`).
    state: i32,
    pos: f32,
    speed: f32,
    bound: f32,
    shake: f32,
}

impl Mention {
    pub fn new(kind: MentionKind) -> Mention {
        Mention { kind, split: None, state: 0, pos: 0.0, speed: 0.5, bound: 2.0, shake: 0.0 }
    }

    /// Quadrilatères de l'image à `age_ms` depuis l'ancre projetée (pieds + hauteur + 35).
    /// `rand` remplace le `rand()` du C (bris) ; la parade avance d'un pas par appel, comme
    /// Archonia qui la fait évoluer à chaque image.
    pub fn quads(&mut self, anchor: Pos2, age_ms: i32, rand: &mut impl FnMut() -> u32) -> Vec<Quad> {
        let life = self.kind.life_ms();
        let half = life / 2;
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let left = anchor.x - 32.0;
        let top = anchor.y + self.kind.top() - MENTION_LIFT;
        let s = MENTION_SIZE;
        let u = (age_ms - half) as f32 / (life - half) as f32;
        let faded = Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 - u * 255.0).clamp(0.0, 255.0) as u8);
        match self.kind {
            MentionKind::Miss | MentionKind::Evade | MentionKind::Dodge | MentionKind::Reflex => {
                let top = top - (14.0 / life as f32 * age_ms as f32);
                if age_ms <= half {
                    return vec![Quad::rect(left, top, s, s, full, Color32::WHITE)];
                }
                let echo = u * 19.2;
                let (gw, gh) = (u * 16.0, u * 8.0);
                vec![
                    Quad::rect(left + echo, top + echo, s - 2.0 * echo, s - 2.0 * echo, full, faded),
                    Quad::rect(left - gw * 0.5, top - gh * 0.5, s + gw, s + gh, full, faded),
                ]
            }
            MentionKind::Critical | MentionKind::Attack => {
                if age_ms <= half {
                    return vec![Quad::rect(left, top, s, s, full, Color32::WHITE)];
                }
                let (a, b) = *self.split.get_or_insert_with(|| {
                    let side = rand() & 1;
                    let r = (rand() % 16) as f32;
                    if side == 0 {
                        (32.0 + r, 32.0 - r)
                    } else {
                        (32.0 - r, 32.0 + r)
                    }
                });
                let (l1, t1) = (left - u * 4.0, top - u * 3.0);
                let (l2, t2) = (left + u * 8.0, top + u * 6.0);
                vec![
                    Quad {
                        pos: [Pos2::new(l1, t1), Pos2::new(l1 + a, t1), Pos2::new(l1 + b, t1 + s), Pos2::new(l1, t1 + s)],
                        uv: [Pos2::new(0.0, 0.0), Pos2::new(a / s, 0.0), Pos2::new(b / s, 1.0), Pos2::new(0.0, 1.0)],
                        color: faded,
                    },
                    Quad {
                        pos: [Pos2::new(l2 + a, t2), Pos2::new(l2 + s, t2), Pos2::new(l2 + s, t2 + s), Pos2::new(l2 + b, t2 + s)],
                        uv: [Pos2::new(a / s, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(b / s, 1.0)],
                        color: faded,
                    },
                ]
            }
            MentionKind::Block => {
                if age_ms <= half {
                    return vec![Quad::rect(left, top, s, s, full, Color32::WHITE)];
                }
                if u < 0.5 {
                    self.bounce_step();
                    return vec![Quad::rect(left + self.pos, top, s, s, full, faded)];
                }
                self.shake_step();
                let mut out = Vec::with_capacity(16);
                for (y, dx) in (0..64).step_by(8).map(|y| (y, self.shake)).chain((4..64).step_by(8).map(|y| (y, -self.shake))) {
                    let v = y as f32 / s;
                    let uv = Rect::from_min_max(Pos2::new(0.0, v), Pos2::new(1.0, v + 0.0625));
                    out.push(Quad::rect(left + dx, top + y as f32, s, 4.0, uv, faded));
                }
                out
            }
        }
    }

    /// Rebond horizontal de la parade, première moitié du fondu (`+0x78` position, `+0x7c`
    /// vitesse, `+0x80` borne).
    fn bounce_step(&mut self) {
        let p = self.pos;
        if self.state == 0 && p > self.bound {
            self.bound *= -1.4;
            self.state = 1;
            self.speed *= -1.5;
            self.pos = p + self.speed - 0.3;
        } else if self.state == 1 {
            if self.bound <= p {
                self.pos = p + self.speed - 0.3;
            } else {
                self.bound *= -1.4;
                self.state = 0;
                self.speed *= -1.5;
                self.pos = self.speed + 0.3 + p;
            }
        } else if self.state == 0 {
            self.pos = self.speed + 0.3 + p;
        }
    }

    /// Tremblement des bandes, seconde moitié du fondu (`+0x80` pas, `+0x84` décalage ±5).
    fn shake_step(&mut self) {
        let up = if self.state < 2 {
            self.bound = 0.02;
            self.state = 2;
            true
        } else {
            self.bound += 0.02;
            self.state == 2
        };
        if up {
            if self.shake <= 5.0 {
                self.shake += self.bound + 0.5;
            } else {
                self.state = 3;
                self.shake = 5.0;
            }
        } else if self.state == 3 {
            if self.shake >= -5.0 {
                self.shake -= self.bound + 0.5;
            } else {
                self.state = 2;
                self.shake = -5.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Panneau d'identité (AgcmIDBoard)
// ---------------------------------------------------------------------------------------------

/// Police n° 0 d'AgcmFont pour les langues occidentales (`FUN_1404ed830`) et sa taille.
pub const ID_FONT: &str = "Georgia";
pub const ID_FONT_SIZE: f32 = 11.0;
/// Écart vertical entre le nom et la ligne de guilde (`0x140e4a060`).
pub const LINE_GAP: f32 = 6.0;
/// Hauts des lignes 1, 2 et 3 au-dessus du haut du nom, moins la hauteur de police (`FUN_1408270a0` : `+0xF4`,
/// `+0xF8`, `+0xFC`) : la signature (`FUN_140826450`) prend la ligne qui suit le titre et la guilde dessinés.
pub const SIGNATURE_GAPS: [f32; 3] = [6.0, 20.0, 34.0];
/// Couleur de la signature : `board+0x1fc` = −1 (blanc) sans couleur choisie (`+0x104` du personnage).
pub const SIGNATURE_COLOR: u32 = 0xFFFF_FFFF;
/// Ombre du texte : décalage d'un pixel, alpha au plus 0xB8 (`FUN_1404f0f80`).
pub const SHADOW_ALPHA: u8 = 0xB8;

/// Sorte de personnage pour la couleur du nom (bits 0 à 2 du type, `+0xb8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Player,
    Npc,
    Monster,
}

/// Couleur du nom (`board+0xEC`, `FUN_140563f90`) au format des polices `0xAABBGGRR` : joueur
/// blanc, PNJ `0xFF0099FF` (#FF9900), monstre la colonne `name_color` du gabarit public
/// (`0xFFFFFF` par défaut ; absente de ce client).
pub fn name_color(kind: IdKind, template_color: Option<u32>) -> u32 {
    match kind {
        IdKind::Player => 0xFFFF_FFFF,
        IdKind::Npc => 0xFF00_99FF,
        IdKind::Monster => {
            // Le gabarit est en 0x00RRGGBB : Archonia échange rouge et bleu.
            let c = template_color.unwrap_or(0x00FF_FFFF);
            (c & 0xFF00_FF00) | ((c & 0xFF) << 16) | ((c >> 16) & 0xFF)
        }
    }
}

/// Ce qui décide de la couleur de la guilde : guilde du joueur, rang du personnage dans cette
/// guilde, guildes alliées de celle du joueur.
pub struct GuildView<'a> {
    pub my_guild: Option<&'a str>,
    /// Rang du personnage nommé dans la guilde du joueur (`None` s'il n'en est pas membre).
    pub rank_in_mine: Option<i32>,
    pub allies: &'a [String],
}

/// Couleur de « [guilde] » (`FUN_140560140` puis `FUN_140560c60`), format `0xAABBGGRR` : celle
/// du nom sauf s'il est blanc (ou nul) ; sinon vert clair `0xFF81FAB2` (#B2FA81), gris
/// `0xFFA0A0A0` pour une demande d'adhésion ou de départ dans la guilde du joueur, vert
/// `0xFF009900` (#009900) pour une guilde alliée.
pub fn guild_color(name_color: u32, guild: &str, view: &GuildView<'_>) -> u32 {
    if name_color != 0xFFFF_FFFF && name_color != 0 {
        return name_color;
    }
    let mine = view.my_guild.is_some_and(|g| g.eq_ignore_ascii_case(guild));
    if mine {
        return match view.rank_in_mine {
            Some(2 | 3) => 0xFFA0_A0A0,
            _ => 0xFF81_FAB2,
        };
    }
    if view.allies.iter().any(|a| a.eq_ignore_ascii_case(guild)) {
        0xFF00_9900
    } else {
        0xFF81_FAB2
    }
}

/// Texte de la ligne de guilde (format `[%s]`, `0x140d4612c`).
pub fn guild_text(guild: &str) -> String {
    format!("[{guild}]")
}

/// Coins haut gauche du nom et de la guilde (`FUN_1408270a0`) : nom centré sous l'ancre, haut à
/// `ancre − h` ; guilde centrée, haut à `ancre − 2h − 6·s` (h = hauteur de police, non mise à
/// l'échelle, comme `board+0x28`). Positions tronquées au pixel comme Archonia.
pub fn id_lines(anchor: Pos2, name_width: f32, guild_width: Option<f32>, font_height: f32, scale: f32) -> (Pos2, Option<Pos2>) {
    let s = scale.max(1.0);
    let name = Pos2::new((anchor.x - name_width * 0.5 * s).trunc(), (anchor.y - font_height).trunc());
    let guild = guild_width.map(|w| Pos2::new((anchor.x - w * 0.5 * s).trunc(), name.y - font_height - LINE_GAP * s));
    (name, guild)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chiffres_de_poids_faible_d_abord() {
        let b = DmgBoard::new(DmgKind::Normal, 1207, Color32::WHITE, false).unwrap();
        assert_eq!(b.digits, vec![7, 0, 2, 1]);
        // −29 + ⌊25,2/2⌋ + ⌊30/2⌋ + ⌊26,4/2⌋ + ⌊25,2/2⌋ = −29 + 12 + 15 + 13 + 12.
        assert_eq!(b.offset, Vec2::new(23.0, -55.0));
        assert!(DmgBoard::new(DmgKind::Normal, 0, Color32::WHITE, false).is_none());
        let me = DmgBoard::new(DmgKind::Normal, 9, Color32::WHITE, true).unwrap();
        assert_eq!(me.offset.x, (-17 + 10) as f32);
    }

    #[test]
    fn chiffres_dessines_vers_la_gauche() {
        let b = DmgBoard::new(DmgKind::Normal, 12, Color32::WHITE, false).unwrap();
        let q = b.quads(Pos2::new(100.0, 200.0), 0.0, 1.0);
        assert_eq!(q.len(), 2);
        // « 2 » d'abord à x = 100 + offset, puis « 1 » décalé de la largeur du « 2 ».
        assert_eq!(q[0].0.left(), 100.0 + b.offset.x);
        assert_eq!(q[1].0.left(), q[0].0.left() - q[0].0.width());
        assert_eq!(q[0].0.top(), 145.0);
        assert!((q[0].0.height() - DIGIT_HEIGHT).abs() < 1e-3);
        assert_eq!(q[0].1.left(), DIGITS[2].0);
        // Fondu sur la seconde moitié.
        assert_eq!(b.quads(Pos2::ZERO, 200.0, 1.0)[0].2.a(), 255);
        assert_eq!(b.quads(Pos2::ZERO, 600.0, 1.0)[0].2.a(), 255 - 127);
    }

    #[test]
    fn critique_zoom_et_blanc() {
        let b = DmgBoard::new(DmgKind::Critical, 5, Color32::RED, true).unwrap();
        assert_eq!(b.color, Color32::WHITE);
        assert!(!b.on_me);
        let q = b.quads(Pos2::ZERO, 0.0, 1.0);
        assert!((q[0].0.height() - DIGIT_HEIGHT * 1.3).abs() < 1e-3);
        assert!((q[0].0.top() - (-105.0 * 1.3 * 0.5)).abs() < 1e-3);
    }

    #[test]
    fn couleurs_des_coups() {
        let mut p = ActionFactorPoints { normal_damage: Some(-27), ..Default::default() };
        assert_eq!(hit_value_and_color(&p, true), (27, Color32::WHITE));
        p.normal_damage = None;
        p.fire_damage = Some(-12);
        p.ice_damage = Some(-3);
        assert_eq!(hit_value_and_color(&p, false), (15, argb(0xFFDA_4D0A)));
        p.fire_damage = None;
        p.ice_damage = None;
        p.magic_damage = Some(0);
        assert_eq!(hit_value_and_color(&p, true).1, argb(0xFFBB_B5F3));
    }

    #[test]
    fn mentions_des_resultats() {
        assert_eq!(MentionKind::from_result(1), Some(MentionKind::Miss));
        assert_eq!(MentionKind::from_result(9), Some(MentionKind::Block));
        assert_eq!(MentionKind::from_result(5), None);
        let mut m = Mention::new(MentionKind::Miss);
        let mut r = || 0;
        let q = m.quads(Pos2::new(100.0, 300.0), 0, &mut r);
        assert_eq!(q[0].pos[0], Pos2::new(68.0, 300.0 - 32.0 - 35.0));
        assert_eq!(m.quads(Pos2::new(100.0, 300.0), 600, &mut r).len(), 2);
        let mut c = Mention::new(MentionKind::Critical);
        let q = c.quads(Pos2::ZERO, 800, &mut r);
        assert_eq!(q[0].pos[1].x - q[0].pos[0].x, 32.0);
    }

    #[test]
    fn parade_rebondit_puis_tremble() {
        let mut m = Mention::new(MentionKind::Block);
        let mut r = || 0;
        let xs: Vec<f32> = (0..6).map(|_| m.quads(Pos2::new(32.0, 0.0), 500, &mut r)[0].pos[0].x).collect();
        assert!((xs[0] - 0.8).abs() < 1e-4 && (xs[2] - 2.4).abs() < 1e-4);
        assert!(xs[3] < xs[2], "le rebond repart dans l'autre sens : {xs:?}");
        assert_eq!(m.quads(Pos2::ZERO, 800, &mut r).len(), 16);
    }

    #[test]
    fn couleurs_du_nom_et_de_la_guilde() {
        assert_eq!(font_color(name_color(IdKind::Npc, None)), Color32::from_rgb(0xFF, 0x99, 0x00));
        assert_eq!(font_color(name_color(IdKind::Monster, None)), Color32::WHITE);
        let none = GuildView { my_guild: None, rank_in_mine: None, allies: &[] };
        assert_eq!(font_color(guild_color(0xFFFF_FFFF, "Lune", &none)), Color32::from_rgb(0xB2, 0xFA, 0x81));
        let mine = GuildView { my_guild: Some("lune"), rank_in_mine: Some(2), allies: &[] };
        assert_eq!(guild_color(0xFFFF_FFFF, "Lune", &mine), 0xFFA0_A0A0);
        assert_eq!(guild_color(0xFF00_99FF, "Lune", &none), 0xFF00_99FF);
        assert_eq!(guild_text("Lune"), "[Lune]");
    }

    #[test]
    fn lignes_centrees() {
        let (name, guild) = id_lines(Pos2::new(200.0, 100.0), 40.0, Some(60.0), 13.0, 1.0);
        assert_eq!(name, Pos2::new(180.0, 87.0));
        assert_eq!(guild, Some(Pos2::new(170.0, 87.0 - 13.0 - 6.0)));
    }
}
