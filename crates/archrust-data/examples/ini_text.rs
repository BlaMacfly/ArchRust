//! Déchiffre une table ini du client et l'écrit en clair sur la sortie standard (usage privé), dans
//! la page de code de son dossier (`ini_fr`, `ini_ja`…).
//! Usage : cargo run -p archrust-data --example ini_text -- <table.ini>
fn main() {
    let path = std::env::args().nth(1).expect("table .ini");
    let raw = std::fs::read(&path).expect("lecture");
    let dir = std::path::Path::new(&path).parent().and_then(|d| d.file_name()).and_then(|d| d.to_str()).unwrap_or("ini");
    print!("{}", archrust_data::ini::decrypt_ini_with(&raw, archrust_data::ini::CodePage::for_dir(dir)));
}
