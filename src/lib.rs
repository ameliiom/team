//! Team: play together without typing.
//!
//! - **Ping** (`ping.rs`): press the ping key while looking at something up to 128 blocks away;
//!   every teammate sees a marker over it, through walls, with its distance, in your colour.
//! - **Roster** (`roster.rs`): a card per teammate at the side of the screen (name, health,
//!   food, distance and direction), an arrow at the screen edge when they are out of view, and a
//!   red flash when they get hurt.
//!
//! Everyone on the server is on the team. The server relays pings and sends each client the
//! roster a few times a second (`payloads.rs`); drawing is client-side (`hud.rs`).
//!
//! Other mods on the client (Map) read the team through `with_mates`, `with_markers` and
//! `colour`: the same state and colours this mod draws, never a copy.
mod hud;
pub mod payloads;
pub mod ping;
pub mod roster;
mod rtl;
mod settings;

pub use hud::colour;

/// The teammates this client knows, read under their lock.
pub fn with_mates<R>(read: impl FnOnce(&[roster::Mate]) -> R) -> R {
    read(&lock(&roster::MATES))
}

/// The live pings this client shows, read under their lock.
pub fn with_markers<R>(read: impl FnOnce(&[ping::Shown]) -> R) -> R {
    read(&lock(&ping::MARKERS))
}

use mod_api::{
    ClientHost, ClientTickPreEvent, GuiLayer, GuiLayerDrawEvent, GuiLayerPlacement, Hooks,
    KeyRegistration, PayloadId, RegisterGuiLayersEvent, Registrar, RegistrationContext,
    ServerHost, ServerTickEvent,
};
#[allow(unused_imports)]
use mod_api::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub(crate) const MOD_ID: &str = "team";
/// Client ticks, the clock markers and flashes are timed by.
static TICKS: AtomicU32 = AtomicU32::new(0);

pub(crate) fn now() -> u32 {
    TICKS.load(Ordering::Relaxed)
}
const LAYER: &str = "team:hud";
const PING_KEY: &str = "ping";

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// The ping key, the settings, and the three payloads (ping up, marker and roster down).
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        // V: free in vanilla, Horizon and Map, and under the left hand while moving (G is
        // vanilla's quick actions).
        let mut key = KeyRegistration::new(PING_KEY, "key.keyboard.v");
        key.label = "key.team.ping".into();
        r.register_key_mapping(key);
        for spec in settings::specs() {
            let _ = r.register_setting(spec);
        }
        let mut handlers = r.payload_handlers();
        let mut view = handlers.registrar("1").optional();
        let send = view.play_to_server_typed(PayloadId("team:ping".into()), ping::relay);
        let show = view.play_to_client_typed(PayloadId("team:marker".into()), ping::receive);
        let roster = view.play_to_client_typed(PayloadId("team:roster".into()), roster::receive);
        payloads::keep(send, show, roster);
    }

    /// One HUD layer under the chat for markers, edge arrows and cards.
    fn register_gui_layers(&self, _: &RegistrationContext, event: &mut RegisterGuiLayersEvent<'_>) {
        event.layers.push(GuiLayer {
            id: LAYER.into(),
            placement: GuiLayerPlacement::Below("minecraft:chat".into()),
        });
    }

    /// Client: a ping key press becomes a ping; expired markers go.
    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        TICKS.fetch_add(1, Ordering::Relaxed);
        while host.consume_click(MOD_ID, PING_KEY) {
            ping::cast(host);
        }
        ping::expire(host);
        roster::alarm(host);
        roster::demo(host);
    }

    /// Server: every few ticks, each player gets the roster of the others.
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        roster::broadcast(host, event);
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        if event.layer == LAYER && host.playing() {
            hud::draw(host, event.draw);
        }
    }
}
