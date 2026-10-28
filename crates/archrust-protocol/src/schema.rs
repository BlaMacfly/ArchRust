//! Schémas des paquets ArchLord connus.
//!
//! Relevés dans l'exécutable : chaque module `Agpm*`/`Agcm*` déclare ses champs avec
//! `AuPacket::SetFieldType` dans son constructeur, et la longueur des drapeaux avec `SetFlagLength`.
//! Le relevé exhaustif des 86 modules est dans `ArchCore/data/re/packet-schemas.json` ; ici ne
//! figurent que les schémas utilisés et validés avec le vrai client.
//!
//! Un champ = `(type, taille)`. Pour `Char`, la taille est le nombre d'octets ; pour les scalaires
//! c'est un nombre de répétitions ; pour `Packet` une taille > 1 signifie « liste de mini-paquets
//! précédée d'un compteur u8 ».

/// Types de champs d'`AuPacket` (valeurs internes du client).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldType {
    Char = 0,
    Int8 = 1,
    UInt8 = 2,
    Int16 = 3,
    UInt16 = 4,
    Int32 = 5,
    UInt32 = 6,
    Int64 = 7,
    UInt64 = 8,
    Float = 9,
    Vec3F = 10,
    Matrix = 11,
    Packet = 12,
    MemoryBlock = 13,
    PosBaseMeter = 14,
    WChar = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub ty: FieldType,
    pub size: u16,
}

#[derive(Debug)]
pub struct Schema {
    pub name: &'static str,
    /// Type de paquet principal, ou identifiant interne (≥ 100) pour un mini-paquet.
    pub id: u8,
    pub flag_len: u8,
    pub fields: &'static [Field],
}

macro_rules! f {
    ($ty:ident) => {
        Field {
            ty: FieldType::$ty,
            size: 1,
        }
    };
    ($ty:ident, $n:expr) => {
        Field {
            ty: FieldType::$ty,
            size: $n,
        }
    };
}

macro_rules! schema {
    ($vis:vis $name:ident = $label:literal, $id:expr, $flags:expr, [$($field:expr),* $(,)?]) => {
        $vis static $name: &Schema = &Schema { name: $label, id: $id, flag_len: $flags, fields: &[$($field),*] };
    };
}

/// Types de paquets principaux.
pub mod packet_type {
    pub const SYSTEM: u8 = 0x00;
    pub const CONFIG: u8 = 0x01;
    pub const CHARACTER: u8 = 0x02;
    pub const CHAR_MANAGER: u8 = 0x03;
    pub const ITEM: u8 = 0x04;
    pub const SKILL: u8 = 0x05;
    pub const PARTY: u8 = 0x08;
    pub const PARTY_MANAGEMENT: u8 = 0x09;
    pub const LOGIN: u8 = 0x0d;
    pub const CHATTING: u8 = 0x0f;
    pub const TIMER: u8 = 0x11;
    /// AgpmEventNature : météo (serveur → client).
    pub const EVENT_NATURE: u8 = 0x30;
    /// AgpmNatureEffect : effet de nature (pluie, neige…), drapeaux 2 octets.
    pub const NATURE_EFFECT: u8 = 0x47;
    pub const ADMIN: u8 = 0x12;
    pub const QUEST: u8 = 0x13;
    pub const UI_STATUS: u8 = 0x14;
    pub const GUILD: u8 = 0x15;
    pub const WORLD: u8 = 0x18;
    pub const RETURN_TO_LOGIN: u8 = 0x2f;
    pub const EVENT_TELEPORT: u8 = 0x31;
    pub const EVENT_NPC_TRADE: u8 = 0x32;
    pub const EVENT_BANK: u8 = 0x36;
    pub const ITEM_CONVERT: u8 = 0x16;
    pub const EVENT_ITEM_CONVERT: u8 = 0x38;
    pub const REFINERY: u8 = 0x4b;
    pub const EVENT_REFINERY: u8 = 0x3f;
    pub const EVENT_GUILD: u8 = 0x39;
    pub const GUILD_WAREHOUSE: u8 = 0x54;
    pub const EVENT_NPC_DIALOG: u8 = 0x37;
    pub const EVENT_SKILL_MASTER: u8 = 0x3b;
    pub const EVENT_MANAGER: u8 = 0x3c;
    pub const EVENT_QUEST: u8 = 0x3e;
    pub const OPT_CHAR_MOVE: u8 = 0x40;
    pub const OPT_CHAR_ACTION: u8 = 0x41;
    pub const OPT_VIEW: u8 = 0x42;
    pub const SYSTEM_MESSAGE: u8 = 0x44;
    pub const STARTUP_ENCRYPTION: u8 = 0x48;
    pub const TITLE: u8 = 0x60;
    pub const RECONNECT: u8 = 0x63;
    pub const PVP: u8 = 0x4a;
    pub const REMISSION: u8 = 0x2a;
    pub const PRODUCT: u8 = 0x17;
    pub const EVENT_PRODUCT: u8 = 0x3a;
    pub const BUDDY: u8 = 0x2b;
    pub const MAILBOX: u8 = 0x2d;
    pub const EVENT_ITEM_REPAIR: u8 = 0x34;
    pub const RIDE: u8 = 0x4d;
    pub const AUCTION: u8 = 0x0b;
    pub const ITEM_SET: u8 = 0x6a;
    /// AgpmAreaChatting : cris de zone (serveur → client seulement).
    pub const AREA_CHATTING: u8 = 0x4c;
    /// AgpmChannel : salons de discussion.
    pub const CHANNEL: u8 = 0x2c;
    /// AgpmEventCharCustomize : changement d'apparence chez les « Stylist » (fonction de PNJ 29).
    pub const EVENT_CHAR_CUSTOMIZE: u8 = 0x43;
    /// AgpmWantedCriminal : avis de recherche (fonction de PNJ 32).
    pub const WANTED_CRIMINAL: u8 = 0x46;
    /// AgpmSummons : invocations (esprits, pièges, familiers).
    pub const SUMMONS: u8 = 0x4f;
    /// AgcmBattleGround : annonces du Battleground, structure brute (serveur → client).
    pub const BATTLEGROUND: u8 = 0x58;
    /// AgpmShrineBattle : combats de Shrine de l'épisode 8, structure brute.
    pub const SHRINE_BATTLE: u8 = 0x62;
    /// AgpmBoss (Archonia) : fenêtre des boss, butin, classement, compteur de dégâts.
    pub const BOSS: u8 = 0x64;
    /// AgpmSiegeWar : guerres de siège des châteaux, drapeaux 2 octets.
    pub const SIEGE_WAR: u8 = 0x49;
    /// AgpmArchlord : titre d'ArchLord, gardes, étapes de la guerre de l'ArchLord, drapeaux 2 octets.
    pub const ARCHLORD: u8 = 0x55;
    /// AgpmTax : taxes des régions (intendant « Grand Advisor », fonction de PNJ 34), drapeaux 2 octets.
    pub const TAX: u8 = 0x53;
    /// AgpmEventSiegeWarNPC : dialogue du « Siege Commander » (fonction de PNJ 33).
    pub const EVENT_SIEGEWAR_NPC: u8 = 0x50;
    /// AgpmWarInfo : tableau d'information des guerres (siège, Battleground, Shrine, Archlord), lu en brut.
    pub const WAR_INFO: u8 = 0x61;
}

/// Schémas des paquets principaux.
pub mod main {
    use super::packet_type as T;
    use super::*;

    // AgpmConfig — drapeaux 4
    schema!(pub CONFIG = "AGPMCONFIG", T::CONFIG, 4, [f!(Int32), f!(Int32), f!(Int8)]);

