//! Star — Meteor-inspired modular client for Horizon (Minecraft 26.3 in Rust).
//!
//! Press Right Shift (customizable in the Settings tab) anywhere in a world —
//! including over the pause menu, inventory, or other GUIs — to open the ClickGUI.
//! Any open screen is closed first so the menu can take over.

mod config;
mod entity_filter;
mod font;
mod font_metrics;
mod gui;
mod guard;
mod hud;
mod module;
mod modules;
mod settings;
mod theme;
mod toast;

use std::sync::{Mutex, OnceLock};

use mod_api::{
    self, BiomeColorsEvent, CameraDistanceEvent, CameraRollEvent, CameraShakeEvent, ClientHost,
    ClientResourcesReloadedEvent, ClientSetupEvent, ClientTickPreEvent, ComputeFogColorEvent,
    ComputeFovEvent, EntityOutlineEvent, Flow, HostSide, InputKeyEvent, KeyAction,
    MouseButtonPreEvent, MovementInputEvent, PlayerBreakSpeedEvent, RegisterRenderPipelinesEvent,
    Registrar, RegistrationContext, RenderFogEvent, RenderFramePreEvent, RenderGuiPostEvent,
    RenderLevelStageEvent, RenderLivingPostEvent, RenderNameTagEvent, RenderPlayerPreEvent,
    ScreenCharTypedPreEvent, ScreenClosingEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, SettingSpec,
    ShaderParamsEvent, SubmitCustomGeometryEvent, VisualsEvent, Zoom, ZoomEvent,
};

use crate::gui::{ClickGuiState, ClipboardAction};
use crate::module::{ModuleId, ModuleManager};
use crate::modules::aim_assist;
use crate::modules::lock_visuals::{self, LockBoard};
use crate::modules::movement::{MoveConfig, MoveInputState};
use crate::modules::trackers::Trackers;
use crate::modules::vis_fx::{self, FxState, ParticleKeys};
use crate::modules::{extras, fast_mine, visuals};
use crate::theme::Theme;
use crate::toast::{ToastKind, Toasts};

const MOD_ID: &str = "star";
const DEFAULT_GUI_KEY: &str = "key.keyboard.right.shift";

/// Modules whose code runs inside the screen-space HUD hook.
const HUD_OWNERS: &[ModuleId] = &[
    ModuleId::Hud,
    ModuleId::Esp,
    ModuleId::Nametags,
    ModuleId::ItemEsp,
    ModuleId::DamageIndicators,
    ModuleId::Radar,
    ModuleId::AimAssist,
    ModuleId::LockVisuals,
    ModuleId::VisPreset,
    ModuleId::VisScreen,
    ModuleId::VisTrip,
];
/// Modules whose code runs inside the world-geometry hook.
const WORLD_OWNERS: &[ModuleId] = &[
    ModuleId::Esp,
    ModuleId::Tracers,
    ModuleId::ItemEsp,
    ModuleId::StorageEsp,
    ModuleId::Trajectories,
    ModuleId::Breadcrumbs,
    ModuleId::AimAssist,
    ModuleId::LockVisuals,
    ModuleId::VisSky,
    ModuleId::VisWorld,
];
/// Modules that need a per-tick world sample.
const TICK_OWNERS: &[ModuleId] = &[
    ModuleId::Breadcrumbs,
    ModuleId::DamageIndicators,
    ModuleId::Trajectories,
    ModuleId::StorageEsp,
    ModuleId::Esp,
    ModuleId::Tracers,
    ModuleId::Nametags,
    ModuleId::ItemEsp,
    ModuleId::AimAssist,
    ModuleId::VisHit,
    ModuleId::VisAmbient,
    ModuleId::VisTrip,
];

const VIS_OWNERS: &[ModuleId] = &[
    ModuleId::VisPreset,
    ModuleId::VisSky,
    ModuleId::VisWorld,
    ModuleId::VisCamera,
    ModuleId::VisScreen,
    ModuleId::VisHit,
    ModuleId::VisAmbient,
    ModuleId::VisTrip,
];

