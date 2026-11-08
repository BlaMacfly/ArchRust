//! Ailes et capes des runes serties dans les colliers : ExtraType → ID commun → race/classe.

use super::{effects_glue::Placed, App};
use crate::assets::Assets;
use glam::Vec3;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
struct Item {
    item_type: i64,
    kind: i64,
    usable: i64,
    subtype: i64,
    extra: i64,
}

impl Item {
    fn visual_rune(self) -> bool {
        self.item_type == 1 && self.usable == 9 && matches!(self.subtype, 22 | 23 | 33)
    }
}

#[derive(Default)]
struct Common {
    ids: HashMap<i64, (usize, i64)>,
    variants: HashMap<(usize, i64, i64, i64), Placed>,
}

impl Common {
    fn parse(text: &str) -> Self {
        let mut lines = text.lines();
        let header: Vec<_> = lines.next().unwrap_or_default().split('\t').collect();
        let mut out = Self::default();
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            let get = |name: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(name)).and_then(|i| fields.get(i)).copied().unwrap_or("").trim();
            let num = |name| get(name).parse::<i64>().unwrap_or(0);
            let Some((kind, group)) = ["WING_ID", "CLOAK_ID"].iter().enumerate().find_map(|(k, n)| (num(n) > 0).then_some((k, num(n)))) else { continue };
            let (id, eid) = (num("ID"), num("EID"));
            if id <= 0 || eid <= 0 { continue }
            out.ids.insert(id, (kind, group));
            let float = |n| get(n).parse::<f32>().unwrap_or(0.0);
            let scale = float("SCALE");
            let placed = Placed { eid: eid as u32, offset: Vec3::new(float("OFFSET_X"), float("OFFSET_Y"), float("OFFSET_Z")), scale: if scale > 0.0 { scale } else { 1.0 }, node: num("NODE_ID") as i32, delay: num("GAP").clamp(0, 60_000) as u32 };
            // Les listes de races et de classes sont appariées ; une classe contient des « : ».
            for (race, classes) in get("RACE").split(',').zip(get("CLASS").split(',')) {
                let Ok(race) = race.parse::<i64>() else { continue };
                for class in classes.split(':').filter_map(|c| c.parse::<i64>().ok()) {
                    out.variants.insert((kind, group, race, class), placed);
                }
            }
        }
        out
    }

    fn resolve(&self, id: i64, race: i64, class: i64) -> Option<Placed> {
        let &(kind, group) = self.ids.get(&id)?;
        self.variants.get(&(kind, group, race, class)).copied()
    }
}

fn load_items(assets: &Assets) -> HashMap<i32, Item> {
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    let columns: Vec<_> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    lines.filter_map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        let num = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name)).and_then(|i| fields.get(i)).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
        let tid = i32::try_from(num("TID")).ok().filter(|t| *t > 0)?;
        Some((tid, Item { item_type: num("ItemType"), kind: num("Kind"), usable: num("Type"), subtype: num("SubType"), extra: num("ExtraType") }))
    }).collect()
}

#[derive(Default)]
pub(super) struct NecklaceGlue {
    items: Option<HashMap<i32, Item>>,
    common: Option<Common>,
    current: HashMap<i32, Vec<Placed>>,
    last: Option<Instant>,
}

