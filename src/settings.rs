//! Options > Mods > Team > Config: what shows, where the cards go, how big markers are.
use crate::kit::setting::{Number, Toggle};
use mod_api::{ClientHost, SettingSpec};

pub const PINGS: Toggle = Toggle::on("show_pings");
pub const CARDS: Toggle = Toggle::on("show_cards");
pub const ARROWS: Toggle = Toggle::on("show_arrows");
pub const SOUND: Toggle = Toggle::on("ping_sound");
/// A sound and a red edge arrow when a teammate is in trouble.
pub const ALERT: Toggle = Toggle::on("hurt_alert");
const TOGGLES: [Toggle; 5] = [PINGS, CARDS, ARROWS, SOUND, ALERT];
const SIDE: &str = "cards_side";
const MARKER_SIZE: Number = Number { key: "marker_size", min: 0.5, max: 2.0, step: 0.1, default: 1.0 };

/// Where the cards sit.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The reading start: left in English, right in Arabic.
    Start,
    End,
}

pub fn specs() -> Vec<SettingSpec> {
    let mut specs: Vec<SettingSpec> = TOGGLES.iter().map(|t| t.spec(crate::MOD_ID)).collect();
    specs.push(SettingSpec::choice(
        SIDE,
        "team.config.cards_side",
        ["start", "end"].map(|s| format!("team.config.cards_side.{s}")),
        // The reading end by default: the start corner is the minimap's (Map, Xaero).
        1,
    ));
    specs.push(MARKER_SIZE.spec(crate::MOD_ID));
    specs
}

/// Whether a toggle is on (on when the setting can't be read).
pub fn on(host: &dyn ClientHost, key: Toggle) -> bool {
    key.read_client(host)
}

pub fn side(host: &dyn ClientHost) -> Side {
    match host.setting_choice(SIDE) {
        Some(0) => Side::Start,
        _ => Side::End,
    }
}

pub fn marker_size(host: &dyn ClientHost) -> f32 {
    MARKER_SIZE.read_client(host) as f32
}