struct ClientState {
    manager: ModuleManager,
    gui: ClickGuiState,
    /// Locks reported by modules like Aim Assist, shown by Lock Visuals.
    lock: LockBoard,
    interp: visuals::EntityInterpCache,
    trackers: Trackers,
    toasts: Toasts,
    move_state: MoveInputState,
    fx: FxState,
    /// Enabled flag of every module as of the previous tick, to notice toggles.
    prev_enabled: Vec<(ModuleId, bool)>,
}

impl ClientState {
    fn new() -> Self {
        Self {
            manager: ModuleManager::new(),
            gui: ClickGuiState::default(),
            lock: LockBoard::default(),
            interp: visuals::EntityInterpCache::default(),
            trackers: Trackers::default(),
            toasts: Toasts::default(),
            move_state: MoveInputState::default(),
            fx: FxState::default(),
            prev_enabled: Vec::new(),
        }
    }
}

/// Which modules are on, in `ModuleId::ALL` order.
fn enabled_snapshot(manager: &ModuleManager) -> Vec<(ModuleId, bool)> {
    ModuleId::ALL
        .iter()
        .map(|&id| (id, manager.get(id).enabled))
        .collect()
}

/// Queues a toast for every module whose state differs from `before`.
fn toast_toggles(manager: &ModuleManager, before: &[(ModuleId, bool)]) {
    for &(id, was) in before {
        if id == ModuleId::ClickGui {
            continue;
        }
        let module = manager.get(id);
        if module.enabled != was {
            let (kind, word) = if module.enabled {
                (ToastKind::Success, "enabled")
            } else {
                (ToastKind::Info, "disabled")
            };
            toast::notify(kind, format!("{} {word}", module.name));
        }
    }
}

static STATE: OnceLock<Mutex<ClientState>> = OnceLock::new();

fn state() -> &'static Mutex<ClientState> {
    STATE.get_or_init(|| Mutex::new(ClientState::new()))
}

fn lock_state() -> std::sync::MutexGuard<'static, ClientState> {
    state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn mod_active(host: &dyn ClientHost) -> bool {
    host.on(MOD_ID) && host.setting_bool("enabled").unwrap_or(true)
}

enum BindAction {
    None,
    OpenGui,
    CloseGui,
}

/// Switches hold-bound modules off when their key or mouse button is released.
fn handle_bind_release(host: &mut dyn ClientHost, key: &str) {
    let mut st = lock_state();
    if !st.manager.config_loaded {
        return;
    }
    let before = enabled_snapshot(&st.manager);
    if st.manager.release_keybind(key) > 0 {
        toast_toggles(&st.manager, &before);
        config::save_throttled(host, &mut st.manager);
    }
}

/// Opens the ClickGUI, closing any obstructing screen (pause, inventory, nested
/// options, chat, …) first. `mod_screen_open` only succeeds with nothing over the
/// world, so we peel screens until it opens or `close_screen` can do no more.
fn open_click_gui(host: &mut dyn ClientHost) {
    for _ in 0..8 {
        if host.mod_screen_open(MOD_ID) {
            return;
        }
        if !host.close_screen() {
            return;
        }
    }
}

/// Returns true when the press opened or closed the ClickGUI (so callers can
/// cancel the underlying input).
fn handle_bind_press(host: &mut dyn ClientHost, key: &str) -> bool {
    let action = {
        let mut st = lock_state();
        if !st.manager.config_loaded {
            config::load(host, &mut st.manager);
        }

        let own_screen = host.mod_screen() == Some(MOD_ID);
        if own_screen && st.gui.is_listening() {
            BindAction::None
        } else {
            let configured_key = st.manager.gui_key().unwrap_or(DEFAULT_GUI_KEY);
            if configured_key == key {
                if own_screen {
                    BindAction::CloseGui
                } else {
                    // Open over gameplay or after closing whatever GUI is up
                    // (pause menu, inventory, chat, etc.).
                    BindAction::OpenGui
                }
            } else if host.playing() {
                let before = enabled_snapshot(&st.manager);
                if st.manager.trigger_keybind(key) > 0 {
                    toast_toggles(&st.manager, &before);
                    config::save_throttled(host, &mut st.manager);
                }
                BindAction::None
            } else {
                BindAction::None
            }
        }
    };

    match action {
        BindAction::OpenGui => {
            open_click_gui(host);
            true
        }
        BindAction::CloseGui => {
            host.mod_screen_close(MOD_ID);
            true
        }
        BindAction::None => false,
    }
}