impl App {
    pub(super) fn necklace_frame(&mut self) {
        if self.necklace.last.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) { return }
        self.necklace.last = Some(Instant::now());
        let items = self.necklace.items.get_or_insert_with(|| load_items(&self.assets));
        let common = self.necklace.common.get_or_insert_with(|| Common::parse(&super::npc_glue::client_text_in(&self.assets, "ini", "ee_common_char.txt")));
        let mut wanted = HashMap::new();
        for entity in self.player.iter().chain(self.entities.iter()) {
            let who = super::convert_glue::Wearer::of(&self.assets, entity.tid, 0);
            let mut effects = Vec::new();
            for equip in &entity.equipment {
                if !items.get(&equip.template_id).is_some_and(|i| i.item_type == 0 && i.kind == 16) { continue }
                let Some(state) = self.convert.states.get(&equip.item_id) else { continue };
                for tid in &state.filled {
                    let Some(rune) = items.get(tid).filter(|i| i.visual_rune()) else { continue };
                    if let Some(effect) = common.resolve(rune.extra, who.race, who.class) {
                        // Les exclusions entre jeux restent celles d'ExcEffTable ; l'ordre des
                        // sockets ne doit pas supprimer arbitrairement une autre famille visuelle.
                        if !effects.contains(&effect) { effects.push(effect); }
                    }
                }
            }
            wanted.insert(entity.cid, effects);
        }
        let gone: Vec<_> = self.necklace.current.keys().filter(|cid| !wanted.contains_key(*cid)).copied().collect();
        for cid in gone {
            self.effects_set_necklace(cid, &[]);
            self.necklace.current.remove(&cid);
        }
        for (cid, placed) in wanted {
            if self.necklace.current.get(&cid) != Some(&placed) || (!placed.is_empty() && !self.effects_has_necklace(cid)) {
                self.effects_set_necklace(cid, &placed);
                self.necklace.current.insert(cid, placed);
            }
        }
        self.necklace_tooltips();
    }

    /// Une rune visuelle sans OptionTID doit rester identifiable dans l'infobulle du collier.
    fn necklace_tooltips(&mut self) {
        let Some(data) = self.necklace.items.as_ref() else { return };
        let color = self.assets.table("ini", "color.ini").ok().and_then(|t| {
            let value = t.section("ItemTooltip")?.get("ITEM_TOOLTIP_COLOR_RUNE")?;
            u32::from_str_radix(value.trim(), 16).ok()
        }).map(|rgb| egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)).unwrap_or(egui::Color32::WHITE);
        for item in self.items.values() {
            if !item.template_id.and_then(|tid| data.get(&tid)).is_some_and(|i| i.item_type == 0 && i.kind == 16) { continue }
            let Some(state) = self.convert.states.get(&item.id) else { continue };
            let Some(tip) = self.hud.item_tips.get_mut(&item.id) else { continue };
            for tid in &state.filled {
                if !data.get(tid).is_some_and(|i| i.visual_rune()) { continue }
                let Some(name) = crate::scene::item_name_for_tid(&self.assets, *tid).filter(|n| !n.is_empty()) else { continue };
                if !tip.iter().any(|(text, _)| text == &name) {
                    tip.push((name, color));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groupes_et_variantes_de_collier() {
        let table = Common::parse("ID\tEID\tWING_ID\tCLOAK_ID\tRACE\tCLASS\tSCALE\tNODE_ID\n40\t3569\t2\t\t1,2\t1:2:,2:,\t0.34\t13\n41\t3569\t2\t\t2\t4:,\t0.41\t13\n502\t3600\t\t1\t1\t1:,\t0\t12\n249\t9000\t9\t\t\t\t1\t13\n");
        assert_eq!(table.resolve(40, 2, 4).unwrap().scale, 0.41);
        assert_eq!(table.resolve(41, 1, 2).unwrap().scale, 0.34);
        assert_eq!(table.resolve(502, 1, 1).unwrap().scale, 1.0);
        assert!(table.resolve(249, 1, 1).is_none());
        assert!(table.resolve(40, 4, 1).is_none());
    }

    #[test]
    fn objets_on_off_distincts_des_runes() {
        assert!(!Item { item_type: 1, usable: 12, subtype: 22, extra: 40, ..Default::default() }.visual_rune());
        for subtype in [22, 23, 33] {
            assert!(Item { item_type: 1, usable: 9, subtype, ..Default::default() }.visual_rune());
        }
        assert!(!Item { item_type: 1, usable: 9, subtype: 44, ..Default::default() }.visual_rune());
    }

    #[test]
    fn necklace_tables_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let items = load_items(&assets);
        assert!(items.values().any(|i| i.visual_rune() && i.extra == 40));
        assert!(items.values().any(|i| i.item_type == 0 && i.kind == 16));
        let common = Common::parse(&super::super::npc_glue::client_text_in(&assets, "ini", "ee_common_char.txt"));
        let p = common.resolve(40, 2, 4).expect("ailes d'ange, sorcier orc");
        assert_eq!(p.eid, 3569);
        assert_eq!(p.node, 13);
        assert_eq!(p.scale, 0.41);
    }
}
