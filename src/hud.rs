//! Drawing, all in one HUD layer, with the game's own pieces: the locator-bar dot for markers,
//! the player-head item for a teammate's face, the HUD's heart, food and armour sprites, the
//! experience level's green number, and block icons for dimensions. Right-to-left languages
//! mirror the cards (`rtl.rs`).
use crate::payloads::PingKind;
use crate::ping::{distance, view_vector, MARKERS};
use crate::roster::{Mate, ALERT_SHOW, FLASH, MATES};
use crate::settings::{self as s, Side};
use crate::{lock, rtl};
use mod_api::client::hud_layout::Blend;
use mod_api::{ClientHost, Compound, DrawContext, GuiDraw, Identifier, ItemKey, ItemStack, Tag};
#[allow(unused_imports)]
use mod_api::prelude::*;

/// Inset of the edge arrows from the screen border, in GUI pixels.
const EDGE: f32 = 14.0;
/// Space between the screen edge and a panel's visible edge, and between two panels.
const MARGIN: f32 = 4.0;
/// A HUD icon (heart, food, armour) as the vanilla HUD spaces them.
const ICON: f32 = 8.0;
const HEAD: f32 = 16.0;
/// A text line's height with its shadow.
const LINE: f32 = 9.0;
/// The gap between the head column and the info column, and between the name and the rows.
const SPACE: f32 = 4.0;
/// Ten icons, as wide as the HUD's heart row.
const ROW_W: f32 = 10.0 * ICON + 1.0;
/// The card is the game's tooltip panel (`tooltip/background` and `tooltip/frame`), drawn as
/// vanilla's `TooltipRenderUtil` does: the sprites span the content plus `BLEED` on every side,
/// and their visible part starts `VISIBLE` outside the content, the same on all four sides.
const BACKGROUND: &str = "tooltip/background";
const FRAME: &str = "tooltip/frame";
const BLEED: f32 = 12.0;
const VISIBLE: f32 = 4.0;
const PANEL_TINT: u32 = 0xFFFF_FFFF;
/// A hurt teammate's panel, for the flash.
const HURT_TINT: u32 = 0xFFFF_6060;
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

/// The top corners' room, in GUI pixels, that edge arrows keep clear: where minimaps sit (Map's
/// is 143 square with its frame, plus its coordinates and margin).
const CORNER: [f32; 2] = [156.0, 172.0];

