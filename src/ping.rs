//! Pings: cast on the client, resolved and relayed by the server to everyone, kept and expired
//! on each client.
use crate::lock;
use crate::payloads::{Marker, PingKind, PingSend, SEND, SHOW};
use mod_api::{ClientHost, Host, HostBlockStates, IPayloadContext, MobCategory, VarInt};
#[allow(unused_imports)]
use mod_api::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The farthest a ping reaches, in blocks.
pub const REACH: f64 = 128.0;
/// How long a marker stays, in client ticks (30 s).
pub const LIFETIME: u32 = 600;
/// A ping this close to your own marker takes it away instead.
const SAME_SPOT: f64 = 1.5;
/// How far from the ray an entity may be and still be the one pinged, in blocks.
const ENTITY_SLACK: f64 = 1.0;

/// The slack for the entity ray: none when the player aimed at a block and the client saw
/// loot there, so a mob standing beside the ore does not take the ping; else `ENTITY_SLACK`,
/// so a far mob is easy to hit.
pub fn entity_slack(ping: &PingSend) -> f64 {
    if ping.loot { 0.0 } else { ENTITY_SLACK }
}

/// A marker on this client and the tick it arrived.
pub struct Shown {
    pub marker: Marker,
    pub since: u32,
}

/// The markers this client shows: at most one per teammate (a new ping replaces the old one).
pub static MARKERS: Mutex<Vec<Shown>> = Mutex::new(Vec::new());
/// A marker arrived since the last tick: play the ping sound once.
static RANG: AtomicBool = AtomicBool::new(false);

