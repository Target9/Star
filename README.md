# Star

A Meteor-inspired modular client mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 in Rust).

## Features

- **Categorized ClickGUI** (`Right Shift` by default): draggable, collapsible, scrollable category panels (`Combat`, `Player`, `Movement`, `Render`, `Client`), live search bar, reset layout, hover tooltips, inline per-module setting drawers, and keybind capture.
- **Full Theme & HUD Customization**: 6 built-in themes (`Meteor`, `Midnight`, `Crimson`, `Emerald`, `Sunset`, `Monochrome`), custom RGBA accent/panel colors, Rainbow Accent mode, watermark, active modules ArrayList, target reticle, FOV ring, and target info card.
- **Aim Assist**: players/mobs/invisible target filtering, `Always` / `Attack Held` / `Use Held` activation, configurable range, FOV, target priority (`FOV`, `Distance`, `Health`, `Hurt Time`), aim point (`Eyes`, `Chest`, `Center`, `Nearest`), block line-of-sight raycast, per-frame or per-tick smoothing (`Ease Out` / `Linear`), independent horizontal & vertical speeds, deadzone, and movement lead.
- **Fast Mine**: configurable block break speed multiplier.
- **Persistent Config**: automatically saves module states, keybinds, settings, and ClickGUI layout to `config/star.cfg`.

## Install into Horizon

### From the Horizon Launcher UI

1. Open the Horizon Launcher and select your instance.
2. Go to the **Mods** page and click **Add from GitHub**.
3. Paste:
   ```text
   https://github.com/Target9/Star
   ```
4. Accept the native mod trust prompt, then click **Build** to compile Star for your Horizon build.

### From the CLI

```sh
horizon-launcher mods add-github https://github.com/Target9/Star --trust-native-code
horizon-launcher native-build
```

## Controls

- `Right Shift` — Open / close the ClickGUI (rebindable in Horizon Controls or inside `Client -> ClickGUI`).
- **Left-click** a module to toggle it; **right-click** to expand its settings; **middle-click** (or click the **Bind** row) to set a keybind.
- **Right-click** any setting row to reset it to its default value.
