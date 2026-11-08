//! Effets des armes serties (pierres d'esprit, runes, renforcement), comme `AgcmEventEffect`
//! d'Archonia (relevés dans `Archonia.exe`, sha256 e9c7609b…) :
//!
//! * **Déclencheur** — rappels d'AgpmItemConvert (`FUN_14056a840`, inscrit deux fois par
//!   `FUN_14056cba0`) sur un objet **porté** (statut 3) : retrait des effets précédents
//!   (`FUN_14056e490`) puis `FUN_1405725e0`.
//! * **Points** (AgpmItemConvert `FUN_14028e610`, table `ItemConvertPoint.txt`) : points du niveau de
//!   renforcement (s'il ne dépasse pas le maximum de l'objet) + points du nombre de sockets + pour
//!   chaque pierre d'esprit les points de son rang + pour chaque rune les points de son rang.
//! * **Catégorie** (`FUN_14056bb10`) : la plus forte parmi AIR (élément 5), WATER (2), MAGIC (1),
//!   EARTH (4) — points des pierres de l'élément, `FUN_14028ec70` —, PHYSICAL (points du renforcement,
//!   `FUN_14028e900`) et RUNE (points des runes, `FUN_14028eb40`) ; égalités départagées par l'ordre
//!   des sockets. FIRE n'est jamais retenu : son total est demandé sans objet (`FUN_14028ec70(…, 0, 3)`
//!   rend 0), fidèlement reproduit. Indices de catégorie : `FUN_140831280` (FIRE 0, AIR 1, WATER 2,
//!   MAGIC 3, EARTH 4, ICE 5, LIGHTENING 6, POISON 7, PHYSICAL 8, RUNE 9) ; élément d'une pierre →
//!   catégorie : `FUN_140831210`.
//! * **Arme** (`Kind` 2, `FUN_140574100`) : lignes d'`ee_ss_change.txt` de la catégorie, de la
//!   dernière à la première : toutes celles du plus haut `POINT` ≤ total sont jouées sur l'objet
//!   (`FUN_1405706d0`), effets permanents tant qu'il est porté. Le repère de l'objet en main est
//!   pris au nœud de la main qui le porte (`worn`).
//! * **Armure de corps** (`Kind` 1, `Part` 2, `FUN_1405724b0`) : matériau `uvAnim.fx` d'`ee_ss_fx.txt`
//!   posé sur les atomics du modèle de l'objet (`FUN_140572410`, matériau retiré si aucun palier).
//!   Lecture (`FUN_14082f0f0`) : au plus 5 paliers par catégorie, triés par `POINT` décroissant
//!   (comparateur `0x14082c8c0`), une ligne de même `POINT` remplace la précédente ; couleurs `/255`
//!   (alpha 1) ; translation, échelle, sélections : nombres entre guillemets séparés par des virgules.
//!   Palier (`FUN_14082cf80`) : premier de la liste dont `POINT` ≤ total, soit le plus haut.
//!   ArchRust : [`ArmorGlow`] posée sur les pièces d'armure de corps du personnage (`Mesh::glow`),
//!   dessinée par `shader.wgsl` (second étage de texture, voir `docs/FORMATS.md`).
//! * **Effet par tranches de renforcement : rien à jouer.** `FUN_1405725e0` (arme `Kind` 2, `Type` 0, non
//!   expirée) lit le niveau de renforcement (`FUN_1403416e0`, premier entier de la donnée attachée, comme
//!   `FUN_14028e900`) et cherche une entrée par tranche 1–2 … 9–10 (`FUN_14082d0b0`, `+0xe18…+0xe38`). Ces
//!   cases ne sont remplies que par `FUN_14082fe60` (lignes `W1`…`W5` d'un fichier), joignable seulement par
//!   `FUN_14056de70`, qui n'a aucun appelant ni pointeur dans l'exécutable : elles restent vides. **Écarté.**
//! * **`ee_ss_target.txt`** (`FUN_140830a80` : `LEVEL`, `SPIRIT_STONE`, 10 lignes au plus par couple) : ce n'est
//!   **pas** la table des tranches. Joué à chaque coup qui touche (`FUN_140573960` → `FUN_140572e20`), selon
//!   l'arme de l'attaquant (partie 10, sinon 9) dont `n` sockets sont remplies (runes comprises, `FUN_14032eac0` ;
//!   sans socket remplie : autre chemin, non repris). Pour chaque rang de ligne, `FUN_14082dac0` tire : chance
//!   `30 + 29(n−1)/9 + 11(Rank−1)/4` sur 100 (divisions entières, `Rank` : facteur 10, 2 de l'objet), puis la
//!   catégorie au prorata des pierres (`FUN_14032eb80`) : WATER, FIRE, EARTH, AIR, MAGIC dans cet ordre, le reste
//!   (runes, glace, foudre, poison) ne donnant rien. `LEVEL` = moitié de `n` arrondie au-dessus. La première
//!   case vide ou le premier tirage perdu arrête la boucle. L'effet part de l'attaquant vers sa cible
//!   (`FUN_1405706d0` : `+0x6dc`, options `G1` et `J1` de `FUN_140572e20`) ; ArchRust le pose **sur la cible**
//!   (déduit : sens exact de ces options non relevé).