/// The point on the screen's inset edge in direction `angle` from the centre (0 = up), slid
/// along its edge out of a top corner.
pub fn edge_point(angle: f32, [w, h]: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (angle.sin(), -angle.cos());
    let (hw, hh) = (w / 2.0 - EDGE, h / 2.0 - EDGE);
    let scale = (hw / dx.abs().max(1e-6)).min(hh / dy.abs().max(1e-6));
    let [x, y] = [w / 2.0 + dx * scale, h / 2.0 + dy * scale];
    let in_corner = y < CORNER[1] && (x < CORNER[0] || x > w - CORNER[0]);
    if !in_corner || CORNER[1] * 2.0 > h || CORNER[0] * 2.0 > w {
        return [x, y];
    }
    let on_side = x <= EDGE + 0.5 || x >= w - EDGE - 0.5;
    if on_side {
        [x, CORNER[1]]
    } else if x < CORNER[0] {
        [CORNER[0], y]
    } else {
        [w - CORNER[0], y]
    }
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
    let now = crate::now();
    let alerts = s::on(host, s::ALERT);
    for mate in mates.iter().filter(|m| m.member.dimension == camera.dimension) {
        // In trouble: a bigger red arrow for 3 s, even with the arrows off.
        let alert = alerts && mate.alert_at.is_some_and(|t| now.saturating_sub(t) < ALERT_SHOW);
        if s::on(host, s::ARROWS) || alert {
            let at = [mate.member.x, mate.member.y + 2.2, mate.member.z];
            if let Some(OnScreen::Edge { x, y, angle }) = place(host, at, screen) {
                let (size, tint) = if alert { (9.0, DANGER) } else { (6.0, colour(&mate.member.name)) };
                arrow(draw, [x, y], angle, size, tint);
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

/// A card's parts, `[x, y, w, h]` in content pixels, left to right with the origin at the
/// content's top left (a right-to-left language mirrors the x's when drawing).
#[derive(Debug)]
pub struct CardLayout {
    pub size: [f32; 2],
    pub head: [f32; 4],
    /// The experience level under the head, its one-pixel outline included; none at level 0.
    pub level: Option<[f32; 4]>,
    pub name: [f32; 4],
    /// Where they are: the arrow and the distance, or the dimension's block.
    pub place: [f32; 4],
    /// The icon rows: armour (when worn), hearts, food.
    pub rows: Vec<[f32; 4]>,
}

/// The one grid every card follows: a head column (the face, the level under it) and an info
/// column (the name with the place at its end, then the icon rows), `SPACE` apart. `name_w`
/// is already cut to what fits.
pub fn card_layout(name_w: f32, level_w: Option<f32>, place_w: f32, rows: usize) -> CardLayout {
    // The head column is as wide as the head, or as the level when a big level is wider.
    let head_col = level_w.map_or(HEAD, |w| HEAD.max(w + 2.0));
    let col = head_col + SPACE;
    let head = [(head_col - HEAD) / 2.0, 0.0, HEAD, HEAD];
    let level = level_w.map(|w| [(head_col - w - 2.0) / 2.0, HEAD + 1.0, w + 2.0, LINE + 1.0]);
    let name = [col, 0.0, name_w.min(ROW_W - place_w - SPACE), LINE];
    let place = [col + ROW_W - place_w, 0.0, place_w, LINE];
    let rows: Vec<[f32; 4]> = (0..rows)
        .map(|k| [col, LINE + SPACE + k as f32 * (ICON + 1.0), ROW_W, ICON + 1.0])
        .collect();
    let bottom = |r: &[f32; 4]| r[1] + r[3];
    let height = rows
        .iter()
        .map(bottom)
        .chain(level.iter().map(bottom))
        .fold(bottom(&head), f32::max);
    CardLayout { size: [col + ROW_W, height], head, level, name, place, rows }
}

/// The widest prefix of `text` that fits `width`, with an ellipsis when it was cut.
fn fit(draw: &dyn DrawContext, text: &str, width: f32) -> String {
    if draw.text_width(text) <= width {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && draw.text_width(&format!("{cut}…")) > width {
        cut.pop();
    }
    format!("{cut}…")
}

/// The game's tooltip panel around a content rectangle, as vanilla draws a tooltip.
fn panel(draw: &mut dyn DrawContext, [x, y, w, h]: [f32; 4], tint: u32) {
    let rect = [x - BLEED, y - BLEED, w + 2.0 * BLEED, h + 2.0 * BLEED];
    blit(draw, BACKGROUND, rect, tint);
    blit(draw, FRAME, rect, PANEL_TINT);
}

fn cards(host: &dyn ClientHost, draw: &mut dyn DrawContext, camera: &mod_api::Camera, mates: &[Mate], [w, _]: [f32; 2]) {
    let rtl = rtl::right_to_left(host);
    // The cards' side: the reading start, or its end; Arabic's start is the right.
    let at_right = (s::side(host) == Side::End) != rtl;
    let now = crate::now();
    let mut top = MARGIN + VISIBLE;
    for mate in mates {
        let m = &mate.member;
        let here = m.dimension == camera.dimension;
        let at = [m.x, m.y, m.z];
        let place_text = here.then(|| distance_text(host, distance(camera.position, at)));
        let place_w = place_text.as_ref().map_or(ICON + 1.0, |t| ICON + 1.0 + draw.text_width(t));
        let level_text = (m.level > 0).then(|| m.level.to_string());
        let level_w = level_text.as_ref().map(|t| draw.text_width(t));
        let rows = if m.armor > 0 { 3 } else { 2 };
        let room = card_layout(f32::MAX, level_w, place_w, rows).name[2];
        let name = fit(draw, &m.name, room);
        let layout = card_layout(draw.text_width(&name), level_w, place_w, rows);
        let [cw, ch] = layout.size;
        let left = if at_right { w - MARGIN - VISIBLE - cw } else { MARGIN + VISIBLE };
        // A part's screen rectangle, mirrored inside the card for a right-to-left language.
        let put = |[x, y, pw, ph]: [f32; 4]| {
            let x = if rtl { cw - x - pw } else { x };
            [left + x, top + y, pw, ph]
        };

        let hurt = mate.hurt_at.is_some_and(|t| now.saturating_sub(t) < FLASH);
        panel(draw, [left, top, cw, ch], if hurt { HURT_TINT } else { PANEL_TINT });

        let [hx, hy, ..] = put(layout.head);
        if let Some(face) = head(host, &m.name) {
            draw.item_stack(&face, [hx, hy], HEAD);
        }
        if let (Some(rect), Some(text)) = (layout.level, &level_text) {
            let [lx, ly, ..] = put(rect);
            outlined(draw, text, [lx + 1.0, ly + 1.0], LEVEL);
        }
        let [nx, ny, ..] = put(layout.name);
        draw.text(&name, [nx, ny], TEXT, true);

        // The place reads arrow then distance; the arrow points where they really are, so it
        // never mirrors.
        let [px, py, pw, _] = put(layout.place);
        match &place_text {
            Some(text) => {
                let arrow_x = if rtl { px + pw - ICON / 2.0 } else { px + ICON / 2.0 };
                let text_x = if rtl { px } else { px + ICON + 1.0 };
                arrow(draw, [arrow_x, py + 4.0], bearing(camera.position, camera.yaw, at), 3.5, DIM);
                draw.text(text, [text_x, py], DIM, true);
            }
            None => {
                if let Some(icon) = dimension_icon(host, &m.dimension) {
                    draw.item(icon, [px, py], ICON + 1.0);
                }
            }
        }

        let hp = (m.health / m.max_health.max(1.0) * 20.0).round() as i32;
        let heart = if hurt { "hud/heart/container_blinking" } else { "hud/heart/container" };
        let mut families = Vec::new();
        if m.armor > 0 {
            families.push((["hud/armor_full", "hud/armor_half", "hud/armor_empty"], m.armor));
        }
        families.push((["hud/heart/full", "hud/heart/half", heart], hp));
        families.push((["hud/food_full", "hud/food_half", "hud/food_empty"], m.food));
        for (rect, (sprites, value)) in layout.rows.iter().zip(families) {
            let [rx, ry, ..] = put(*rect);
            // Icon i from the row's reading start, so a mirrored row fills from the right.
            let x = |i: usize| if rtl { rx + ROW_W - (i + 1) as f32 * ICON - 1.0 } else { rx + i as f32 * ICON };
            icon_row(draw, sprites, value, x, ry);
        }

        top += ch + 2.0 * VISIBLE + MARGIN;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(part: [f32; 4], [w, h]: [f32; 2]) -> bool {
        part[0] >= 0.0 && part[1] >= 0.0 && part[0] + part[2] <= w + 1e-3 && part[1] + part[3] <= h + 1e-3
    }

    fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    #[test]
    fn every_card_part_is_inside_and_apart() {
        // Short, long and empty names; no level, a small and a huge one; distance or block;
        // with and without armour.
        for name_w in [0.0, 30.0, 500.0] {
            for level_w in [None, Some(6.0), Some(30.0)] {
                for place_w in [ICON + 1.0, ICON + 1.0 + 40.0] {
                    for rows in [2, 3] {
                        let l = card_layout(name_w, level_w, place_w, rows);
                        let mut parts = vec![l.head, l.name, l.place];
                        parts.extend(l.level);
                        parts.extend(l.rows.iter().copied());
                        for (i, a) in parts.iter().enumerate() {
                            assert!(inside(*a, l.size), "{a:?} leaves the card {:?} in {l:?}", l.size);
                            for b in &parts[i + 1..] {
                                assert!(!overlap(*a, *b), "{a:?} overlaps {b:?} in {l:?}");
                            }
                        }
                        // The info column's parts all start on the same line.
                        assert!(l.rows.iter().all(|r| r[0] == l.name[0]));
                    }
                }
            }
        }
    }

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
    fn edge_arrows_keep_out_of_the_top_corners() {
        let screen = [800.0, 450.0];
        for step in 0..72 {
            let angle = step as f32 * std::f32::consts::TAU / 72.0;
            let [x, y] = edge_point(angle, screen);
            assert!(!(y < CORNER[1] && (x < CORNER[0] || x > screen[0] - CORNER[0])), "{angle}: {x},{y}");
            assert!((EDGE - 1e-3..=screen[0] - EDGE + 1e-3).contains(&x) && (EDGE - 1e-3..=screen[1] - EDGE + 1e-3).contains(&y));
        }
    }

    #[test]
    fn edge_points_stay_inside_the_inset() {
        let [x, y] = edge_point(0.0, [400.0, 300.0]);
        assert!((x - 200.0).abs() < 1e-3 && (y - EDGE).abs() < 1e-3);
        let [x, _] = edge_point(std::f32::consts::FRAC_PI_2, [400.0, 300.0]);
        assert!((x - (400.0 - EDGE)).abs() < 1e-3);
    }
}
