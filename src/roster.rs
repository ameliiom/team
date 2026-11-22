//! The roster: the server sends each player the others' state; the client keeps it and notices
//! who just got hurt (health lower than last time) for the red flash.
use crate::lock;
use crate::payloads::{Member, Roster, ROSTER};
use mod_api::{Host, IPayloadContext, PlayerRef, ServerHost, ServerTickEvent, ServerTickPhase};
#[allow(unused_imports)]
use mod_api::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// How often the server sends the roster, in server ticks (4 times a second).
pub const EVERY: u32 = 5;
/// How long a hurt card flashes, in client ticks.
pub const FLASH: u32 = 10;

/// A teammate on this client, with the client tick their health last dropped.
pub struct Mate {
    pub member: Member,
    pub hurt_at: Option<u32>,
}

pub static MATES: Mutex<Vec<Mate>> = Mutex::new(Vec::new());
static SERVER_TICKS: AtomicU32 = AtomicU32::new(0);

/// One player as a `Member`, or none when the server can't read them (dead, leaving).
fn member(read: &dyn Host, player: PlayerRef) -> Option<Member> {
    let name = player.name(read)?;
    let entity = player.entity();
    let [x, y, z] = entity.position(read)?;
    let living = read.living(entity)?;
    Some(Member {
        name,
        health: living.health(read)?,
        max_health: living.max_health(read)?,
        food: player.food(read).map(|f| f.level).unwrap_or(20),
        armor: living.attribute_value(read, "minecraft:armor").unwrap_or(0.0).round() as i32,
        level: player.experience(read).map(|e| e.level).unwrap_or(0),
        x,
        y,
        z,
        dimension: player.dimension(read).map(|d| d.to_string()).unwrap_or_default(),
    })
}

/// Server, every `EVERY` ticks: each player gets every other player's `Member`.
pub fn broadcast(host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
    if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
        return;
    }
    if !SERVER_TICKS.fetch_add(1, Ordering::Relaxed).is_multiple_of(EVERY) {
        return;
    }
    let Some(token) = ROSTER.get() else { return };
    let players = host.players().server_players();
    let members: Vec<(PlayerRef, Option<Member>)> = {
        let read: &dyn Host = &*host;
        players.iter().map(|p| (*p, member(read, *p))).collect()
    };
    for (to, _) in &members {
        let others = members
            .iter()
            .filter(|(p, _)| p != to)
            .filter_map(|(_, m)| m.clone())
            .collect();
        // A client without the mod refuses the payload: it just gets no cards.
        let _ = token.send(host, *to, &Roster { members: others });
    }
}

/// Merges a new roster into the old one: each mate keeps its hurt time, which is set to `now`
/// when its health dropped.
pub fn merge(old: &[Mate], new: Vec<Member>, now: u32) -> Vec<Mate> {
    new.into_iter()
        .map(|member| {
            let before = old.iter().find(|m| m.member.name == member.name);
            let dropped = before.is_some_and(|b| member.health < b.member.health);
            let hurt_at = if dropped { Some(now) } else { before.and_then(|b| b.hurt_at) };
            Mate { member, hurt_at }
        })
        .collect()
}

/// Client: replaces the mates.
pub fn receive(roster: Roster, _cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let mut mates = lock(&MATES);
    let now = crate::now();
    *mates = merge(&mates, roster.members, now);
    Ok(())
}

/// Test builds only (`TEAM_DEMO` set at build time): two made-up mates around the camera, one
/// circling near and getting hurt every few seconds, one in the Nether, so the cards, arrows and
/// flash can be checked with one player. A normal build compiles this to nothing.
pub fn demo(host: &dyn mod_api::ClientHost) {
    if option_env!("TEAM_DEMO").is_none() {
        return;
    }
    // Where the camera last was: a full-screen map draws no world and has no camera.
    static LAST: Mutex<Option<([f64; 3], String)>> = Mutex::new(None);
    if let Some(camera) = host.camera() {
        *lock(&LAST) = Some((camera.position, camera.dimension.clone()));
    }
    let Some((position, dimension)) = lock(&LAST).clone() else { return };
    let now = crate::now();
    let turn = now as f64 / 40.0;
    let [x, y, z] = position;
    let near = Member {
        name: "Friend".into(),
        health: 20.0 - ((now / 60) % 4) as f32 * 4.0,
        max_health: 20.0,
        food: 14,
        armor: 12,
        level: 23,
        x: x + 30.0 * turn.cos(),
        y,
        z: z + 30.0 * turn.sin(),
        dimension,
    };
    let far = Member {
        name: "Explorer".into(),
        health: 9.0,
        max_health: 20.0,
        food: 6,
        armor: 0,
        level: 7,
        x,
        y,
        z,
        dimension: "minecraft:the_nether".into(),
    };
    let mut mates = lock(&MATES);
    *mates = merge(&mates, vec![near, far], now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, health: f32) -> Member {
        Member {
            name: name.into(),
            health,
            max_health: 20.0,
            food: 20,
            armor: 0,
            level: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            dimension: "minecraft:overworld".into(),
        }
    }

    #[test]
    fn a_health_drop_marks_the_mate_hurt() {
        let first = merge(&[], vec![m("a", 20.0), m("b", 20.0)], 1);
        assert!(first.iter().all(|x| x.hurt_at.is_none()));
        let second = merge(&first, vec![m("a", 14.0), m("b", 20.0)], 7);
        assert_eq!(second[0].hurt_at, Some(7));
        assert_eq!(second[1].hurt_at, None);
        // Healing keeps the last hurt time; a mate who left is gone.
        let third = merge(&second, vec![m("a", 16.0)], 9);
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].hurt_at, Some(7));
    }
}