use super::App;
use crate::app::effects_glue::Placed;
use crate::assets::Assets;
use crate::render::ArmorGlow;
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Catégories d'`ee_ss_*.txt` dans l'ordre des indices du client (`FUN_140831280`).
pub const CATEGORIES: [&str; 10] = ["FIRE", "AIR", "WATER", "MAGIC", "EARTH", "ICE", "LIGHTENING", "POISON", "PHYSICAL", "RUNE"];
const PHYSICAL: usize = 8;
const RUNE: usize = 9;
/// `Kind` d'une arme.
const KIND_WEAPON: i64 = 2;
/// Recalcul des effets des armes portées au plus toutes les…
const REFRESH: Duration = Duration::from_millis(500);

/// Catégorie d'une pierre d'esprit selon son élément (`FUN_140831210`) : 1 magie → MAGIC, 2 eau →
/// WATER, 3 feu → FIRE, 4 terre → EARTH, 5 air → AIR, 6 → ICE, 7 → LIGHTENING, 8 → POISON.
pub fn stone_category(spirit: i64) -> Option<usize> {
    Some(match spirit {
        1 => 3,
        2 => 2,
        3 => 0,
        4 => 4,
        5 => 1,
        6 => 5,
        7 => 6,
        8 => 7,
        _ => return None,
    })
}

/// `ItemConvertPoint.txt` : points par niveau de renforcement, rang de pierre, rang de rune et
/// nombre de sockets (indice = niveau, 0 → 0 point).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConvertPoints {
    pub reinforcement: Vec<i32>,
    pub stone: Vec<i32>,
    pub rune: Vec<i32>,
    pub socket: Vec<i32>,
}

impl ConvertPoints {
    /// Table à quatre paires de colonnes (niveau, points), en-tête sur la première ligne.
    pub fn parse(text: &str) -> ConvertPoints {
        let mut out = ConvertPoints::default();
        for line in text.lines().skip(1) {
            let c: Vec<&str> = line.split('\t').collect();
            for (k, list) in [&mut out.reinforcement, &mut out.stone, &mut out.rune, &mut out.socket].into_iter().enumerate() {
                let (Some(level), Some(points)) = (c.get(2 * k).and_then(|v| v.trim().parse::<usize>().ok()), c.get(2 * k + 1).and_then(|v| v.trim().parse::<i32>().ok())) else { continue };
                if level < 64 {
                    if list.len() <= level {
                        list.resize(level + 1, 0);
                    }
                    list[level] = points;
                }
            }
        }
        out
    }

    fn get(list: &[i32], i: i64) -> i32 {
        usize::try_from(i).ok().and_then(|i| list.get(i)).copied().unwrap_or(0)
    }
}

/// Points de conversion d'un objet (`FUN_14028e610`) et points de chaque catégorie
/// (`FUN_14056bb10`, voir l'en-tête).
pub fn convert_points(t: &ConvertPoints, level: u8, sockets: u8, fills: &[crate::app::convert_glue::SocketFill]) -> (i32, [i32; 10]) {
    let mut cat = [0i32; 10];
    let reinforcement = ConvertPoints::get(&t.reinforcement, level as i64);
    let socket = ConvertPoints::get(&t.socket, sockets as i64);
    let mut stones = 0;
    for f in fills {
        if f.rune {
            let p = if (1..=5).contains(&f.rank) { ConvertPoints::get(&t.rune, f.rank) } else { 0 };
            cat[RUNE] += p;
        } else if (1..=5).contains(&f.rank) {
            let p = ConvertPoints::get(&t.stone, f.rank);
            stones += p;
            // FIRE (0) jamais compté : FUN_14056bb10 demande son total sans objet.
            if let Some(c) = stone_category(f.spirit).filter(|c| *c != 0) {
                cat[c] += p;
            }
        }
    }
    cat[PHYSICAL] = reinforcement;
    (reinforcement + socket + stones + cat[RUNE], cat)
}

