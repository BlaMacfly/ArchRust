//! Caractéristiques de la fenêtre du personnage d'Archonia (`Character_Status`, touche C).
//!
//! * Caractéristiques : contrôles `CONTROL_Edit_CON`, `_WIS`, `_DEX`, `_STR`, `_INT`, `_CHAR` (sous les
//!   libellés STA, WIS, AGI, STR, INT, CHA), valeurs du facteur de statut reçu du serveur (stockées
//!   × 100 : 1300 pour 13).
//! * Onglets `Button_SelectProperty_1…4` (Attaque, Défense, Éléments, PvP ; `CS_Tab_A/B/C`) et lignes
//!   `Edit_ValueName_NN` / `Edit_Value_NN` : liste de `ini/characterpropertyview.xml`
//!   (`CharacterPropertyAttack`, `…Defense`, `…Attribute`, `…PVP`), nom tiré de `uimessage.txt`
//!   (`UiMsg_…`), format `%d` ou `%d%%`.
//! * Valeurs : fonctions `fn_CharacterProperty_*` d'Archonia (table FUN_14051af80, une fonction par ligne),
//!   reproduites dans [`property_value`] à partir des facteurs « résultat » reçus (statut × 100 relu × 0,01,
//!   FUN_1401a54b0) et des formules d'AgpmCombat pour un personnage joueur (drapeau 1 d'`AgpdCharacter+0xb8`).
//!   Couleur (FUN_1407a6390) : `Color` si la valeur égale sa référence, `UpperColor` au-dessus, `LowerColor`
//!   au-dessous ; la référence (facteurs du modèle au niveau du personnage) n'est connue qu'en partie, les
//!   autres lignes restent en `Color`. Sans valeur : taux et dégâts critiques, résistance et défense ignorées,
//!   résistances aux critiques et à l'étourdissement, renvoi de dégâts (somme des options des objets portés,
//!   runes, conversions et prestige, FUN_14051aa90, plus une part des compétences, non relevée).

use super::*;
use archrust_netclient::world::factor_part as fp;
use archrust_netclient::world::CharacterStats;

/// Une ligne d'onglet de `characterpropertyview.xml`.
#[derive(Debug, Clone, PartialEq)]
struct Property {
    name_ctrl: String,
    value_ctrl: String,
    label_key: String,
    percent: bool,
    func: String,
    /// Couleurs `Color`, `UpperColor`, `LowerColor` de `PropertyValue` (noms de `color.ini`).
    colors: [String; 3],
}

/// Onglets, dans l'ordre des boutons.
const TAB_SECTIONS: [&str; 4] = ["CharacterPropertyAttack", "CharacterPropertyDefense", "CharacterPropertyAttribute", "CharacterPropertyPVP"];

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

/// Lignes de chaque onglet.
fn parse_tabs(xml: &str) -> Vec<Vec<Property>> {
    TAB_SECTIONS
        .iter()
        .map(|section| {
            let open = format!("<{section}>");
            let close = format!("</{section}>");
            let Some(body) = xml.find(&open).and_then(|a| xml[a..].find(&close).map(|b| &xml[a + open.len()..a + b])) else { return Vec::new() };
            body.split("<Property ")
                .skip(1)
                .filter_map(|p| {
                    let head = &p[..p.find('>')?];
                    let name = p.find("<PropertyName ").map(|i| &p[i..])?;
                    let value = p.find("<PropertyValue ").map(|i| &p[i..])?;
                    let call = p.find("<PropertyCallBack ").map(|i| &p[i..])?;
                    Some(Property {
                        name_ctrl: attr(head, "NameControl")?.to_string(),
                        value_ctrl: attr(head, "ValueControl")?.to_string(),
                        label_key: attr(name, "String")?.to_string(),
                        percent: attr(value, "Format").is_some_and(|f| f.contains("%%")),
                        func: attr(call, "Function")?.to_string(),
                        colors: ["Color", "UpperColor", "LowerColor"].map(|k| attr(&value[..value.find('>').unwrap_or(value.len())], k).unwrap_or("White").to_string()),
                    })
                })
                .collect()
        })
        .collect()
}

fn load_tabs(assets: &Assets) -> Vec<Vec<Property>> {
    let text = assets
        .read_file("ini/characterpropertyview.xml")
        .map(|raw| {
            let plain = raw.iter().take(256).filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t').count() * 20 < raw.len().min(256);
            if plain { String::from_utf8_lossy(&raw).into_owned() } else { archrust_data::ini::decrypt_ini(&raw) }
        })
        .unwrap_or_default();
    let tabs = parse_tabs(&text);
    log::info!("fenêtre du personnage : {} ligne(s) de propriétés", tabs.iter().map(Vec::len).sum::<usize>());
    tabs
}