    // AgpmCharacter — drapeaux 4
    schema!(pub CHARACTER = "AGPMCHARACTER", T::CHARACTER, 4, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 CID
        f!(Int32),          // 2 TID (modèle)
        f!(MemoryBlock),    // 3 nom
        f!(Int8),           // 4 statut
        f!(Packet),         // 5 mini déplacement (CHAR_MOVE)
        f!(Packet),         // 6 mini action
        f!(Packet),         // 7 mini facteurs
        f!(Int64),          // 8 argent inventaire
        f!(Int64),          // 9 argent banque
        f!(Int64),          // 10 cash
        f!(Int8),           // 11 statut d'action
        f!(Int8),           // 12 statut criminel
        f!(Int32),          // 13 attaquant
        f!(Int8),           // 14 nouveau perso
        f!(UInt8),          // 15 dernier point de liaison
        f!(UInt8),          // 16 animation sociale
        f!(UInt64),         // 17 statut spécial
        f!(Int8),           // 18 transformation
        f!(Char, 32),       // 19 skill init
        f!(Int8),           // 20 visage
        f!(Int8),           // 21 cheveux
        f!(Int32),          // 22 option flag
        f!(Int8),           // 23 taille de banque
        f!(UInt16),         // 24 event status flag
        f!(Int64),          // 25 temps criminel restant
        f!(Int64),          // 26 temps meurtrier restant
        f!(Char, 16),       // 27 signature
        f!(MemoryBlock),    // 28 données GameGuard
        f!(UInt64),         // 29 dernière mort en Battle Square
    ]);

    // AgpmItem — drapeaux 4 (constructeur relevé dans ap_item.c).
    schema!(pub ITEM = "AGPMITEM", T::ITEM, 4, [
        f!(UInt8), f!(Int8), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
        f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet),
        f!(Int32), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Int32),
        // 18 et 22 : temps de recharge en UINT64 (relevé d'Archonia, comme ArchCore) ; 19 drapeaux d'état
        // de l'objet (`AgpdItem+0x218` : bits 0-2 lié, bit 30 lié définitivement).
        f!(UInt64), f!(Int32), f!(Packet), f!(Packet), f!(UInt64), f!(Packet), f!(Packet),
        // 25 bloc (inconnu), 26 identifiant 64 bits de l'objet (`AgpdItem+0x58`, commutateur d'équipement).
        f!(MemoryBlock), f!(UInt64),
    ]);

    // AgcmCharacter (type 3) — drapeaux 4 (SetFlagLength(4))
    schema!(pub CHAR_MANAGER = "AGSMCHARMANAGER", T::CHAR_MANAGER, 4, [
        f!(Int8),           // 0 opération
        f!(Char, 12),       // 1
        f!(Int32),          // 2
        f!(Char, 49),       // 3 nom de personnage
        f!(Int32),          // 4 CID
        f!(Vec3F),          // 5 position
        f!(Int32),          // 6 clé d'authentification
        f!(Char, 248),      // 7 (nouveau dans le client x64)
    ]);

    // AgpmLogin — drapeaux 2
    schema!(pub LOGIN = "AGPMLOGIN", T::LOGIN, 2, [
        f!(Int8),           // 0 opération
        f!(Char, 32),       // 1 clé de hachage (8 octets utiles)
        f!(Char, 49),       // 2 compte (chiffré RC4)
        f!(Int8),           // 3 longueur du compte
        f!(Char, 65),       // 4 mot de passe (chiffré RC4)
        f!(Int8),           // 5 longueur du mot de passe
        f!(Int32),          // 6 CID
        f!(Char, 32),       // 7 nom du monde
        f!(Packet),         // 8 mini CHAR_INFO
        f!(Packet),         // 9 mini VERSION_INFO
        f!(Int32),          // 10 résultat
        f!(Packet),         // 11 mini SERVER_INFO
        f!(Char, 2049),     // 12 extra
        f!(Char, 5),        // 13 code
        f!(Int32),          // 14 compte limité
        f!(Int32),          // 15 compte protégé
    ]);

    // AgpmTimer — drapeaux 1 : vitesse du temps (multiplicateur, 0 = inchangée), temps du jeu en ms
    // (0 = inchangé), deux UINT32 sans lecteur dans Archonia (FUN_14022e800).
    schema!(pub TIMER = "AGPMTIMER", T::TIMER, 1, [f!(UInt8), f!(UInt64), f!(UInt32), f!(UInt32)]);

    // AgpmEventNature — drapeaux 1 : météo (−1 absent ; 0 clair, 3 pluie, 5 neige), deux INT64 non
    // lus par le client (FUN_1401ea2d0).
    schema!(pub EVENT_NATURE = "AGPMEVENTNATURE", T::EVENT_NATURE, 1, [f!(Int8), f!(Int64), f!(Int64)]);

    // AgpmNatureEffect — drapeaux 2 : identifiant d'effet de `natureeffect.ini` (−1 = arrêt).
    schema!(pub NATURE_EFFECT = "AGPMNATUREEFFECT", T::NATURE_EFFECT, 2, [f!(Int32)]);

    // AgpmWorld — drapeaux 1
    schema!(pub WORLD = "AGPMWORLD", T::WORLD, 1, [f!(Int8), f!(Char, 32), f!(Int16), f!(MemoryBlock), f!(Char, 32)]);

    // AgpmChatting — drapeaux 1
    schema!(pub CHATTING = "AGPMCHATTING", T::CHATTING, 1, [
        f!(Int8), f!(Int32), f!(Int8), f!(Int32), f!(Char, 49), f!(Char, 49), f!(MemoryBlock),
    ]);

    // AgpmSystemMessage — drapeaux 1
    schema!(pub SYSTEM_MESSAGE = "AGPMSYSTEMMESSAGE", T::SYSTEM_MESSAGE, 1, [
        f!(Int8), f!(Int32), f!(Int32), f!(Int32), f!(Char, 65), f!(Char, 65),
    ]);

    // AgpmReconnect — drapeaux 4 (Archonia FUN_14027c3f0) : op (1 demande, 2 réponse), CID, nom, compte,
    // mot de passe, clé de reconnexion, accord.
    schema!(pub RECONNECT = "AGPMRECONNECT", T::RECONNECT, 4, [
        f!(Int8), f!(Int32), f!(Char, 48), f!(Char, 1024), f!(Char, 1024), f!(Int32), f!(Int8),
    ]);

    // AgpmReturnToLogin — drapeaux 1
    schema!(pub RETURN_TO_LOGIN = "AGPMRETURNTOLOGIN", T::RETURN_TO_LOGIN, 1, [
        f!(Int8), f!(Int32), f!(Char, 49), f!(Char, 32), f!(Char, 25), f!(Int32),
    ]);

    // AgpmOptimizedPacket2 (0x40) — drapeaux 1
    schema!(pub OPT_CHAR_MOVE = "AGPMOPTIMIZEDCHARMOVE", T::OPT_CHAR_MOVE, 1, [
        f!(Int32),          // 0 CID
        f!(Vec3F),          // 1 position
        f!(Vec3F),          // 2 destination
        f!(Int32),          // 3 cible suivie
        f!(UInt16),         // 4 distance de suivi
        f!(Int8),           // 5 drapeaux de déplacement
        f!(Int8),           // 6 direction
        f!(Int8),           // 7
        f!(Int32),          // 8
    ]);

    // AgpmOptimizedPacket2 (0x41) — drapeaux 2
    schema!(pub OPT_CHAR_ACTION = "AGPMOPTIMIZEDCHARACTION", T::OPT_CHAR_ACTION, 2, [
        f!(Int32), f!(Int32), f!(Int8), f!(Packet), f!(Int32), f!(Int8), f!(Int8), f!(UInt32), f!(UInt8),
    ]);

    // AgpmOptimizedPacket2 (0x42) — drapeaux 2
    schema!(pub OPT_VIEW = "AGPMOPTIMIZEDVIEW", T::OPT_VIEW, 2, [
        f!(Int8),           // 0 opération
        f!(Packet),         // 1 vue d'un personnage
        f!(Packet, 32),     // 2 liste de vues
        f!(MemoryBlock),    // 3
        f!(MemoryBlock),    // 4 fonctions PNJ (ApmEventManager)
        f!(Char, 33),       // 5
        f!(MemoryBlock), f!(MemoryBlock), f!(MemoryBlock),
        f!(Int32), f!(Int32), f!(Int32), f!(Int32),
        f!(MemoryBlock),
    ]);

    // AgpmStartupEncryption — drapeaux 1
    schema!(pub STARTUP_ENCRYPTION = "AGPMSTARTUPENCRYPTION", T::STARTUP_ENCRYPTION, 1, [f!(Int8), f!(MemoryBlock)]);

    // AgpmSkill (0x05) — drapeaux 2 : compétences, lancement, effets actifs (op 4 ajout, 5 retrait).
    schema!(pub SKILL = "AGPMSKILL", T::SKILL, 2, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 identifiant de la compétence
        f!(Packet),         // 2 propriétaire (mini SKILL_BASE)
        f!(Packet),         // 3 facteurs
        f!(Int32),          // 4 TID
        f!(Int8),           // 5 statut
        f!(Packet),         // 6 action (mini SKILL_ACTION)
        f!(UInt8),          // 7 points de compétence
        f!(Packet, 96),     // 8 effets actifs (mini SKILL_BUFF)
        f!(MemoryBlock),    // 9
    ]);

    // AgpmParty (0x08) — drapeaux 2 : le groupe (création, membres, réglages du chef).
    schema!(pub PARTY = "AGPMPARTY", T::PARTY, 2, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 identifiant du groupe
        f!(Int8),           // 2 membres au plus
        f!(Int8),           // 3 nombre de membres
        f!(Int32),          // 4 CID du membre ajouté ou retiré
        f!(MemoryBlock),    // 5 liste des CID (u32, chef en tête)
        f!(Packet),         // 6 facteurs
        f!(Packet),         // 7 zone d'effet
        f!(Int8),           // 8 répartition de l'expérience (0 dégâts, 1 niveau)
        f!(Packet),         // 9 bonus
        f!(Int8),           // 10 répartition des objets
    ]);

    // AgpmParty, gestion (0x09) — drapeaux 1 : invitation, réponse, départ, exclusion, chef.
    schema!(pub PARTY_MANAGEMENT = "AGPMPARTYMANAGEMENT", T::PARTY_MANAGEMENT, 1, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 CID de l'auteur
        f!(Int32),          // 2 CID de la cible
        f!(Char, 49),       // 3 nom
    ]);

    // AgpmGuild (0x15) — drapeaux 4 : guildes. Les champs 1 à 4 portent aussi le code, les textes et les
    // nombres d'un message système (op 10).
    schema!(pub GUILD = "AGPMGUILD", T::GUILD, 4, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 CID (message système : code)
        f!(Char, 33),       // 2 nom de la guilde (message système : texte 1)
        f!(Char, 49),       // 3 chef ou joueur (message système : texte 2)
        f!(Int32, 8),       // 4 nombres (message système : le premier)
        f!(Int32),          // 5 rang de la guilde
        f!(Int64),          // 6 date de création
        f!(Int32),          // 7 taille maximale
        f!(Int32),          // 8 union
        f!(Char, 13),       // 9 mot de passe
        f!(MemoryBlock),    // 10 annonce
        f!(Int8),           // 11 statut
        f!(Int32),          // 12 victoires
        f!(Int32),          // 13 nuls
        f!(Int32),          // 14 défaites
        f!(Int32),          // 15 points
        f!(Packet),         // 16 membre (mini GUILD_MEMBER)
        f!(Packet),         // 17 combat
        f!(Packet),         // 18 combattant
        f!(Packet),         // 19 membre au combat
        f!(Packet),         // 20 liste des membres au combat
        f!(Packet),         // 21 liste des guildes
        f!(Int32),          // 22 emblème : TID
        f!(Int32),          // 23 emblème : couleur
        f!(Int32),          // 24 guilde victorieuse
    ]);

    // AgpmEventGuild (0x39) — drapeaux 1 : op (0 demande, 1 accord), événement (EVENT_BASE), CID du joueur.
    schema!(pub EVENT_GUILD = "AGPMEVENTGUILD", T::EVENT_GUILD, 1, [f!(Int8), f!(Packet), f!(Int32)]);
    // AgpmGuildWarehouse (0x54) — drapeaux 2 : op, CID, argent, pages en plus, IID, page, ligne, colonne, résultat.
    schema!(pub GUILD_WAREHOUSE = "AGPMGUILDWAREHOUSE", T::GUILD_WAREHOUSE, 2, [
        f!(Int8),
        f!(Int32),
        f!(Int64),
        f!(Int32),
        f!(Int32),
        f!(Int32),
        f!(Int32),
        f!(Int32),
        f!(Int32),
    ]);
    // AgpmPvP (0x4A) — drapeaux 2, du serveur au client : opération, CID (code du message à l'op 18), cible,
    // victoires (entier du message), défaites, mode, statut, octet de l'op 0, guilde (chaîne 1), nom (chaîne 2).
    schema!(pub PVP = "AGPMPVP", T::PVP, 2, [
        f!(Int8),
        f!(Int32),
        f!(Int32),
        f!(Int32),
        f!(Int32),
        f!(Int8),
        f!(Int8),
        f!(Int8),
        f!(Char, 33),
        f!(Char, 49),
    ]);

    // AgpmRemission (0x2A) — drapeaux 1 : opération, CID, mini-paquet (EVENT_BASE ou REMISSION).
    schema!(pub REMISSION = "AGPMREMISSION", T::REMISSION, 1, [f!(Int8), f!(Int32), f!(Packet)]);

    // AgpmProduct (0x17, constructeur d'Archonia FUN_1402a5d10) — drapeaux 2 : production (récolte, fabrication).
    schema!(pub PRODUCT = "AGPMPRODUCT", T::PRODUCT, 2, [
        f!(Int8),           // 0 opération (0 fabriquer, 1 recettes connues, 2 récolter, 3 résultat)
        f!(Int32),          // 1 CID
        f!(Int32),          // 2 identifiant (instance) de la compétence
        f!(Int32),          // 3 recette (op 0), cadavre (op 2), TID obtenu (op 3)
        f!(Int32),          // 4 IID de l'objet recette (op 0), quantité obtenue (op 3)
        f!(Int32),          // 5 code de résultat (op 3)
        f!(Int32),          // 6 expérience de production gagnée (op 3)
        f!(MemoryBlock),    // 7 recettes connues, INT32 mis bout à bout (op 1)
    ]);

    // AgpmEventProduct (0x3A, FUN_1402d0180) — drapeaux 1 : PNJ de production (fonction 24).
    schema!(pub EVENT_PRODUCT = "AGPMEVENTPRODUCT", T::EVENT_PRODUCT, 1, [
        f!(Int8),           // 0 opération (0 → 1 ouverture, 2 → 3 compétence, 4 → 5 recette)
        f!(Packet),         // 1 événement (EVENT_BASE)
        f!(Int32),          // 2 CID du joueur
        f!(Int32),          // 3 TID de la compétence ou identifiant de la recette
        f!(Int32),          // 4 code de résultat
    ]);

    // AgpmBuddy (0x2B, constructeur d'Archonia FUN_1402ab0c0) — drapeaux 4 : amis et ignorés.
    schema!(pub BUDDY = "AGPMBUDDY", T::BUDDY, 4, [
        f!(Int8),           // 0 opération (1 à 15)
        f!(Int32),          // 1 CID du propriétaire de la liste
        f!(Char, 33),       // 2 nom de l'autre joueur
        f!(UInt32),         // 3 drapeaux de l'entrée (blocages, en ligne, ami)
        f!(Int32),          // 4 région (op 7) ; -1 ou 0 ailleurs
        f!(Int32),          // 5 relation « Buddy » (mentor)
        f!(Int32),          // 6 code de résultat (op 8)
    ]);

    // AgpmBuddy, second paquet (op 15, même type 0x2B) — drapeaux 4 : ignorés d'`IgnoreList.txt`.
    schema!(pub BUDDY_LIST = "AGPMBUDDY_LIST", T::BUDDY, 4, [
        f!(Int8),           // 0 opération (15)
        f!(Int32),          // 1 CID du propriétaire
        f!(Int16),          // 2 nombre de noms (30 au plus)
        f!(Char, 990),      // 3 30 noms de 33 octets bout à bout
        f!(UInt32, 30),     // 4 drapeaux de chaque nom
    ]);

    // AgpmMailBox (0x2D, FUN_1402ead30) — drapeaux 2 : courrier entre joueurs.
    schema!(pub MAILBOX = "AGPMMAILBOX", T::MAILBOX, 2, [
        f!(Int8),           // 0 opération (0 ajout … 5 tout retirer)
        f!(Int32),          // 1 CID du personnage concerné
        f!(Packet),         // 2 mini courrier (MAIL)
        f!(Int32),          // 3 IID de l'objet joint (op 3)
        f!(Int32),          // 4 code de résultat (0 succès)
    ]);

    // AgpmUIStatus (0x14) — drapeaux 2 : barre de raccourcis, potions et seuils de leur prise automatique.
    schema!(pub UI_STATUS = "AGPMUISTATUS", T::UI_STATUS, 2, [
        f!(Int8),        // 0 opération (0 envoi complet, 1 changement, 2 barre encodée, 3 casque)
        f!(Int32),       // 1 CID
        f!(Packet, 50),  // 2 barre de raccourcis
        f!(Int8),        // 3 case changée
        f!(Packet),      // 4 contenu de la case
        f!(MemoryBlock), // 5 barre encodée (texte)
        f!(Int32),       // 6 TID de la potion de vie
        f!(Int32),       // 7 TID de la potion de mana
        f!(Int8),        // 8 affichage du casque
        f!(Int8),        // 9 seuil de prise automatique des PV (%)
        f!(Int8),        // 10 seuil de prise automatique des PM (%)
        f!(MemoryBlock), // 11 temps de recharge encodés (texte)
    ]);

    // ApmEventManager (0x3C) — drapeaux 1 : désignation (mini EVENT_BASE), données de la fonction.
    // Les fonctions d'un PNJ arrivent ainsi, paquets complets mis bout à bout, au champ 4 de sa vue.
    schema!(pub EVENT_MANAGER = "APMEVENTMANAGER", T::EVENT_MANAGER, 1, [f!(Packet), f!(Packet)]);

    // AgpmEventNPCDialog (0x37) — drapeaux 1 : op (0 demande, 1 accord), événement, CID du joueur.
    schema!(pub EVENT_NPC_DIALOG = "AGPMEVENTNPCDIALOG", T::EVENT_NPC_DIALOG, 1, [f!(Int8), f!(Packet), f!(Int32)]);

    // AgpmEventTeleport (0x31) — drapeaux 1 : portails (relevé d'Archonia, validé avec ArchCore `sim portal`).
    schema!(pub EVENT_TELEPORT = "AGPMEVENTTELEPORT", T::EVENT_TELEPORT, 1, [
        f!(Int8),           // 0 opération (4 demande → 5 accord ; 0 point choisi → 1 départ, 8 chargement fini ; 7 retour)
        f!(Packet),         // 1 événement (EVENT_BASE de l'objet portail)
        f!(Int32),          // 2 CID du joueur
        f!(Char, 33),       // 3 nom du point d'arrivée (CP949)
        f!(Vec3F),          // 4 position d'arrivée
        f!(MemoryBlock),    // 5 points d'arrivée désactivés
    ]);

    // AgcmEventNPCTrade (0x32) — drapeaux 2 : boutique d'un PNJ.
    schema!(pub EVENT_NPC_TRADE = "AGCMEVENTNPCTRADE", T::EVENT_NPC_TRADE, 2, [
        f!(Packet),         // 0 événement (EVENT_BASE)
        f!(Int8),           // 1 opération (0 liste, 1 fin de liste, 2 achat, 3 vente)
        f!(Int32),          // 2 CID du joueur
        f!(Int32),          // 3 IID
        f!(Packet),         // 4 détail (mini NPC_TRADE_DETAIL)
        f!(Int64),          // 5 horodatage
        f!(Int32),          // 6 résultat
        f!(Int8),           // 7 sac visé
        f!(Int8),           // 8 ligne visée
        f!(Int8),           // 9 colonne visée
    ]);

    // AgpmEventBank (0x36) — drapeaux 1 : op (0 demande, 1 accord), événement, CID du joueur.
    schema!(pub EVENT_BANK = "AGPMEVENTBANK", T::EVENT_BANK, 1, [f!(Int8), f!(Packet), f!(Int32)]);

    // AgpmItemConvert (0x16) — drapeaux 2 : conversion d'un objet (renforcement, sockets, pierres).
    schema!(pub ITEM_CONVERT = "AGPMITEMCONVERT", T::ITEM_CONVERT, 2, [
        f!(Int8),           // 0 opération (0 état ; 1, 2, 4 demandes → 5, 6, 7)
        f!(Int32),          // 1 CID du joueur
        f!(Int32),          // 2 IID de l'objet
        f!(Int32),          // 3 IID de ce qui le convertit (potion, pierre)
        f!(Int8),           // 4 résultat
        f!(Int8),           // 5 niveau de renforcement
        f!(Int8),           // 6 sockets
        f!(Int8),           // 7 sockets remplis
        f!(Int32),          // 8 TID ajouté
        f!(Packet),         // 9 TID sertis (mini ITEM_CONVERT_TIDS)
    ]);

    // AgpmEventItemConvert (0x38) — drapeaux 1 : op (0 demande, 1 accord), événement, CID du joueur.
    schema!(pub EVENT_ITEM_CONVERT = "AGPMEVENTITEMCONVERT", T::EVENT_ITEM_CONVERT, 1, [f!(Int8), f!(Packet), f!(Int32)]);

    // AgpmRefinery (0x4B) — drapeaux 2 : raffinage à la sphère d'alchimie.
    schema!(pub REFINERY = "AGPMREFINERY", T::REFINERY, 2, [
        f!(Int8),           // 0 opération (5 devis, 2 grille, 6 recette → 1 résultat)
        f!(Int32),          // 1 CID du joueur
        f!(Int32),          // 2 identifiant de recette
        f!(Int32, 20),      // 3 IID des 20 cases de la grille (0 : vide)
        f!(Int32),          // 4 résultat
        f!(Int32),          // 5 TID produit
        f!(Int64),          // 6 prix
    ]);

    // AgpmEventRefinery (0x3F) — drapeaux 1 : op (0 demande, 1 accord), événement, CID du joueur.
    schema!(pub EVENT_REFINERY = "AGPMEVENTREFINERY", T::EVENT_REFINERY, 1, [f!(Int8), f!(Packet), f!(Int32)]);

    // AgpmEventQuest (0x3E) — drapeaux 1 : op, événement, CID, entier.
    schema!(pub EVENT_QUEST = "AGPMEVENTQUEST", T::EVENT_QUEST, 1, [f!(Int8), f!(Packet), f!(Int32), f!(Int32)]);

    // AgpmQuest (0x13) — drapeaux 2 : quêtes (prise, fin, abandon, journal, progression).
    schema!(pub QUEST = "AGPMQUEST", T::QUEST, 2, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 CID
        f!(Packet),         // 2 événement (EVENT_BASE) du PNJ
        f!(Packet),         // 3 quête (mini QUEST_INFO)
        f!(Int32),          // 4 TID de la quête
        f!(Int32),          // 5
        f!(Int32),          // 6 résultat (0 refus, 1 réussi)
        f!(Int32),          // 7 récompense choisie
        f!(MemoryBlock),    // 8 quêtes terminées (bits, TID 0 à 16 383)
        f!(MemoryBlock),    // 9 (TID 16 384 à 32 767)
        f!(MemoryBlock),    // 10 (TID 32 768 à 49 151)
    ]);

    // AgpmEventSkillMaster (0x3B) — drapeaux 1 : instructeur (ouverture, apprentissage, amélioration).
    schema!(pub EVENT_SKILL_MASTER = "AGPMEVENTSKILLMASTER", T::EVENT_SKILL_MASTER, 1, [
        f!(Int8),           // 0 opération (4 ouvrir → 5 ; 1 apprendre → 6 ; 2 améliorer → 3)
        f!(Packet),         // 1 événement (EVENT_BASE)
        f!(Int32),          // 2 CID du joueur
        f!(Int32),          // 3 identifiant de compétence
        f!(Int32),          // 4 TID de compétence
        f!(Int8),           // 5 résultat
        f!(Int8),           // 6 points de compétence
        f!(Int8),           // 7 réinitialisation consommée
    ]);

    // AgpmEventItemRepair (0x34) — drapeaux 1 : réparation chez les forgerons (constructeur FUN_1402b3b50
    // d'Archonia ; op 3 demande → 4 accord ; op 0 réparation → 1 résultat).
    schema!(pub EVENT_ITEM_REPAIR = "AGPMEVENTITEMREPAIR", T::EVENT_ITEM_REPAIR, 1, [
        f!(Int8),           // 0 opération
        f!(Packet),         // 1 événement (EVENT_BASE, fonction 17)
        f!(Int32),          // 2 CID du joueur
        f!(Int32, 20),      // 3 IID des 20 cases de la grille (0 : vide)
        f!(Int64),          // 4 coût total calculé par le client
        f!(Int32),          // 5 code de résultat
    ]);

    // AgpmRide (0x4D) — drapeaux 1 : montures (constructeur FUN_14027ad80 ; le client n'en émet pas).
    schema!(pub RIDE = "AGPMRIDE", T::RIDE, 1, [
        f!(Int8),           // 0 opération (2 monté, 4 descendu, 5 fin de durée, 6 monté sur un TID)
        f!(Int32),          // 1 CID du cavalier
        f!(Int32),          // 2 IID de l'objet monture (op 2) ou TID du modèle de monture (op 6)
        f!(Int32),          // 3 temps restant (s), ignoré par le client
    ]);

    // AgpmAuction (0x0B, constructeur d'Archonia FUN_140268eb0) — drapeaux 2 : maison des ventes.
    schema!(pub AUCTION = "AGPMAUCTION", T::AUCTION, 2, [
        f!(Int8),           // 0 opération (1 ajout, 2 retrait, 3 statut, 4 vente, 5 annulation, 6 encaissement,
                            //   7 achat, 8 avis, 9 demande au PNJ, 10 accord, 11 connexion, 12 recherche)
        f!(Int32),          // 1 CID du joueur
        f!(Packet),         // 2 sous-paquet de l'opération (mini AUCTION_*, EVENT_BASE aux ops 9 et 10)
        f!(Int32),          // 3 résultat (0 réussite)
    ]);

    // AgpmItemSet (0x6A, FUN_14024fb10) — drapeaux 1 : commutateur d'équipement.
    schema!(pub ITEM_SET = "AGPMITEMSET", T::ITEM_SET, 1, [
        f!(Int8),           // 0 opération (0 capturer, 1 équiper, 2 vider, 3 drapeau, 4 actif, 5 contenu,
                            //   6 déplacer, 7 retirer)
        f!(Int32),          // 1 emplacement 0 à 99 (-1 : aucun, op 4)
        f!(Int8),           // 2 drapeau (op 3), case (op 7), `(de << 4) | vers` (op 6)
        f!(Char, 4000),     // 3 objets du jeu, `E,%d,%llu` séparés par `:` (op 5)
    ]);

    // AgpmAreaChatting (0x4C, constructeur d'Archonia FUN_1402b49c0) — drapeaux 1 : cri de zone (op 0 seule).
    schema!(pub AREA_CHATTING = "AGPMAREACHATTING", T::AREA_CHATTING, 1, [
        f!(Int8),           // 0 opération (0 message)
        f!(Int32),          // 1 CID de l'émetteur
        f!(Int8),           // 2 genre de cri (lu, inutilisé par le client)
        f!(Int8),           // 3 (lu, inutilisé)
        f!(UInt32),         // 4 type AgpmChatting du message (onglet et couleur)
        f!(Char, 49),       // 5 nom de l'émetteur
        f!(MemoryBlock),    // 6 texte
    ]);

    // AgpmChannel (0x2C, FUN_1402af5d0) — drapeaux 4 : salons de discussion.
    schema!(pub CHANNEL = "AGPMCHANNEL", T::CHANNEL, 4, [
        f!(Int8),           // 0 opération (1 créer, 3 entrer, 4 sortir, 5 arrivée, 6 départ, 7 invitation,
                            //   8 mot de passe, 9 message, 10 recherche, 11 résultat)
        f!(Int32),          // 1 CID du demandeur
        f!(Char, 41),       // 2 nom du salon
        f!(Char, 9),        // 3 mot de passe
        f!(Char, 33),       // 4 nom de personnage (membre, invitant, cible, auteur)
        f!(Int32),          // 5 genre (1 public, 2 privé, 3 guilde)
        f!(Int32),          // 6 code de résultat (op 11)
        f!(Char, 121),      // 7 texte du message
        f!(MemoryBlock),    // 8 membres `nom1;nom2;` (op 3)
    ]);

    // AgpmEventCharCustomize (0x43, FUN_14022ceb0) — drapeaux 1 : coiffeur.
    schema!(pub EVENT_CHAR_CUSTOMIZE = "AGPMEVENTCHARCUSTOMIZE", T::EVENT_CHAR_CUSTOMIZE, 1, [
        f!(Int8),           // 0 opération (0 demande, 1 accord, 2 achat)
        f!(Packet),         // 1 événement (EVENT_BASE, fonction 29)
        f!(Int32),          // 2 CID du joueur
        f!(Int8),           // 3 visage (-1 : inchangé)
        f!(Int8),           // 4 coiffure (-1 : inchangée)
    ]);

    // AgpmWantedCriminal (0x46, FUN_14028a080) — drapeaux 2 : avis de recherche.
    schema!(pub WANTED_CRIMINAL = "AGPMWANTEDCRIMINAL", T::WANTED_CRIMINAL, 2, [
        f!(Int8),           // 0 opération (0 à 8)
        f!(Int32),          // 1 CID du personnage concerné
        f!(Int32),          // 2 page (ops 3 et 4 ; -100 : fin de page)
        f!(Int32),          // 3 rang (ops 4 et 5)
        f!(Int32),          // 4 résultat (op 2), recherché (op 7), genre de notification (op 8)
        f!(Packet),         // 5 EVENT_BASE (ops 0 et 1) ou mini WANTED_CRIMINAL
    ]);

    // AgpmSummons (0x4F, constructeur d'Archonia FUN_14028f6d0) — drapeaux 2.
    schema!(pub SUMMONS = "AGPMSUMMONS", T::SUMMONS, 2, [
        f!(Int8),           // 0 opération (0 ajout, 1 retrait, 2 propension, 3 limite, 4 limite des apprivoisées)
        f!(Int32),          // 1 CID du maître
        f!(Int32),          // 2 CID de la créature invoquée
        f!(Int32),          // 3 TID de la créature
        f!(Int8),           // 4 type d'invocation (op 0) ou nouvelle limite (ops 3 et 4)
        f!(Int8),           // 5 propension (0 attaque, 1 défense, 2 ombre)
        f!(UInt64),         // 6 horodatage 1 (apparition)
        f!(UInt64),         // 7 horodatage 2 (fin prévue)
        f!(UInt8),          // 8 « forcée » : hors limite (toujours posé par le client)
    ]);

    // AgpmBoss (0x64, constructeur d'Archonia FUN_1402ed3e0) — drapeaux 1 : fenêtre des boss.
    // AgpmSiegeWar (0x49, FUN_14020fe90, réception FUN_140213fc0) — drapeaux 2 : guerres de siège.
    schema!(pub SIEGE_WAR = "AGPMSIEGEWAR", T::SIEGE_WAR, 2, [
        f!(Int8),           // 0 opération (0 à 0x1D)
        f!(Int8),           // 1 résultat (op 2) ou drapeau (ops 0x11/0x13/0x15)
        f!(Packet),         // 2 château (mini SIEGE_CASTLE)
        f!(Int32),          // 3 CID du personnage
        f!(Packet),         // 4 page de guildes (mini SIEGE_GUILD_PAGE)
        f!(Int32),          // 5 CID de l'objet de siège visé
        f!(Packet),         // 6 fiche de l'objet (mini SIEGE_OBJECT_INFO)
        f!(Int32),          // 7 IID d'un objet d'inventaire
        f!(Packet),         // 8 message de siège (mini SIEGE_STATUS_MSG)
        f!(UInt64),         // 9 jamais lu ni écrit par le client
        f!(Packet),         // 10 compte à rebours (mini SIEGE_TIME)
    ]);

    // AgpmEventSiegeWarNPC (0x50, FUN_140284d60) — drapeaux 1 : op 1 demande, op 2 accord.
    schema!(pub EVENT_SIEGEWAR_NPC = "AGPMEVENTSIEGEWARNPC", T::EVENT_SIEGEWAR_NPC, 1, [
        f!(Int8),           // 0 opération
        f!(Packet),         // 1 événement (EVENT_BASE, fonction 33)
        f!(Int32),          // 2 CID du joueur
    ]);

    // AgpmWarInfo (0x61, FUN_140309760) — drapeaux 1 : schéma déclaré ; les rappels lisent le paquet brut
    // (données ajoutées après ces 4 octets, voir `archrust-netclient::siege`).
    schema!(pub WAR_INFO = "AGPMWARINFO", T::WAR_INFO, 1, [f!(Int8), f!(Int8), f!(Int8), f!(Int8)]);

    // AgpmArchlord (0x55, constructeur d'Archonia FUN_140293b50, réception FUN_140293eb0) — drapeaux 2.
    schema!(pub ARCHLORD = "AGPMARCHLORD", T::ARCHLORD, 2, [
        f!(Int8),           // 0 opération (0 à 6 reçues, 7 envoyée)
        f!(Char, 32),       // 1 nom de l'ArchLord
        f!(Char, 32),       // 2 nom du garde
        f!(Int8),           // 3 résultat (lu, non affiché)
        f!(Int32),          // 4 étape de la guerre de l'ArchLord (1 à 4)
        f!(Int32),          // 5 gardes actifs
        f!(Int32),          // 6 gardes maximum
        f!(Int32),          // 7 identifiant de message (op 6)
    ]);

    // AgpmTax (0x53, FUN_14026c110, réception FUN_14026c7b0) — drapeaux 2.
    schema!(pub TAX = "AGPMTAX", T::TAX, 2, [
        f!(Int8),           // 0 opération
        f!(Int32),          // 1 CID du personnage concerné
        f!(Int32),          // 2 résultat
        f!(Packet),         // 3 EVENT_BASE (ops 0 et 1) ou château (mini TAX_CASTLE, ops 2 et 3)
        f!(Packet),         // 4 région 1 (mini TAX_REGION ; seule lue à l'op 4)
        f!(Packet),         // 5 région 2
        f!(Packet),         // 6 région 3
        f!(Packet),         // 7 région 4
    ]);

    schema!(pub BOSS = "AGPMBOSS", T::BOSS, 1, [
        f!(UInt8),          // 0 opération (1 liste, 3 butin, 4 boss choisi, 5 classement, 6 page, 7 dégâts)
        f!(Int8),           // 1 octet recopié à l'op 1 (aucun lecteur)
        f!(Packet, 32),     // 2 boss suivis (mini BOSS_SPAWN), op 1
        f!(Packet),         // 3 butin d'un boss (mini BOSS_DROP), op 3
        f!(Int32),          // 4 TID du boss (ops 4 et 6)
        f!(Packet),         // 5 classement (mini BOSS_RANK), op 5 ; page demandée à l'op 6
        f!(Packet),         // 6 compteur de dégâts (mini BOSS_DPS), op 7
    ]);

    /// Schéma principal d'un type de paquet, s'il est connu.
    pub fn by_id(id: u8) -> Option<&'static Schema> {
        Some(match id {
            T::BOSS => BOSS,
            T::ARCHLORD => ARCHLORD,
            T::TAX => TAX,
            T::SIEGE_WAR => SIEGE_WAR,
            T::EVENT_SIEGEWAR_NPC => EVENT_SIEGEWAR_NPC,
            T::WAR_INFO => WAR_INFO,
            T::SUMMONS => SUMMONS,
            T::AREA_CHATTING => AREA_CHATTING,
            T::CHANNEL => CHANNEL,
            T::EVENT_CHAR_CUSTOMIZE => EVENT_CHAR_CUSTOMIZE,
            T::WANTED_CRIMINAL => WANTED_CRIMINAL,
            T::AUCTION => AUCTION,
            T::ITEM_SET => ITEM_SET,
            T::EVENT_ITEM_REPAIR => EVENT_ITEM_REPAIR,
            T::RIDE => RIDE,
            T::EVENT_SKILL_MASTER => EVENT_SKILL_MASTER,
            T::EVENT_TELEPORT => EVENT_TELEPORT,
            T::EVENT_MANAGER => EVENT_MANAGER,
            T::EVENT_NPC_DIALOG => EVENT_NPC_DIALOG,
            T::EVENT_NPC_TRADE => EVENT_NPC_TRADE,
            T::EVENT_BANK => EVENT_BANK,
            T::ITEM_CONVERT => ITEM_CONVERT,
            T::EVENT_ITEM_CONVERT => EVENT_ITEM_CONVERT,
            T::REFINERY => REFINERY,
            T::EVENT_REFINERY => EVENT_REFINERY,
            T::GUILD => GUILD,
            T::EVENT_GUILD => EVENT_GUILD,
            T::GUILD_WAREHOUSE => GUILD_WAREHOUSE,
            T::UI_STATUS => UI_STATUS,
            T::PVP => PVP,
            T::REMISSION => REMISSION,
            T::PRODUCT => PRODUCT,
            T::EVENT_PRODUCT => EVENT_PRODUCT,
            T::BUDDY => BUDDY,
            T::MAILBOX => MAILBOX,
            T::EVENT_QUEST => EVENT_QUEST,
            T::QUEST => QUEST,
            T::PARTY => PARTY,
            T::PARTY_MANAGEMENT => PARTY_MANAGEMENT,
            T::CONFIG => CONFIG,
            T::ITEM => ITEM,
            T::CHARACTER => CHARACTER,
            T::CHAR_MANAGER => CHAR_MANAGER,
            T::LOGIN => LOGIN,
            T::TIMER => TIMER,
            T::EVENT_NATURE => EVENT_NATURE,
            T::NATURE_EFFECT => NATURE_EFFECT,
            T::WORLD => WORLD,
            T::CHATTING => CHATTING,
            T::SYSTEM_MESSAGE => SYSTEM_MESSAGE,
            T::RETURN_TO_LOGIN => RETURN_TO_LOGIN,
            T::RECONNECT => RECONNECT,
            T::OPT_CHAR_MOVE => OPT_CHAR_MOVE,
            T::OPT_CHAR_ACTION => OPT_CHAR_ACTION,
            T::OPT_VIEW => OPT_VIEW,
            T::STARTUP_ENCRYPTION => STARTUP_ENCRYPTION,
            T::SKILL => SKILL,
            _ => return None,
        })
    }
}

