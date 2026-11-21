//! Drawing, all in one HUD layer, with the game's own pieces: the locator-bar dot for markers,
//! the player-head item for a teammate's face, the HUD's heart, food and armour sprites, the
//! experience level's green number, and block icons for dimensions. Right-to-left languages
//! mirror the cards (`rtl.rs`).
use crate::payloads::{Member, PingKind};
use crate::ping::{distance, view_vector, MARKERS};
use crate::roster::{Mate, FLASH, MATES};
use crate::settings::{self as s, Side};
use crate::{lock, rtl};
use mod_api::client::hud_layout::Blend;
use mod_api::{ClientHost, Compound, DrawContext, GuiDraw, Identifier, ItemKey, ItemStack, Tag};
#[allow(unused_imports)]
use mod_api::prelude::*;

/// Inset of the edge arrows from the screen border, in GUI pixels.
const EDGE: f32 = 14.0;
const MARGIN: f32 = 4.0;
const GAP: f32 = 3.0;
/// The panel's own border (popup/background's nine-slice border), so nothing touches it.
const PAD: f32 = 6.0;
/// A HUD icon (heart, food, armour) as the vanilla HUD spaces them.
const ICON: f32 = 8.0;
const HEAD: f32 = 16.0;
/// The card: padding, head, gap, ten icons, padding.
const CARD_W: f32 = PAD + HEAD + 4.0 + 10.0 * ICON + 1.0 + PAD;
/// The card is the game's own popup panel (`popup/background`, nine-slice), as its settings
/// windows draw it; a hurt teammate's panel is tinted red for the flash.
const PANEL: &str = "popup/background";
const PANEL_TINT: u32 = 0xFFFF_FFFF;
const HURT_TINT: u32 = 0xFFFF_7070;
const DANGER: u32 = 0xFFFF_5555;
const TEXT: u32 = 0xFFFF_FFFF;
const DIM: u32 = 0xFFAA_AAAA;
/// The experience level's colour (`Gui.renderExperienceLevel`).
const LEVEL: u32 = 0xFF80_FF20;

/// Where a world point lands on screen, in GUI pixels, or the edge point and angle of the arrow
/// toward it when it is behind the camera or off screen.
pub enum OnScreen {
    Visible { x: f32, y: f32 },
    Edge { x: f32, y: f32, angle: f32 },
}

/// A player's stable colour (ARGB), from their name: the same on every client and every run.
pub fn colour(name: &str) -> u32 {
    // FNV-1a, then a hue on a fixed saturation and value so every colour reads on the HUD.
    let hash = name.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    let hue = (hash % 360) as f32 / 60.0;
    let (sat, val) = (0.6, 0.95);
    let c = val * sat;
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = val - c;
    let byte = |v: f32| ((v + m) * 255.0).round() as u32;
    0xFF00_0000 | byte(r) << 16 | byte(g) << 8 | byte(b)
}

/// The horizontal angle from the view direction to `at`, in radians, clockwise (right) positive.
pub fn bearing(eye: [f64; 3], yaw: f32, at: [f64; 3]) -> f32 {
    let forward = view_vector(yaw, 0.0);
    let right = [-forward[2], 0.0, forward[0]];
    let rel = [at[0] - eye[0], 0.0, at[2] - eye[2]];
    let ahead = rel[0] * forward[0] + rel[2] * forward[2];
    let side = rel[0] * right[0] + rel[2] * right[2];
    side.atan2(ahead) as f32
}

/// The point on the screen's inset edge in direction `angle` from the centre (0 = up).
pub fn edge_point(angle: f32, [w, h]: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (angle.sin(), -angle.cos());
    let (hw, hh) = (w / 2.0 - EDGE, h / 2.0 - EDGE);
    let scale = (hw / dx.abs().max(1e-6)).min(hh / dy.abs().max(1e-6));
    [w / 2.0 + dx * scale, h / 2.0 + dy * scale]
}