/// Indice d'attribut des fonctions d'élément (AgpdFactorAttribute : 1 magie, 2 eau, 3 feu, 4 terre,
/// 5 air, 6 poison, 7 foudre, 8 glace, 11 héroïque).
fn element(name: &str) -> Option<u8> {
    Some(match name {
        "Magic" => 1,
        "Water" => 2,
        "Fire" => 3,
        "Earth" => 4,
        "Air" => 5,
        "Poison" => 6,
        "Thunder" => 7,
        "Ice" => 8,
        "Heroic" => 11,
        _ => return None,
    })
}

/// Données du modèle du personnage (`CharacterTemplatePublic.ini`) et de son arme utiles aux lignes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Basis {
    /// Vitesse de course du modèle (`MovementFast`, statut 9).
    pub run_speed: Option<i32>,
    /// Vitesse d'attaque du modèle (`Speed`, attaque 2) et de l'arme portée (`Speed` d'ItemTemplateAll).
    pub attack_speed: Option<i32>,
    pub weapon_speed: Option<i32>,
    /// Classe du modèle (`Class`), si le facteur de type manque.
    pub class: Option<i32>,
}

/// Colonne numérique `name` du modèle `tid` d'une table positionnelle.
fn template_number(assets: &Assets, file: &str, tid: i32, name: &str) -> Option<i32> {
    let t = assets.table("ini", file).ok()?;
    let col = t.section("")?.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(name)).map(|(k, _)| k.clone())?;
    t.section(&tid.to_string())?.get(&col)?.trim().parse::<f64>().ok().map(|v| v as i32)
}

impl Basis {
    pub(super) fn load(assets: &Assets, tid: Option<i32>, weapon_tid: Option<i32>) -> Self {
        let tpl = |name: &str| tid.and_then(|t| template_number(assets, "CharacterTemplatePublic.ini", t, name));
        Basis {
            run_speed: tpl("MovementFast").filter(|v| *v > 0),
            attack_speed: tpl("Speed").filter(|v| *v > 0),
            weapon_speed: weapon_tid.and_then(|t| template_number(assets, "ItemTemplateAll.ini", t, "Speed")).filter(|v| *v > 0),
            class: tpl("Class"),
        }
    }
}

/// Couleur d'une valeur (`Color`, `UpperColor`, `LowerColor` de la ligne, ou nom de `color.ini`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Normal,
    Upper,
    Lower,
    Named(&'static str),
}

/// Comparaison de FUN_1407a6390 : référence égale, plus basse ou plus haute que la valeur.
fn compare(base: i64, value: i64) -> Tone {
    match base.cmp(&value) {
        std::cmp::Ordering::Less => Tone::Upper,
        std::cmp::Ordering::Greater => Tone::Lower,
        std::cmp::Ordering::Equal => Tone::Normal,
    }
}

/// Valeur positive en `UpperColor`, sinon `Color` (ténacités, dégâts dans la durée, PvP, minuteries).
fn positive(v: i64) -> (i64, Tone) {
    (v, if v > 0 { Tone::Upper } else { Tone::Normal })
}

/// Couleur des points de meurtrier selon leur palier (FUN_1401793e0 : 40, 60, 100).
fn murderer_tone(points: i32) -> Tone {
    match points {
        p if p >= 100 => Tone::Named("Red"),
        p if p >= 60 => Tone::Named("Yellow"),
        _ => Tone::Named("White"),
    }
}

/// Constantes flottantes d'AgpmCombat (valeurs du binaire).
mod k {
    /// 0x140e49c68.
    pub const SIXTY: f32 = 0.6;
    /// 0x140e49c20.
    pub const FORTY: f32 = 0.4;
    /// 0x140e4a0bc.
    pub const AR_BASE: f32 = 12.0;
    /// 0x140e4a0b0.
    pub const DR_BASE: f32 = 8.0;
    /// 0x140e49bf8, 0x140e49be4, 0x140e49c10.
    pub const DEX_RANGER: f32 = 2.0 / 9.0;
    pub const STR_RANGER: f32 = 1.0 / 9.0;
    pub const STR_MELEE: f32 = 1.0 / 3.0;
    /// 0x140e49d48.
    pub const MAX_RATIO: f32 = 1.05;
}