/// Schémas des mini-paquets (identifiants internes, jamais transmis).
pub mod mini {
    use super::*;

    schema!(pub LOGIN_SERVER_ADDR = "AGPMLOGIN_SERVER_ADDR", 100, 1, [f!(Char, 23), f!(Char, 23), f!(Char, 23)]);

    // Désignation dans AGPMSKILL (type d'objet, identifiant) — drapeaux 1.
    schema!(pub SKILL_BASE = "AGPMSKILL_BASE", 150, 1, [f!(Int8), f!(Int32)]);
    // Rémission (AgpmRemission op 2) — drapeaux 1 : type (4 lettres de pardon), résultat.
    schema!(pub REMISSION = "AGPMREMISSION_MINI", 190, 1, [f!(Int8), f!(Int32)]);
    // Criminel d'un avis de recherche (AgpmWantedCriminal champ 5 hors ops 0 et 1) — drapeaux 2 : nom, montant
    // (prime cumulée, prime posée, or gagné ou expérience perdue), niveau, TID du modèle, en ligne.
    schema!(pub WANTED_CRIMINAL = "AGPMWANTEDCRIMINAL_MINI", 210, 2, [f!(Char, 33), f!(Int64), f!(Int32), f!(Int32), f!(Int32)]);
    // Fenêtre des boss (AgpmBoss 0x64, FUN_1402ed3e0) — drapeaux 1 partout.
    // A : `stAgpmBossSpawnData` brut de 48 o (+0 TID, +0x18 prochaine apparition s Unix, +0x20 heure, +0x21 minute,
    // +0x28 vivant).
    schema!(pub BOSS_SPAWN = "AGPMBOSS_SPAWN", 230, 1, [f!(Char, 48)]);
    // B : butin brut de 196 o (+0 TID du boss, +4 16 TID d'objet, +0x44 16 entiers ignorés, +0x84 16 quantités).
    schema!(pub BOSS_DROP = "AGPMBOSS_DROP", 231, 1, [f!(Char, 196)]);
    // C : page demandée, nombre de pages, rang du joueur, nom du joueur, boss tués par le joueur, 16 lignes D.
    schema!(pub BOSS_RANK = "AGPMBOSS_RANK", 232, 1, [f!(Int8), f!(Int8), f!(Int8), f!(Char, 49), f!(Int32), f!(Packet, 16)]);
    // D : rang, nom, victoires sur le boss.
    schema!(pub BOSS_RANK_ROW = "AGPMBOSS_RANK_ROW", 233, 1, [f!(Int8), f!(Char, 49), f!(Int32)]);
    // F : TID du boss, nombre d'entrées (20 au plus), 20 lignes E.
    schema!(pub BOSS_DPS = "AGPMBOSS_DPS", 234, 1, [f!(Int32), f!(Int8), f!(Packet, 20)]);
    // E : nom, TID du modèle, niveau, guilde, marque de guilde, (inconnu), dégâts.
    schema!(pub BOSS_DPS_ROW = "AGPMBOSS_DPS_ROW", 235, 1, [f!(Char, 49), f!(Int32), f!(Int32), f!(Char, 33), f!(Int32), f!(Int32), f!(Int64)]);
    // Guerres de siège (AgpmSiegeWar 0x49). Château (+0x5d24) — drapeaux 2 : nom, nom du propriétaire (≤ 0x30),
    // guilde propriétaire (≤ 0x20), membres de la guilde, dernier siège, prochain siège (FILETIME locaux), statut,
    // résurrection des assaillants, des défenseurs (0/1, 2 absent), drapeau +0xd7c, guilde du dernier carving.
    schema!(pub SIEGE_CASTLE = "AGPMSIEGEWAR_CASTLE", 240, 2, [
        f!(MemoryBlock), f!(MemoryBlock), f!(MemoryBlock), f!(UInt8), f!(UInt64), f!(UInt64), f!(UInt8), f!(UInt8), f!(UInt8), f!(UInt8), f!(MemoryBlock),
    ]);
    // Page de guildes (+0x5eb0) — drapeaux 1 : page (1-based), nombre de pages, 15 lignes SIEGE_GUILD_ENTRY.
    schema!(pub SIEGE_GUILD_PAGE = "AGPMSIEGEWAR_GUILD_PAGE", 241, 1, [f!(Int16), f!(Int16), f!(Packet, 15)]);
    // Ligne (+0x603c) — drapeaux 1 : nom de guilde, nom du chef, points de guilde.
    schema!(pub SIEGE_GUILD_ENTRY = "AGPMSIEGEWAR_GUILD_ENTRY", 242, 1, [f!(MemoryBlock), f!(MemoryBlock), f!(UInt32)]);
    // Fiche d'un objet de siège (+0x61c8) — drapeaux 2 : réparations restantes, coût, durée d'une réparation (min),
    // inconnu, 8 TID chargés (-1 vide), secondes de réparation écoulées, utilisateur ou réparateur.
    schema!(pub SIEGE_OBJECT_INFO = "AGPMSIEGEWAR_OBJECT_INFO", 243, 2, [
        f!(UInt8), f!(Int32), f!(UInt8), f!(UInt8), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(UInt8), f!(MemoryBlock),
    ]);
    // Message de siège (+0x6354) — drapeaux 1 : code, château, guilde, 2e nom, type d'objet, inconnu, côté.
    schema!(pub SIEGE_STATUS_MSG = "AGPMSIEGEWAR_STATUS_MSG", 244, 1, [f!(UInt8), f!(MemoryBlock), f!(MemoryBlock), f!(MemoryBlock), f!(UInt8), f!(UInt32), f!(UInt8)]);
    // Compte à rebours (+0x64e0) — drapeaux 1 : genre (1 avant le début, 2 avant la fin), secondes.
    schema!(pub SIEGE_TIME = "AGPMSIEGEWAR_TIME", 245, 1, [f!(UInt8), f!(UInt16)]);
    // Taxes (AgpmTax 0x53). Château (+0x1874) — drapeaux 2 : nom, recette totale, date du dernier versement
    // (time_t), date de la dernière modification des taux (time_t).
    schema!(pub TAX_CASTLE = "AGPMTAX_CASTLE", 246, 2, [f!(Char, 128), f!(Int64), f!(UInt64), f!(UInt64)]);
    // Région (+0x1a00) — drapeaux 2 : nom (traduit par le glossaire du joueur), recette, taux (%).
    schema!(pub TAX_REGION = "AGPMTAX_REGION", 247, 2, [f!(Char, 33), f!(Int64), f!(Int32)]);
    // Maison des ventes (AgpmAuction champ 2, constructeur FUN_140268eb0 d'Archonia).
    // Vente du joueur — drapeaux 2 : salesID, docID, n° d'objet en base, IID, prix or, prix CC, quantité, statut
    // (0 en vente, 1 vendu), date d'enregistrement, TID.
    schema!(pub AUCTION_SALES = "AGPMAUCTION_SALES", 192, 2, [
        f!(Int32), f!(UInt64), f!(UInt64), f!(Int32), f!(Int64), f!(Int64), f!(Int16), f!(Int16), f!(Char, 33), f!(Int32),
    ]);
    // Recherche — drapeaux 1 : TID, docID (curseur), sens (0 docID décroissants sous le curseur), total, rowset.
    schema!(pub AUCTION_SELECT2 = "AGPMAUCTION_SELECT2", 193, 1, [f!(Int32), f!(UInt64), f!(Int16), f!(Int32), f!(Packet)]);
    // Mise en vente — drapeaux 1 : IID, quantité, prix or, prix CC, date, IID du parchemin Marvel (0 sans).
    schema!(pub AUCTION_SELL = "AGPMAUCTION_SELL", 194, 1, [f!(Int32), f!(Int16), f!(Int64), f!(Int64), f!(Char, 33), f!(Int32)]);
    // Annulation — drapeaux 1 : salesID, TID, quantité.
    schema!(pub AUCTION_CANCEL = "AGPMAUCTION_CANCEL", 195, 1, [f!(Int32), f!(Int32), f!(Int16)]);
    // Encaissement — drapeaux 1 : salesID, TID, quantité vendue, gain or, gain CC.
    schema!(pub AUCTION_CONFIRM = "AGPMAUCTION_CONFIRM", 196, 1, [f!(Int32), f!(Int32), f!(Int16), f!(Int64), f!(Int64)]);
    // Achat — drapeaux 1 : docID, quantité, TID, prix or, prix CC.
    schema!(pub AUCTION_BUY = "AGPMAUCTION_BUY", 197, 1, [f!(Int64), f!(Int16), f!(Int32), f!(Int64), f!(Int64)]);
    // Résultat d'une requête — drapeaux 2 : index, texte de la requête, en-têtes, lignes, colonnes, taille d'une
    // ligne, tampon des lignes, décalages des colonnes (U32).
    schema!(pub AUCTION_ROWSET = "AGPMAUCTION_ROWSET", 198, 2, [
        f!(Int32), f!(MemoryBlock), f!(MemoryBlock), f!(UInt32), f!(UInt32), f!(UInt32), f!(MemoryBlock), f!(MemoryBlock),
    ]);
    // Courrier (AgpmMailBox champ 2) — drapeaux 2 : ID, expéditeur, destinataire, objet, date (s Unix), drapeaux
    // (0x01 lu, 0x10 pièce récupérée), TID et quantité de la pièce jointe, texte (NUL final compris).
    schema!(pub MAIL = "AGPMMAILBOX_MAIL", 191, 2, [
        f!(Int32), f!(Char, 33), f!(Char, 33), f!(Char, 51), f!(Int32), f!(Int32), f!(Int32), f!(Int16), f!(MemoryBlock),
    ]);
    // Action d'une compétence — drapeaux 2 : type (0 demande, 1 lancement, 2 résultat), cible
    // (SKILL_BASE), facteurs du résultat, position visée, attaque forcée, incantation, durée et relance
    // (ms), niveau, facteurs à mettre en file, effet supplémentaire, CID des cibles.
    schema!(pub SKILL_ACTION = "AGPMSKILL_ACTION", 151, 2, [
        f!(Int8), f!(Packet), f!(Packet), f!(Vec3F), f!(Int8), f!(UInt32), f!(UInt32), f!(UInt32), f!(UInt8), f!(UInt8), f!(UInt32), f!(MemoryBlock),
    ]);
    // Données d'un maître des compétences (fonction 25 d'un PNJ) : race et classe enseignées.
    schema!(pub SKILL_MASTER_DATA = "AGPMEVENTSKILLMASTER_DATA", 153, 1, [f!(Int8), f!(Int8)]);
    // Effet actif d'une compétence — drapeaux 2 : TID, durée (ms), TID du lanceur, …
    schema!(pub SKILL_BUFF = "AGPMSKILL_BUFF", 152, 2, [
        f!(Int32), f!(UInt64), f!(Int32), f!(UInt8), f!(UInt64), f!(MemoryBlock),
    ]);