/// Catégorie retenue (`FUN_14056bb10`), avec ses égalités : `None` si aucune.
pub fn dominant(cat: &[i32; 10], fills: &[crate::app::convert_glue::SocketFill]) -> Option<usize> {
    let mut best = 0;
    let mut index: Option<usize> = None;
    let mut tie = false;
    let mut resolve = false;
    for (i, &v) in cat.iter().enumerate() {
        if v == 0 {
            continue;
        }
        match i {
            0 => {
                if v > best {
                    best = v;
                    index = Some(0);
                    tie = false;
                } else if v == best {
                    tie = true;
                }
            }
            PHYSICAL => {
                if v > best {
                    best = v;
                } else if v == best {
                    tie = false;
                } else {
                    continue;
                }
                index = Some(PHYSICAL);
            }
            RUNE => {
                if v > best {
                    best = v;
                    index = Some(RUNE);
                } else if v == best && index != Some(PHYSICAL) {
                    resolve = true;
                }
            }
            _ => {
                if v > best {
                    best = v;
                    index = Some(i);
                } else if v == best && (i == 1 || index != Some(PHYSICAL)) {
                    tie = true;
                }
            }
        }
    }
    if !(tie || resolve) {
        return index;
    }
    // Égalité : première socket dont la catégorie a le meilleur total.
    for f in fills {
        if f.rune {
            if cat[RUNE] == best {
                return Some(RUNE);
            }
        } else if let Some(c) = stone_category(f.spirit) {
            if cat[c] == best {
                return Some(c);
            }
        }
    }
    index
}

/// Ligne d'`ee_ss_change.txt`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChangeRow {
    pub point: i32,
    pub placed: Placed,
}

/// `ee_ss_change.txt` : lignes par catégorie (`SPIRIT_STONE`), dans l'ordre du fichier (10 au plus).
pub fn parse_change(text: &str) -> HashMap<usize, Vec<ChangeRow>> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let mut out: HashMap<usize, Vec<ChangeRow>> = HashMap::new();
    let (Some(name), Some(point), Some(eid)) = (col("SPIRIT_STONE"), col("POINT"), col("EID")) else { return out };
    let (x, y, z, s, node, gap) = (col("OFFSET_X"), col("OFFSET_Y"), col("OFFSET_Z"), col("SCALE"), col("NODE_ID"), col("GAP"));
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let get = |i: Option<usize>| i.and_then(|i| c.get(i)).map(|v| v.trim()).unwrap_or("");
        let Some(cat) = CATEGORIES.iter().position(|n| n.eq_ignore_ascii_case(get(Some(name)))) else { continue };
        let (Ok(p), Ok(e)) = (get(Some(point)).parse::<i32>(), get(Some(eid)).parse::<u32>()) else { continue };
        let num = |i| get(i).parse::<f32>().ok();
        let placed = Placed {
            eid: e,
            offset: Vec3::new(num(x).unwrap_or(0.0), num(y).unwrap_or(0.0), num(z).unwrap_or(0.0)),
            scale: num(s).filter(|v| *v > 0.0).unwrap_or(1.0),
            node: get(node).parse().unwrap_or(0),
            delay: get(gap).parse::<u32>().unwrap_or(0).min(60_000),
        };
        let rows = out.entry(cat).or_default();
        if rows.len() < 10 && e > 0 {
            rows.push(ChangeRow { point: p, placed });
        }
    }
    out
}

/// Effets joués sur l'arme (`FUN_140574100`) : de la dernière ligne à la première, toutes celles du plus
/// haut `POINT` ≤ `total`.
pub fn change_effects(rows: &[ChangeRow], total: i32) -> Vec<Placed> {
    let mut found: Option<i32> = None;
    let mut out = Vec::new();
    for r in rows.iter().rev() {
        if found.is_some_and(|f| f != r.point) {
            break;
        }
        if r.point <= total {
            out.push(r.placed);
            found = Some(r.point);
        }
    }
    out
}

/// `ee_ss_target.txt` (`FUN_140830a80`) : lignes par (catégorie, `LEVEL`), dans l'ordre du fichier, 10 au plus.
pub fn parse_target(text: &str) -> HashMap<(usize, i32), Vec<Placed>> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let mut out: HashMap<(usize, i32), Vec<Placed>> = HashMap::new();
    let (Some(level), Some(name), Some(eid)) = (col("LEVEL"), col("SPIRIT_STONE"), col("EID")) else { return out };
    let (x, y, z, s, node, gap) = (col("OFFSET_X"), col("OFFSET_Y"), col("OFFSET_Z"), col("SCALE"), col("NODE_ID"), col("GAP"));
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let get = |i: Option<usize>| i.and_then(|i| c.get(i)).map(|v| v.trim()).unwrap_or("");
        let Some(cat) = CATEGORIES.iter().position(|n| n.eq_ignore_ascii_case(get(Some(name)))) else { continue };
        let (Ok(lv), Ok(e)) = (get(Some(level)).parse::<i32>(), get(Some(eid)).parse::<u32>()) else { continue };
        let num = |i| get(i).parse::<f32>().ok();
        let placed = Placed {
            eid: e,
            offset: Vec3::new(num(x).unwrap_or(0.0), num(y).unwrap_or(0.0), num(z).unwrap_or(0.0)),
            scale: num(s).filter(|v| *v > 0.0).unwrap_or(1.0),
            node: get(node).parse().unwrap_or(0),
            delay: get(gap).parse::<u32>().unwrap_or(0).min(60_000),
        };
        let rows = out.entry((cat, lv)).or_default();
        if rows.len() < 10 {
            rows.push(placed);
        }
    }
    out
}