/// Valeur d'une ligne et sa couleur (voir l'en-tête du module) ; `None` si Archonia la tire de données
/// qu'ArchRust n'a pas.
fn property_value(func: &str, st: &CharacterStats, basis: &Basis) -> Option<(i64, Tone)> {
    use archrust_netclient::world::character_property_op as cp;
    let f = |part: u8, i: u8| st.factors.get(&(part, i)).copied();
    // Statut relu × 0,01 (FUN_1401a54b0, indices 0 à 6).
    let stat = |i: u8| f(fp::STATUS, i).unwrap_or(0) as f32 * 0.01;
    let level = f(fp::STATUS, 7).unwrap_or(0) as f32;
    let prop = |op: i8, i: u8| st.properties.get(&(op, i)).copied().map(i64::from);
    let plain = |v: Option<i32>| v.map(|v| (i64::from(v), Tone::Normal));
    let name = func.strip_prefix("fn_CharacterProperty_")?;
    // Dégâts élémentaires : maximum de l'attribut (FUN_1402978a0 sans cible) ; résistances : points de défense.
    if let Some(e) = name.strip_suffix("Damage").and_then(element) {
        return plain(f(fp::DAMAGE_MAX, e));
    }
    if let Some(e) = name.strip_suffix("Registance").and_then(element) {
        return plain(f(fp::DEFENSE_POINT, e));
    }
    Some(match name {
        // FUN_140296740 (joueur, sans cible, maximum) : part de la classe × 1,05 + dégâts maximum physiques.
        "PhysicalDamage" => {
            let class = f(fp::TYPE, 2).or(basis.class).unwrap_or(1);
            let part = match class {
                2 => stat(3) * k::DEX_RANGER + stat(1) * k::STR_RANGER,
                4 => stat(2) * k::FORTY,
                _ => stat(1) * k::STR_MELEE,
            };
            ((part * k::MAX_RATIO + f(fp::DAMAGE_MAX, 0).unwrap_or(0) as f32) as i64, Tone::Normal)
        }
        // AR (FUN_140294e70) : AR des facteurs + niveau × 0,6 + DEX × 0,6 + 12.
        "RateAttack" => ((f(fp::POINT_MAX, 8).unwrap_or(0) as f32 + level * k::SIXTY + stat(3) * k::SIXTY + k::AR_BASE) as i64, Tone::Normal),
        // DR (FUN_140295130) : (niveau + DEX) × 0,4 + 8 + DR des facteurs.
        "DefenseRate" => (((level + stat(3)) * k::FORTY + k::DR_BASE + f(fp::POINT_MAX, 9).unwrap_or(0) as f32) as i64, Tone::Normal),
        // Facteur d'attaque : 0 portée, 3 incantation, 4 recharge, 5 précision, 6 évasion, 7 esquive.
        "AttackRange" => plain(f(fp::ATTACK, 0))?,
        "DurationCastSkill" => plain(f(fp::ATTACK, 3))?,
        "DurationSkillCoolTime" => plain(f(fp::ATTACK, 4))?,
        "RateHit" => plain(f(fp::ATTACK, 5))?,
        "EvadeRate" => plain(f(fp::ATTACK, 6))?,
        "DodgeRate" => plain(f(fp::ATTACK, 7))?,
        // Défense : points (0 physique, 11 héroïque) et taux (0 physique, 9 blocage, 10 blocage de compétence,
        // 12 à 14 héroïques au corps à corps, à distance, magiques).
        "PhysicalDefense" => plain(f(fp::DEFENSE_POINT, 0))?,
        "HeroicDefense" => plain(f(fp::DEFENSE_POINT, 11))?,
        "PhysicalRegistance" => plain(f(fp::DEFENSE_RATE, 0))?,
        "BlockRate" => plain(f(fp::DEFENSE_RATE, 9))?,
        "SkillBlockRate" => plain(f(fp::DEFENSE_RATE, 10))?,
        "HeroicMeleeRegistance" => plain(f(fp::DEFENSE_RATE, 12))?,
        "HeroicMissileRegistance" => plain(f(fp::DEFENSE_RATE, 13))?,
        "HeroicMagicRegistance" => plain(f(fp::DEFENSE_RATE, 14))?,
        "HeroicDamage" => plain(f(fp::DAMAGE_MAX, 11))?,
        // Vitesse de course rapportée à celle du modèle (référence 100).
        "SpeedMove" => {
            let v = (f64::from(f(fp::STATUS, 9)?) * 100.0 / f64::from(basis.run_speed?)) as i64;
            (v, compare(100, v))
        }
        // FUN_1407acf90 : vitesse d'attaque × 100 / celle de l'arme portée (sinon du modèle), arrondi d'Archonia.
        "SpeedAttack" => {
            let speed = f(fp::ATTACK, 2)? as f32;
            let base = basis.weapon_speed.or(basis.attack_speed)? as f32;
            let v = speed * 100.0 / base;
            let mut r = v + 0.4;
            if r - (r as i32) as f32 >= 0.5 {
                r = v + 1.4;
            }
            let v = r as i64;
            (v, compare(100, v))
        }
        // Opérations 34 à 36 d'AGPMCHARACTER (part du personnage ; Archonia ajoute à l'esquive à distance ignorée
        // une part des compétences, non relevée).
        "IgnoreTargetPhysicalRes" => (prop(cp::IGNORE, 0)?, Tone::Normal),
        "IgnoreTargetBlock" => (prop(cp::IGNORE, 1)?, Tone::Normal),
        "IgnoreTargetSkillBlock" => (prop(cp::IGNORE, 2)?, Tone::Normal),
        "IgnoreTargetMeleeDodge" => (prop(cp::IGNORE, 3)?, Tone::Normal),
        "IgnoreTargetRangeDodge" => (prop(cp::IGNORE, 4)?, Tone::Normal),
        "IgnoreTargetCriticalRes" => (prop(cp::IGNORE, 5)?, Tone::Normal),
        "IgnoreTargetStunRes" => (prop(cp::IGNORE, 6)?, Tone::Normal),
        "MovementTenacity" => positive(prop(cp::TENACITY, 0)?),
        "AttackTenacity" => positive(prop(cp::TENACITY, 1)?),
        "CastTenacity" => positive(prop(cp::TENACITY, 2)?),
        "CooldownTenacity" => positive(prop(cp::TENACITY, 3)?),
        "DotIncrease" => positive(prop(cp::DOT, 0)?),
        "DotDecrease" => positive(prop(cp::DOT, 1)?),
        "HeroicDotIncrease" => positive(prop(cp::DOT, 2)?),
        "HeroicDotDecrease" => positive(prop(cp::DOT, 3)?),
        // PvP : points et palier de meurtrier, minuteries en minutes entamées (secondes / 60 + 0,9999).
        "MurdererPoint" => {
            let p = st.murderer?;
            (i64::from(p), if p > 0 { murderer_tone(p) } else { Tone::Normal })
        }
        "MurdererLevel" => {
            let p = st.murderer?;
            let level = [40, 60, 100].iter().filter(|l| p >= **l).count() as i64;
            (level, if level > 0 { murderer_tone(p) } else { Tone::Normal })
        }
        "RemainTimeAttacker" => positive((st.criminal_left? as f64 / 60.0 + 0.9999) as i64),
        "RemainTimeRemission" => positive((st.murderer_left? as f64 / 60.0 + 0.9999) as i64),
        "PVPTotalWin" => positive(st.pvp?.0),
        "PVPTotalLose" => positive(st.pvp?.1),
        _ => return None,
    })
}