    schema!(pub LOGIN_CHAR_INFO = "AGPMLOGIN_CHAR_INFO", 101, 2, [
        f!(Int32),          // 0 TID
        f!(Char, 49),       // 1 nom
        f!(Int32),          // 2 nombre de personnages
        f!(Int32),          // 3 index de slot
        f!(Int32),          // 4 union / race courante
        f!(Int32),          // 5 race
        f!(Int32),          // 6 cheveux
        f!(Int32),          // 7 visage
        f!(Char, 49),       // 8
        f!(Int32),          // 9 (nouveau dans le client x64)
    ]);

    schema!(pub LOGIN_SERVER_INFO = "AGPMLOGIN_SERVER_INFO", 102, 1, [f!(Char, 23)]);
    schema!(pub LOGIN_VERSION_INFO = "AGPMLOGIN_VERSION_INFO", 103, 1, [f!(Int32), f!(Int32)]);
    schema!(pub LOGIN_COMPEN_INFO = "AGPMLOGIN_COMPEN_INFO", 104, 1, [f!(Int32), f!(Int32), f!(Char, 49), f!(Char, 100), f!(MemoryBlock)]);

    // Déplacement (constructeur AgpmCharacter) — drapeaux 1
    schema!(pub CHAR_MOVE = "AGPMCHAR_MOVE", 105, 1, [
        f!(Vec3F),          // 0 position
        f!(Vec3F),          // 1 destination
        f!(Int32),          // 2 cible suivie
        f!(Int32),          // 3 distance de suivi
        f!(Float),          // 4 axe X
        f!(Float),          // 5 axe Y
        f!(Int8),           // 6 drapeaux
        f!(Int8),           // 7 direction
    ]);

