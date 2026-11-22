//! Options > Mods > Team > Config: what shows, where the cards go, how big markers are.
use mod_api::{ClientHost, SettingSpec};

pub const PINGS: &str = "show_pings";
pub const CARDS: &str = "show_cards";
pub const ARROWS: &str = "show_arrows";
pub const SOUND: &str = "ping_sound";
const TOGGLES: [&str; 4] = [PINGS, CARDS, ARROWS, SOUND];
const SIDE: &str = "cards_side";
const MARKER_SIZE: &str = "marker_size";

/// Where the cards sit.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The reading start: left in English, right in Arabic.
    Start,
    End,
}

pub fn specs() -> Vec<SettingSpec> {
    let mut specs: Vec<SettingSpec> = TOGGLES
        .iter()
        .map(|key| SettingSpec::toggle(*key, format!("team.config.{key}"), true))
        .collect();
    specs.push(SettingSpec::choice(
        SIDE,
        "team.config.cards_side",
        ["start", "end"].map(|s| format!("team.config.cards_side.{s}")),
        // The reading end by default: the start corner is the minimap's (Map, Xaero).
        1,
    ));
    specs.push(SettingSpec::range(MARKER_SIZE, "team.config.marker_size", 0.5, 2.0, 0.1, 1.0));
    specs
}

/// Whether a toggle is on (on when the setting can't be read).
pub fn on(host: &dyn ClientHost, key: &str) -> bool {
    host.setting_bool(key).unwrap_or(true)
}

pub fn side(host: &dyn ClientHost) -> Side {
    match host.setting_choice(SIDE) {
        Some(0) => Side::Start,
        _ => Side::End,
    }
}

pub fn marker_size(host: &dyn ClientHost) -> f32 {
    host.setting_f64(MARKER_SIZE).unwrap_or(1.0).clamp(0.5, 2.0) as f32
}