/// Tirage de `FUN_14082dac0` : (catégorie, `LEVEL`) d'une ligne d'`ee_ss_target.txt`, ou `None` s'il est perdu.
/// `fills` : sockets remplies de l'arme ; `rank` : facteur 10, 2 de l'objet ; `roll` : entier de 0 à 99.
pub fn target_draw(fills: &[crate::app::convert_glue::SocketFill], rank: i64, mut roll: impl FnMut() -> i32) -> Option<(usize, i32)> {
    let n = fills.len() as i32;
    if n == 0 {
        return None;
    }
    if roll() + 1 > (rank as i32 * 11 - 11) / 4 + 30 + (n * 29 - 29) / 9 {
        return None;
    }
    let share = 1.0 / n as f32;
    let count = |spirit: i64| fills.iter().filter(|f| !f.rune && f.spirit == spirit).count() as f32;
    let r = roll();
    let mut edge = 0;
    // Ordre de FUN_14056be90 : eau (2), feu (3), terre (4), air (5), magie (1).
    for (spirit, cat) in [(2, 2), (3, 0), (4, 4), (5, 1), (1, 3)] {
        edge += (count(spirit) * share * 100.0) as i32;
        if r < edge {
            return Some((cat, (n + 1) / 2));
        }
    }
    None
}

/// `Kind` d'une armure, `Part` du corps.
const KIND_ARMOR: i64 = 1;
const PART_BODY: i64 = 2;
/// Paliers gardés par catégorie d'`ee_ss_fx.txt` (`FUN_14082cf80`).
const FX_LEVELS: usize = 5;

/// Palier d'`ee_ss_fx.txt` : `POINT` et matériau.
#[derive(Debug, Clone, PartialEq)]
pub struct FxRow {
    pub point: i32,
    pub glow: Arc<ArmorGlow>,
}

/// Nombres d'une case `"a,b,c,d"` (guillemets ôtés), absents à 0.
fn numbers<const N: usize>(cell: &str) -> [f32; N] {
    let mut out = [0.0; N];
    for (o, v) in out.iter_mut().zip(cell.trim().trim_matches('"').split(',')) {
        *o = v.trim().parse().unwrap_or(0.0);
    }
    out
}

/// `ee_ss_fx.txt` (`FUN_14082f0f0`) : paliers par catégorie (`TYPE`), triés par `POINT` décroissant,
/// cinq au plus, une ligne de même `POINT` remplaçant la précédente. Couleurs entières `/255`.
/// Seul le matériau `uvAnim.fx` est reconnu (toutes les lignes du client).
pub fn parse_fx(text: &str) -> HashMap<usize, Vec<FxRow>> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let mut out: HashMap<usize, Vec<FxRow>> = HashMap::new();
    let (Some(kind), Some(point)) = (col("TYPE"), col("POINT")) else { return out };
    for l in lines {
        let c: Vec<&str> = l.split('\t').collect();
        let get = |n: &str| col(n).and_then(|i| c.get(i)).map(|v| v.trim()).unwrap_or("");
        let Some(cat) = CATEGORIES.iter().position(|n| n.eq_ignore_ascii_case(c.get(kind).map_or("", |v| v.trim()))) else { continue };
        let Ok(p) = c.get(point).map_or("", |v| v.trim()).parse::<i32>() else { continue };
        if !get("FX").eq_ignore_ascii_case("uvAnim.fx") {
            continue;
        }
        let color = |n: &str| numbers::<3>(get(n)).map(|v| (v as i32) as f32 / 255.0);
        let [tu, tv] = numbers::<2>(get("Uv Translation"));
        let [su, sv] = numbers::<2>(get("UV Scale"));
        let glow = ArmorGlow {
            // Nom RenderWare sans extension (les textures du client sont rangées sans elle).
            texture: get("TEXTURE").rsplit_once('.').map_or(get("TEXTURE"), |(stem, _)| stem).to_string(),
            scale: [su, sv],
            velocity: [tu, tv],
            rotation: get("UV Rotation").parse().unwrap_or(0.0),
            weight: get("UV Anim weight").parse().unwrap_or(0.0),
            u_select: numbers::<3>(get("U Selection vector")),
            v_select: numbers::<3>(get("V Selection vector")),
            ambient: color("AMBIENT"),
            diffuse: color("DIFFUSE"),
            specular: color("SPECULAR"),
        };
        let rows = out.entry(cat).or_default();
        if let Some(r) = rows.iter_mut().find(|r| r.point == p) {
            r.glow = Arc::new(glow);
        } else if rows.len() < FX_LEVELS {
            rows.push(FxRow { point: p, glow: Arc::new(glow) });
            rows.sort_by(|a, b| b.point.cmp(&a.point));
        }
    }
    out
}