    schema!(pub CHAR_ACTION = "AGPMCHAR_ACTION", 106, 2, [
        f!(Int8), f!(Int32), f!(Int32), f!(Int8), f!(Packet), f!(Vec3F), f!(UInt8), f!(Int8), f!(UInt32), f!(UInt8),
    ]);

    schema!(pub CHAR_FACTORS = "AGPMCHAR_FACTORS", 107, 2, [
        f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet),
        f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet),
    ]);
    schema!(pub FACTOR_STATUS = "AGPMCHAR_FACTOR_STATUS", 108, 2, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);
    schema!(pub FACTOR_TYPE = "AGPMCHAR_FACTOR_TYPE", 109, 1, [f!(Int32), f!(Int32), f!(Int32)]);
    schema!(pub FACTOR_POINT = "AGPMCHAR_FACTOR_POINT", 110, 4, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);
    schema!(pub FACTOR_POINTMAX = "AGPMCHAR_FACTOR_POINTMAX", 111, 2, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);
    schema!(pub FACTOR_RECOVERY = "AGPMCHAR_FACTOR_RECOVERY", 112, 1, [f!(Int32), f!(Int32), f!(Int32)]);
    schema!(pub FACTOR_ATTRIBUTE = "AGPMCHAR_FACTOR_ATTRIBUTE", 113, 2, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);
    schema!(pub FACTOR_DMG = "AGPMCHAR_FACTOR_DMG", 114, 1, [f!(Packet), f!(Packet)]);
    schema!(pub FACTOR_DEFENSE = "AGPMCHAR_FACTOR_DEFENSE", 115, 1, [f!(Packet), f!(Packet)]);
    schema!(pub FACTOR_ATTACK = "AGPMCHAR_FACTOR_ATTACK", 116, 1, [f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32)]);
    schema!(pub FACTOR_ITEM = "AGPMCHAR_FACTOR_ITEM", 117, 1, [f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32)]);
    schema!(pub FACTOR_DIRT = "AGPMCHAR_FACTOR_DIRT", 118, 1, [f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32)]);
    schema!(pub FACTOR_PRICE = "AGPMCHAR_FACTOR_PRICE", 119, 1, [f!(Int32), f!(Int32), f!(Int32), f!(Int32)]);

    // AgpmOptimizedPacket2 : vue d'un personnage — drapeaux 4
    schema!(pub OPT_VIEW_CHAR = "AGPMOPTIMIZEDPACKET_VIEW", 120, 4, [
        f!(Int32),          // 0 CID
        f!(Int32),          // 1 TID
        f!(MemoryBlock),    // 2 nom
        f!(Int8), f!(Int8),
        f!(Packet),         // 5 facteurs
        f!(Int8), f!(Int8),
        f!(Vec3F),          // 8 position
        f!(Vec3F),          // 9 destination
        f!(Int32), f!(UInt16), f!(Int16), f!(Int16), f!(UInt64), f!(Int8), f!(Int8), f!(Int8),
        f!(Int8), f!(UInt16), f!(Int8), f!(Int32), f!(Char, 16), f!(Int32), f!(Int32), f!(Int32),
    ]);

    // AgpmOptimizedPacket2 : vue d'un objet — drapeaux 4
    schema!(pub OPT_VIEW_ITEM = "AGPMOPTIMIZEDPACKET_ITEMVIEW", 121, 4, [
        f!(Int32), f!(Int32), f!(Int16), f!(Int8), f!(Int8), f!(Int8),
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
        f!(UInt16), f!(UInt16), f!(UInt16), f!(UInt16), f!(UInt16), f!(MemoryBlock),
    ]);

    // Position dans l'inventaire / banque : trois INT16 (ap_item.c).
    schema!(pub ITEM_GRID = "AGPMITEM_INVENTORY", 123, 1, [f!(Int16), f!(Int16), f!(Int16)]);

    // Objet au sol (statut 1) : position, un seul VEC3F (champ 6 d'AGPMITEM).
    schema!(pub ITEM_FIELD = "AGPMITEM_FIELD", 124, 1, [f!(Vec3F)]);

    // TID de ce qui remplit les sockets d'un objet (AGPMITEMCONVERT champ 9), dans l'ordre — drapeaux 1.
    schema!(pub ITEM_CONVERT_TIDS = "AGPMITEMCONVERT_TID", 141, 1, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);

    // Options d'un objet (AGPMITEM champ 20) : TID de 5 options d'itemoptiontable.txt au plus — drapeaux 1.
    schema!(pub ITEM_OPTION = "AGPMITEM_OPTION", 142, 1, [f!(UInt16), f!(UInt16), f!(UInt16), f!(UInt16), f!(UInt16)]);

    // Désignation d'un événement : type de la source (1 objet du décor, 3 personnage), identifiant (section
    // de l'objet, CID du PNJ), identifiant d'événement, fonction (8 portail, 21 dialogue, 9 boutique, 20 banque,
    // 25 compétences, 27 quêtes).
    schema!(pub EVENT_BASE = "APMEVENTMANAGER_BASE", 60, 1, [f!(Int8), f!(Int32), f!(Int32), f!(Int32)]);

    // Données d'une fonction réduites à un entier (modèle de dialogue, de boutique, groupe de quêtes).
    schema!(pub EVENT_DATA_INT32 = "APMEVENT_DATA_INT32", 61, 1, [f!(Int32)]);

    // Détail d'un objet de boutique — drapeaux 2 : TID, prix, quantité, statut, sac, ligne,
    // colonne, puis trois entiers.
    schema!(pub NPC_TRADE_DETAIL = "AGCMEVENTNPCTRADE_DETAIL", 125, 2, [
        f!(Int32), f!(Int32), f!(Int32), f!(Int16), f!(Int16), f!(Int16), f!(Int16), f!(Int32), f!(Int32), f!(Int32),
    ]);

    // Quête du journal — drapeaux 1 : TID, monstres A et B tués, objectifs remplis, valeur, texte.
    schema!(pub QUEST_INFO = "AGPMQUEST_INFO", 154, 1, [f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Char, 49)]);

    // Membre d'une guilde (champ 16 d'AGPMGUILD) — drapeaux 1 : nom, rang (1 membre, 2 demande d'adhésion,
    // 3 demande de départ, 10 chef, 11 second), date d'adhésion, niveau, TID, en ligne (0 ou 1).
    schema!(pub GUILD_MEMBER = "AGPMGUILD_MEMBER", 155, 1, [f!(Char, 49), f!(Int32), f!(Int64), f!(UInt64), f!(Int32), f!(Int8)]);

    // Liste des guildes (champ 21 d'AGPMGUILD, op 28) — drapeaux 4 : nombre total de guildes, page, nombre de la
    // page (INT16), puis 19 entrées GUILD_LIST_ITEM (FUN_140203630 d'Archonia).
    schema!(pub GUILD_LIST = "AGPMGUILD_LIST", 156, 4, [
        f!(Int32), f!(Int32), f!(Int16),
        f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet),
        f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet), f!(Packet),
    ]);

    // Combat de guildes (champ 17 d'AGPMGUILD) — drapeaux 2 : guilde adverse, chef adverse, acceptation, délai,
    // début (secondes), type, durée (s), nombre de combattants, horloge, score, score adverse, gain, gain
    // adverse, résultat, manche.
    schema!(pub GUILD_BATTLE = "AGPMGUILD_BATTLE", 158, 2, [
        f!(Char, 33), f!(Char, 49), f!(UInt64), f!(UInt64), f!(UInt64), f!(UInt32), f!(UInt32), f!(UInt32), f!(UInt64),
        f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int8), f!(UInt32),
    ]);

    // Entrée de la liste des guildes — drapeaux 2 : nom, chef, second, niveau, (inconnu), membres, taille,
    // (inconnu), emblème (TID, couleur), points de combat.
    schema!(pub GUILD_LIST_ITEM = "AGPMGUILD_LIST_ITEM", 157, 2, [
        f!(Char, 33), f!(Char, 49), f!(Char, 49), f!(Int32), f!(UInt32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32), f!(Int32),
    ]);
}

