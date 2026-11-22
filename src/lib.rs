//! Star — Meteor-inspired modular client for Horizon (Minecraft 26.3 in Rust).
//!
//! Press Right Shift (customizable in Controls or the Settings tab) while in a world
//! to open the ClickGUI.

mod config;
mod entity_filter;
mod font;
mod gui;
mod hud;
mod module;
mod modules;
mod settings;
mod theme;

use std::sync::{Mutex, OnceLock};

use mod_api::{
    self, CameraDistanceEvent, CameraShakeEvent, ClientHost, ClientTickPreEvent, Flow, HostSide,
    InputKeyEvent, KeyAction, KeyConflictContext, KeyRegistration, PlayerBreakSpeedEvent,
    Registrar, RegistrationContext, RenderFogEvent, RenderFramePreEvent, RenderGuiPostEvent,
    RenderNameTagEvent, ScreenCharTypedPreEvent, ScreenClosingEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, SettingSpec,
    SubmitCustomGeometryEvent, Zoom, ZoomEvent,
};

use crate::gui::ClickGuiState;
use crate::module::{ModuleId, ModuleManager};
use crate::modules::aim_assist::{self, AimTargetInfo};
use crate::modules::{fast_mine, visuals};

const MOD_ID: &str = "star";
const OPEN_GUI_KEY_ID: &str = "open_gui";

struct ClientState {
    manager: ModuleManager,
    gui: ClickGuiState,
    aim_target: Option<AimTargetInfo>,
}

impl ClientState {
    fn new() -> Self {
        Self {
            manager: ModuleManager::new(),
            gui: ClickGuiState::default(),
            aim_target: None,
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

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, ctx: &RegistrationContext) {
        if let Ok(entity_types) = ctx.registry("minecraft:entity_type") {
            entity_filter::init_catalog(&entity_types);
        }

        let _ = registrar.register_setting(
            SettingSpec::toggle("enabled", "star.setting.enabled", true)
                .tooltip("star.setting.enabled.tip"),
        );
        registrar.register_key_mapping(
            KeyRegistration::new(OPEN_GUI_KEY_ID, "key.keyboard.right.shift")
                .label("star.key.open_gui")
                .category("star.category.keys")
                .context(KeyConflictContext::Universal),
        );
    }

    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if !mod_active(host) || event.action != KeyAction::Press {
            return;
        }

        let controls_key = host.mod_key(MOD_ID, OPEN_GUI_KEY_ID).map(|(k, _)| k);

        enum Action {
            None,
            OpenGui,
            CloseGui,
        }

        let action = {
            let mut st = lock_state();
            if !st.manager.config_loaded {
                config::load(host, &mut st.manager);
            }

            let own_screen = host.mod_screen() == Some(MOD_ID);
            if own_screen && st.gui.is_listening() {
                Action::None
            } else {
                let configured_key = st
                    .manager
                    .gui_key()
                    .map(str::to_owned)
                    .or(controls_key);
                let is_gui_key = configured_key.as_deref() == Some(event.key);

                if is_gui_key {
                    if own_screen {
                        Action::CloseGui
                    } else if host.playing() {
                        Action::OpenGui
                    } else {
                        Action::None
                    }
                } else if host.playing() {
                    st.manager.trigger_keybind(event.key);
                    config::save(host, &mut st.manager);
                    Action::None
                } else {
                    Action::None
                }
            }
        };

        match action {
            Action::OpenGui => {
                let _ = host.mod_screen_open(MOD_ID);
            }
            Action::CloseGui => {
                host.mod_screen_close(MOD_ID);
            }
            Action::None => {}
        }
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        if !mod_active(host) {
            lock_state().aim_target = None;
            return;
        }

        let (aim_module, should_load, should_save) = {
            let st = lock_state();
            (
                st.manager.get(ModuleId::AimAssist).clone(),
                !st.manager.config_loaded,
                st.manager.dirty,
            )
        };

        if should_load {
            let mut st = lock_state();
            config::load(host, &mut st.manager);
        } else if should_save {
            let mut st = lock_state();
            config::save(host, &mut st.manager);
        }

        if aim_module.enabled && aim_module.mode("update_rate", 0) == 1 {
            let target = aim_assist::step(host, &aim_module, 1.0, false);
            lock_state().aim_target = target;
        } else if !aim_module.enabled || !host.playing() {
            lock_state().aim_target = None;
        }
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        if !mod_active(host) || !host.playing() {
            return;
        }

        let aim_module = lock_state().manager.get(ModuleId::AimAssist).clone();
        if aim_module.enabled && aim_module.mode("update_rate", 0) == 0 {
            let tick_scale = (event.elapsed.as_secs_f32() * 20.0).clamp(0.02, 2.0);
            let target = aim_assist::step(host, &aim_module, tick_scale, true);
            lock_state().aim_target = target;
        }
    }

