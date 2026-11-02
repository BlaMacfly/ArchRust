//! Objets portés dessinés sur le personnage (armes, boucliers, lances) et ensemble d'animations
//! selon l'arme, d'après Archonia (AgcmItem, AgcmCharacter ; sha256 e9c7609b…).
//!
//! **Attache** (`FUN_1407518a0`, `FUN_140755110`) : chaque atomic du modèle porté (`BASE_DFF`
//! d'`ItemTemplateAll.ini`) est cloné et posé sur un cadre enfant du nœud HAnim de la main du porteur :
//! arme (`Kind` 2) → 102 si `Part` = 9, sinon 101 (lance comprise) ; bouclier (`Kind` 4) → 101 si
//! `Part` = 10, sinon 102. 101 = main droite, 102 = main gauche. Le cadre reçoit la matrice
//! `TRANSFORM_DATA[TID du porteur][0]` (en mode « remplacer »), l'identité si le TID n'a pas d'entrée
//! ou si l'un des trois axes est nul (le bouclier se passe de ce test). Les cadres de l'objet dans son
//! `.ecl` sont ignorés : seuls les sommets de l'atomic comptent. D'où, en glam :
//! `monde = entité × monde_nœud(pose courante, repère du clump) × M`.
//! `EQUIP_TWOHANDS` ≠ 0 double l'arme dans l'autre main avec `TRANSFORM_DATA[TID][1]` (griffes, ailes,
//! Karon doublés). Le Xenon (`WeaponType` 26) en main droite est skinné sur le squelette du porteur
//! (`FUN_140754d50`, associé par ID de nœud : voir `Clump::skin_matrices_on`).
//!
//! **Non dessinés** (`FUN_1407553e0`) : lance et étendard (`Part` 15) à pied ; à cheval, les armes
//! des mains et le bouclier (la lance seule est en main droite : à la montée, `FUN_140750660` détache
//! les mains 10 et 9 puis attache la lance, l'inverse à la descente, `FUN_14061d530`) — ici un filtre
//! au tracé ([`Worn::visible`]) ; objet du joueur à durabilité nulle ([`durability_hides`]) ; Summoner
//! (race 4, classe 4) : seuls les objets d'`avatarsetsummoner.ini` ([`summoner_items`]).
//!
//! **Ensemble d'animations** (`AgcdCharacter+0xfc`) : [`anim_equip`] = `FUN_140754290` suivi de
//! `FUN_140756060`, relus dans le binaire.

use crate::animation::{Animator, SkinBinding};
use crate::assets::Assets;
use crate::render::{Mesh, Renderer};
use archrust_data::ini::IniSection;
use archrust_data::rw::Clump;
use archrust_netclient::world::EquipmentView;
use glam::{Mat4, Vec4};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Nœuds HAnim des mains.
pub const NODE_RIGHT_HAND: u32 = 101;
pub const NODE_LEFT_HAND: u32 = 102;
/// Parties (`Part`) : main gauche, main droite, arme montée (lance, étendard).
pub const PART_LEFT_HAND: i64 = 9;
pub const PART_RIGHT_HAND: i64 = 10;
pub const PART_LANCE: i64 = 15;
/// Genres (`Kind`) : arme, bouclier.
pub const KIND_WEAPON: i64 = 2;
pub const KIND_SHIELD: i64 = 4;
/// `WeaponType` : lance, Karon 1M, Xenon 1M.
pub const WT_LANCE: i64 = 17;
pub const WT_KARON: i64 = 24;
pub const WT_XENON: i64 = 26;

/// Pièce d'objet porté d'un maillage (`Mesh::worn`).
#[derive(Debug, Clone)]
pub struct Worn {
    /// `Part` et `Kind` de l'objet.
    pub part: i64,
    pub kind: i64,
    /// Nœud de la main ; `None` pour une pièce skinnée sur le squelette (Xenon).
    pub node: Option<u32>,
    /// Matrice d'attache (`TRANSFORM_DATA`).
    pub matrix: Mat4,
    /// `monde_nœud(pose de liaison) × M` : sans animateur.
    pub bind: Mat4,
}

impl Worn {
    /// Dessinée selon l'état monté (`AgpdCharacter+0xbfc`) : la lance (`Part` 15) seulement à cheval,
    /// armes et boucliers des mains seulement à pied.
    pub fn visible(&self, riding: bool) -> bool {
        if self.part == PART_LANCE {
            return riding;
        }
        !(riding && matches!(self.kind, KIND_WEAPON | KIND_SHIELD))
    }
}

/// Matrice monde d'une pièce d'un personnage : `base × local` pour une pièce ordinaire, `base ×
/// nœud × M × local` pour un objet en main ; `None` si la pièce n'est pas dessinée (voir
/// [`Worn::visible`]). `animator` donne la pose courante du nœud.
pub fn piece_model(mesh: &Mesh, base: Mat4, local: Mat4, riding: bool, animator: Option<&Animator>) -> Option<Mat4> {
    let Some(w) = &mesh.worn else { return Some(base * local) };
    if !w.visible(riding) {
        return None;
    }
    let Some(node) = w.node else { return Some(base * local) };
    let attach = animator.and_then(|a| a.node_matrix(node)).map(|n| n * w.matrix).unwrap_or(w.bind);
    Some(base * attach * local)
}