/// Opérations (champ 0) du paquet de connexion (AgpmLogin).
pub mod login_op {
    pub const HASHKEY: i8 = 0;
    pub const USER_LOGIN: i8 = 1;
    pub const UNION_INFO: i8 = 3;
    pub const CHAR_NAME: i8 = 4;
    pub const CHAR_NAME_FINISH: i8 = 5;
    pub const CHAR_LIST: i8 = 6;
    pub const CHAR_LIST_FINISH: i8 = 7;
    pub const WORLD_ENTER: i8 = 8;
    pub const CHAR_CREATION_ENTER: i8 = 10;
    pub const CREATE_CHAR: i8 = 11;
    pub const CREATE_CHAR_FINISH: i8 = 12;
    pub const DELETE_CHAR: i8 = 13;
    pub const LOGIN_RESULT: i8 = 15;
}

pub mod login_result {
    pub const INCORRECT_USER: i32 = 0;
    pub const GIBBERISH: i32 = 1;
    pub const TOO_MANY_ATTEMPTS: i32 = 2;
    pub const TOS_AGREEMENT: i32 = 3;
    pub const NAME_TAKEN: i32 = 7;
}

/// Opérations de la poignée de main (AgpmStartupEncryption).
pub mod crypt_op {
    pub const REQUEST: i8 = 0;
    pub const PUBLIC_KEY: i8 = 1;
    pub const PRIVATE_KEY: i8 = 2;
    pub const COMPLETE: i8 = 3;
    pub const DYNCODE_PUB: i8 = 4;
    pub const DYNCODE_PRV: i8 = 5;
    pub const SET_ALGO: i8 = 6;
}