/// Texte et couleur de la valeur d'une ligne.
fn property_text(p: &Property, st: &CharacterStats, basis: &Basis) -> Option<(String, Tone)> {
    property_value(&p.func, st, basis).map(|(v, tone)| (if p.percent { format!("{v}%") } else { v.to_string() }, tone))
}

/// Couleur `RRGGBB` d'un nom de `color.ini` (blanc sinon).
fn named_color(assets: &Assets, name: &str) -> Color32 {
    let rgb = assets
        .table("ini", "color.ini")
        .ok()
        .and_then(|t| t.sections.iter().find_map(|s| s.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())))
        .and_then(|v| u32::from_str_radix(v.trim(), 16).ok())
        .unwrap_or(0xff_ffff);
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Caractéristiques, onglets et lignes de propriétés, posés dans `Character_Status`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_status(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, w: &UiWindow, st: &CharacterStats, basis: &Basis, font: f32) {
    // Caractéristiques (CON 0, STR 1, INT 2, DEX 3, CHA 4, WIS 6 du facteur de statut).
    for (ctrl, index) in [("CONTROL_Edit_CON", 0u8), ("CONTROL_Edit_WIS", 6), ("CONTROL_Edit_DEX", 3), ("CONTROL_Edit_STR", 1), ("CONTROL_Edit_INT", 2), ("CONTROL_Edit_CHAR", 4)] {
        let (Some(c), Some(v)) = (w.control(ctrl), st.factors.get(&(fp::STATUS, index))) else { continue };
        let r = layout.rect(origin, c.pos, c.size);
        shadow_text(painter, Pos2::new(r.left(), r.top() + 6.5 * layout.scale), Align2::LEFT_CENTER, &(v / 100).to_string(), font, Color32::WHITE);
    }
    // Onglets.
    let tab_id = egui::Id::new("archonia-character-property-tab");
    let mut tab: usize = ctx.data(|d| d.get_temp(tab_id)).unwrap_or(0);
    for k in 0..TAB_SECTIONS.len() {
        let Some(c) = w.control(&format!("Button_SelectProperty_{}", k + 1)) else { continue };
        let r = layout.rect(origin, c.pos, c.size);
        let resp = ui.interact(r, egui::Id::new(("archonia-character-property", k)), egui::Sense::click());
        if resp.clicked() {
            tab = k;
            ctx.data_mut(|d| d.insert_temp(tab_id, k));
        }
        // `Button=2:3:4:1` : repos A, survol B, appui (onglet choisi) C.
        let index = if tab == k { 4 } else if resp.hovered() { 3 } else { 2 };
        if let Some(t) = c.textures.iter().find(|(i, _)| *i == index).map(|(_, n)| n.clone()) {
            control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
        }
        let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
        let color = if tab == k { Color32::from_rgb(255, 230, 160) } else { Color32::from_rgb(220, 220, 220) };
        shadow_text(painter, r.center(), Align2::CENTER_CENTER, label, font, color);
    }
    // Lignes de l'onglet choisi.
    let tabs_id = egui::Id::new("archonia-character-property-rows");
    let tabs: Arc<Vec<Vec<Property>>> = match ctx.data(|d| d.get_temp::<Arc<Vec<Vec<Property>>>>(tabs_id)) {
        Some(t) => t,
        None => {
            let t = Arc::new(load_tabs(assets));
            ctx.data_mut(|d| d.insert_temp(tabs_id, t.clone()));
            t
        }
    };
    let Some(rows) = tabs.get(tab) else { return };
    for p in rows {
        if let Some(c) = w.control(&p.name_ctrl) {
            let r = layout.rect(origin, c.pos, c.size);
            let label = super::ui_message(hud, assets, &p.label_key);
            shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, &label, font * 0.95, Color32::from_rgb(255, 240, 200));
        }
        if let (Some(c), Some((text, tone))) = (w.control(&p.value_ctrl), property_text(p, st, basis)) {
            let r = layout.rect(origin, c.pos, c.size);
            let name = match tone {
                Tone::Normal => p.colors[0].as_str(),
                Tone::Upper => p.colors[1].as_str(),
                Tone::Lower => p.colors[2].as_str(),
                Tone::Named(n) => n,
            };
            shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, &text, font * 0.95, named_color(assets, name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<CharacterPropertyView>
  <CharacterPropertyAttack>
    <Property Type="x" NameControl="Edit_ValueName_01" ValueControl="Edit_Value_01">
      <PropertyName String="UiMsg_PhysicalDamage" Color="Gold" />
      <PropertyValue FormatType="Normal" Format="%d" Color="White" UpperColor="Green" LowerColor="Red" />
      <PropertyCallBack Function="fn_CharacterProperty_PhysicalDamage" Script="" />
    </Property>
    <Property Type="y" NameControl="Edit_ValueName_04" ValueControl="Edit_Value_04">
      <PropertyName String="UiMsg_RateHit" Color="Gold" />
      <PropertyValue FormatType="Normal" Format="%d%%" Color="White" />
      <PropertyCallBack Function="fn_CharacterProperty_RateHit" Script="" />
    </Property>
  </CharacterPropertyAttack>
  <CharacterPropertyAttribute>
    <Property Type="z" NameControl="Edit_ValueName_04" ValueControl="Edit_Value_04">
      <PropertyName String="UiMsg_FireRegistance" Color="Gold" />
      <PropertyValue FormatType="Normal" Format="%d%%" Color="White" />
      <PropertyCallBack Function="fn_CharacterProperty_FireRegistance" Script="" />
    </Property>
  </CharacterPropertyAttribute>
</CharacterPropertyView>"#;

    #[test]
    fn onglets_et_valeurs() {
        let tabs = parse_tabs(XML);
        assert_eq!(tabs.len(), 4);
        assert_eq!(tabs[0].len(), 2);
        assert_eq!((tabs[0][1].value_ctrl.as_str(), tabs[0][1].percent), ("Edit_Value_04", true));
        assert_eq!(tabs[2][0].label_key, "UiMsg_FireRegistance");
        assert_eq!(tabs[0][0].colors, ["White".to_string(), "Green".to_string(), "Red".to_string()]);
        let mut st = CharacterStats::default();
        st.factors.insert((fp::DAMAGE_MIN, 0), 12);
        st.factors.insert((fp::DAMAGE_MAX, 0), 18);
        st.factors.insert((fp::DEFENSE_POINT, 3), 7);
        // Chevalier, FOR 14 (1400) : 14 / 3 × 1,05 + 18 = 22,9.
        st.factors.insert((fp::TYPE, 2), 1);
        st.factors.insert((fp::STATUS, 1), 1400);
        let basis = Basis::default();
        assert_eq!(property_text(&tabs[0][0], &st, &basis), Some(("22".to_string(), Tone::Normal)));
        assert_eq!(property_text(&tabs[0][1], &st, &basis), None);
        st.factors.insert((fp::ATTACK, 5), 81);
        assert_eq!(property_text(&tabs[0][1], &st, &basis), Some(("81%".to_string(), Tone::Normal)));
        assert_eq!(property_text(&tabs[2][0], &st, &basis), Some(("7%".to_string(), Tone::Normal)));
    }

    #[test]
    fn formules_d_agpmcombat() {
        let mut st = CharacterStats::default();
        let basis = Basis { run_speed: Some(5000), attack_speed: Some(75), weapon_speed: None, class: Some(2) };
        let v = |f: &str, st: &CharacterStats| property_value(&format!("fn_CharacterProperty_{f}"), st, &basis);
        // Archer niveau 10, FOR 10, DEX 20 : (20 × 2/9 + 10/9) × 1,05 + 30 = 35,83.
        for (i, x) in [(1u8, 1000), (3, 2000), (7, 10), (9, 6000)] {
            st.factors.insert((fp::STATUS, i), x);
        }
        st.factors.insert((fp::DAMAGE_MAX, 0), 30);
        assert_eq!(v("PhysicalDamage", &st), Some((35, Tone::Normal)));
        // AR : 5 + 10 × 0,6 + 20 × 0,6 + 12 = 35 ; DR : (10 + 20) × 0,4 + 8 + 3 = 23.
        st.factors.insert((fp::POINT_MAX, 8), 5);
        st.factors.insert((fp::POINT_MAX, 9), 3);
        assert_eq!(v("RateAttack", &st), Some((35, Tone::Normal)));
        assert_eq!(v("DefenseRate", &st), Some((23, Tone::Normal)));
        // Course 6000 pour 5000 : 120 %, au-dessus de la référence.
        assert_eq!(v("SpeedMove", &st), Some((120, Tone::Upper)));
        // Vitesse d'attaque 60 pour 75 : 80 (80,4 arrondi par défaut) ; 76 pour 75 : 101,33 + 0,4 = 101,73, partie décimale ≥ 0,5 donc 101,33 + 1,4 → 102 (sic).
        st.factors.insert((fp::ATTACK, 2), 60);
        assert_eq!(v("SpeedAttack", &st), Some((80, Tone::Lower)));
        st.factors.insert((fp::ATTACK, 2), 76);
        assert_eq!(v("SpeedAttack", &st), Some((102, Tone::Upper)));
        // Défense héroïque à distance : taux 13 ; minuterie : 61 s = 2 minutes entamées.
        st.factors.insert((fp::DEFENSE_RATE, 13), 4);
        assert_eq!(v("HeroicMissileRegistance", &st), Some((4, Tone::Normal)));
        st.criminal_left = Some(61);
        assert_eq!(v("RemainTimeAttacker", &st), Some((2, Tone::Upper)));
        st.murderer = Some(65);
        assert_eq!(v("MurdererLevel", &st), Some((2, Tone::Named("Yellow"))));
        st.properties.insert((archrust_netclient::world::character_property_op::TENACITY, 2), 15);
        assert_eq!(v("CastTenacity", &st), Some((15, Tone::Upper)));
        assert_eq!(v("RateCritical", &st), None);
    }
}
