//! Format d'une texture `.tx1` et répartition de son alpha — diagnostic privé.
//! Usage : `cargo run --release -p archrust-engine --example tx_info -- <Game> texture/character b00060aa`
use archrust_data::rw::TextureDictionary;
use archrust_engine::assets::Assets;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    for name in &args[2..] {
        let raw = assets.read_packed(&args[1], &format!("{name}.tx1"))?;
        let plain = archrust_crypto::rc4::capi_crypt(archrust_crypto::KEY_TEXTURE, &raw);
        let dict = TextureDictionary::parse(&plain)?;
        for t in &dict.textures {
            let rgba = t.to_rgba8()?;
            let zero = rgba.chunks_exact(4).filter(|p| p[3] == 0).count();
            let black_opaque = rgba.chunks_exact(4).filter(|p| p[3] == 255 && p[0] < 8 && p[1] < 8 && p[2] < 8).count();
            println!("{name} : {} masque {:?} {}x{} {:?} alpha {} mips {} — alpha 0 : {zero}, noir opaque : {black_opaque} / {}", t.name, t.mask, t.width, t.height, t.format, t.has_alpha, t.mips.len(), rgba.len() / 4);
        }
    }
    Ok(())
}