/// The unit view direction from yaw and pitch in degrees, as Minecraft's `calculateViewVector`.
pub fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = ((yaw as f64).to_radians(), (pitch as f64).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

pub use crate::kit::near::distance;

/// An ore or a block that keeps something: a block entity is the game's own sign of that.
fn is_loot(host: &dyn ClientHost, block: mod_api::BlockKey, id: &str) -> bool {
    let has_entity = host
        .block_state_default(block)
        .and_then(|state| host.block_states().has_block_entity(state))
        == Some(true);
    has_entity || id.ends_with("_ore") || id == "minecraft:ancient_debris"
}

/// Key pressed: sends the ray the player looks along, to the block it hits or to `REACH`.
pub fn cast(host: &mut dyn ClientHost) {
    let Some(camera) = host.camera() else { return };
    let origin = camera.position;
    let dir = view_vector(camera.yaw, camera.pitch);
    let far = [0, 1, 2].map(|i| origin[i] + dir[i] * REACH);
    // The view's first block, in the loaded world and past it over the distant terrain.
    let hit = host.raycast(REACH as f32).map(|h| ([h.x, h.y, h.z], h.block));
    let (to, block, loot) = match hit {
        Some((at, key)) => {
            let id = key.id(&*host);
            let loot = is_loot(&*host, key, &id);
            (at, Some(id), loot)
        }
        None => (far, None, false),
    };
    let ping = PingSend {
        ox: origin[0],
        oy: origin[1],
        oz: origin[2],
        x: to[0],
        y: to[1],
        z: to[2],
        block,
        loot,
    };
    if let Some(send) = SEND.get() {
        let _ = send.send(host, &ping);
    }
}

/// Server: finds the entity on the ray (the nearest one within `ENTITY_SLACK` of it, before the
/// block), picks the kind, stamps the sender, and sends the marker to every player.
pub fn relay(ping: PingSend, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let player = cx.player().map_err(|e| format!("{e:?}"))?;
    let host = cx.server_host().map_err(|e| format!("{e:?}"))?;
    let origin = [ping.ox, ping.oy, ping.oz];
    let to = [ping.x, ping.y, ping.z];
    let length = distance(origin, to);
    if !(1e-6..=REACH + 2.0).contains(&length) {
        return Ok(());
    }
    let marker = {
        let read: &dyn Host = &*host;
        let owner = player.name(read).unwrap_or_default();
        let dimension = player.dimension(read).map(|d| d.to_string()).unwrap_or_default();
        // The game's own entity ray (`ProjectileUtil.getEntityHitResult`), each box grown by
        // `ENTITY_SLACK` so a far mob is easy to hit; the nearest one wins.
        let hit = player
            .level(read)
            .and_then(|level| level.ray_trace_entities(read, origin, to, entity_slack(&ping), Some(player.entity())))
            .and_then(|hits| hits.into_iter().min_by(|a, b| a.distance.total_cmp(&b.distance)));
        match hit {
            Some(hit) => {
                let (entity, at) = (hit.entity, hit.location);
                let hostile = entity.classification(read) == Some(MobCategory::Monster);
                Marker {
                    owner,
                    kind: if hostile { PingKind::Danger } else { PingKind::Entity },
                    x: at[0],
                    y: at[1],
                    z: at[2],
                    entity: Some(VarInt(entity.id() as i32)),
                    block: None,
                    dimension,
                }
            }
            None => Marker {
                owner,
                kind: if ping.loot { PingKind::Loot } else { PingKind::Place },
                x: to[0],
                y: to[1],
                z: to[2],
                entity: None,
                block: ping.block,
                dimension,
            },
        }
    };
    let Some(show) = SHOW.get() else { return Ok(()) };
    for to in host.players().server_players() {
        let _ = show.send(host, to, &marker);
    }
    Ok(())
}

/// Puts a marker in the list: it replaces its owner's previous one, or, when it lands on that
/// one (same spot or same entity), takes it away.
pub fn place(markers: &mut Vec<Shown>, marker: Marker, now: u32) {
    let previous = markers.iter().position(|s| s.marker.owner == marker.owner);
    if let Some(i) = previous {
        let old = &markers.remove(i).marker;
        let same_entity = old.entity.is_some() && old.entity.map(|e| e.0) == marker.entity.map(|e| e.0);
        let gap = distance([old.x, old.y, old.z], [marker.x, marker.y, marker.z]);
        if same_entity || (marker.entity.is_none() && gap <= SAME_SPOT) {
            return;
        }
    }
    markers.push(Shown { marker, since: now });
}

/// Client: a marker arrived.
pub fn receive(marker: Marker, _cx: &mut dyn IPayloadContext) -> Result<(), String> {
    place(&mut lock(&MARKERS), marker, crate::now());
    RANG.store(true, Ordering::Relaxed);
    Ok(())
}

/// Client, each tick: drops markers older than `LIFETIME` and ones whose entity is gone, and
/// plays the ping sound for new ones.
pub fn expire(host: &dyn ClientHost) {
    let now = crate::now();
    lock(&MARKERS).retain(|s| {
        now.saturating_sub(s.since) < LIFETIME
            && s.marker.entity.is_none_or(|e| host.entity(e.0 as u32).is_some())
    });
    if RANG.swap(false, Ordering::Relaxed) && crate::settings::on(host, crate::settings::SOUND) {
        let mut sound = mod_api::SoundPlay::ui("minecraft:block.amethyst_block.chime");
        sound.volume = 0.6;
        let _ = host.play_local_sound(&sound, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(owner: &str, at: [f64; 3], entity: Option<i32>) -> Marker {
        Marker {
            owner: owner.into(),
            kind: PingKind::Place,
            x: at[0],
            y: at[1],
            z: at[2],
            entity: entity.map(VarInt),
            block: None,
            dimension: "minecraft:overworld".into(),
        }
    }

    #[test]
    fn a_loot_ping_takes_the_block_not_a_mob_beside_it() {
        let ping = |loot| PingSend { ox: 0.0, oy: 0.0, oz: 0.0, x: 1.0, y: 0.0, z: 0.0, block: Some("minecraft:diamond_ore".into()), loot };
        assert_eq!(entity_slack(&ping(true)), 0.0);
        assert_eq!(entity_slack(&ping(false)), ENTITY_SLACK);
    }

    #[test]
    fn the_view_vector_matches_minecraft() {
        let south = view_vector(0.0, 0.0);
        assert!((south[2] - 1.0).abs() < 1e-9);
        let down = view_vector(0.0, 90.0);
        assert!((down[1] + 1.0).abs() < 1e-9);
        let west = view_vector(90.0, 0.0);
        assert!((west[0] + 1.0).abs() < 1e-9);
    }

    #[test]
    fn one_marker_per_owner_and_a_second_ping_takes_it_away() {
        let mut list = Vec::new();
        place(&mut list, marker("a", [0.0, 64.0, 0.0], None), 1);
        place(&mut list, marker("b", [9.0, 64.0, 0.0], None), 1);
        place(&mut list, marker("a", [20.0, 64.0, 0.0], None), 2);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].marker.x, 20.0);
        place(&mut list, marker("a", [20.5, 64.0, 0.0], None), 3);
        assert_eq!(list.len(), 1);
        place(&mut list, marker("b", [0.0, 0.0, 0.0], Some(7)), 4);
        place(&mut list, marker("b", [3.0, 0.0, 0.0], Some(7)), 5);
        assert!(list.is_empty());
    }
}
