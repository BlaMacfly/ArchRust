//! Moteur du client ArchRust (x86-64, wgpu).
//!
//! * [`assets`] : packs du client, textures, modèles, tables, patchs HD.
//! * [`server_data`] : tables de valeurs écrites par le serveur (`server-data/`), lues avant les packs.
//! * [`render`] : contexte wgpu, maillages, textures GPU, pipeline éclairé.
//! * [`camera`] : caméra orbitale (clic droit + molette) façon MMO.
//! * [`animation`] : animation squelettique des personnages (clips `.ean`, skin CPU).
//! * [`scene`] : conversion des clumps RenderWare en maillages GPU, entités du monde.
//! * [`worn`] : armes, boucliers et lances en main, ensemble d'animations selon l'arme.
//! * [`gltf_model`] : modèles glTF/GLB des patchs (`[[models]]`) en maillages GPU.
//! * [`net`] : liaison avec `archrust-netclient` sur un fil tokio, événements/commandes.
//! * [`audio`] : musique, ambiance et effets (rodio), résolution des sons (packs `.pk`, patchs).
//! * [`zones`] : région d'une position (grille `.amf`), musique de région (`RegionTemplate.ini`, `skyset.ini`).
//! * [`sky`], [`sky_render`], [`weather`] : heure du monde, ciel d'Archonia (gabarits, transitions,
//!   nuages, étoiles), météo (pluie, neige) et leur rendu.
//! * [`shadow`], [`shadow_render`] : ombres projetées des objets du décor sur le sol (AgcmShadow2).
//! * [`options`] : options du client persistées (`saved-variables/options.toml`).
//! * [`ui`] : interface egui (connexion, discussion, console) et rendu des frames Lua.
//! * [`app`] : boucle winit, états (connexion → personnages → monde).

pub mod account;
pub mod addon_ui;
pub mod animation;
pub mod app;
pub mod assets;
pub mod audio;
pub mod boards;
pub mod camera;
pub mod catalog;
pub mod decor;
pub mod gltf_model;
pub mod grass;
pub mod hud;
pub mod login_settings;
pub mod net;
pub mod objects;
pub mod options;
pub mod realmlist;
pub mod render;
pub mod scene;
pub mod server_data;
pub mod shadow;
pub mod shadow_render;
pub mod sky;
pub mod sky_render;
pub mod targeting;
pub mod ui;
pub mod water;
pub mod weather;
pub mod world;
pub mod world_render;
pub mod worn;
pub mod zones;

pub use app::{run, Config};
pub use assets::Assets;