    fn submit_custom_geometry(
        &self,
        host: &dyn ClientHost,
        event: &mut SubmitCustomGeometryEvent<'_>,
    ) {
        if !mod_active(host) {
            return;
        }
        let st = lock_state();
        visuals::render_world_geometry(host, event.view, event.draw, &st.manager);
    }

    fn render_fog(&self, host: &dyn ClientHost, event: &mut RenderFogEvent<'_>) {
        if !mod_active(host) {
            return;
        }
        let no_fog = lock_state().manager.get(ModuleId::NoFog).clone();
        visuals::apply_no_fog(&no_fog, event);
    }

    fn zoom(&self, host: &dyn ClientHost, _: &mut ZoomEvent) -> Option<Zoom> {
        if !mod_active(host) {
            return None;
        }
        let zoom_mod = lock_state().manager.get(ModuleId::Zoom).clone();
        visuals::apply_zoom(&zoom_mod)
    }

    fn camera_shake(&self, host: &dyn ClientHost, event: &mut CameraShakeEvent<'_>) {
        if !mod_active(host) {
            return;
        }
        let cam = lock_state().manager.get(ModuleId::Camera).clone();
        visuals::apply_camera_shake(&cam, event);
    }

    fn camera_distance(&self, host: &dyn ClientHost, event: &mut CameraDistanceEvent<'_>) {
        if !mod_active(host) {
            return;
        }
        let cam = lock_state().manager.get(ModuleId::Camera).clone();
        visuals::apply_camera_distance(&cam, event);
    }

    fn render_name_tag(&self, host: &dyn ClientHost, event: &mut RenderNameTagEvent<'_>) {
        if !mod_active(host) {
            return;
        }
        let nt = lock_state().manager.get(ModuleId::Nametags).clone();
        visuals::apply_nametag_hide(&nt, event);
    }

    fn player_break_speed(
        &self,
        _host: HostSide<'_>,
        event: &mut PlayerBreakSpeedEvent,
    ) -> Flow {
        let fast = lock_state().manager.get(ModuleId::FastMine).clone();
        fast_mine::apply_break_speed(&fast, event);
        Flow::Continue
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !mod_active(host) {
            return;
        }
        let st = lock_state();
        hud::render(host, event.draw, &st.manager, st.aim_target.as_ref());
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(MOD_ID) {
            return;
        }
        let mut st = lock_state();
        let ClientState { manager, gui, .. } = &mut *st;
        gui.render(host, event.draw, event.screen.mouse, manager);
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = lock_state();
        let ClientState { manager, gui, .. } = &mut *st;
        if gui.mouse_pressed(event.screen.mouse, event.button, manager) {
            config::save(host, manager);
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = lock_state();
        let ClientState { manager, gui, .. } = &mut *st;
        if gui.mouse_dragged(event.screen.mouse, manager) {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        _event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
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
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = lock_state();
        let ClientState { manager, gui, .. } = &mut *st;
        if gui.mouse_scrolled(event.screen.mouse, event.scroll[1], event.modifiers, manager) {
            config::save(host, manager);
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = lock_state();
        let ClientState { manager, gui, .. } = &mut *st;
        if gui.key_pressed(event.key, manager) {
            config::save(host, manager);
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = lock_state();
        if st.gui.char_typed(event.c) {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn screen_closing(&self, host: &dyn ClientHost, _event: &mut ScreenClosingEvent<'_>) {
        if host.mod_screen() != Some(MOD_ID) {
            return;
        }
        let mut st = lock_state();
        st.gui.on_close();
        config::save(host, &mut st.manager);
    }
}