/// Colonnes d'`ItemTemplateAll.ini` utiles à l'affichage d'un objet porté.
#[derive(Debug, Clone, Default)]
pub struct ItemLook {
    pub tid: i32,
    pub part: i64,
    pub kind: i64,
    pub weapon_type: i64,
    /// `BASE_DFF` : n° du modèle porté (`d%07x.ecl`, pack `character`), 0 sans modèle.
    pub base_dff: u32,
    /// `EQUIP_TWOHANDS` : arme doublée dans l'autre main.
    pub twohands: bool,
    pub durability: i64,
    pub max_durability: i64,
    /// `TRANSFORM` (TID du porteur) → matrices `TRANSFORM_DATA` (`ATOMIC_COUNT` par TID).
    pub transforms: HashMap<i32, Vec<Mat4>>,
}

/// Matrice d'une ligne `TRANSFORM_DATA` : `"%f×12:%d"` = right.xyz, up.xyz, at.xyz, pos.xyz (`RwMatrix`),
/// puis un entier sans effet sur l'attache (−1, ou `0xCDCDCDCD`, déchet de l'outil).
pub fn transform_matrix(text: &str) -> Option<Mat4> {
    let v: Vec<f32> = text.split(':').take(12).map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
    if v.len() < 12 {
        return None;
    }
    let col = |i: usize, w: f32| Vec4::new(v[i], v[i + 1], v[i + 2], w);
    Some(Mat4::from_cols(col(0, 0.0), col(3, 0.0), col(6, 0.0), col(9, 1.0)))
}

impl ItemLook {
    /// Lit la section d'un objet (`header` : en-tête `[]` qui nomme les colonnes positionnelles). Les
    /// clés se répètent (blocs de facteurs) : la première valeur de chaque colonne compte, sauf
    /// `TRANSFORM` / `TRANSFORM_DATA`, lus dans l'ordre (`FUN_1408afec0`, `FUN_1408e1620`).
    pub fn parse(tid: i32, header: &IniSection, sec: &IniSection) -> ItemLook {
        let name_of = |k: &str| header.get(k).unwrap_or("");
        let mut look = ItemLook { tid, ..Default::default() };
        let mut seen: HashSet<&str> = HashSet::new();
        let mut current: Option<i32> = None;
        for (k, v) in &sec.entries {
            let name = name_of(k);
            let num = || v.trim().parse::<f64>().map(|x| x as i64).unwrap_or(0);
            if name.eq_ignore_ascii_case("TRANSFORM") {
                current = v.trim().parse().ok();
                continue;
            }
            if name.eq_ignore_ascii_case("TRANSFORM_DATA") {
                if let (Some(t), Some(m)) = (current, transform_matrix(v)) {
                    look.transforms.entry(t).or_default().push(m);
                }
                continue;
            }
            if !seen.insert(name) {
                continue;
            }
            match name.to_ascii_lowercase().as_str() {
                "part" => look.part = num(),
                "kind" => look.kind = num(),
                "weapontype" => look.weapon_type = num(),
                "base_dff" => look.base_dff = num().max(0) as u32,
                "equip_twohands" => look.twohands = num() != 0,
                "durability" => look.durability = num(),
                "maxdurability" => look.max_durability = num(),
                _ => {}
            }
        }
        look
    }

    /// Fichier du modèle porté.
    pub fn model_file(&self) -> Option<String> {
        (self.base_dff > 0).then(|| format!("d{:07x}.ecl", self.base_dff))
    }

    /// Nœud de la main (`(Part == 9) + 'e'` pour une arme, `(Part != 10) + 'e'` pour un bouclier).
    pub fn hand_node(&self) -> u32 {
        let left = if self.kind == KIND_SHIELD { self.part != PART_RIGHT_HAND } else { self.part == PART_LEFT_HAND };
        if left { NODE_LEFT_HAND } else { NODE_RIGHT_HAND }
    }

    /// Nœud de la copie d'une arme doublée (`(Part != 9) + 101`).
    pub fn other_hand_node(&self) -> u32 {
        if self.part != PART_LEFT_HAND { NODE_LEFT_HAND } else { NODE_RIGHT_HAND }
    }

    /// Xenon en main droite : skinné comme une armure (`FUN_140754d50`).
    pub fn skinned(&self) -> bool {
        self.kind == KIND_WEAPON && self.weapon_type == WT_XENON && self.part == PART_RIGHT_HAND
    }

