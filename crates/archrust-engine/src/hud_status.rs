//! Caractéristiques de la fenêtre du personnage d'Archonia (`Character_Status`, touche C).
//!
//! * Caractéristiques : contrôles `CONTROL_Edit_CON`, `_WIS`, `_DEX`, `_STR`, `_INT`, `_CHAR` (sous les
//!   libellés STA, WIS, AGI, STR, INT, CHA), valeurs du facteur de statut reçu du serveur (stockées
//!   × 100 : 1300 pour 13).
//! * Onglets `Button_SelectProperty_1…4` (Attaque, Défense, Éléments, PvP ; `CS_Tab_A/B/C`) et lignes
//!   `Edit_ValueName_NN` / `Edit_Value_NN` : liste de `ini/characterpropertyview.xml`
//!   (`CharacterPropertyAttack`, `…Defense`, `…Attribute`, `…PVP`), nom tiré de `uimessage.txt`
//!   (`UiMsg_…`), format `%d` ou `%d%%`.
//! * Valeurs : Archonia les calcule par les fonctions `fn_CharacterProperty_*` (FUN_14051af80) à partir
//!   des facteurs « résultat » du personnage et d'AgpmCombat. ArchRust donne celles qui se lisent
//!   directement dans les facteurs reçus (dégâts min.–max., AR, DR, portée, défense et résistances par
//!   attribut, points de meurtrier et minuteries PvP) ; les autres lignes gardent leur nom sans valeur
//!   (formules d'AgpmCombat non relevées).

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

/// Valeur d'une ligne, quand elle se lit dans les facteurs reçus.
fn property_value(func: &str, st: &CharacterStats) -> Option<i64> {
    let f = |part: u8, i: u8| st.factors.get(&(part, i)).copied().map(i64::from);
    let name = func.strip_prefix("fn_CharacterProperty_")?;
    if let Some(e) = name.strip_suffix("Damage").and_then(element) {
        return f(fp::DAMAGE_MAX, e);
    }
    if let Some(e) = name.strip_suffix("Registance").and_then(element) {
        return f(fp::DEFENSE_POINT, e);
    }
    match name {
        "RateAttack" => f(fp::POINT_MAX, 8),
        "DefenseRate" => f(fp::POINT_MAX, 9),
        "AttackRange" => f(fp::ATTACK, 0),
        "PhysicalDefense" => f(fp::DEFENSE_POINT, 0),
        "HeroicDefense" => f(fp::DEFENSE_POINT, 11),
        "MurdererPoint" => st.murderer.map(i64::from),
        "RemainTimeAttacker" => st.criminal_left,
        "RemainTimeRemission" => st.murderer_left,
        _ => None,
    }
}

/// Texte de la valeur d'une ligne (les dégâts physiques : minimum – maximum).
fn property_text(p: &Property, st: &CharacterStats) -> Option<String> {
    if p.func == "fn_CharacterProperty_PhysicalDamage" {
        let get = |part: u8| st.factors.get(&(part, 0)).copied();
        return match (get(fp::DAMAGE_MIN), get(fp::DAMAGE_MAX)) {
            (Some(a), Some(b)) if a != b => Some(format!("{a} - {b}")),
            (_, Some(b)) => Some(b.to_string()),
            (Some(a), None) => Some(a.to_string()),
            _ => None,
        };
    }
    property_value(&p.func, st).map(|v| if p.percent { format!("{v}%") } else { v.to_string() })
}

/// Caractéristiques, onglets et lignes de propriétés, posés dans `Character_Status`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_status(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, w: &UiWindow, st: &CharacterStats, font: f32) {
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
        if let (Some(c), Some(text)) = (w.control(&p.value_ctrl), property_text(p, st)) {
            let r = layout.rect(origin, c.pos, c.size);
            shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, &text, font * 0.95, Color32::WHITE);
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
      <PropertyValue FormatType="Normal" Format="%d" Color="White" />
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
        let mut st = CharacterStats::default();
        st.factors.insert((fp::DAMAGE_MIN, 0), 12);
        st.factors.insert((fp::DAMAGE_MAX, 0), 18);
        st.factors.insert((fp::DEFENSE_POINT, 3), 7);
        assert_eq!(property_text(&tabs[0][0], &st).as_deref(), Some("12 - 18"));
        assert_eq!(property_text(&tabs[0][1], &st), None);
        assert_eq!(property_text(&tabs[2][0], &st).as_deref(), Some("7%"));
    }
}