/// Copies the setup to, or loads it from, the clipboard and tells the player how it went.
fn run_clipboard_action(host: &dyn ClientHost, manager: &mut ModuleManager, action: ClipboardAction) {
    match action {
        ClipboardAction::Export => {
            if host.set_clipboard_text(&config::export_text(manager)) {
                toast::notify(ToastKind::Success, "Config copied to the clipboard");
            } else {
                toast::notify(ToastKind::Warning, "Could not copy the config");
            }
        }
        ClipboardAction::Import => {
            let text = host.clipboard_text().unwrap_or_default();
            match config::import_text(manager, &text) {
                Ok(()) => {
                    manager.dirty = true;
                    toast::notify(ToastKind::Success, "Config loaded from the clipboard");
                }
                Err(msg) => toast::notify(ToastKind::Warning, msg),
            }
        }
    }
}

/// Switches off modules that kept panicking and tells the player.
fn apply_guard_trips() {
    for id in guard::take_trips() {
        let mut st = lock_state();
        let module = st.manager.get_mut(id);
        let name = module.name;
        module.enabled = false;
        st.manager.dirty = true;
        toast::notify(
            ToastKind::Error,
            format!("{name} hit an error and was switched off"),
        );
    }
}

/// Gives a module a fresh panic budget whenever the player turns it back on.
fn refresh_enabled_snapshot() {
    let mut st = lock_state();
    let now = enabled_snapshot(&st.manager);
    for (&(id, was), &(_, is)) in st.prev_enabled.iter().zip(now.iter()) {
        if is && !was {
            guard::clear(id);
        }
    }
    st.prev_enabled = now;
}