    /// Matrice `TRANSFORM_DATA[wearer][index]` ; identité sans entrée, ou si `checked` et qu'un des
    /// axes est nul (`FUN_140755110`, `FUN_140755dc0`).
    pub fn matrix(&self, wearer: i32, index: usize, checked: bool) -> Mat4 {
        let Some(m) = self.transforms.get(&wearer).and_then(|l| l.get(index)).copied() else { return Mat4::IDENTITY };
        let null = [m.x_axis, m.y_axis, m.z_axis].iter().any(|a| a.truncate().length() == 0.0);
        if checked && null { Mat4::IDENTITY } else { m }
    }
}

type Cache<T> = Mutex<Option<(Option<PathBuf>, HashMap<i32, Option<Arc<T>>>)>>;

fn cached<T>(cache: &Cache<T>, assets: &Assets, key: i32, make: impl FnOnce() -> Option<T>) -> Option<Arc<T>> {
    {
        let guard = cache.lock().unwrap();
        if let Some((dir, map)) = guard.as_ref() {
            if *dir == assets.game_dir {
                if let Some(v) = map.get(&key) {
                    return v.clone();
                }
            }
        }
    }
    let value = make().map(Arc::new);
    let mut guard = cache.lock().unwrap();
    if guard.as_ref().is_none_or(|(d, _)| *d != assets.game_dir) {
        *guard = Some((assets.game_dir.clone(), HashMap::new()));
    }
    if let Some((_, map)) = guard.as_mut() {
        map.insert(key, value.clone());
    }
    value
}

/// Objet `tid` d'`ItemTemplateAll.ini` (fusionnée avec `ItemTemplateAll2.ini`), gardé en cache.
pub fn item_look(assets: &Assets, tid: i32) -> Option<Arc<ItemLook>> {
    static CACHE: Cache<ItemLook> = Mutex::new(None);
    if tid <= 0 {
        return None;
    }
    cached(&CACHE, assets, tid, || {
        let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
        Some(ItemLook::parse(tid, table.section("")?, table.section(&tid.to_string())?))
    })
}

/// Race, classe et `ANIMATION_TYPE2` d'un gabarit de personnage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WearerInfo {
    /// 1 humain, 2 orc, 3 elfe de lune, 4 Dragon Scion.
    pub race: i32,
    /// 1 guerrier, 2 archer, 3 Scion, 4 mage, 5 autres.
    pub class: i32,
    /// `ANIMATION_TYPE2` (`charactertemplateanimation.ini`) : 0 base, 1 guerrier, 2 archer, 3 mage,
    /// 4 boss, 5 ArchLord.
    pub type2: i64,
}

/// Gabarits elfes sans `Race` ni `Class` dans `CharacterTemplatePublic.ini` (le serveur les envoie) :
/// **déduit** (Swashbuckler 377, Ranger 460, Elementalist 9).
fn elf_race_class(tid: i32) -> Option<(i32, i32)> {
    match tid {
        377 => Some((3, 1)),
        460 => Some((3, 2)),
        9 => Some((3, 4)),
        _ => None,
    }
}

/// Race, classe et `ANIMATION_TYPE2` du gabarit `tid`, gardés en cache.
pub fn wearer_info(assets: &Assets, tid: i32) -> WearerInfo {
    static CACHE: Cache<WearerInfo> = Mutex::new(None);
    let info = cached(&CACHE, assets, tid, || {
        let (race, class) = crate::scene::template_race_class(assets, tid).or_else(|| elf_race_class(tid)).unwrap_or((0, 0));
        let type2 = assets.table("ini", "charactertemplateanimation.ini").ok().and_then(|t| {
            let key = t.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("ANIMATION_TYPE2")).map(|(k, _)| k.clone())?;
            t.section(&tid.to_string())?.get(&key)?.trim().parse::<i64>().ok()
        });
        Some(WearerInfo { race, class, type2: type2.unwrap_or(0) })
    });
    info.map(|i| *i).unwrap_or_default()
}

/// Objets des mains pris en compte par `FUN_140754290` : `WeaponType` de l'objet en `Part` 9 et 10
/// (un bouclier vaut 0), présence d'un objet en `Part` 15.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hands {
    pub left: Option<i64>,
    pub right: Option<i64>,
    pub lance: bool,
}

impl Hands {
    pub fn of(assets: &Assets, equipment: &[EquipmentView]) -> Hands {
        let mut h = Hands::default();
        for e in equipment {
            let Some(look) = item_look(assets, e.template_id) else { continue };
            match look.part {
                PART_LEFT_HAND => h.left = h.left.or(Some(look.weapon_type)),
                PART_RIGHT_HAND => h.right = h.right.or(Some(look.weapon_type)),
                PART_LANCE => h.lance = true,
                _ => {}
            }
        }
        h
    }
}

fn base_type2(type2: i64) -> bool {
    matches!(type2, 0 | 4 | 5)
}

