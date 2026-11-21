//! What crosses the network: a ping going up to the server, a marker coming down to every
//! teammate, and the roster the server sends each player. Registered (version "1", optional) in
//! `Hooks::register`: on a server without the mod, pings and cards stay empty.
use mod_api::{PayloadCodec, ToClient, ToServer, VarInt};
use std::sync::OnceLock;

/// What a ping points at, which picks its look.
#[derive(PayloadCodec, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PingKind {
    /// A block or a spot: "here".
    Place,
    /// A hostile mob: "danger", red. The marker follows the mob.
    Danger,
    /// Any other entity (an animal, a villager, a player). The marker follows it.
    Entity,
    /// An ore or a block with storage: "look at this", with the block's icon.
    Loot,
}

/// Client → server: the ray the player pinged along. The server finds the entity on it, if any
/// (it knows every entity; the client only the near ones), and adds who sent it.
#[derive(PayloadCodec, Clone)]
pub struct PingSend {
    /// The eye the ray starts from.
    pub ox: f64,
    pub oy: f64,
    pub oz: f64,
    /// Where the ray ends: the block it hit, or its full reach.
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The block hit, when the ray hit one (`minecraft:diamond_ore`).
    pub block: Option<String>,
    /// Whether that block is loot (an ore or a block with storage), as the client sees it.
    pub loot: bool,
}

/// Server → every client: a marker to show.
#[derive(PayloadCodec, Clone)]
pub struct Marker {
    /// Who pinged: the colour comes from it.
    pub owner: String,
    pub kind: PingKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The entity pinged (its network id), so the marker follows it.
    pub entity: Option<VarInt>,
    /// The block pinged, for the loot icon.
    pub block: Option<String>,
    /// The dimension it was pinged in (`minecraft:overworld`): shown only there.
    pub dimension: String,
}

/// One teammate as the server sees them.
#[derive(PayloadCodec, Clone)]
pub struct Member {
    pub name: String,
    pub health: f32,
    pub max_health: f32,
    /// Food level, 0 to 20.
    pub food: i32,
    /// Armour points, 0 to 20 (and beyond with mods).
    pub armor: i32,
    /// Experience level.
    pub level: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dimension: String,
}

/// Server → one client: everyone else on the team.
#[derive(PayloadCodec, Clone)]
pub struct Roster {
    pub members: Vec<Member>,
}

pub static SEND: OnceLock<ToServer<PingSend>> = OnceLock::new();
pub static SHOW: OnceLock<ToClient<Marker>> = OnceLock::new();
pub static ROSTER: OnceLock<ToClient<Roster>> = OnceLock::new();

/// Keeps the registered tokens for the senders.
pub fn keep(send: ToServer<PingSend>, show: ToClient<Marker>, roster: ToClient<Roster>) {
    let _ = SEND.set(send);
    let _ = SHOW.set(show);
    let _ = ROSTER.set(roster);
}