pub mod world_op {
    pub const SERVER_LIST_REQ: i8 = 0;
    pub const SERVER_LIST_ANS: i8 = 2;
    pub const SERVER_EXTRA: i8 = 3;
}

/// Opérations du gestionnaire de personnages (type 0x03).
pub mod char_mgr_op {
    /// client → monde : champ 2 = 17, nom (3), clé d'auth (6), CHAR[248] (7).
    pub const GAME_ENTER: i8 = 2;
    pub const FINISH_ENTER: i8 = 7;
    pub const LOADING_POS: i8 = 8;
    /// client → monde après LOADING_POS : CID au champ 4.
    pub const ENTER_WORLD: i8 = 9;
    /// login → client : clé pour le serveur monde.
    pub const AUTH_KEY: i8 = 11;
    /// client → monde toutes les 15 s, champ 2.
    pub const KEEPALIVE: i8 = 13;
}

/// Opérations de la vue optimisée (type 0x42).
pub mod view_op {
    pub const ADD_CHAR: i8 = 1;
    pub const ADD_CHARACTER_ITEMS: i8 = 2;
}

/// Opérations `AGPMCHARACTER` (type 0x02), relevées dans `ap_character.h`.
pub mod character_op {
    pub const ADD: i8 = 0;
    pub const UPDATE: i8 = 1;
    pub const REMOVE: i8 = 2;
    pub const REMOVE_FOR_VIEW: i8 = 3;
    /// Montée de niveau (serveur → clients).
    pub const LEVEL_UP: i8 = 5;
    /// Demande de résurrection au village (client → serveur).
    pub const RESURRECT_TOWN: i8 = 9;
    /// Drapeaux d'options (refus d'échange, de groupe…) au champ 22 (client → serveur,
    /// `FUN_14017ae50` d'Archonia).
    pub const UPDATE_OPTION_FLAG: i8 = 0x11;
}

/// Opérations d'AgpmItem utiles au butin.
pub mod item_op {
    pub const ADD: u8 = 0;
    pub const REMOVE: u8 = 1;
    pub const UPDATE: u8 = 2;
    /// Demande de ramassage d'un objet au sol (client → serveur, IID au champ 2).
    pub const PICKUP: u8 = 11;
    /// Résultat du ramassage (champ 1 : 0 réussi, 1 échec).
    pub const PICKUP_RESULT: u8 = 12;
    /// Achat d'une page de banque (client → serveur, CID au champ 4).
    pub const BUY_BANK_SLOT: u8 = 22;
}

/// Statut AgpmItem d'un objet posé au sol.
pub const ITEM_STATUS_FIELD: i8 = 1;

/// Statut d'action d'un personnage (AGPMCHARACTER champ 11, vue champ 3).
pub mod action_status {
    pub const NORMAL: i8 = 0;
    pub const DEAD: i8 = 2;
}
