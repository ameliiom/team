//! Reading direction, the same rule as Jade's: the first letter of the language's own name.
//! Kept as its own file so it moves as is into a shared library mod once a third mod needs it.
use mod_api::ClientHost;

/// Whether the player's language reads right to left.
pub fn right_to_left(host: &dyn ClientHost) -> bool {
    host.translate("language.name").is_some_and(|name| starts_rtl(&name))
}

/// The direction of the first letter: true for the right-to-left scripts' blocks.
pub fn starts_rtl(text: &str) -> bool {
    text.chars().find(|c| c.is_alphabetic()).is_some_and(|c| {
        matches!(c as u32,
            0x0590..=0x08FF
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF)
    })
}
