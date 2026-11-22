# Star

A Meteor-inspired modular client mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 in Rust).

## Features

- **Top-Bar Navigation ClickGUI** (`Right Shift` by default): switch cleanly between **`Modules`** (`Combat`, `Player`, `Movement`, `Render`), **`Settings`** (theme, font, layout, colors, keybind), and **`HUD`** directly from the top bar.
- **Font & Theme Customization**: bundles 4×-supersampled TrueType fonts selectable from a live-preview dropdown (`JetBrains Mono`, `Comfortaa`, `Tw Cen MT`, `Pixelation`, `Geist`, `Geist Bold`, `Geist Mono`, `Bricolage`, `Pixelify`, plus `Minecraft`), font scale (`0.70x–1.20x`), text shadow, bold headers, italic values, 6 themes + custom RGBA pickers, and Rainbow Accent.
- **Reusable Entity Selector Window**: pick exact Minecraft entity types (loaded dynamically from `minecraft:entity_type`) with live search, group quick-toggles (`Players`, `Hostile`, `Neutral`, `Passive`, `Misc`), and a 2-column checklist. Used by **Aim Assist**, **ESP**, **Tracers**, and **Nametags**.
- **Combat**: **Aim Assist** with exact Entity Selector targeting, activation modes, range, FOV, priority, aim point, block raycast line-of-sight, per-frame/per-tick smoothing, deadzone, and movement lead.
- **Render / Visuals**:
  - **ESP**: 3D wireframe box, 3D filled + outline box (through walls), 2D box, 2D corner box, live health bars, and category/custom/distance colors.
  - **Tracers**: 3D lines from your view to selected entities (`Feet`, `Center`, `Eyes`).
  - **Nametags**: clean floating name + health + distance pills with optional vanilla tag hiding.
  - **No Fog**: independently clear atmospheric, water, lava, and powder snow fog.
  - **Zoom**: smooth camera magnification with reduced mouse sensitivity (`C` by default).
  - **Camera**: disable camera/hurt shake and customize third-person camera distance.
- **Player**: **Fast Mine** block break speed multiplier.
- **HUD**: watermark, live `XYZ` coordinates, active modules ArrayList, target reticle, target info card, and FOV ring.
- **Persistent Config**: automatically saves to `config/star.cfg`.

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