/// Palier d'armure pour `total` points (`FUN_14082cf80`) : premier de la liste décroissante dont
/// `POINT` ≤ total.
pub fn fx_for(rows: &[FxRow], total: i32) -> Option<&FxRow> {
    rows.iter().find(|r| r.point <= total)
}

/// État de la colle (champ `stones` d'[`App`]).
#[derive(Default)]
pub(super) struct StonesGlue {
    points: Option<ConvertPoints>,
    change: Option<HashMap<usize, Vec<ChangeRow>>>,
    fx: Option<HashMap<usize, Vec<FxRow>>>,
    target: Option<Arc<HashMap<(usize, i32), Vec<Placed>>>>,
    /// Générateur des tirages d'`ee_ss_target.txt` (xorshift ; 0 : pas encore amorcé).
    rng: u32,
    /// Effets en cours par personnage : (IID de l'arme, EID joués).
    current: HashMap<i32, (i32, Vec<u32>)>,
    last: Option<Instant>,
}

fn load_tables(assets: &Assets) -> (ConvertPoints, HashMap<usize, Vec<ChangeRow>>) {
    let points = ConvertPoints::parse(&super::npc_glue::client_text_in(assets, "ini", "ItemConvertPoint.txt"));
    let change = parse_change(&super::npc_glue::client_text_in(assets, "ini", "ee_ss_change.txt"));
    (points, change)
}

impl StonesGlue {
    /// Entier de 0 à 99.
    fn roll(&mut self) -> i32 {
        if self.rng == 0 {
            self.rng = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() | 1);
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 100) as i32
    }
}

impl App {
    /// Coup qui touche (`FUN_140573960` → `FUN_140572e20`) : effets d'`ee_ss_target.txt` selon l'arme de
    /// l'attaquant, posés sur la cible.
    pub(super) fn stones_on_hit(&mut self, attacker: i32, target: i32) {
        let assets = self.assets.clone();
        let weapons: Vec<(i64, i32, i32)> = self
            .player
            .iter()
            .chain(self.entities.iter())
            .find(|e| e.cid == attacker)
            .map(|e| e.equipment.iter().filter_map(|q| crate::worn::item_look(&assets, q.template_id).filter(|l| l.kind == KIND_WEAPON).map(|l| (l.part, q.item_id, q.template_id))).collect())
            .unwrap_or_default();
        // Partie 10, sinon 9 (`FUN_1403116e0`).
        let Some(&(_, iid, tid)) = weapons.iter().find(|w| w.0 == 10).or_else(|| weapons.iter().find(|w| w.0 == 9)) else { return };
        let Some((_, _, fills)) = self.convert.socket_fills(&assets, iid) else { return };
        let rank = self.convert.item_rank(&assets, tid);
        let table = self.stones.target.get_or_insert_with(|| Arc::new(parse_target(&super::npc_glue::client_text_in(&assets, "ini", "ee_ss_target.txt")))).clone();
        let mut placed = Vec::new();
        for i in 0..10 {
            let stones = &mut self.stones;
            let Some(p) = target_draw(&fills, rank, || stones.roll()).and_then(|key| table.get(&key)?.get(i).copied()) else { break };
            placed.push(p);
        }
        if !placed.is_empty() {
            self.effects_play_combat(target, &placed);
        }
    }