/// Ensemble à cheval d'un joueur hors Dragon Scion (`FUN_140756060`, `FUN_140754290`) selon
/// `ANIMATION_TYPE2` et le type passé (17 lance, 20 étendard, autre) ; −1 : aucune animation.
pub fn ride_set(type2: i64, weapon_type: i64) -> i32 {
    match (type2, weapon_type) {
        (0 | 4, _) => 0,
        (1, 17) => 8,
        (1, 20) => 9,
        (1, _) => 7,
        (2 | 3, 17) => 5,
        (2 | 3, 20) => 7,
        (2 | 3, _) => 4,
        (5, 20) => 2,
        (5, _) => 1,
        _ => -1,
    }
}

/// `FUN_140756060` : ensemble d'animations pour le type d'arme `t` (−1 sans arme) ; `current` est
/// l'ensemble actuel (« inchangé »).
pub fn set_for_type(w: WearerInfo, player: bool, riding: bool, t: i64, current: i32) -> i32 {
    if !player {
        return if base_type2(w.type2) { 0 } else { current };
    }
    if riding && w.race != 4 {
        return ride_set(w.type2, t);
    }
    let unchanged = current;
    let mounted = |v: i32| if riding { v } else { unchanged };
    match w.class {
        1 => match t {
            0 | 21 | 22 => 1,
            1..=3 => 2,
            4 => 3,
            5..=7 | 23 => 4,
            8 => 5,
            9 => 6,
            17 => mounted(8),
            20 => mounted(9),
            24 | 26 => 10,
            25 | 27 => 11,
            -1 => if riding { 7 } else { 0 },
            _ => unchanged,
        },
        2 => match t {
            10 => 1,
            11 => 2,
            17 => mounted(5),
            18 => 6,
            20 => mounted(7),
            24 | 26 => 8,
            25 | 27 => 9,
            -1 => if riding { 4 } else { 0 },
            _ => unchanged,
        },
        3 => match t {
            -1 => if riding { 4 } else { 0 },
            24..=27 => 8,
            _ => unchanged,
        },
        4 if w.race == 4 => 8,
        4 => match t {
            14 => 1,
            15 | 16 => 2,
            17 => mounted(5),
            19 => 6,
            20 => mounted(7),
            24 | 26 => 8,
            25 | 27 => 9,
            -1 => if riding { 4 } else { 0 },
            _ => unchanged,
        },
        5 if base_type2(w.type2) => 0,
        _ => unchanged,
    }
}

/// Ensemble direct de `FUN_140754290` (Karon d'un Scion, rapière + dague d'un elfe guerrier) : non
/// joueur et monté traités comme dans `FUN_140756060`, puis `by_class`.
fn direct_set(w: WearerInfo, player: bool, riding: bool, current: i32, by_class: impl Fn(i32) -> i32) -> i32 {
    if !player {
        return if base_type2(w.type2) { 0 } else { current };
    }
    if riding && w.race != 4 {
        return ride_set(w.type2, 0);
    }
    by_class(w.class)
}

/// `FUN_140754290` puis `FUN_140756060` : ensemble d'animations d'un personnage d'après ses mains,
/// son état monté, sa race, sa classe et son `ANIMATION_TYPE2`. `current` : ensemble actuel.
pub fn anim_equip(w: WearerInfo, player: bool, riding: bool, hands: Hands, current: i32) -> i32 {
    if riding && hands.lance {
        // Lance comme étendard : le type passé est 17.
        return set_for_type(w, player, riding, WT_LANCE, current);
    }
    let scion = w.race == 4 && matches!(w.class, 1 | 4);
    let karon = || {
        direct_set(w, player, riding, current, |class| match class {
            1 => 10,
            2..=4 => 8,
            5 if base_type2(w.type2) => 0,
            _ => current,
        })
    };
    match (hands.left, hands.right) {
        (None, None) => set_for_type(w, player, riding, -1, current),
        (Some(g), None) if scion => if g == WT_KARON { karon() } else { set_for_type(w, player, riding, -1, current) },
        (Some(g), None) => set_for_type(w, player, riding, g, current),
        (Some(22), Some(21)) if w.race == 3 && w.class == 1 => direct_set(w, player, riding, current, |_| 4),
        (g, Some(d)) if scion => match g {
            None => set_for_type(w, player, riding, -1, current),
            Some(WT_KARON) => karon(),
            Some(_) => set_for_type(w, player, riding, d, current),
        },
        (_, Some(d)) => set_for_type(w, player, riding, d, current),
    }
}

/// [`anim_equip`] pour le personnage de gabarit `tid` et d'équipement `equipment` (objets réels,
/// avant l'apparence d'évolution).
pub fn anim_equip_for(assets: &Assets, tid: i32, player: bool, riding: bool, equipment: &[EquipmentView], current: i32) -> i32 {
    anim_equip(wearer_info(assets, tid), player, riding, Hands::of(assets, equipment), current)
}