/// Projects a world point through this frame's camera.
pub fn place(host: &dyn ClientHost, at: [f64; 3], screen: [f32; 2]) -> Option<OnScreen> {
    let [w, h] = screen;
    if let Some([x, y, _]) = host.project(at) {
        let (sx, sy) = ((x + 1.0) / 2.0 * w, (1.0 - y) / 2.0 * h);
        if (0.0..=w).contains(&sx) && (0.0..=h).contains(&sy) {
            return Some(OnScreen::Visible { x: sx, y: sy });
        }
    }
    let camera = host.camera()?;
    let angle = bearing(camera.position, camera.yaw, at);
    let [x, y] = edge_point(angle, screen);
    Some(OnScreen::Edge { x, y, angle })
}

/// A triangle pointing in `angle` (0 = up), centred on `at`.
fn arrow(draw: &mut dyn DrawContext, at: [f32; 2], angle: f32, r: f32, argb: u32) {
    let turn = |dx: f32, dy: f32| {
        let (sin, cos) = angle.sin_cos();
        [at[0] + dx * cos - dy * sin, at[1] + dx * sin + dy * cos]
    };
    let tip = turn(0.0, -r);
    draw.quad([tip, turn(r * 0.8, r * 0.7), turn(0.0, r * 0.3), turn(-r * 0.8, r * 0.7)], argb);
}

/// One of the game's GUI sprites (`hud/heart/full`), as vanilla's `blitSprite` draws it,
/// multiplied by `tint` (opaque white keeps its colours).
fn blit(draw: &mut dyn DrawContext, name: &str, [x, y, w, h]: [f32; 4], tint: u32) -> bool {
    let op = GuiDraw::Sprite {
        sprite: name.to_owned().into(),
        x: x.round() as i32,
        y: y.round() as i32,
        w: w.round() as i32,
        h: h.round() as i32,
        region: None,
        color: tint,
        blend: Blend::Textured,
    };
    draw.gui_paint(&[op]).is_ok()
}

/// The locator bar's dot (`hud/locator_bar_dot/default_0`), tinted, centred on `at`.
fn dot(draw: &mut dyn DrawContext, [x, y]: [f32; 2], size: f32, argb: u32) {
    let half = size / 2.0;
    blit(draw, "hud/locator_bar_dot/default_0", [x - half, y - half, size, size], argb);
}

fn distance_text(host: &dyn ClientHost, blocks: f64) -> String {
    let template = host.translate("team.distance").unwrap_or_else(|| "%sm".into());
    template.replace("%s", &format!("{}", blocks.round() as i64))
}

/// Text centred on `x`, kept whole on screen.
fn centred_text(draw: &mut dyn DrawContext, text: &str, [x, y]: [f32; 2], argb: u32) {
    let w = draw.text_width(text);
    let [sw, _] = draw.size();
    let left = (x - w / 2.0).clamp(2.0, (sw - w - 2.0).max(2.0));
    draw.text(text, [left.round(), y.round()], argb, true);
}

fn item_key(host: &dyn ClientHost, id: &str) -> Option<ItemKey> {
    host.item_key(&Identifier::parse(id).ok()?)
}

/// A dimension's icon: the block it is made of (the Nether's netherrack, the End's end stone),
/// else grass for the Overworld and any other dimension.
fn dimension_icon(host: &dyn ClientHost, id: &str) -> Option<ItemKey> {
    let block = match id {
        "minecraft:the_nether" => "minecraft:netherrack",
        "minecraft:the_end" => "minecraft:end_stone",
        _ => "minecraft:grass_block",
    };
    item_key(host, block)
}

/// A player-head item wearing `name`'s skin, as a placed head resolves it.
fn head(host: &dyn ClientHost, name: &str) -> Option<ItemStack> {
    let mut stack = ItemStack::of(item_key(host, "minecraft:player_head")?);
    let mut profile = Compound::new();
    profile.insert("name".into(), Tag::String(name.into()));
    let mut patch = Compound::new();
    patch.insert("minecraft:profile".into(), Tag::Compound(profile));
    stack.apply_components(&patch).ok()?;
    Some(stack)
}

/// Text with a one-pixel black outline, as the experience level is drawn.
fn outlined(draw: &mut dyn DrawContext, text: &str, [x, y]: [f32; 2], argb: u32) {
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        draw.text(text, [x + dx, y + dy], 0xFF00_0000, false);
    }
    draw.text(text, [x, y], argb, false);
}