    /// Effets des armes serties des personnages en vue (toutes les 500 ms).
    pub(super) fn stones_frame(&mut self) {
        if self.stones.last.is_some_and(|l| l.elapsed() < REFRESH) {
            return;
        }
        self.stones.last = Some(Instant::now());
        if self.stones.points.is_none() {
            let (p, c) = load_tables(&self.assets);
            self.stones.points = Some(p);
            self.stones.change = Some(c);
            self.stones.fx = Some(parse_fx(&super::npc_glue::client_text_in(&self.assets, "ini", "ee_ss_fx.txt")));
        }
        self.armor_glow_frame();
        let assets = self.assets.clone();
        // Arme portée de chaque personnage en vue (IID, nœud de la main qui la tient).
        let carriers: Vec<(i32, i32, i32)> = self
            .player
            .iter()
            .chain(self.entities.iter())
            .filter_map(|e| {
                let weapon = e.equipment.iter().find(|q| crate::worn::item_look(&assets, q.template_id).is_some_and(|l| l.kind == KIND_WEAPON))?;
                let node = e.meshes.iter().find_map(|(m, _)| m.worn.as_ref().filter(|w| w.kind == KIND_WEAPON).and_then(|w| w.node)).map_or(0, |n| n as i32);
                Some((e.cid, weapon.item_id, node))
            })
            .collect();
        let seen: Vec<i32> = carriers.iter().map(|c| c.0).collect();
        // Personnages partis ou désarmés : effets retirés.
        let gone: Vec<i32> = self.stones.current.keys().copied().filter(|c| !seen.contains(c)).collect();
        for cid in gone {
            self.stones.current.remove(&cid);
            self.effects_set_stones(cid, &[]);
        }
        for (cid, iid, node) in carriers {
            let wanted: Vec<Placed> = self
                .convert
                .socket_fills(&assets, iid)
                .and_then(|(level, sockets, fills)| {
                    let (total, cat) = convert_points(self.stones.points.as_ref()?, level, sockets, &fills);
                    let c = dominant(&cat, &fills)?;
                    let rows = self.stones.change.as_ref()?.get(&c)?;
                    Some(change_effects(rows, total))
                })
                .unwrap_or_default()
                .into_iter()
                .map(|p| Placed { node: if p.node == 0 { node } else { p.node }, ..p })
                .collect();
            let eids: Vec<u32> = wanted.iter().map(|p| p.eid).collect();
            let playing = self.effects_has_stones(cid);
            let same = self.stones.current.get(&cid).is_some_and(|(i, e)| *i == iid && *e == eids);
            if same && (eids.is_empty() || playing) {
                continue;
            }
            self.effects_set_stones(cid, &wanted);
            self.stones.current.insert(cid, (iid, eids));
        }
    }