/// Objet du joueur non dessiné faute de durabilité (`FUN_1407553e0`) : genre autre qu'anneau,
/// collier et 0x40, pièce de partie normale (pas avatar 16-29), durabilité `current` (paquet, sinon
/// celle du modèle) nulle. **Choix d'ArchRust** : un objet sans durabilité maximale (armes de monstres,
/// griffes, trophées : `MaxDurability` 0) reste dessiné.
pub fn durability_hides(look: &ItemLook, current: Option<i32>) -> bool {
    if matches!(look.kind, 8 | 0x10 | 0x40) || (16..30).contains(&look.part) || look.max_durability <= 0 {
        return false;
    }
    current.map_or(look.durability, i64::from) <= 0
}

/// Objets dessinés sur un Summoner (`AgpmItem+0x1b60`, `FUN_1401c9cc0`) : TID des ensembles
/// d'`avatarsetsummoner.ini` (`tid0`…`tid5`), **déduit**.
pub fn summoner_items(assets: &Assets) -> Arc<HashSet<i32>> {
    static CACHE: Cache<HashSet<i32>> = Mutex::new(None);
    cached(&CACHE, assets, 0, || {
        let table = assets.table("ini", "avatarsetsummoner.ini").ok();
        let set: HashSet<i32> = table
            .iter()
            .flat_map(|t| t.sections.iter())
            .flat_map(|s| s.entries.iter())
            .filter(|(k, _)| k.to_ascii_lowercase().starts_with("tid"))
            .filter_map(|(_, v)| v.trim().parse().ok())
            .collect();
        Some(set)
    })
    .unwrap_or_default()
}