/// Ten HUD icons for `value` out of 20 (two halves each): the full, half and empty sprites of
/// one family, from the row's reading start. `blink` uses the hurt container.
fn icon_row(
    draw: &mut dyn DrawContext,
    [full, half, empty]: [&str; 3],
    value: i32,
    x: impl Fn(usize) -> f32,
    y: f32,
) {
    for i in 0..10 {
        let at = [x(i), y, ICON + 1.0, ICON + 1.0];
        blit(draw, empty, at, TEXT);
        let left = value - 2 * i as i32;
        if left >= 2 {
            blit(draw, full, at, TEXT);
        } else if left == 1 {
            blit(draw, half, at, TEXT);
        }
    }
}

/// Draws the markers, the edge arrows and the cards, each part behind its setting.
pub fn draw(host: &dyn ClientHost, draw: &mut dyn DrawContext) {
    let Some(camera) = host.camera() else { return };
    let screen = draw.size();
    if s::on(host, s::PINGS) {
        markers(host, draw, &camera, screen);
    }
    let mates = lock(&MATES);
    if s::on(host, s::ARROWS) {
        for mate in mates.iter().filter(|m| m.member.dimension == camera.dimension) {
            let at = [mate.member.x, mate.member.y + 2.2, mate.member.z];
            if let Some(OnScreen::Edge { x, y, angle }) = place(host, at, screen) {
                arrow(draw, [x, y], angle, 6.0, colour(&mate.member.name));
                if let Some(face) = head(host, &mate.member.name) {
                    draw.item_stack(&face, [x - 6.0, y + 7.0], 12.0);
                }
            }
        }
    }
    if s::on(host, s::CARDS) {
        cards(host, draw, &camera, &mates, screen);
    }
}

fn markers(host: &dyn ClientHost, draw: &mut dyn DrawContext, camera: &mod_api::Camera, screen: [f32; 2]) {
    let size = 9.0 * s::marker_size(host);
    let shown = lock(&MARKERS);
    for s in shown.iter().filter(|s| s.marker.dimension == camera.dimension) {
        let m = &s.marker;
        // A marker on an entity follows it while this client can see the entity.
        let at = m
            .entity
            .and_then(|e| host.entity(e.0 as u32))
            .and_then(|e| e.position(host))
            .map(|[x, y, z]| [x, y + 2.3, z])
            .unwrap_or([m.x, m.y + 0.5, m.z]);
        let tint = if m.kind == PingKind::Danger { DANGER } else { colour(&m.owner) };
        let label = distance_text(host, distance(camera.position, at));
        match place(host, at, screen) {
            Some(OnScreen::Visible { x, y }) => {
                dot(draw, [x, y], size, tint);
                let icon = match m.kind {
                    PingKind::Loot => m.block.as_deref().and_then(|b| item_key(host, b)),
                    PingKind::Danger => item_key(host, "minecraft:iron_sword"),
                    _ => None,
                };
                if let Some(icon) = icon {
                    draw.item(icon, [x - 8.0, y - size / 2.0 - 18.0], 16.0);
                }
                centred_text(draw, &label, [x, y + size / 2.0 + 2.0], TEXT);
            }
            Some(OnScreen::Edge { x, y, angle }) if s::on(host, s::ARROWS) => {
                arrow(draw, [x, y], angle, 6.0, tint);
                centred_text(draw, &label, [x, y + 8.0], TEXT);
            }
            _ => {}
        }
    }
}

/// How tall a member's card is: the name row, an armour row when they wear any, hearts, food.
fn card_height(m: &Member) -> f32 {
    let rows = if m.armor > 0 { 3.0 } else { 2.0 };
    // The last row's icons hang a pixel low, as the HUD's do: two more keep them off the border.
    PAD + 9.0 + 2.0 + rows * (ICON + 1.0) + PAD + 2.0
}