    /// Lueur des armures de corps serties (`FUN_1405724b0`) : palier d'`ee_ss_fx.txt` de la catégorie
    /// dominante posé sur les pièces d'armure de corps du personnage, retiré sans palier.
    fn armor_glow_frame(&mut self) {
        let assets = self.assets.clone();
        let mut textures: Vec<String> = Vec::new();
        let ids: Vec<(i32, Option<i32>)> = self
            .player
            .iter()
            .chain(self.entities.iter())
            .map(|e| {
                let armor = e.equipment.iter().find(|q| crate::worn::item_look(&assets, q.template_id).is_some_and(|l| l.kind == KIND_ARMOR && l.part == PART_BODY));
                (e.cid, armor.map(|a| a.item_id))
            })
            .collect();
        let mut wanted: HashMap<i32, Option<Arc<ArmorGlow>>> = HashMap::new();
        for (cid, iid) in ids {
            let glow = iid.and_then(|iid| {
                let (level, sockets, fills) = self.convert.socket_fills(&assets, iid)?;
                let (total, cat) = convert_points(self.stones.points.as_ref()?, level, sockets, &fills);
                let c = dominant(&cat, &fills)?;
                fx_for(self.stones.fx.as_ref()?.get(&c)?, total).map(|r| r.glow.clone())
            });
            if let Some(g) = &glow {
                textures.push(g.texture.clone());
            }
            wanted.insert(cid, glow);
        }
        for e in self.player.iter().chain(self.entities.iter()) {
            let Some(glow) = wanted.get(&e.cid) else { continue };
            for (mesh, _) in &e.meshes {
                let body = mesh.item_tid.is_some_and(|t| crate::scene::item_part_for_tid(&assets, t) == Some(PART_BODY));
                if !body {
                    continue;
                }
                let mut slot = mesh.glow.lock().unwrap();
                if slot.as_ref().map(Arc::as_ptr) != glow.as_ref().map(Arc::as_ptr) {
                    *slot = glow.clone();
                }
            }
        }
        if let Some(gpu) = self.gpu.as_mut() {
            textures.sort();
            textures.dedup();
            for t in textures {
                crate::scene::ensure_texture(&mut gpu.renderer, &assets, &t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::convert_glue::SocketFill;

    const POINTS: &str = "Reinforcement Level\tReinforcement Points\tElemental Stone Level\tElemental Stone Point\tRune Level\tRune Points\tSocket Amount\tSocket Points\n1\t2\t1\t2\t1\t2\t1\t2\n2\t4\t2\t3\t2\t4\t2\t4\n3\t6\t3\t4\t3\t6\t3\t6\n4\t8\t4\t5\t4\t8\t4\t8\n5\t10\t5\t6\t5\t10\t5\t10\n6\t12\t\t\t\t\t6\t12\n";

    fn stone(spirit: i64, rank: i64) -> SocketFill {
        SocketFill { rune: false, spirit, rank }
    }

    #[test]
    fn points_d_une_arme_sertie() {
        let t = ConvertPoints::parse(POINTS);
        assert_eq!(t.reinforcement[6], 12);
        assert_eq!(t.stone[5], 6);
        // Renfort 3 (6) + 3 sockets (6) + air rang 5 (6) + air rang 2 (3) + rune rang 1 (2).
        let fills = [stone(5, 5), stone(5, 2), SocketFill { rune: true, spirit: 0, rank: 1 }];
        let (total, cat) = convert_points(&t, 3, 3, &fills);
        assert_eq!(total, 23);
        assert_eq!((cat[1], cat[PHYSICAL], cat[RUNE]), (9, 6, 2));
        assert_eq!(dominant(&cat, &fills), Some(1));
        // Le feu ne compte jamais (FUN_14056bb10) : le renforcement l'emporte.
        let (_, cat) = convert_points(&t, 3, 1, &[stone(3, 5)]);
        assert_eq!(cat[0], 0);
        assert_eq!(dominant(&cat, &[stone(3, 5)]), Some(PHYSICAL));
        // Égalité eau / magie : l'ordre des sockets tranche.
        let fills = [stone(1, 3), stone(2, 3)];
        let (_, cat) = convert_points(&t, 0, 2, &fills);
        assert_eq!(dominant(&cat, &fills), Some(3));
    }

    #[test]
    fn effets_du_plus_haut_palier() {
        let text = "SPIRIT_STONE\tPOINT\tEID\tOFFSET_X\tOFFSET_Y\tOFFSET_Z\tSCALE\tNODE_ID\tGAP\nAIR\t2\t3151\t0\t0\t0\t1\t0\t0\nAIR\t20\t3155\t0\t0\t0\t1\t0\t0\nAIR\t30\t3155\t0\t0\t0\t1\t0\t0\nAIR\t45\t3156\t0\t0\t0\t1\t0\t0\n";
        let rows = parse_change(text);
        let air = &rows[&1];
        let eids = |total| change_effects(air, total).iter().map(|p| p.eid).collect::<Vec<_>>();
        assert_eq!(eids(1), Vec::<u32>::new());
        assert_eq!(eids(2), vec![3151]);
        assert_eq!(eids(29), vec![3155]);
        assert_eq!(eids(44), vec![3155]);
        assert_eq!(eids(100), vec![3156]);
    }

    #[test]
    fn effet_sur_la_cible_selon_les_pierres() {
        let text = "LEVEL\tSPIRIT_STONE\tEID\tOFFSET_X\tOFFSET_Y\tOFFSET_Z\tSCALE\tNODE_ID\tGAP\n1\tEARTH\t3202\t0\t0\t0\t0\t-4\t0\n3\tWATER\t3104\t0\t0\t0\t0.5\t-4\t0\n";
        let t = parse_target(text);
        assert_eq!((t[&(4, 1)][0].eid, t[&(4, 1)][0].scale, t[&(4, 1)][0].node), (3202, 1.0, -4));
        assert_eq!((t[&(2, 3)][0].eid, t[&(2, 3)][0].scale), (3104, 0.5));
        let rolls = |v: Vec<i32>| {
            let mut i = v.into_iter();
            move || i.next().unwrap()
        };
        // Aucune socket remplie : rien.
        assert_eq!(target_draw(&[], 1, rolls(vec![])), None);
        // 1 pierre, rang 1 : chance de 30 sur 100 (29 passe, 30 non) ; LEVEL 1.
        assert_eq!(target_draw(&[stone(4, 1)], 1, rolls(vec![29, 0])), Some((4, 1)));
        assert_eq!(target_draw(&[stone(4, 1)], 1, rolls(vec![30])), None);
        // Rang 5, 8 sockets : 30 + 29×7/9 (22) + 11×4/4 (11) = 63.
        assert_eq!(target_draw(&[stone(2, 1); 8], 5, rolls(vec![62, 99])), Some((2, 4)));
        assert_eq!(target_draw(&[stone(2, 1); 8], 5, rolls(vec![63])), None);
        // Eau 2, feu 1, magie 1 (25 % par pierre) : eau jusqu'à 49, feu jusqu'à 74, magie ensuite ; LEVEL 2.
        let mix = [stone(2, 1), stone(2, 1), stone(3, 1), stone(1, 1)];
        assert_eq!(target_draw(&mix, 1, rolls(vec![0, 49])), Some((2, 2)));
        assert_eq!(target_draw(&mix, 1, rolls(vec![0, 50])), Some((0, 2)));
        assert_eq!(target_draw(&mix, 1, rolls(vec![0, 75])), Some((3, 2)));
        // Rune et glace comptent dans n, mais ne donnent aucune catégorie.
        let other = [stone(6, 1), SocketFill { rune: true, spirit: 0, rank: 1 }];
        assert_eq!(target_draw(&other, 1, rolls(vec![0, 0])), None);
    }

    const FX: &str = "TYPE\tPOINT\tFX\tTEXTURE\tAMBIENT\tDIFFUSE\tSPECULAR\tUv Translation\tUV Scale\tUV Rotation\tUV Anim weight\tU Selection vector\tV Selection vector\t\t\n\
        AIR\t2\tuvAnim.fx\tSOUL_SKYBLUE_00.tif\t\"255,255,255\"\t\"255,255,255\"\t\"60,60,60\"\t\"0,0.040,0,0\"\t\"0.001,0.001,0,0\"\t0\t0.3\t\"1,0,0,0\"\t\"0,1,0,0\"\t\t\n\
        AIR\t45\tuvAnim.fx\tSOUL_SKYBLUE_00.tif\t\"255,255,255\"\t\"255,255,255\"\t\"60,60,60\"\t\"0.230,0.230,0,0\"\t\"0.050,0.050,0,0\"\t0\t0.65\t\"0,0,0.330,0\"\t\"0,0,0.330,0\"\t\t\n\
        AIR\t20\tuvAnim.fx\tSOUL_SKYBLUE_00.tif\t\"255,255,255\"\t\"255,255,255\"\t\"60,60,60\"\t\"0.120,0.120,0,0\"\t\"0.026,0.026,0,0\"\t0\t0.4\t\"0,0,0.330,0\"\t\"0,0,0.330,0\"\t\t\n\
        AIR\t20\tuvAnim.fx\tSOUL_SKYBLUE_00.tif\t\"255,255,255\"\t\"255,255,255\"\t\"60,60,60\"\t\"0.120,0.120,0,0\"\t\"0.026,0.026,0,0\"\t0\t0.5\t\"0,0,0.330,0\"\t\"0,0,0.330,0\"\t\t\n";

    #[test]
    fn lueur_d_armure_du_plus_haut_palier() {
        let fx = parse_fx(FX);
        let air = &fx[&1];
        // Triés par POINT décroissant ; la seconde ligne 20 remplace la première.
        assert_eq!(air.iter().map(|r| r.point).collect::<Vec<_>>(), vec![45, 20, 2]);
        assert_eq!(air[1].glow.weight, 0.5);
        let g = &air[2].glow;
        assert_eq!(g.texture, "SOUL_SKYBLUE_00");
        assert_eq!((g.scale, g.velocity, g.rotation, g.weight), ([0.001, 0.001], [0.0, 0.04], 0.0, 0.3));
        assert_eq!((g.u_select, g.v_select), ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        assert_eq!(g.ambient, [1.0; 3]);
        assert!((g.specular[0] - 60.0 / 255.0).abs() < 1e-6);
        let pick = |total| fx_for(air, total).map(|r| r.point);
        assert_eq!((pick(1), pick(2), pick(19), pick(20), pick(44), pick(200)), (None, Some(2), Some(2), Some(20), Some(20), Some(45)));
        // Paramètres du shader : poids en `glow_v.w`, temps en `glow_spec.w`.
        let u = g.uniform(3.0);
        assert_eq!(u[0], [0.001, 0.001, 0.0, 0.04]);
        assert_eq!((u[2][3], u[3][3]), (0.3, 3.0));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : tables d'`ItemConvertPoint.txt` et
    /// d'`ee_ss_change.txt` (7 catégories × 5 paliers).
    #[test]
    fn tables_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let (points, change) = load_tables(&assets);
        assert_eq!(points.reinforcement.get(16), Some(&32));
        assert_eq!(points.socket.get(8), Some(&16));
        assert_eq!(change.values().map(Vec::len).sum::<usize>(), 35);
        assert_eq!(change_effects(&change[&2], 45).iter().map(|p| p.eid).collect::<Vec<_>>(), vec![3106]);
        // `ee_ss_target.txt` : 3 niveaux × 8 catégories, une ligne chacun.
        let target = parse_target(&super::super::npc_glue::client_text_in(&assets, "ini", "ee_ss_target.txt"));
        assert_eq!((target.len(), target[&(4, 1)][0].eid, target[&(2, 3)][0].eid), (24, 3202, 3104));
        // `ee_ss_fx.txt` : 7 catégories × 5 paliers (2, 20, 30, 45, 60), textures `SOUL_*` présentes.
        let fx = parse_fx(&super::super::npc_glue::client_text_in(&assets, "ini", "ee_ss_fx.txt"));
        assert_eq!(fx.len(), 7);
        assert!(fx.values().all(|r| r.iter().map(|r| r.point).collect::<Vec<_>>() == vec![60, 45, 30, 20, 2]));
        for rows in fx.values() {
            assert!(assets.texture_found(&rows[0].glow.texture), "{}", rows[0].glow.texture);
        }
        assert_eq!(fx[&4].iter().find(|r| r.point == 45).unwrap().glow.u_select, [0.0, 0.0, 0.33]);
    }
}
