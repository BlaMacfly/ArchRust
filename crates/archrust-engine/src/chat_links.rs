//! Liens d'objet de la discussion, comme Archonia (sha256 e9c7609b…).
//!
//! * **Écriture** (FUN_14053dc90 → FUN_140787f20) : Ctrl + clic sur un objet du sac (statut 2, 13 ou 15) quand la
//!   saisie de la discussion est ouverte ajoute `<ITEM::IID::nom du modèle>` à la fin de la saisie, si le texte
//!   reste sous la longueur permise (`+0x6240`) ; sinon le clic garde son effet ordinaire. Les modificateurs des
//!   raccourcis d'Archonia (FUN_14054da00) font de `DAT_140f8b0a0` / `a4` la touche Ctrl gauche / droite.
//! * **Lecture** (FUN_14079c040 → FUN_14079ad60) : une ligne reçue est coupée entre `<` ou `[` et `>` ou `]` ;
//!   un morceau qui contient `ITEM` donne un lien de genre 1 : IID entre le 1ᵉʳ et le 2ᵉ `::`, nom entre le
//!   2ᵉ `::` et `>`. Il s'affiche `<nom>` (caractères d'ouverture et de fermeture gardés), sur un bouton
//!   (`AcUIEventButton`) au fond `0x400d4ef9` (bleu à 25 %). Les autres balises (`MAPPOS`, genre 2 ; `[nom]`,
//!   genre 3) ne sont pas reprises : rien ne les écrit dans ArchRust.
//! * **Clic** (FUN_140511500, genre 1) : objet déjà reçu (FUN_1407a5c20) → son infobulle (FUN_1407a5e50) ;
//!   sinon AGPMITEM op 31 genre 8 (FUN_1407a5f60 → FUN_140649610), l'infobulle venant avec l'op 32.
//!
//! Archonia met aussi le lien dans « Gained <C%u><ITEM::…><C16777215> » (FUN_1407a5fe0, FUN_1406ff780) ;
//! ArchRust compose ses messages de butin avec l'[EventMap] et ne les lie pas.

/// Balise d'un objet pour la saisie (`<ITEM::%d::%s>`, FUN_140787f20).
pub fn item_link(iid: i32, name: &str) -> String {
    format!("<ITEM::{iid}::{name}>")
}

/// Morceau d'une ligne : texte affiché et, pour un lien, l'IID de l'objet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub iid: Option<i32>,
}

/// Lien `ITEM` d'un morceau `<…>` ou `[…]` (FUN_14079ad60) : IID et texte affiché.
fn parse_item(open: char, inner: &str, close: char) -> Option<(i32, String)> {
    if !inner.contains("ITEM") {
        return None;
    }
    let first = inner.find("::")?;
    let rest = &inner[first + 2..];
    let second = rest.find("::")?;
    let iid: i32 = rest[..second].trim().parse().ok()?;
    let name = &rest[second + 2..];
    (iid != 0).then(|| (iid, format!("{open}{name}{close}")))
}

/// Coupe `text` en morceaux, liens d'objet à part.
pub fn split(text: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(['<', '[']) {
        let open = rest[at..].chars().next().unwrap_or('<');
        let after = &rest[at + 1..];
        let Some(end) = after.find(['>', ']']) else { break };
        let close = after[end..].chars().next().unwrap_or('>');
        match parse_item(open, &after[..end], close) {
            Some((iid, shown)) => {
                plain.push_str(&rest[..at]);
                if !plain.is_empty() {
                    out.push(Piece { text: std::mem::take(&mut plain), iid: None });
                }
                out.push(Piece { text: shown, iid: Some(iid) });
            }
            None => plain.push_str(&rest[..at + 1 + end + close.len_utf8()]),
        }
        rest = &after[end + close.len_utf8()..];
    }
    plain.push_str(rest);
    if !plain.is_empty() || out.is_empty() {
        out.push(Piece { text: plain, iid: None });
    }
    out
}

/// Texte affiché d'une ligne (liens remplacés par `<nom>`), pour les bulles et les journaux.
pub fn display(text: &str) -> String {
    if !text.contains("ITEM") {
        return text.to_string();
    }
    split(text).into_iter().map(|p| p.text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lien_ecrit_et_relu() {
        let line = format!("[Bob] regarde {} !", item_link(77, "Épée longue"));
        assert_eq!(line, "[Bob] regarde <ITEM::77::Épée longue> !");
        let pieces = split(&line);
        assert_eq!(
            pieces,
            vec![
                Piece { text: "[Bob] regarde ".into(), iid: None },
                Piece { text: "<Épée longue>".into(), iid: Some(77) },
                Piece { text: " !".into(), iid: None },
            ]
        );
        assert_eq!(display(&line), "[Bob] regarde <Épée longue> !");
    }

    #[test]
    fn texte_sans_lien() {
        assert_eq!(split("a < b et [c]"), vec![Piece { text: "a < b et [c]".into(), iid: None }]);
        assert_eq!(split(""), vec![Piece { text: String::new(), iid: None }]);
        // IID illisible ou nul : texte gardé tel quel.
        assert_eq!(display("<ITEM::x::Arc>"), "<ITEM::x::Arc>");
        assert_eq!(split("<ITEM::0::Arc>")[0].iid, None);
    }

    #[test]
    fn deux_liens_colles() {
        let p = split("<ITEM::1::A><ITEM::2::B>");
        assert_eq!(p.iter().map(|p| p.iid).collect::<Vec<_>>(), vec![Some(1), Some(2)]);
        assert_eq!(display("<ITEM::1::A><ITEM::2::B>"), "<A><B>");
    }
}
