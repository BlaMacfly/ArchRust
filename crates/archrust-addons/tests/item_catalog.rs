//! Catalogue des objets vu des AddOns : `ArchRust.GetItemTemplate`, `ArchRust.SearchItems`,
//! `ArchRust.GetNumItemTemplates`, `GetItemIcon`, `GetItemInfo` (inventaire puis catalogue).

use archrust_addons::{AddonManager, ItemInfo, ItemTemplateInfo};
use std::sync::Arc;

fn template(tid: i32, name: &str, item_type: i64, kind: i64, level: i64, icon: &str) -> ItemTemplateInfo {
    ItemTemplateInfo { tid, name: name.into(), icon: icon.into(), level, item_type, kind, part: 0, stack: if item_type == 1 { 99 } else { 1 } }
}

#[test]
fn catalogue_des_objets() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = AddonManager::new(tmp.path().join("sv")).unwrap();
    let eval = |src: &str| mgr.eval_string(src).unwrap_or_else(|e| panic!("{src} : {e}"));
    // Sans catalogue (moteur ancien ou hors jeu) : rien, sans erreur.
    assert_eq!(eval("return tostring(ArchRust.GetItemTemplate(190))"), "nil");
    assert_eq!(eval("return ArchRust.GetNumItemTemplates()"), "0");
    assert_eq!(eval("return tostring(GetItemIcon(190))"), "nil");

    let list = vec![
        template(1, "Short Bow", 0, 2, 1, "l00006ba"),
        template(190, "Small Life Potion", 1, 0, 0, "l0000040"),
        template(530, "Lionheart Shield", 0, 4, 55, "l00002f1"),
        template(531, "Steelhorn Shield", 0, 4, 63, ""),
        template(900, "Blood Ring", 0, 8, 30, "l0000100"),
        template(1900, "Box Shard", 2, 0, 0, "l0000200"),
    ];
    mgr.set_host(|h| {
        h.item_templates = Arc::new(list.clone());
        h.inventory = vec![ItemInfo { id: 5, tid: Some(190), name: "Potion (sac)".into(), count: Some(3), status: Some(2), grid: Some([0, 0, 0]) }];
    });
    assert_eq!(eval("return ArchRust.GetNumItemTemplates()"), "6");
    let t = "local t = ArchRust.GetItemTemplate(530) return t.name .. '|' .. t.level .. '|' .. t.category .. '|' .. t.icon .. '|' .. t.kind .. '|' .. t.stack";
    assert_eq!(eval(t), "Lionheart Shield|55|armor|l00002f1|4|1");
    assert_eq!(eval("return tostring(ArchRust.GetItemTemplate(531).icon)"), "nil", "pas d'icône");
    assert_eq!(eval("return tostring(ArchRust.GetItemTemplate(2))"), "nil");
    assert_eq!(eval("return ArchRust.GetItemTemplate(190).category .. ArchRust.GetItemTemplate(190).stack"), "usable99");

    // Recherche : nom sans tenir compte de la casse, TID numérique, catégorie, limite et total.
    let names = |q: &str| eval(&format!("local r, n = {q} local s = {{}} for _, t in ipairs(r) do s[#s + 1] = t.tid end return table.concat(s, ',') .. '/' .. n"));
    assert_eq!(names("ArchRust.SearchItems('shield')"), "530,531/2");
    assert_eq!(names("ArchRust.SearchItems('SHIELD', 'armor', 1)"), "530/2");
    assert_eq!(names("ArchRust.SearchItems('19')"), "190,1900/2");
    assert_eq!(names("ArchRust.SearchItems('', 'jewel')"), "900/1");
    assert_eq!(names("ArchRust.SearchItems(nil, 'all')"), "1,190,530,531,900,1900/6");
    assert_eq!(names("ArchRust.SearchItems('', 'weapon')"), "1/1");
    assert_eq!(names("ArchRust.SearchItems('', 'other')"), "1900/1");
    assert_eq!(names("ArchRust.SearchItems('dragon')"), "/0");

    // Icônes et GetItemInfo (ordre de WoW) : l'objet du sac garde son nom, le catalogue complète.
    assert_eq!(eval("return GetItemIcon(530)"), "l00002f1");
    assert_eq!(eval("return GetItemIcon('900')"), "l0000100");
    assert_eq!(eval("return tostring(GetItemIcon(531))"), "nil");
    assert_eq!(eval("local n, _, _, l, m, c, _, s, _, i = GetItemInfo(190) return n .. '|' .. l .. '|' .. c .. '|' .. s .. '|' .. i"), "Potion (sac)|0|usable|99|l0000040");
    assert_eq!(eval("local n, _, _, l, _, c = GetItemInfo(530) return n .. '|' .. l .. '|' .. c"), "Lionheart Shield|55|armor");
    assert_eq!(eval("return (GetItemInfo('blood ring'))"), "Blood Ring");
    assert_eq!(eval("return tostring(GetItemInfo(424242))"), "nil");
}
