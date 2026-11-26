# Star

A Meteor-inspired modular client mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 in Rust).

## Features

- **Top-Bar Navigation ClickGUI** (`Right Shift` by default): switch cleanly between **`Modules`** (`Combat`, `Player`, `Movement`, `Render`), **`Settings`** (theme, font, layout, colors, keybind), **`HUD`**, **`Visuals`**, and **`Profiles`** from the top bar.
- **Visuals tab**: full look overhaul with section pills (`Presets`, `Sky & Fog`, `World`, `Camera`, `Screen`, `Hit FX`, `Ambient`, `Trip`).
  - **Presets**: `Vanilla`, **`PVP`** (accent-matched hit bursts, aurora sky, clear fog, glow, punch/flash), **`LSD`** (Dose-driven trip), `Synthwave`, `Dreamy`, `Void`, and `Custom`.
  - **Dose** (0–150%): global intensity for trip-style effects, with onset and wave timing.
  - **Colour Source**: Theme Accent, Complement, Rainbow, or Custom A→B — PVP hit particles and lights follow your ClickGUI theme (including Rainbow Accent).
  - **Sky**: optional WGSL sky domes (Gradient / Aurora / Galaxy / Psychedelic), fog tint/density/breathing.
  - **World**: biome grass/leaf/water tint overrides (rate-limited rebuild), glow rims, coloured player light.
  - **Camera / Screen**: FOV offset, sway, roll, hit punch, vignette, colour wash, scanlines, edge glow, chroma bars, low-HP pulse.
  - **Hit FX**: Burst / Sparks / Shock Ring / Stardust / Fireworks / Nova with crit boost, kill burst, ding, and “only my hits”.
  - **Trip (LSD)**: FOV breathing, roll drift, rainbow sky/biomes, wash, ambient sparkles, optional audio warp; writes `frame.mods` for a future `native-star` shader pack (pack itself not bundled — Horizon’s shader format reference is not in the SDK).
- **Font & Theme Customization**: bundles 4×-supersampled TrueType fonts selectable from a live-preview dropdown (`JetBrains Mono`, `Comfortaa`, `Tw Cen MT`, `Pixelation`, `Geist`, `Geist Bold`, `Geist Mono`, `Bricolage`, `Pixelify`, plus `Minecraft`), font scale (`0.70x–1.20x`), text shadow, bold headers, italic values, 6 themes + custom RGBA pickers, and Rainbow Accent.
- **Reusable Entity Selector Window**: pick exact Minecraft entity types (loaded dynamically from `minecraft:entity_type`) with live search, group quick-toggles (`Players`, `Hostile`, `Neutral`, `Passive`, `Misc`), and a 2-column checklist. Used by **Aim Assist**, **ESP**, **Tracers**, and **Nametags**.
- **Combat**: **Aim Assist** with exact Entity Selector targeting, activation modes, range, FOV, priority, aim point, block raycast line-of-sight, per-frame/per-tick smoothing, deadzone, and movement lead.
- **Lock Visuals** (Render): a shared component for any module that locks onto a target. While it is on, it automatically shows target HUD (position, scale, background, health gradient, "locked by"), reticle (cross, brackets, ring, diamond, with a lock-on animation), 3D target box and link line. Color modes (accent, health, distance, custom), fade-out and a per-source on/off switch. Module-specific indicators stay on that module (Aim Assist keeps its own FOV ring and range circle).
  - **Adding a new lock module:** add a `LockSource` to `LOCK_SOURCES` in `src/modules/lock_visuals.rs`, then call `LockBoard::publish(module_id, Some(LockTarget { .. }), now_ms)` while locked and `None` when not. No Lock Visuals drawing code is needed.
- **Render / Visuals**:
  - **ESP**: 3D wireframe box, 3D filled + outline box (through walls), 2D box, 2D corner box, live health bars, and category/custom/distance colors.
  - **Tracers**: 3D lines from your view to selected entities (`Feet`, `Center`, `Eyes`).
  - **Nametags**: clean floating name + health + distance pills with optional vanilla tag hiding, plus an optional line for the held item, armor durability, and ping.
  - **No Fog**: independently clear atmospheric, water, lava, and powder snow fog.
  - **Zoom**: smooth camera magnification with reduced mouse sensitivity (`C` by default).
  - **Camera**: disable camera/hurt shake and customize third-person camera distance.
  - **Item ESP**: boxes and labels (`Diamond x12`) for dropped items.
  - **Storage ESP**: highlights chests, trapped chests, ender chests, barrels, shulker boxes, furnaces, hoppers, and more, with per-type toggles and a color per container type.
  - **Trajectories**: predicted flight path and landing marker for arrows, tridents, pearls, snowballs, and potions. "Yours" is estimated from projectiles that appear right next to you, because the client has no projectile-owner query.
  - **Breadcrumbs**: fading trail behind you or behind selected entities.
  - **Damage Indicators**: floating damage numbers, with optional healing numbers and an "only my hits" filter.
  - **Radar**: draggable mini-map with north-up or rotating modes, range, size, dot size, and edge markers.
- **Movement**: **Sprint** (Forward/Omni), **Toggle Sneak**, and **Auto Walk**. These use Horizon's `movement_input` hook and only change your own client-side input.
- **Player**: **Fast Mine** block break speed multiplier, with an "effective tools only" option and instant-mining protection.
- **HUD**: watermark, live `XYZ` coordinates, active modules ArrayList, target reticle, target info card, and FOV ring, plus an info panel (FPS, ping, speed, armor, compass, session timer), keystrokes with clicks per second, and toast notifications. The info panel, keystrokes, and radar can be dragged on the **HUD** tab (right-click a widget to reset it).
- **ClickGUI extras**: per-module keybind mode (`Toggle` or `Hold`), per-module reset, favorites pinned to the top of a panel, search that hides panels without a match, and **Copy Config / Paste Config** on the **Profiles** tab for sharing a setup through the clipboard.
- **Persistent Config**: saves to `config/star.cfg` with a last-good backup (`star.cfg.bak`), throttled writes, truncation detection, and clamped values on load. Old configs migrate automatically.
- **Stability**: every hook is panic-guarded. A module that keeps failing is switched off with a notification instead of crashing the game, and re-enabling it gives it a fresh start.

### Not included

- **Fullbright**: Horizon's mod API exposes no gamma or lightmap control, so it cannot be built honestly.
- **True post-process warp** (chromatic aberration / kaleidoscope screen warp): needs a Horizon shader pack. Star already writes `shader_params` for a `native-star` pack; the pack files are not shipped until the shader format reference is available.
- No auto-attack, kill-aura, anti-detection, or server-probe evasion features.

### Notes

- Registering the movement hook makes Horizon take over sprint handling. Star re-implements vanilla's sprint latch (sprint key or double-tap forward) so sprinting feels the same with the movement modules off.

## Development

Run the unit tests with `cargo test`. If the MSVC linker is not installed, use the GNU toolchain: `cargo +stable-x86_64-pc-windows-gnu test --target-dir target-gnu`.

## Install into Horizon

### From the Horizon Launcher UI

1. Open the Horizon Launcher and select your instance.
2. Go to the **Mods** page and click **Add from GitHub**.
3. Paste:
   ```text
   https://github.com/Target9/Star
   ```
4. Accept the native mod trust prompt, then click **Build**.

### From the CLI

```sh
horizon-launcher mods add-github https://github.com/Target9/Star --trust-native-code
horizon-launcher native-build
```

To pull updates later:
```sh
horizon-launcher mods update-github star
horizon-launcher native-build
```
