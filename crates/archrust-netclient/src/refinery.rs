//! Raffinage à la sphère d'alchimie : types 0x3F (AgpmEventRefinery) et 0x4B (AgpmRefinery).
//!
//! Dialogue validé avec ArchCore (scénario `sim refinery`, wiki « Raffinage ») :
//!
//! ```text
//! 0x3F op 0 (1 EVENT_BASE : objet du monde, fonction 26 ; 2 CID) → op 1 : sphère ouverte
//! 0x4B op 5 (1 CID, 3 IID des 20 cases)          → op 5, prix au champ 6 (devis)
//! 0x4B op 2 (1 CID, 2 = 0, 3 IID des 20 cases)   → op 1 (raffinage de la grille)
//! 0x4B op 6 (1 CID, 2 identifiant de recette)    → op 1 (raffinage d'une recette)
//! op 1 : champ 4 = 0 réussi (5 TID produit), 1 ou 3 ingrédients manquants, 2 objets sans recette,
//!        4 or insuffisant, 5 inventaire plein
//! ```

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, Schema};

/// Fonction d'événement de la sphère d'alchimie (objets du monde, `Function=26` des `obj*.ini`).
pub const FUNCTION_REFINERY: i32 = 26;
/// Cases de la grille (4 lignes × 5 colonnes).
pub const GRID_CELLS: usize = 20;
/// Type d'objet « objet du monde » d'une désignation d'événement.
const BASE_TYPE_OBJECT: i8 = 1;

/// Opérations d'AGPMREFINERY.
pub mod op {
    pub const RESULT: i8 = 1;
    pub const REFINE: i8 = 2;
    pub const QUOTE: i8 = 5;
    pub const REFINE_RECIPE: i8 = 6;
}

/// Résultats (op 1).
pub mod result {
    pub const SUCCESS: i32 = 0;
    pub const MISSING_ITEMS: i32 = 1;
    pub const NO_RECIPE: i32 = 2;
    pub const MISSING_ITEMS_2: i32 = 3;
    pub const NO_MONEY: i32 = 4;
    pub const FULL_INVENTORY: i32 = 5;
}

/// Demandes de raffinage.
#[derive(Debug, Clone, PartialEq)]
pub enum RefineryCommand {
    /// Ouvrir la sphère portée par l'objet du monde `object`.
    Open(u32),
    /// Prix des objets de la grille.
    Quote([i32; GRID_CELLS]),
    /// Raffiner les objets de la grille.
    Refine([i32; GRID_CELLS]),
    /// Raffiner une recette avec les objets du sac.
    RefineRecipe(i32),
}

/// Réponses.
#[derive(Debug, Clone, PartialEq)]
pub enum RefineryEvent {
    /// Sphère ouverte (0x3F op 1).
    Opened,
    /// Prix de la grille (op 5).
    Quote { price: i64 },
    /// Résultat d'un raffinage (op 1).
    Result { code: i32, tid: Option<i32> },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Mini EVENT_BASE de la sphère portée par l'objet `object`.
pub fn event_base(object: u32) -> Value {
    let s = mini::EVENT_BASE;
    let values = sparse(s, &[(0, Value::I8(BASE_TYPE_OBJECT)), (1, Value::I32(object as i32)), (3, Value::I32(FUNCTION_REFINERY))]);
    Value::Packet(build_mini(s, &values).unwrap_or_default())
}

fn grid_value(grid: &[i32; GRID_CELLS]) -> Value {
    Value::Ints(grid.iter().map(|i| *i as i64).collect())
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &RefineryCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::REFINERY;
    match cmd {
        RefineryCommand::Open(object) => (
            main::EVENT_REFINERY,
            sparse(main::EVENT_REFINERY, &[(0, Value::I8(0)), (1, event_base(*object)), (2, Value::I32(me))]),
        ),
        RefineryCommand::Quote(grid) => (s, sparse(s, &[(0, Value::I8(op::QUOTE)), (1, Value::I32(me)), (3, grid_value(grid))])),
        RefineryCommand::Refine(grid) => (s, sparse(s, &[(0, Value::I8(op::REFINE)), (1, Value::I32(me)), (2, Value::I32(0)), (3, grid_value(grid))])),
        RefineryCommand::RefineRecipe(id) => (s, sparse(s, &[(0, Value::I8(op::REFINE_RECIPE)), (1, Value::I32(me)), (2, Value::I32(*id))])),
    }
}

/// Traduit un paquet 0x3F ou 0x4B, sinon `None`.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<RefineryEvent> {
    use archrust_protocol::schema::packet_type as T;
    match packet_type {
        T::EVENT_REFINERY if int(values, 0) == Some(1) => Some(RefineryEvent::Opened),
        T::REFINERY => match int(values, 0)? as i8 {
            op::QUOTE => Some(RefineryEvent::Quote { price: int(values, 6).unwrap_or(0) }),
            op::RESULT => Some(RefineryEvent::Result { code: int(values, 4).unwrap_or(-1) as i32, tid: int(values, 5).filter(|t| *t > 0).map(|t| t as i32) }),
            _ => None,
        },
        _ => None,
    }
}

/// Objet du monde désigné par un EVENT_BASE (pour les essais).
pub fn object_of(base: &[u8]) -> Option<i64> {
    decode_mini(mini::EVENT_BASE, base).ok().and_then(|(_, v)| int(&v, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn demandes_et_reponses() {
        let mut grid = [0; GRID_CELLS];
        grid[0] = 42;
        let (s, v) = command_packet(&RefineryCommand::Refine(grid), 7);
        let bytes = build_packet(s, &v).unwrap();
        let back = decode_packet(main::REFINERY, &bytes).unwrap();
        assert_eq!((back.op(), int(&back.values, 2)), (Some(2), Some(0)));
        assert_eq!(back.get(3), Some(&Value::Ints({
            let mut g = vec![0; GRID_CELLS];
            g[0] = 42;
            g
        })));
        let (s, v) = command_packet(&RefineryCommand::Open(1925), 7);
        assert_eq!(s.id, 0x3f);
        let base = v[1].as_ref().and_then(Value::as_bytes).unwrap();
        assert_eq!(object_of(base), Some(1925));

        let quote = sparse(main::REFINERY, &[(0, Value::I8(5)), (6, Value::I64(5000))]);
        assert_eq!(translate(0x4b, &quote), Some(RefineryEvent::Quote { price: 5000 }));
        let done = sparse(main::REFINERY, &[(0, Value::I8(1)), (4, Value::I32(0)), (5, Value::I32(245))]);
        assert_eq!(translate(0x4b, &done), Some(RefineryEvent::Result { code: 0, tid: Some(245) }));
    }
}