fn tick_pre(host: &mut dyn ClientHost) {
    if !mod_active(host) {
        let mut st = lock_state();
        st.lock.clear();
        st.interp.clear();
        return;
    }

    apply_guard_trips();
    refresh_enabled_snapshot();

    let (aim_module, need_interp, should_load) = {
        let st = lock_state();
        let need_interp = [
            ModuleId::Esp,
            ModuleId::Tracers,
            ModuleId::Nametags,
            ModuleId::ItemEsp,
            ModuleId::LockVisuals,
        ]
        .iter()
        .any(|&id| st.manager.is_enabled(id));
        (
            st.manager.get(ModuleId::AimAssist).clone(),
            need_interp,
            !st.manager.config_loaded,
        )
    };

    {
        let mut st = lock_state();
        if should_load {
            config::load(host, &mut st.manager);
        } else {
            config::save_throttled(host, &mut st.manager);
        }
    }

    // Snapshot entity boxes once per tick before Horizon advances positions for smooth 3D/2D lerp.
    // Samples such as speed, trails and damage numbers share the same tick.
    guard::run(TICK_OWNERS, (), || {
        let mut st = lock_state();
        st.interp.on_tick_pre(host, need_interp);
        let ClientState {
            manager, trackers, ..
        } = &mut *st;
        trackers.on_tick(host, manager);
    });

    guard::run(VIS_OWNERS, (), || {
        // Host APIs can re-enter hooks that also lock STATE. Drop the mutex before
        // refresh_biome_colors (and keep particle/audio on a separate short lock so a
        // biome rebuild never runs while STATE is held).
        let now = host.millis();
        let refresh = {
            let mut st = lock_state();
            let ClientState { manager, fx, .. } = &mut *st;
            vis_fx::on_tick(now, manager, fx).refresh_biomes
        };
        {
            let mut st = lock_state();
            let theme = Theme::from_manager(&st.manager, now);
            let ClientState {
                manager,
                trackers,
                fx,
                ..
            } = &mut *st;
            vis_fx::emit_frame_fx(host, manager, trackers, fx, &theme, now);
        }
        if refresh {
            host.refresh_biome_colors();
        }
    });

    guard::run(&[ModuleId::AimAssist], (), || {
        if aim_module.enabled && aim_module.mode("update_rate", 0) == 1 {
            let target = aim_assist::step(host, &aim_module, 1.0, false);
            let now = host.millis();
            lock_state().lock.publish(ModuleId::AimAssist, target, now);
        } else if !aim_module.enabled || !host.playing() {
            let now = host.millis();
            lock_state().lock.publish(ModuleId::AimAssist, None, now);
        }
    });
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, ctx: &RegistrationContext) {
        if let Ok(entity_types) = ctx.registry("minecraft:entity_type") {
            entity_filter::init_catalog(&entity_types);
        }

        // Use plain literal labels and do not register global key mappings or lang
        // files: Star keeps all keybinds private in config/star.cfg so servers
        // cannot probe translation keys or keybind components on join.
        let _ = registrar.register_setting(
            SettingSpec::toggle("enabled", "Enable Star", true).tooltip(
                "Master switch for all modules and overlays. Press Right Shift anywhere in a world (pause menu included) to open the ClickGUI.",
            ),
        );

        vis_fx::store_particle_keys(ParticleKeys {
            dust: Some(registrar.particle_key("minecraft:dust")),
            dust_transition: Some(registrar.particle_key("minecraft:dust_color_transition")),
            end_rod: Some(registrar.particle_key("minecraft:end_rod")),
            crit: Some(registrar.particle_key("minecraft:crit")),
            electric_spark: Some(registrar.particle_key("minecraft:electric_spark")),
            firework: Some(registrar.particle_key("minecraft:firework")),
            glow: Some(registrar.particle_key("minecraft:glow")),
            enchanted_hit: Some(registrar.particle_key("minecraft:enchanted_hit")),
        });
    }

    fn register_render_pipelines(
        &self,
        _context: &RegistrationContext,
        event: &mut RegisterRenderPipelinesEvent<'_>,
    ) {
        vis_fx::register_pipelines(event);
    }

    fn client_setup(&self, host: &mut dyn ClientHost, _: &mut ClientSetupEvent) {
        guard::run(VIS_OWNERS, (), || {
            let pipes = vis_fx::resolve_pipelines(host);
            lock_state().fx.pipelines = pipes;
        });
    }

    fn client_resources_reloaded(
        &self,
        host: &mut dyn ClientHost,
        _: &mut ClientResourcesReloadedEvent,
    ) {
        guard::run(VIS_OWNERS, (), || {
            let pipes = vis_fx::resolve_pipelines(host);
            lock_state().fx.pipelines = pipes;
        });
    }

    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        guard::run(&[], (), || {
            if !mod_active(host) {
                return;
            }
            match event.action {
                KeyAction::Press => {
                    let _ = handle_bind_press(host, event.key);
                }
                KeyAction::Release => handle_bind_release(host, event.key),
                _ => {}
            }
        });
    }

    fn mouse_button_pre(
        &self,
        host: &mut dyn ClientHost,
        event: &mut MouseButtonPreEvent,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if !mod_active(host) || event.cancelled {
                return Flow::Continue;
            }
            // ClickGUI already captures mouse through screen_* hooks while open.
            if host.mod_screen() == Some(MOD_ID) {
                return Flow::Continue;
            }
            let Some(key) = settings::mouse_button_key(event.button) else {
                return Flow::Continue;
            };
            if event.pressed {
                // Cancel so inventory/pause/etc. don't also act on the click.
                if handle_bind_press(host, key) {
                    return Flow::Cancel;
                }
            } else {
                handle_bind_release(host, key);
            }
            Flow::Continue
        })
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        guard::run(&[], (), || tick_pre(host));
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        guard::run(&[ModuleId::AimAssist], (), || {
            if !mod_active(host) || !host.playing() {
                return;
            }

            let aim_module = {
                let mut st = lock_state();
                st.interp.last_partial_tick = event.world_partial_tick;
                st.trackers.on_frame(host, event.elapsed.as_secs_f32());
                st.manager.get(ModuleId::AimAssist).clone()
            };
            if aim_module.enabled && aim_module.mode("update_rate", 0) == 0 {
                let tick_scale = (event.elapsed.as_secs_f32() * 20.0).clamp(0.02, 2.0);
                let target = aim_assist::step(host, &aim_module, tick_scale, true);
                let now = host.millis();
                lock_state().lock.publish(ModuleId::AimAssist, target, now);
            } else if !aim_module.enabled {
                let now = host.millis();
                lock_state().lock.publish(ModuleId::AimAssist, None, now);
            }
        });
    }

    fn movement_input(&self, host: &dyn ClientHost, event: &mut MovementInputEvent) {
        let owners = [ModuleId::Sprint, ModuleId::ToggleSneak, ModuleId::AutoWalk];
        guard::run(&owners, (), || {
            let active = mod_active(host);
            let mut st = lock_state();
            // The hook replaces vanilla's sprint latch, so it always runs through the
            // emulation, but the modules only act while Star itself is on.
            let cfg = if active {
                MoveConfig::from_modules(
                    st.manager.get(ModuleId::Sprint),
                    st.manager.get(ModuleId::ToggleSneak),
                    st.manager.get(ModuleId::AutoWalk),
                )
            } else {
                MoveConfig::default()
            };
            st.move_state.apply(cfg, event, host.millis());
        });
    }

    fn submit_custom_geometry(
        &self,
        host: &dyn ClientHost,
        event: &mut SubmitCustomGeometryEvent<'_>,
    ) {
        guard::run(WORLD_OWNERS, (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            visuals::render_world_geometry(host, event.view, event.draw, &st.manager, &st.interp);
            extras::render_world(
                host,
                event.view,
                event.draw,
                &st.manager,
                &st.interp,
                &st.trackers,
            );
            let theme = theme::Theme::from_manager(&st.manager, host.millis());
            aim_assist::render_range_circle(
                host,
                event.draw,
                st.manager.get(ModuleId::AimAssist),
                theme.accent,
            );
            lock_visuals::render_world(
                host,
                event.view,
                event.draw,
                &st.manager,
                &st.lock,
                &st.interp,
                theme.accent,
            );
        });
    }

    fn render_player_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut RenderPlayerPreEvent<'_>,
    ) -> Flow {
        guard::run(&[ModuleId::Esp], Flow::Continue, || {
            if mod_active(host) {
                let st = lock_state();
                visuals::apply_skin_player_pre(host, &st.manager, event);
            }
            Flow::Continue
        })
    }

    fn render_living_post(&self, host: &dyn ClientHost, event: &mut RenderLivingPostEvent<'_>) {
        guard::run(&[ModuleId::Esp], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            visuals::apply_skin_living_post(host, &st.manager, event);
        });
    }

    fn entity_outline(&self, host: &dyn ClientHost, event: &mut EntityOutlineEvent<'_>) {
        guard::run(&[ModuleId::Esp], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            visuals::apply_skin_outline(host, &st.manager, event);
        });
    }

    fn render_fog(&self, host: &dyn ClientHost, event: &mut RenderFogEvent<'_>) {
        guard::run(&[ModuleId::NoFog, ModuleId::VisSky, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            let no_fog = st.manager.get(ModuleId::NoFog).clone();
            visuals::apply_no_fog(&no_fog, event);
            let now = host.millis();
            vis_fx::apply_fog_data(&st.manager, &st.fx, now, event);
        });
    }

    fn compute_fog_color(&self, host: &dyn ClientHost, event: &mut ComputeFogColorEvent<'_>) {
        guard::run(&[ModuleId::VisSky, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            let theme = Theme::from_manager(&st.manager, host.millis());
            vis_fx::apply_fog_color(&st.manager, &st.fx, &theme, host.millis(), event);
        });
    }

    fn compute_fov(&self, host: &dyn ClientHost, event: &mut ComputeFovEvent) {
        guard::run(&[ModuleId::VisCamera, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            vis_fx::apply_fov(&st.manager, &st.fx, &st.trackers, host.millis(), event);
        });
    }

    fn camera_roll(&self, host: &dyn ClientHost, event: &mut CameraRollEvent) {
        guard::run(&[ModuleId::VisCamera, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            vis_fx::apply_roll(&st.manager, &st.fx, host.millis(), event);
        });
    }

    fn biome_colors(&self, host: &dyn ClientHost, event: &mut BiomeColorsEvent<'_>) {
        guard::run(&[ModuleId::VisWorld, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            // try_lock: refresh_biome_colors may re-enter this hook. If the tick
            // path still held STATE we'd deadlock with a blocking lock.
            let st = match state().try_lock() {
                Ok(g) => g,
                Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            vis_fx::apply_biome_colors(&st.manager, &st.fx, host.millis(), event);
        });
    }

    fn visuals(&self, host: &dyn ClientHost, event: &mut VisualsEvent<'_>) {
        guard::run(&[ModuleId::VisWorld, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            let theme = Theme::from_manager(&st.manager, host.millis());
            vis_fx::apply_visuals_lights(host, &st.manager, &st.fx, &theme, host.millis(), event);
        });
    }

    fn shader_params(&self, host: &dyn ClientHost, event: &mut ShaderParamsEvent<'_>) {
        guard::run(&[ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            vis_fx::apply_shader_params(&st.manager, &st.fx, host.millis(), event);
        });
    }

    fn render_level_stage(&self, host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        guard::run(&[ModuleId::VisSky, ModuleId::VisTrip], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            let theme = Theme::from_manager(&st.manager, host.millis());
            vis_fx::draw_sky_dome(&st.manager, &st.fx, &theme, host.millis(), event);
        });
    }

    fn zoom(&self, host: &dyn ClientHost, _: &mut ZoomEvent) -> Option<Zoom> {
        guard::run(&[ModuleId::Zoom], None, || {
            if !mod_active(host) {
                return None;
            }
            let zoom_mod = lock_state().manager.get(ModuleId::Zoom).clone();
            visuals::apply_zoom(&zoom_mod)
        })
    }

    fn camera_shake(&self, host: &dyn ClientHost, event: &mut CameraShakeEvent<'_>) {
        guard::run(&[ModuleId::Camera, ModuleId::VisCamera], (), || {
            if !mod_active(host) {
                return;
            }
            let st = lock_state();
            let cam = st.manager.get(ModuleId::Camera).clone();
            visuals::apply_camera_shake(&cam, event);
            vis_fx::apply_shake(&st.manager, &st.fx, host.millis(), event);
        });
    }

    fn camera_distance(&self, host: &dyn ClientHost, event: &mut CameraDistanceEvent<'_>) {
        guard::run(&[ModuleId::Camera], (), || {
            if !mod_active(host) {
                return;
            }
            let cam = lock_state().manager.get(ModuleId::Camera).clone();
            visuals::apply_camera_distance(&cam, event);
        });
    }

    fn render_name_tag(&self, host: &dyn ClientHost, event: &mut RenderNameTagEvent<'_>) {
        guard::run(&[ModuleId::Nametags], (), || {
            if !mod_active(host) {
                return;
            }
            let nt = lock_state().manager.get(ModuleId::Nametags).clone();
            visuals::apply_nametag_hide(&nt, event);
        });
    }

    fn player_break_speed(
        &self,
        _host: HostSide<'_>,
        event: &mut PlayerBreakSpeedEvent,
    ) -> Flow {
        guard::run(&[ModuleId::FastMine], Flow::Continue, || {
            let fast = lock_state().manager.get(ModuleId::FastMine).clone();
            fast_mine::apply_break_speed(&fast, event);
            Flow::Continue
        })
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        guard::run(HUD_OWNERS, (), || {
            if !mod_active(host) {
                return;
            }
            let mut st = lock_state();
            let theme = Theme::from_manager(&st.manager, host.millis());
            let now = host.millis();
            vis_fx::draw_screen_overlays(
                host,
                event.draw,
                &st.manager,
                &st.fx,
                &theme,
                now,
            );
            let ClientState {
                manager,
                interp,
                lock,
                trackers,
                toasts,
                ..
            } = &mut *st;
            hud::render(
                host,
                event.draw,
                manager,
                interp,
                lock,
                trackers,
                toasts,
            );
        });
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        guard::run(&[], (), || {
            if host.mod_screen() != Some(MOD_ID) {
                return;
            }
            let mut st = lock_state();
            let ClientState {
                manager,
                gui,
                trackers,
                ..
            } = &mut *st;
            gui.render(host, event.draw, event.screen.mouse, manager, trackers);
        });
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if host.mod_screen() != Some(MOD_ID) {
                return Flow::Continue;
            }
            let mut st = lock_state();
            let ClientState { manager, gui, .. } = &mut *st;
            if gui.mouse_pressed(event.screen.mouse, event.button, manager) {
                if let Some(action) = gui.take_clipboard_action() {
                    run_clipboard_action(host, manager, action);
                }
                config::save(host, manager);
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if host.mod_screen() != Some(MOD_ID) {
                return Flow::Continue;
            }
            let mut st = lock_state();
            let ClientState { manager, gui, .. } = &mut *st;
            if gui.mouse_dragged(event.screen.mouse, manager) {
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        _event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if host.mod_screen() != Some(MOD_ID) {
                return Flow::Continue;
            }
            let mut st = lock_state();
            let ClientState { manager, gui, .. } = &mut *st;
            if gui.mouse_released() {
                config::save(host, manager);
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if host.mod_screen() != Some(MOD_ID) {
                return Flow::Continue;
            }
            let mut st = lock_state();
            let ClientState { manager, gui, .. } = &mut *st;
            if gui.mouse_scrolled(event.screen.mouse, event.scroll[1], event.modifiers, manager) {
                config::save_throttled(host, manager);
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if !mod_active(host) {
                return Flow::Continue;
            }
            if host.mod_screen() == Some(MOD_ID) {
                let mut st = lock_state();
                let ClientState { manager, gui, .. } = &mut *st;
                if gui.key_pressed(event.key, manager) {
                    config::save(host, manager);
                    return Flow::Cancel;
                }
                return Flow::Continue;
            }
            // Steal the open key from pause/inventory/chat/etc. so those screens
            // don't consume it; `input_key` (mutable host) opens the ClickGUI after.
            if event.repeat {
                return Flow::Continue;
            }
            let configured = {
                let st = lock_state();
                st.manager
                    .gui_key()
                    .unwrap_or(DEFAULT_GUI_KEY)
                    .to_owned()
            };
            if configured == event.key {
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        guard::run(&[], Flow::Continue, || {
            if host.mod_screen() != Some(MOD_ID) {
                return Flow::Continue;
            }
            let mut st = lock_state();
            if st.gui.char_typed(event.c) {
                return Flow::Cancel;
            }
            Flow::Continue
        })
    }

    fn screen_closing(&self, host: &dyn ClientHost, _event: &mut ScreenClosingEvent<'_>) {
        guard::run(&[], (), || {
            if host.mod_screen() != Some(MOD_ID) {
                return;
            }
            let mut st = lock_state();
            st.gui.on_close();
            config::save(host, &mut st.manager);
        });
    }
}
