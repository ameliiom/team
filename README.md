# Team

Play together without typing, for [Horizon](https://playhorizon.dev) (Minecraft: Java Edition 26.3 in Rust, mod API 0.2). A native mod, in English and Arabic.

- **Ping**: press **G** (rebindable) while looking at anything up to 128 blocks away. Everyone sees a marker over it, through walls, with its distance: a locator-bar dot in your colour, a sword over a hostile mob (the marker follows it), the block's icon over an ore or a container. Ping your own marker again to take it away; each player has one marker, which lasts 30 seconds.
- **Team cards**: each teammate in the game's tooltip panel at the side of the screen: their head with their level, armour, hearts and food as the HUD draws them, and how far they are with an arrow toward them, or their dimension's block when they are elsewhere. A card flashes red when its player gets hurt.
- **Edge arrows**: when a teammate or a marker is out of view, an arrow at the screen edge points to it.
- **On the map**: with [Map](https://github.com/ameliiom/map) installed, teammates and pings also show on its minimap and world map. Map reads them through Team's small public API (`with_mates`, `with_markers`, `colour`).

Everyone on the server is on the team. Install it on every client and on the server (in single player or LAN, the host's game is the server).

**Options > Mods > Team > Config**: pings, cards, arrows and the ping sound on or off, the cards' side, the marker size.

## Install

1. Copy `team.hmod` into `%APPDATA%\horizon\horizon-native-mods\`.
2. Add the line `mod.team:true` to `%APPDATA%\horizon\horizon-mods.txt`.
3. Build: in the launcher's Mods page press **Build**, or run `horizon-launcher native-build --instance horizon --trust-native-code`. A native mod runs with your rights: install it only from a source you trust.
4. Play.

Built for Horizon Nightly `f037744` (2026-11-21).

## Build

```powershell
cargo horizon-mod build
```

The source package is `target\horizon-mod\team.hmod`; the launcher compiles it (step 3 above).

## Licence

MIT, see [LICENSE](LICENSE).