fn cards(host: &dyn ClientHost, draw: &mut dyn DrawContext, camera: &mod_api::Camera, mates: &[Mate], [w, _]: [f32; 2]) {
    let rtl = rtl::right_to_left(host);
    // The cards' side: the reading start, or its end; Arabic's start is the right.
    let at_right = (s::side(host) == Side::End) != rtl;
    let x0 = if at_right { w - CARD_W - MARGIN } else { MARGIN };
    // Inside a card, `flip` maps an LTR x to its mirrored place in a right-to-left language.
    let flip = |x: f32, width: f32| if rtl { x0 + CARD_W - (x - x0) - width } else { x };
    let now = crate::now();
    let mut y = MARGIN;
    for mate in mates {
        let m = &mate.member;
        let h = card_height(m);
        let hurt = mate.hurt_at.is_some_and(|t| now.saturating_sub(t) < FLASH);
        blit(draw, PANEL, [x0, y, CARD_W, h], if hurt { HURT_TINT } else { PANEL_TINT });

        // Face, with the experience level over its corner as the HUD draws it.
        let head_x = flip(x0 + PAD, HEAD);
        if let Some(face) = head(host, &m.name) {
            draw.item_stack(&face, [head_x, y + PAD], HEAD);
        }
        if m.level > 0 {
            let lv = m.level.to_string();
            let lw = draw.text_width(&lv);
            outlined(draw, &lv, [head_x + HEAD - lw + 1.0, y + PAD + HEAD - 6.0], LEVEL);
        }

        // Name, and where they are: an arrow and the distance, or their dimension's block.
        let col = x0 + PAD + HEAD + 4.0;
        let name_w = draw.text_width(&m.name);
        draw.text(&m.name, [flip(col, name_w), y + PAD], TEXT, true);
        if m.dimension == camera.dimension {
            let at = [m.x, m.y, m.z];
            let text = distance_text(host, distance(camera.position, at));
            let tw = draw.text_width(&text);
            let end = x0 + CARD_W - PAD;
            draw.text(&text, [flip(end - tw, tw), y + PAD], DIM, true);
            // The arrow points where they really are, so it never mirrors.
            let angle = bearing(camera.position, camera.yaw, at);
            arrow(draw, [flip(end - tw - 6.0, 0.0), y + PAD + 4.0], angle, 3.5, DIM);
        } else if let Some(icon) = dimension_icon(host, &m.dimension) {
            draw.item(icon, [flip(x0 + CARD_W - PAD - 10.0, 10.0), y + PAD - 1.0], 10.0);
        }

        // Armour (when worn), hearts and food: the HUD's own icons, ten to a row.
        let mut row_y = y + PAD + 11.0;
        let icon_x = |i: usize| flip(col + i as f32 * ICON, ICON + 1.0);
        if m.armor > 0 {
            icon_row(draw, ["hud/armor_full", "hud/armor_half", "hud/armor_empty"], m.armor, icon_x, row_y);
            row_y += ICON + 1.0;
        }
        let hp = (m.health / m.max_health.max(1.0) * 20.0).round() as i32;
        let container = if hurt { "hud/heart/container_blinking" } else { "hud/heart/container" };
        icon_row(draw, ["hud/heart/full", "hud/heart/half", container], hp, icon_x, row_y);
        row_y += ICON + 1.0;
        icon_row(draw, ["hud/food_full", "hud/food_half", "hud/food_empty"], m.food, icon_x, row_y);

        y += h + GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_stable_and_opaque() {
        assert_eq!(colour("ameliiom"), colour("ameliiom"));
        assert_ne!(colour("ameliiom"), colour("friend"));
        assert_eq!(colour("x") >> 24, 0xFF);
    }

    #[test]
    fn bearing_is_zero_ahead_and_positive_to_the_right() {
        // Yaw 0 looks south (+z); screen right is west (-x).
        assert!(bearing([0.0; 3], 0.0, [0.0, 0.0, 10.0]).abs() < 1e-6);
        assert!(bearing([0.0; 3], 0.0, [-10.0, 0.0, 10.0]) > 0.0);
        assert!(bearing([0.0; 3], 0.0, [10.0, 0.0, 10.0]) < 0.0);
        assert!((bearing([0.0; 3], 0.0, [0.0, 0.0, -10.0]).abs() - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn edge_points_stay_inside_the_inset() {
        let [x, y] = edge_point(0.0, [400.0, 300.0]);
        assert!((x - 200.0).abs() < 1e-3 && (y - EDGE).abs() < 1e-3);
        let [x, _] = edge_point(std::f32::consts::FRAC_PI_2, [400.0, 300.0]);
        assert!((x - (400.0 - EDGE)).abs() < 1e-3);
    }
}