/// Pièces GPU d'une arme ou d'un bouclier porté par `wearer` (TID après évolution) dont `skeleton`
/// est le squelette : `(maillage, locale)` avec `Mesh::worn` rempli (voir [`piece_model`]). Rien si
/// la main manque au squelette (`RpHAnimIDGetIndex` < 0) ou si le modèle est introuvable.
pub fn worn_pieces(renderer: &mut Renderer, assets: &Assets, look: &ItemLook, wearer: i32, skeleton: &Arc<Clump>) -> Vec<(Arc<Mesh>, Mat4)> {
    let Some(file) = look.model_file() else { return Vec::new() };
    let frame_of = |id: u32| skeleton.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == id));
    let bind_world = skeleton.world_matrices_rooted();
    let checked = look.kind == KIND_WEAPON;
    let mut hands = vec![(look.hand_node(), look.matrix(wearer, 0, checked))];
    if look.kind == KIND_WEAPON && look.twohands {
        hands.push((look.other_hand_node(), look.matrix(wearer, 1, true)));
    }
    let worn = |node: Option<u32>, matrix: Mat4| {
        let bind = node.and_then(frame_of).and_then(|f| bind_world.get(f).copied()).unwrap_or(Mat4::IDENTITY) * matrix;
        Worn { part: look.part, kind: look.kind, node, matrix, bind }
    };
    // Modèle remplacé par un glTF d'un patch : ses maillages, avec leur matrice, dans la main.
    let key = format!("character/{file}");
    if assets.patched_model(&key).is_some() {
        let mut out = Vec::new();
        for (node, matrix) in &hands {
            if frame_of(*node).is_none() {
                continue;
            }
            for (mut mesh, local) in crate::scene::patched_model_meshes(renderer, assets, &key).unwrap_or_default() {
                if let Some(m) = Arc::get_mut(&mut mesh) {
                    m.worn = Some(worn(Some(*node), *matrix));
                    out.push((mesh, local));
                }
            }
        }
        return out;
    }
    let clump = match assets.clump("character", &file) {
        Ok(c) => c,
        Err(e) => {
            log::debug!("objet TID {} (modèle {file}) : {e:#}", look.tid);
            return Vec::new();
        }
    };
    let own_world = clump.world_matrices_rooted();
    let mut out = Vec::new();
    for atomic in &clump.atomics {
        let Some(geom) = clump.geometries.get(atomic.geometry as usize) else { continue };
        if geom.vertices.is_empty() {
            continue;
        }
        if look.skinned() || geom.skin.is_some() {
            // Peau liée au squelette du porteur ; la pièce suit les mains au tracé.
            let mut mesh = crate::scene::build_geometry_mesh(renderer, assets, geom);
            if geom.skin.is_some() {
                mesh.skin = Some(Arc::new(SkinBinding {
                    clump: clump.clone(),
                    geometry: atomic.geometry as usize,
                    base: crate::scene::geometry_vertices(geom),
                    wearer: Some(skeleton.clone()),
                }));
            }
            mesh.worn = Some(worn(None, Mat4::IDENTITY));
            out.push((Arc::new(mesh), own_world.get(atomic.frame as usize).copied().unwrap_or(Mat4::IDENTITY)));
            continue;
        }
        for (node, matrix) in &hands {
            if frame_of(*node).is_none() {
                continue;
            }
            let mut mesh = crate::scene::build_geometry_mesh(renderer, assets, geom);
            mesh.worn = Some(worn(Some(*node), *matrix));
            out.push((Arc::new(mesh), Mat4::IDENTITY));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_data::ini::IniTable;
    use glam::Vec3;

    fn knight() -> WearerInfo {
        WearerInfo { race: 1, class: 1, type2: 1 }
    }

    #[test]
    fn matrice_transform_data() {
        // Bastard Sword (TID 2) pour le chevalier 96.
        let m = transform_matrix("-0.130897:0.056741:-1.193403:0.240501:1.1772:0.02959:1.170306:-0.235583:-0.139565:5.938569:-5.940835:-2.858799:-842150451").unwrap();
        assert_eq!(m.x_axis, Vec4::new(-0.130897, 0.056741, -1.193403, 0.0));
        assert_eq!(m.z_axis, Vec4::new(1.170306, -0.235583, -0.139565, 0.0));
        assert_eq!(m.w_axis, Vec4::new(5.938569, -5.940835, -2.858799, 1.0));
        assert!((m.x_axis.truncate().length() - 1.2).abs() < 0.01, "échelle ≈ 1,2 incluse");
        assert!(transform_matrix("1:2:3").is_none());
    }

    #[test]
    fn lecture_d_un_objet() {
        let t = IniTable::parse("[]\n0=Name\n1=Part\n2=Kind\n3=WeaponType\n4=BASE_DFF\n5=ATOMIC_COUNT\n6=TRANSFORM\n7=TRANSFORM_DATA\n8=EQUIP_TWOHANDS\n9=Durability\n10=MaxDurability\n[7820]\n0=Slayer Basic Karon\n1=10\n2=2\n3=24\n1=4\n4=7820\n5=2\n6=1722\n7=1:0:0:0:1:0:0:0:1:0:0:0:-1\n7=1:-0.01:-0.01:-0.01:-0.97:0.25:-0.02:-0.25:-0.97:0:0:0:-1\n6=96\n7=0:0:0:0:0:0:0:0:0:0:0:0:-1\n7=1:0:0:0:1:0:0:0:1:3:4:5:-1\n8=1\n9=0\n10=0\n");
        let look = ItemLook::parse(7820, t.section("").unwrap(), t.section("7820").unwrap());
        assert_eq!((look.part, look.kind, look.weapon_type, look.twohands), (10, 2, 24, true), "première valeur d'une colonne répétée");
        assert_eq!(look.model_file().as_deref(), Some("d0001e8c.ecl"));
        assert_eq!(look.transforms.get(&1722).map(Vec::len), Some(2));
        assert_eq!(look.matrix(1722, 0, true), Mat4::IDENTITY);
        let m1 = look.matrix(1722, 1, true);
        assert!((m1.y_axis.y + 0.97).abs() < 1e-6, "[1] : rotation de 180° autour de x");
        assert_eq!(look.matrix(96, 0, true), Mat4::IDENTITY, "axes nuls : identité pour une arme");
        assert_eq!(look.matrix(96, 0, false), Mat4::ZERO + Mat4::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::W), "bouclier : sans test");
        assert_eq!(look.matrix(5, 0, true), Mat4::IDENTITY, "TID absent : identité");
        assert_eq!((look.hand_node(), look.other_hand_node()), (101, 102));
    }

    #[test]
    fn noeuds_des_mains() {
        let l = |kind, part| ItemLook { kind, part, ..Default::default() };
        assert_eq!(l(2, 10).hand_node(), 101, "épée en main droite");
        assert_eq!(l(2, 9).hand_node(), 102, "arc en main gauche");
        assert_eq!(l(2, 15).hand_node(), 101, "lance : main droite");
        assert_eq!(l(4, 9).hand_node(), 102, "bouclier à gauche");
        assert_eq!(l(4, 10).hand_node(), 101, "bouclier à droite");
        assert_eq!(l(2, 9).other_hand_node(), 101);
        assert!(ItemLook { kind: 2, part: 10, weapon_type: 26, ..Default::default() }.skinned());
        assert!(!ItemLook { kind: 2, part: 9, weapon_type: 26, ..Default::default() }.skinned());
    }

    #[test]
    fn visibilite_a_pied_et_a_cheval() {
        let w = |part, kind| Worn { part, kind, node: Some(101), matrix: Mat4::IDENTITY, bind: Mat4::IDENTITY };
        assert!(w(10, 2).visible(false) && !w(10, 2).visible(true), "épée cachée à cheval");
        assert!(w(9, 4).visible(false) && !w(9, 4).visible(true), "bouclier caché à cheval");
        assert!(!w(15, 2).visible(false) && w(15, 2).visible(true), "lance seulement à cheval");
    }

    #[test]
    fn durabilite_nulle() {
        let sword = ItemLook { kind: 2, part: 10, durability: 69, max_durability: 69, ..Default::default() };
        assert!(!durability_hides(&sword, None));
        assert!(durability_hides(&sword, Some(0)));
        assert!(!durability_hides(&sword, Some(3)));
        let claw = ItemLook { kind: 2, part: 10, ..Default::default() };
        assert!(!durability_hides(&claw, None), "sans durabilité maximale : dessiné");
        let avatar = ItemLook { kind: 1, part: 16, durability: 10, max_durability: 10, ..Default::default() };
        assert!(!durability_hides(&avatar, Some(0)));
    }

    #[test]
    fn ensembles_comme_archonia() {
        let h = |left, right| Hands { left, right, lance: false };
        let k = knight();
        // Chevalier : épée 1, hache 2, épée 2M 3, arme d'hast 5, mains nues 0 ; un arc garde l'ensemble.
        assert_eq!(anim_equip(k, true, false, h(None, Some(0)), 0), 1);
        assert_eq!(anim_equip(k, true, false, h(None, Some(1)), 0), 2);
        assert_eq!(anim_equip(k, true, false, h(None, Some(4)), 0), 3);
        assert_eq!(anim_equip(k, true, false, h(None, Some(8)), 0), 5);
        assert_eq!(anim_equip(k, true, false, h(None, None), 3), 0);
        assert_eq!(anim_equip(k, true, false, h(None, Some(10)), 3), 3, "arc au guerrier : inchangé");
        // Bouclier (WeaponType 0) à gauche et hache à droite : la hache ; bouclier seul : épée 1M.
        assert_eq!(anim_equip(k, true, false, h(Some(0), Some(1)), 0), 2);
        assert_eq!(anim_equip(k, true, false, h(Some(0), None), 0), 1);
        // Archer 1 : arc (main gauche) 1, arbalète 2 ; katariya 6.
        let archer = WearerInfo { race: 1, class: 2, type2: 2 };
        assert_eq!(anim_equip(archer, true, false, h(Some(10), None), 0), 1);
        assert_eq!(anim_equip(archer, true, false, h(None, Some(11)), 0), 2);
        assert_eq!(anim_equip(WearerInfo { race: 3, class: 2, type2: 2 }, true, false, h(None, Some(18)), 0), 6);
        // Mage : bâton 1, baguette 2, chakram 6.
        let mage = WearerInfo { race: 1, class: 4, type2: 3 };
        assert_eq!(anim_equip(mage, true, false, h(None, Some(14)), 0), 1);
        assert_eq!(anim_equip(mage, true, false, h(None, Some(16)), 0), 2);
        assert_eq!(anim_equip(WearerInfo { race: 3, class: 4, type2: 3 }, true, false, h(None, Some(19)), 0), 6);
        // Swashbuckler (elfe guerrier) rapière + dague : 4 ; dague seule (G) : 1.
        let swash = WearerInfo { race: 3, class: 1, type2: 1 };
        assert_eq!(anim_equip(swash, true, false, h(Some(22), Some(21)), 0), 4);
        assert_eq!(anim_equip(swash, true, false, h(Some(22), None), 0), 1);
        assert_eq!(anim_equip(swash, true, true, h(Some(22), Some(21)), 0), 7, "monté : table à cheval");
        // Scions : Karon en G → Slayer 10, Orbiter 8 ; Slayer sans G → aucune arme → 0 ; Summoner 8.
        let slayer = WearerInfo { race: 4, class: 1, type2: 1 };
        assert_eq!(anim_equip(slayer, true, false, h(Some(24), None), 0), 10);
        assert_eq!(anim_equip(slayer, true, false, h(Some(24), Some(24)), 0), 10);
        assert_eq!(anim_equip(slayer, true, false, h(None, Some(24)), 5), 0);
        assert_eq!(anim_equip(WearerInfo { race: 4, class: 2, type2: 2 }, true, false, h(None, Some(26)), 0), 8);
        assert_eq!(anim_equip(WearerInfo { race: 4, class: 4, type2: 3 }, true, false, h(None, None), 0), 8, "Summoner");
        // Non joueur : ANIMATION_TYPE2 0, 4, 5 → 0, sinon inchangé.
        assert_eq!(anim_equip(WearerInfo { type2: 0, ..k }, false, false, h(None, Some(4)), 2), 0);
        assert_eq!(anim_equip(k, false, false, h(None, Some(4)), 2), 2);
    }

    #[test]
    fn ensembles_a_cheval() {
        let lance = Hands { lance: true, ..Default::default() };
        let k = knight();
        assert_eq!(anim_equip(k, true, true, Hands::default(), 0), 7);
        assert_eq!(anim_equip(k, true, true, lance, 0), 8, "lance");
        assert_eq!(anim_equip(WearerInfo { class: 2, type2: 2, ..k }, true, true, lance, 0), 5);
        assert_eq!(anim_equip(WearerInfo { type2: 5, ..k }, true, true, lance, 0), 1, "ArchLord");
        assert_eq!(anim_equip(k, true, false, lance, 0), 0, "à pied, la lance ne compte pas");
        // FUN_140756060 : colonne étendard (20), atteinte seulement si le type 20 est passé.
        assert_eq!((ride_set(1, 20), ride_set(2, 20), ride_set(5, 20)), (9, 7, 2));
        assert_eq!((ride_set(0, 17), ride_set(4, 0), ride_set(6, 0)), (0, 0, -1));
        // Scion monté : tableau par classe (lance 8 pour le Slayer).
        let slayer = WearerInfo { race: 4, class: 1, type2: 1 };
        assert_eq!(anim_equip(slayer, true, true, lance, 0), 8);
        assert_eq!(anim_equip(slayer, true, true, Hands::default(), 0), 7);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : exemples réels de la spécification.
    #[test]
    fn objets_reels_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let look = |t| item_look(&assets, t).unwrap_or_else(|| panic!("objet {t}"));
        let near = |a: Vec3, b: Vec3| (a - b).length() < 0.02;
        // Short Axe (10) pour le chevalier 96 : main droite, matrice relevée.
        let axe = look(10);
        assert_eq!((axe.kind, axe.weapon_type, axe.part, axe.hand_node()), (2, 1, 10, 101));
        let m = axe.matrix(96, 0, true);
        assert!(near(m.x_axis.truncate(), Vec3::new(0.35, -0.17, 1.32)), "{m:?}");
        assert!(near(m.w_axis.truncate(), Vec3::new(6.22, -2.48, -2.04)));
        // Bastard Sword (2), Short Bow (1) pour l'archer 1, Wood Shield (8) pour 96.
        assert!(near(look(2).matrix(96, 0, true).w_axis.truncate(), Vec3::new(5.94, -5.94, -2.86)));
        let bow = look(1);
        assert_eq!((bow.weapon_type, bow.part, bow.hand_node()), (10, 9, 102));
        assert!(near(bow.matrix(1, 0, true).w_axis.truncate(), Vec3::new(7.80, -2.24, -1.63)));
        let shield = look(8);
        assert_eq!((shield.kind, shield.part, shield.hand_node()), (4, 9, 102));
        assert!(near(shield.matrix(96, 0, false).w_axis.truncate(), Vec3::new(-3.65, 14.87, 1.58)));
        // Rapière 4920 + dague 4922 pour 377 ; Karon 7820 doublé pour 1722 ; Xenon 7830 skinné.
        assert!(near(look(4920).matrix(377, 0, true).z_axis.truncate(), Vec3::new(-1.0, 0.0, 0.0)));
        assert!(look(4922).matrix(377, 0, true).abs_diff_eq(Mat4::IDENTITY, 0.01), "dague : identité");
        let karon = look(7820);
        assert!(karon.twohands);
        assert_eq!(karon.matrix(1722, 0, true), Mat4::IDENTITY);
        assert!(karon.matrix(1722, 1, true).y_axis.y < -0.9, "[1] : 180° autour de x");
        assert!(look(7830).skinned());
        // Lance 2696 (Part 15, main droite) et matrice pour 96.
        let lance = look(2696);
        assert_eq!((lance.weapon_type, lance.part, lance.hand_node()), (17, 15, 101));
        assert!(near(lance.matrix(96, 0, true).w_axis.truncate(), Vec3::new(4.77, -9.65, -2.11)));
        // Porteurs : chevalier, archer, Swashbuckler (déduit), Slayer.
        assert_eq!(wearer_info(&assets, 96), WearerInfo { race: 1, class: 1, type2: 1 });
        assert_eq!(wearer_info(&assets, 1).type2, 2);
        assert_eq!(wearer_info(&assets, 377), WearerInfo { race: 3, class: 1, type2: 1 });
        assert_eq!((wearer_info(&assets, 1722).race, wearer_info(&assets, 1722).class), (4, 1));
        assert_eq!(wearer_info(&assets, 567).type2, 0, "cheval blanc");
        // Squelette du chevalier : mains 101 à droite (x < 0), 102 à gauche.
        let skeleton = assets.clump("character", "a0000060.ecl").unwrap();
        let world = skeleton.world_matrices_rooted();
        let at = |id: u32| world[skeleton.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == id)).unwrap()].w_axis.truncate();
        assert!(near(at(101), Vec3::new(-54.6, 121.4, 3.8)) || (at(101) - Vec3::new(-54.6, 121.4, 3.8)).length() < 0.2, "{:?}", at(101));
        assert!(at(102).x > 50.0);
        // Xenon : peau à 80 os liée au squelette du porteur ; en pose de liaison, la pièce reste en
        // main droite (près de 101).
        let xenon = assets.clump("character", "d0001e96.ecl").unwrap();
        let geometry = xenon.atomics[0].geometry as usize;
        let pose: Vec<archrust_data::ean::HAnimSample> = Vec::new();
        let m = xenon.skin_matrices_on(geometry, &skeleton, &pose).unwrap();
        assert_eq!(m.len(), 80);
        let local = xenon.world_matrices_rooted()[xenon.atomics[0].frame as usize];
        let g = &xenon.geometries[geometry];
        let p = local.transform_point3(g.skin.as_ref().unwrap().deform_positions(&g.vertices, &m).unwrap()[0]);
        assert!((p - at(101)).length() < 60.0, "Xenon près de la main droite : {p:?}");
        assert!(summoner_items(&assets).contains(&15568));
    }
}
