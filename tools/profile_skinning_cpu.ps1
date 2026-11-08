# Profil sans Cargo : fonctions CPU extraites textuellement du code courant, dépendances déjà bâties.
# Aucun accès GPU ni écriture dans les données du client. À lancer depuis la racine ArchRust.
param([Parameter(Mandatory=$true)][string]$Game)
$ErrorActionPreference = 'Stop'
function Function-Source([string]$Source, [string]$Signature) {
    $start = $Source.IndexOf($Signature, [StringComparison]::Ordinal)
    if ($start -lt 0) { throw "Signature absente : $Signature" }
    $open = $Source.IndexOf('{', $start)
    $depth = 1
    $end = $open + 1
    while ($depth -gt 0 -and $end -lt $Source.Length) {
        if ($Source[$end] -eq '{') { $depth++ }
        if ($Source[$end] -eq '}') { $depth-- }
        $end++
    }
    if ($depth -ne 0) { throw "Fonction incomplète : $Signature" }
    $Source.Substring($start, $end - $start)
}
$animationSource = Get-Content crates/archrust-engine/src/animation.rs -Raw -Encoding utf8
$sceneSource = Get-Content crates/archrust-engine/src/scene.rs -Raw -Encoding utf8
$renderSource = Get-Content crates/archrust-engine/src/render.rs -Raw -Encoding utf8
$bindingStart = $animationSource.IndexOf('pub struct SkinBinding {', [StringComparison]::Ordinal)
$bindingEnd = $animationSource.IndexOf('/// Variante d', $bindingStart, [StringComparison]::Ordinal)
if ($bindingStart -lt 0 -or $bindingEnd -lt 0) { throw 'Bornes SkinBinding/Clip absentes' }
$snapshot = @'
#![allow(dead_code)]
use archrust_data::ean::{EanFile, HAnimSample};
use archrust_data::rw::{Clump, Geometry};
use archrust_data::pack::PackIndex;
use glam::{Mat3, Quat, Vec3};
use std::sync::Arc;
use std::path::PathBuf;
use std::fs::File;
#[derive(Clone, Copy, Debug)]
'@
$snapshot += Function-Source $renderSource 'pub struct Vertex {'
$snapshot += $animationSource.Substring($bindingStart, $bindingEnd - $bindingStart)
$snapshot += "`npub mod prepared_skin {`n" + (Get-Content crates/archrust-engine/src/animation_prepared.rs -Raw -Encoding utf8) + "`n}`npub use prepared_skin::SkinPoseBatch;`n"
$snapshot += Function-Source $animationSource 'pub fn deform('
$snapshot += Function-Source $sceneSource 'pub fn geometry_vertices('
$snapshot += Function-Source $sceneSource 'fn compute_normals('
$snapshot += @'

pub struct Assets { game: PathBuf }
impl Assets {
    pub fn new(game: Option<PathBuf>, _: &[()]) -> Self { Self { game: game.unwrap() } }
    fn read(&self, dir: &str, file: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let dir = self.game.join(dir);
        let index = PackIndex::parse_encrypted(&std::fs::read(dir.join("Reference.Dat"))?)?;
        let entry = index.find(file).ok_or("entrée du pack absente")?;
        Ok(index.read_entry(&mut File::open(dir.join("Data.Dat"))?, entry)?)
    }
    pub fn clump(&self, dir: &str, file: &str) -> Result<Arc<Clump>, Box<dyn std::error::Error>> {
        Ok(Arc::new(Clump::parse(&self.read(dir, file)?)?))
    }
}
pub fn models_for_tid(_: &Assets, tid: i32) -> Vec<String> {
    assert_eq!(tid, 96);
    vec!["a0000060.ecl".into(), "b0000060.ecl".into()]
}
pub fn load_clip(assets: &Assets, file: &str) -> Option<Arc<Clip>> {
    let raw = assets.read("character/animation", file).ok()?;
    Some(Arc::new(Clip::from_ean(file, &EanFile::parse(&raw).ok()?)?))
}
'@
[IO.File]::WriteAllText((Join-Path $PWD 'target/skinning_cpu_snapshot.rs'), $snapshot, [Text.UTF8Encoding]::new($false))
Write-Output ('SHA256 animation.rs : ' + (Get-FileHash crates/archrust-engine/src/animation.rs).Hash)
Write-Output ('SHA256 rw.rs : ' + (Get-FileHash crates/archrust-data/src/rw.rs).Hash)
& rustup run 1.98.1 rustc --edition=2021 --cfg archrust_cpu_snapshot -C opt-level=1 -C target-cpu=x86-64-v2 --target x86_64-pc-windows-msvc -L dependency=target/x86_64-pc-windows-msvc/debug/deps -L dependency=target/debug/deps --extern archrust_data=target/x86_64-pc-windows-msvc/debug/deps/libarchrust_data-67faac3061927a0f.rlib --extern glam=target/x86_64-pc-windows-msvc/debug/deps/libglam-333f782367142cf9.rlib crates/archrust-engine/examples/skinning_cpu.rs -o target/skinning_cpu.exe
if ($LASTEXITCODE -ne 0) { throw 'Compilation directe du profil impossible (dépendances locales requises)' }
& ./target/skinning_cpu.exe $Game
if ($LASTEXITCODE -ne 0) { throw 'Échec du profil CPU' }
