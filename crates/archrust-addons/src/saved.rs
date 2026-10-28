//! SavedVariables : sérialisation des globales listées dans le `.toc` en source Lua, relue au
//! chargement suivant (même principe que `WTF/Account/…/SavedVariables/<AddOn>.lua`).

use mlua::{Lua, Table, Value};
use std::fmt::Write;

/// Sérialise une valeur Lua en littéral Lua. Les fonctions, userdata et threads sont ignorés.
pub fn serialize_value(v: &Value, indent: usize, out: &mut String) -> mlua::Result<bool> {
    match v {
        Value::Nil => out.push_str("nil"),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Integer(i) => write!(out, "{i}").unwrap(),
        Value::Number(n) => {
            if n.is_finite() {
                write!(out, "{n:?}").unwrap();
            } else {
                out.push_str("nil");
            }
        }
        Value::String(s) => {
            // Texte UTF-8 recopié tel quel ; sinon les octets ≥ 128 en échappement décimal. Les
            // pousser un à un comme `char` les réencodait à chaque sauvegarde (é → Ã© → ÃƒÂ©…) :
            // la chaîne doublait à chaque sortie du jeu.
            let bytes = s.as_bytes();
            out.push('"');
            match std::str::from_utf8(&bytes) {
                Ok(text) => text.chars().for_each(|c| push_escaped(c, out)),
                Err(_) => bytes.iter().for_each(|&b| match b {
                    128.. => write!(out, "\\{b}").unwrap(),
                    _ => push_escaped(b as char, out),
                }),
            }
            out.push('"');
        }
        Value::Table(t) => serialize_table(t, indent, out)?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// Un caractère dans un littéral Lua entre guillemets.
fn push_escaped(c: char, out: &mut String) {
    match c {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\0'..='\x1f' | '\x7f' => write!(out, "\\{:03}", c as u32).unwrap(),
        c => out.push(c),
    }
}

fn serialize_table(t: &Table, indent: usize, out: &mut String) -> mlua::Result<()> {
    out.push_str("{\n");
    let pad = "\t".repeat(indent + 1);
    // Entrées triées par clé : l'ordre de parcours d'une table Lua varie d'un chargement à l'autre, et le
    // fichier sauvegardé doit rester le même tant que les données ne changent pas.
    let mut entries = Vec::new();
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let mut key = String::new();
        match &k {
            Value::String(s) => {
                let s = s.to_str()?;
                if is_identifier(&s) {
                    key.push_str(&s);
                } else {
                    key.push('[');
                    serialize_value(&k, indent + 1, &mut key)?;
                    key.push(']');
                }
            }
            Value::Integer(_) | Value::Number(_) | Value::Boolean(_) => {
                key.push('[');
                serialize_value(&k, indent + 1, &mut key)?;
                key.push(']');
            }
            _ => continue,
        }
        let mut val = String::new();
        if !serialize_value(&v, indent + 1, &mut val)? {
            continue;
        }
        entries.push((key, val));
    }
    entries.sort();
    for (key, val) in entries {
        out.push_str(&pad);
        out.push_str(&key);
        out.push_str(" = ");
        out.push_str(&val);
        out.push_str(",\n");
    }
    out.push_str(&"\t".repeat(indent));
    out.push('}');
    Ok(())
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(
            s,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "goto"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
        )
}

/// Source Lua affectant les globales `names` à leur valeur courante.
pub fn dump_globals(lua: &Lua, names: &[String]) -> mlua::Result<String> {
    let globals = lua.globals();
    let mut out = String::new();
    for name in names {
        let v: Value = globals.get(name.as_str())?;
        let mut lit = String::new();
        if matches!(v, Value::Nil) || !serialize_value(&v, 0, &mut lit)? {
            continue;
        }
        out.push_str(name);
        out.push_str(" = ");
        out.push_str(&lit);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let lua = Lua::new();
        lua.load(r#"HelloDB = { count = 3, name = "a\"b", list = { 1, 2.5, true }, ["k y"] = { nested = "x" }, fn = function() end }"#).exec().unwrap();
        let src = dump_globals(&lua, &["HelloDB".into(), "Absent".into()]).unwrap();
        assert!(src.starts_with("HelloDB = {"));
        assert!(!src.contains("fn"));
        let lua2 = Lua::new();
        lua2.load(&src).exec().unwrap();
        let v: Table = lua2.globals().get("HelloDB").unwrap();
        assert_eq!(v.get::<i64>("count").unwrap(), 3);
        assert_eq!(v.get::<String>("name").unwrap(), "a\"b");
        let list: Table = v.get("list").unwrap();
        assert_eq!(list.get::<f64>(2).unwrap(), 2.5);
        let ky: Table = v.get("k y").unwrap();
        assert_eq!(ky.get::<String>("nested").unwrap(), "x");
    }

    #[test]
    fn accents_stables() {
        // Sauvegarder puis relire plusieurs fois ne doit pas réencoder les accents.
        let lua = Lua::new();
        lua.load("DB = { s = \"Vitesse de déplacement\", b = \"\\200\\255x\" }").exec().unwrap();
        let first = dump_globals(&lua, &["DB".into()]).unwrap();
        assert!(first.contains("\"Vitesse de déplacement\""), "{first}");
        let mut src = first.clone();
        for _ in 0..3 {
            let next = Lua::new();
            next.load(&src).exec().unwrap();
            src = dump_globals(&next, &["DB".into()]).unwrap();
        }
        assert_eq!(src, first);
        let last = Lua::new();
        last.load(&src).exec().unwrap();
        let db: Table = last.globals().get("DB").unwrap();
        assert_eq!(db.get::<mlua::String>("b").unwrap().as_bytes().to_vec(), vec![200, 255, b'x']);
    }
}
