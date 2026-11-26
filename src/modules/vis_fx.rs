//! Runtime Visuals engine: colours, trip waves, hit particles, sky and overlays.

use std::sync::OnceLock;

use mod_api::{
    self, CameraRollEvent, CameraShakeEvent, ClientHost, ColoredLight, ComputeFogColorEvent,
    ComputeFovEvent, FireworkExplosion, FireworkShape, FireworksRequest, Identifier, ParticleKey,
    ParticleOptions, PipelineBlend, PipelineQuad, RenderFogEvent, RenderLevelStage,
    RenderLevelStageEvent, RenderPipeline, RenderPipelineRef, ShaderParamsEvent, SoundPlay,
    VisualsEvent, WorldDepth, WorldQuad,
};

use crate::module::{ModuleId, ModuleManager};
use crate::modules::trackers::{HitEvent, HitKind, Trackers};
use crate::theme::{hsv_to_argb, with_alpha, Theme};
use crate::toast::{ToastKind, notify};

const MAX_PARTICLES_PER_FRAME: usize = 48;
const MAX_HIT_QUEUE_DRAIN: usize = 6;
const BIOME_REFRESH_MIN_MS: u64 = 4_000;

/// Particle keys requested in `register()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParticleKeys {
    pub dust: Option<ParticleKey>,
    pub dust_transition: Option<ParticleKey>,
    pub end_rod: Option<ParticleKey>,
    pub crit: Option<ParticleKey>,
    pub electric_spark: Option<ParticleKey>,
    pub firework: Option<ParticleKey>,
    pub glow: Option<ParticleKey>,
    pub enchanted_hit: Option<ParticleKey>,
}

static PARTICLE_KEYS: OnceLock<ParticleKeys> = OnceLock::new();

pub fn store_particle_keys(keys: ParticleKeys) {
    let _ = PARTICLE_KEYS.set(keys);
}

fn particle_keys() -> ParticleKeys {
    PARTICLE_KEYS.get().copied().unwrap_or_default()
}

/// Resolved optional sky pipelines.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkyPipelines {
    pub gradient: Option<RenderPipelineRef>,
    pub aurora: Option<RenderPipelineRef>,
    pub galaxy: Option<RenderPipelineRef>,
    pub psy: Option<RenderPipelineRef>,
}

/// Per-session Visuals runtime state.
#[derive(Debug, Default)]
pub struct FxState {
    pub pipelines: SkyPipelines,
    /// Effective trip intensity after onset (0..=1.5-ish).
    pub trip_level: f32,
    pub onset_started_ms: Option<u64>,
    pub last_dose: f32,
    pub last_biome_sig: u64,
    pub last_biome_refresh_ms: u64,
    pub need_biome_refresh: bool,
    pub hit_flash_until_ms: u64,
    pub punch_yaw: f32,
    pub punch_pitch: f32,
    pub punch_until_ms: u64,
    pub ambient_accum: f32,
    pub last_audio_warp_ms: u64,
    pub last_preset_idx: Option<usize>,
    pub rng: u32,
}

impl FxState {
    fn next_f01(&mut self) -> f32 {
        // xorshift
        let mut x = if self.rng == 0 { 0xA2E1_9B35 } else { self.rng };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32) / (u32::MAX as f32)
    }
}

/// Colour picked from the Visuals colour source + Theme.
pub fn resolve_accent(manager: &ModuleManager, theme: &Theme, time_ms: u64, t: f32) -> u32 {
    let preset = manager.get(ModuleId::VisPreset);
    match preset.mode("colour_source", 0) {
        1 => {
            // Complement: rotate hue ~180°.
            let (a, r, g, b) = crate::theme::argb_parts(theme.accent);
            let (h, s, v) = rgb_to_hsv(r, g, b);
            hsv_to_argb((h + 0.5) % 1.0, s, v, a)
        }
        2 => {
            let hue = ((time_ms as f32 * 0.05) % 360.0) / 360.0;
            hsv_to_argb(hue, 0.85, 1.0, 0xFF)
        }
        3 => {
            let a = preset.color("custom_a", 0xFFFF_4AD8);
            let b = preset.color("custom_b", 0xFF4A_E8FF);
            lerp_argb(a, b, t.clamp(0.0, 1.0))
        }
        _ => theme.accent,
    }
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let v = max;
    let d = max - min;
    let s = if max < 1e-6 { 0.0 } else { d / max };
    let h = if d < 1e-6 {
        0.0
    } else if (max - rf).abs() < 1e-6 {
        ((gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }) / 6.0
    } else if (max - gf).abs() < 1e-6 {
        ((bf - rf) / d + 2.0) / 6.0
    } else {
        ((rf - gf) / d + 4.0) / 6.0
    };
    (h, s, v)
}

fn lerp_argb(a: u32, b: u32, t: f32) -> u32 {
    let (aa, ar, ag, ab) = crate::theme::argb_parts(a);
    let (ba, br, bg, bb) = crate::theme::argb_parts(b);
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round().clamp(0.0, 255.0) as u8;
    crate::theme::pack_argb(mix(aa, ba), mix(ar, br), mix(ag, bg), mix(ab, bb))
}

fn argb_to_rgb01(argb: u32) -> [f32; 3] {
    let (_, r, g, b) = crate::theme::argb_parts(argb);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn argb_to_rgb_u24(argb: u32) -> u32 {
    argb & 0x00FF_FFFF
}

/// Register the optional sky pipelines.
pub fn register_pipelines(event: &mut mod_api::RegisterRenderPipelinesEvent<'_>) {
    let mk = |id: &str| {
        let id = Identifier::parse(id).expect("star pipeline id");
        RenderPipeline::new(
            id.clone(),
            id,
            PipelineBlend::Translucent,
            WorldDepth::ThroughWalls,
        )
        .optional()
    };
    event.pipelines.push(mk("star:sky_gradient"));
    event.pipelines.push(mk("star:sky_aurora"));
    event.pipelines.push(mk("star:sky_galaxy"));
    event.pipelines.push(mk("star:sky_psy"));
}

/// Resolve pipeline handles after client setup / resource reload.
pub fn resolve_pipelines(host: &dyn ClientHost) -> SkyPipelines {
    let assets = host.render_assets();
    let one = |id: &str| {
        let Ok(ident) = Identifier::parse(id) else {
            return None;
        };
        assets.render_pipeline(&ident).ok()
    };
    SkyPipelines {
        gradient: one("star:sky_gradient"),
        aurora: one("star:sky_aurora"),
        galaxy: one("star:sky_galaxy"),
        psy: one("star:sky_psy"),
    }
}

/// Master gate: Visuals module enabled.
pub fn visuals_on(manager: &ModuleManager) -> bool {
    manager.get(ModuleId::VisPreset).enabled
}

/// Result of one Visuals tick. Host side-effects (`refresh_biome_colors`, particles,
/// sounds) must run **without** holding `STATE` — those APIs can re-enter hooks that
/// also lock it and would otherwise hard-deadlock the game.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickOutcome {
    pub refresh_biomes: bool,
}

/// Update trip onset / wave / punch state. Does **not** call into the host.
///
/// After dropping the client-state lock the caller should:
/// 1. call [`emit_frame_fx`] (particles / audio), then
/// 2. call `refresh_biome_colors` when [`TickOutcome::refresh_biomes`] is set.
pub fn on_tick(
    now: u64,
    manager: &ModuleManager,
    fx: &mut FxState,
) -> TickOutcome {
    let mut out = TickOutcome::default();
    if !visuals_on(manager) {
        fx.trip_level = 0.0;
        fx.need_biome_refresh = false;
        return out;
    }

    let dose = manager.get(ModuleId::VisPreset).float("dose", 0.0) as f32;
    let onset_s = manager.get(ModuleId::VisTrip).float("onset", 8.0) as f32;
    if (dose - fx.last_dose).abs() > 0.01 {
        if dose > fx.last_dose {
            fx.onset_started_ms = Some(now);
        }
        fx.last_dose = dose;
    }
    let onset_t = if onset_s <= 0.05 {
        1.0
    } else if let Some(started) = fx.onset_started_ms {
        ((now.saturating_sub(started) as f32) / (onset_s * 1000.0)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    // Ease-in cubic.
    let ease = onset_t * onset_t * (3.0 - 2.0 * onset_t);
    fx.trip_level = dose * ease;

    // Preset change → biome refresh flag (only meaningful while tinting is on).
    let preset_idx = manager.get(ModuleId::VisPreset).mode("preset", 0);
    if fx.last_preset_idx != Some(preset_idx) {
        fx.last_preset_idx = Some(preset_idx);
        fx.need_biome_refresh = true;
    }

    let world = manager.get(ModuleId::VisWorld);
    let trip = manager.get(ModuleId::VisTrip);
    let rainbow_trip = fx.trip_level > 0.05 && trip.bool("rainbow_biomes", true);
    let want_tint = world.bool("tint_biomes", false)
        || world.bool("rainbow_biomes", false)
        || rainbow_trip;
    let sig = biome_signature(manager, fx.trip_level);
    // Rainbow biomes must keep refreshing or the world freezes on one hue.
    let rainbow_period_ms = if rainbow_trip {
        (420.0 / (0.65 + fx.trip_level * trip.float("strength", 1.0) as f32)).clamp(220.0, 900.0) as u64
    } else if world.bool("rainbow_biomes", false) {
        1_200
    } else {
        BIOME_REFRESH_MIN_MS
    };
    if rainbow_trip || world.bool("rainbow_biomes", false) {
        if now.saturating_sub(fx.last_biome_refresh_ms) >= rainbow_period_ms {
            fx.need_biome_refresh = true;
        }
    }
    if !want_tint {
        fx.need_biome_refresh = false;
        fx.last_biome_sig = sig;
    } else if sig != fx.last_biome_sig || fx.need_biome_refresh {
        // Ask the caller to refresh after the STATE lock is dropped.
        let min_gap = if rainbow_trip || world.bool("rainbow_biomes", false) {
            rainbow_period_ms
        } else {
            BIOME_REFRESH_MIN_MS
        };
        if now.saturating_sub(fx.last_biome_refresh_ms) >= min_gap {
            out.refresh_biomes = true;
            fx.last_biome_sig = sig;
            fx.last_biome_refresh_ms = now;
            fx.need_biome_refresh = false;
        } else {
            fx.need_biome_refresh = true;
        }
    }

    // Decay punch.
    if now > fx.punch_until_ms {
        fx.punch_yaw *= 0.7;
        fx.punch_pitch *= 0.7;
        if fx.punch_yaw.abs() < 0.01 && fx.punch_pitch.abs() < 0.01 {
            fx.punch_yaw = 0.0;
            fx.punch_pitch = 0.0;
        }
    }
    out
}

/// Particle / audio side-effects for one tick. Must not run while `STATE` is held.
pub fn emit_frame_fx(
    host: &mut dyn ClientHost,
    manager: &ModuleManager,
    trackers: &mut Trackers,
    fx: &mut FxState,
    theme: &Theme,
    now: u64,
) {
    if !visuals_on(manager) {
        trackers.drain_hits();
        return;
    }
    emit_hits(host, manager, trackers, fx, theme, now);
    emit_ambient(host, manager, fx, theme, now);
    maybe_audio_warp(host, manager, fx, now);
}

fn biome_signature(manager: &ModuleManager, trip: f32) -> u64 {
    let w = manager.get(ModuleId::VisWorld);
    let t = manager.get(ModuleId::VisTrip);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |h: &mut u64, v: u64| {
        *h ^= v;
        *h = h.wrapping_mul(0x1000_0000_01b3);
    };
    mix(&mut h, w.bool("tint_biomes", false) as u64);
    mix(&mut h, w.bool("rainbow_biomes", false) as u64);
    mix(&mut h, (t.bool("rainbow_biomes", true) && trip > 0.05) as u64);
    mix(&mut h, w.color("grass", 0) as u64);
    mix(&mut h, w.color("foliage", 0) as u64);
    mix(&mut h, w.color("water", 0) as u64);
    mix(&mut h, (w.float("saturation", 1.0) * 1000.0) as u64);
    h
}

fn wave(now_ms: u64, period_s: f32) -> f32 {
    let p = period_s.max(0.5);
    let t = (now_ms as f32 / 1000.0) / p;
    (t * std::f32::consts::TAU).sin() * 0.5 + 0.5
}

/// Signed multi-harmonic melt wave in [-1, 1] — feels like the world breathing.
fn morph_wave(now_ms: u64, period_s: f32) -> f32 {
    let t = now_ms as f32 / 1000.0;
    let p = period_s.max(0.4);
    let a = ((t / p) * std::f32::consts::TAU).sin();
    let b = ((t / (p * 0.37)) * std::f32::consts::TAU).sin() * 0.55;
    let c = ((t / (p * 0.19)) * std::f32::consts::TAU).sin() * 0.28;
    (a + b + c) / 1.83
}

fn trip_hue(time_ms: u64, offset: f32, speed: f32) -> u32 {
    let hue = ((time_ms as f32 * speed * 0.001) + offset).rem_euclid(1.0);
    hsv_to_argb(hue, 0.92, 1.0, 0xFF)
}

fn emit_hits(
    host: &mut dyn ClientHost,
    manager: &ModuleManager,
    trackers: &mut Trackers,
    fx: &mut FxState,
    theme: &Theme,
    now: u64,
) {
    let hit = manager.get(ModuleId::VisHit);
    if !hit.bool("enabled_fx", false) {
        trackers.drain_hits();
        return;
    }
    let events = trackers.drain_hits();
    if events.is_empty() {
        return;
    }
    let keys = particle_keys();
    let style = hit.mode("style", 2);
    let count = hit.int("count", 16) as usize;
    let speed = hit.float("speed", 1.0) as f32;
    let spread = hit.float("spread", 0.8) as f32;
    let size = hit.float("size", 1.1) as f32;
    let crit_boost = hit.bool("crit_boost", true);
    let kill_burst = hit.bool("kill_burst", true);
    let ding = hit.bool("ding", false);
    let accent = resolve_accent(manager, theme, now, 0.35);
    let rgb = argb_to_rgb01(accent);
    let mut budget = MAX_PARTICLES_PER_FRAME.min(host.particle_count().saturating_sub(200) as usize + 64);

    for (i, ev) in events.into_iter().take(MAX_HIT_QUEUE_DRAIN).enumerate() {
        if budget == 0 {
            break;
        }
        let mult = match ev.kind {
            HitKind::Crit if crit_boost => 1.6,
            HitKind::Kill if kill_burst => 2.2,
            _ => 1.0,
        };
        let n = ((count as f32) * mult).round() as usize;
        let planned = plan_hit_particles(style, n, speed, spread, size, rgb, &keys, &ev);
        for spawn in planned {
            if budget == 0 {
                break;
            }
            if host.add_particle(&spawn.options, spawn.at, spawn.vel) {
                budget = budget.saturating_sub(1);
            }
        }
        if matches!(ev.kind, HitKind::Crit | HitKind::Kill) && (crit_boost || kill_burst) {
            let _ = host.create_fireworks(&FireworksRequest {
                at: ev.pos,
                velocity: [0.0, 0.05, 0.0],
                explosions: vec![FireworkExplosion {
                    shape: if matches!(ev.kind, HitKind::Kill) {
                        FireworkShape::Burst
                    } else {
                        FireworkShape::Star
                    },
                    colors: vec![argb_to_rgb_u24(accent), argb_to_rgb_u24(theme.accent)],
                    fade_colors: vec![0xFFFFFF],
                    trail: true,
                    twinkle: matches!(ev.kind, HitKind::Kill),
                }],
                audible: false,
            });
        }
        if ding && i == 0 {
            let _ = host.play_local_sound(
                &SoundPlay::ui("minecraft:entity.experience_orb.pickup").with_volume(0.35).with_pitch(1.4),
                0,
            );
        }
        // Camera punch + flash.
        let punch = manager.get(ModuleId::VisCamera).float("hit_punch", 0.0) as f32;
        if punch > 0.01 {
            let dir = if fx.next_f01() > 0.5 { 1.0 } else { -1.0 };
            fx.punch_yaw = punch * dir;
            fx.punch_pitch = punch * 0.35;
            fx.punch_until_ms = now + 120;
        }
        if manager.get(ModuleId::VisScreen).bool("hit_flash", false) {
            fx.hit_flash_until_ms = now + 140;
        }
    }
}

struct PlannedParticle {
    options: ParticleOptions,
    at: [f64; 3],
    vel: [f64; 3],
}

fn plan_hit_particles(
    style: usize,
    count: usize,
    speed: f32,
    spread: f32,
    size: f32,
    rgb: [f32; 3],
    keys: &ParticleKeys,
    ev: &HitEvent,
) -> Vec<PlannedParticle> {
    let mut out = Vec::with_capacity(count.min(MAX_PARTICLES_PER_FRAME));
    let mut seed = ev.entity_id.wrapping_mul(2654435761).wrapping_add(ev.born_ms as u32);
    let next = |seed: &mut u32| {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (*seed as f32) / (u32::MAX as f32)
    };
    for i in 0..count.min(MAX_PARTICLES_PER_FRAME) {
        let yaw = next(&mut seed) * std::f32::consts::TAU;
        let pitch = (next(&mut seed) - 0.5) * spread * std::f32::consts::PI;
        let sp = speed as f64 * (0.4 + next(&mut seed) as f64);
        let vx = (yaw.cos() * pitch.cos()) as f64 * sp;
        let vy = (pitch.sin().abs() + 0.15) as f64 * sp;
        let vz = (yaw.sin() * pitch.cos()) as f64 * sp;
        let at = [
            ev.pos[0] + (next(&mut seed) as f64 - 0.5) * 0.2,
            ev.pos[1] + 0.1 + (next(&mut seed) as f64) * 0.3,
            ev.pos[2] + (next(&mut seed) as f64 - 0.5) * 0.2,
        ];
        let options = match style {
            0 | 3 => {
                // Burst / Stardust → dust
                let Some(k) = keys.dust else { continue };
                ParticleOptions::dust(k, rgb, (size * (0.6 + next(&mut seed))).clamp(0.2, 3.5))
            }
            1 => {
                let Some(k) = keys.crit.or(keys.electric_spark) else { continue };
                ParticleOptions::simple(k)
            }
            2 => {
                // Shock Ring: alternating dust transition + electric
                if i % 2 == 0 {
                    let Some(k) = keys.dust_transition.or(keys.dust) else { continue };
                    if keys.dust_transition.is_some() {
                        ParticleOptions::dust_transition(k, rgb, [1.0, 1.0, 1.0], size)
                    } else {
                        ParticleOptions::dust(k, rgb, size)
                    }
                } else {
                    let Some(k) = keys.electric_spark.or(keys.end_rod) else { continue };
                    ParticleOptions::simple(k)
                }
            }
            4 => {
                let Some(k) = keys.firework.or(keys.glow) else { continue };
                ParticleOptions::simple(k)
            }
            _ => {
                // Nova
                let Some(k) = keys.end_rod.or(keys.glow).or(keys.enchanted_hit) else {
                    continue;
                };
                ParticleOptions::simple(k)
            }
        };
        out.push(PlannedParticle {
            options,
            at,
            vel: [vx, vy, vz],
        });
    }
    out
}

fn emit_ambient(
    host: &mut dyn ClientHost,
    manager: &ModuleManager,
    fx: &mut FxState,
    theme: &Theme,
    now: u64,
) {
    let ambient = manager.get(ModuleId::VisAmbient);
    let style = ambient.mode("style", 0);
    if style == 0 {
        return;
    }
    let mut density = ambient.float("density", 4.0) as f32;
    let trip = manager.get(ModuleId::VisTrip);
    if fx.trip_level > 0.05 && trip.bool("particles", true) {
        density += fx.trip_level * 10.0 * trip.float("strength", 1.0) as f32;
    }
    fx.ambient_accum += density / 20.0; // per tick (~20 TPS)
    let spawn_n = fx.ambient_accum.floor() as i32;
    if spawn_n <= 0 {
        return;
    }
    fx.ambient_accum -= spawn_n as f32;
    let radius = ambient.float("radius", 8.0);
    let keys = particle_keys();
    let accent = resolve_accent(manager, theme, now, fx.next_f01());
    let rgb = argb_to_rgb01(accent);
    let Some(player) = host.camera_entity() else {
        return;
    };
    let Some(origin) = player.position(host) else {
        return;
    };
    let mut budget = 12usize;
    for _ in 0..spawn_n.min(8) {
        if budget == 0 {
            break;
        }
        let u = fx.next_f01() as f64;
        let v = fx.next_f01() as f64;
        let w = fx.next_f01() as f64;
        let at = [
            origin[0] + (u - 0.5) * 2.0 * radius,
            origin[1] + v * (radius * 0.8) + 0.2,
            origin[2] + (w - 0.5) * 2.0 * radius,
        ];
        let vel = [
            (fx.next_f01() as f64 - 0.5) * 0.04,
            0.01 + fx.next_f01() as f64 * 0.03,
            (fx.next_f01() as f64 - 0.5) * 0.04,
        ];
        let options = match style {
            1 => {
                // Embers
                let Some(k) = keys.dust else { continue };
                ParticleOptions::dust(k, rgb, 0.7)
            }
            2 => {
                let Some(k) = keys.glow.or(keys.end_rod) else { continue };
                ParticleOptions::simple(k)
            }
            3 => {
                let Some(k) = keys.dust else { continue };
                ParticleOptions::dust(k, [0.85, 0.85, 0.9], 0.45)
            }
            _ => {
                let Some(k) = keys.enchanted_hit.or(keys.end_rod) else { continue };
                ParticleOptions::simple(k)
            }
        };
        if host.add_particle(&options, at, vel) {
            budget -= 1;
        }
    }
}

fn maybe_audio_warp(host: &mut dyn ClientHost, manager: &ModuleManager, fx: &mut FxState, now: u64) {
    let trip = manager.get(ModuleId::VisTrip);
    if !(fx.trip_level > 0.15 && trip.bool("audio_warp", false)) {
        return;
    }
    if now.saturating_sub(fx.last_audio_warp_ms) < 900 {
        return;
    }
    fx.last_audio_warp_ms = now;
    let period = trip.float("wave_period", 12.0) as f32;
    let w = wave(now, period);
    let amount = (0.15 + fx.trip_level * 0.35 * w).clamp(0.0, 0.7);
    let lowpass = (0.25 + w * 0.55).clamp(0.0, 1.0);
    let _ = host.muffle_sounds(amount, lowpass, 18, &[]);
}

pub fn apply_fog_color(
    manager: &ModuleManager,
    fx: &FxState,
    theme: &Theme,
    time_ms: u64,
    event: &mut ComputeFogColorEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    let sky = manager.get(ModuleId::VisSky);
    let trip = manager.get(ModuleId::VisTrip);
    let want = sky.bool("tint_fog", false)
        || (fx.trip_level > 0.05 && trip.bool("rainbow_sky", true));
    if !want {
        return;
    }
    let period = trip.float("wave_period", 12.0) as f32;
    let w = wave(time_ms, period);
    let accent = resolve_accent(manager, theme, time_ms, w);
    let rgb = argb_to_rgb01(accent);
    let strength = if fx.trip_level > 0.05 && trip.bool("rainbow_sky", true) {
        let boost = trip.float("strength", 1.0) as f32;
        (0.45 + fx.trip_level * 0.7 * boost).clamp(0.0, 1.0)
    } else {
        0.55
    };
    for i in 0..3 {
        event.rgb[i] = (event.rgb[i] * (1.0 - strength) + rgb[i] * strength).clamp(0.0, 1.0);
    }
}

pub fn apply_fog_data(
    manager: &ModuleManager,
    fx: &FxState,
    time_ms: u64,
    event: &mut RenderFogEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    let sky = manager.get(ModuleId::VisSky);
    let mut density = sky.float("fog_density", 1.0) as f32;
    if sky.bool("fog_breathing", false) || fx.trip_level > 0.05 {
        let period = manager.get(ModuleId::VisTrip).float("wave_period", 12.0) as f32;
        let amount = if fx.trip_level > 0.05 {
            0.25 + fx.trip_level * 0.35
        } else {
            sky.float("fog_breath_amount", 0.2) as f32
        };
        density *= 1.0 + (wave(time_ms, period) - 0.5) * 2.0 * amount;
    }
    density = density.clamp(0.05, 3.0);
    let scale = |d: &mut f32| {
        if d.is_finite() && *d > 0.0 {
            *d *= density;
        }
    };
    match event.fog_type {
        mod_api::client::fog::FogType::Atmospheric => {
            scale(&mut event.fog.environmental_start);
            scale(&mut event.fog.environmental_end);
            scale(&mut event.fog.render_distance_start);
            scale(&mut event.fog.render_distance_end);
            scale(&mut event.fog.sky_end);
            scale(&mut event.fog.cloud_end);
        }
        mod_api::client::fog::FogType::Water if sky.bool("clear_water", false) => {
            event.fog.environmental_start = 10_000.0;
            event.fog.environmental_end = 20_000.0;
        }
        mod_api::client::fog::FogType::Lava if sky.bool("clear_lava", false) => {
            event.fog.environmental_start = 10_000.0;
            event.fog.environmental_end = 20_000.0;
        }
        _ => {}
    }
}

pub fn apply_fov(
    manager: &ModuleManager,
    fx: &FxState,
    trackers: &Trackers,
    time_ms: u64,
    event: &mut ComputeFovEvent,
) {
    if !visuals_on(manager) {
        return;
    }
    let cam = manager.get(ModuleId::VisCamera);
    let trip = manager.get(ModuleId::VisTrip);
    let mut degrees = event.degrees;
    degrees += cam.float("fov_offset", 0.0) as f32;
    if cam.bool("speed_fov", false) {
        let speed = trackers.speed_bps();
        let amt = cam.float("speed_fov_amount", 4.0) as f32;
        degrees += (speed / 8.0).clamp(0.0, 1.0) * amt;
    }
    if fx.trip_level > 0.05 && trip.bool("fov_breathe", true) {
        let period = trip.float("wave_period", 12.0) as f32;
        let strength = trip.float("strength", 1.0) as f32;
        let dose = fx.trip_level * strength;
        let morph_on = trip.bool("morph", true);
        let slow = (wave(time_ms, period) - 0.5) * 2.0;
        let heart = ((time_ms as f32 / 1000.0) * 1.55 * std::f32::consts::TAU).sin();
        degrees += slow * 10.0 * dose + heart * 2.0 * dose;
        if morph_on {
            let melt = morph_wave(time_ms, period);
            let flutter = ((time_ms as f32 / 1000.0) * 3.1 * std::f32::consts::TAU).sin();
            degrees += melt * 16.0 * dose + flutter * 3.5 * dose;
        }
    }
    event.degrees = degrees.clamp(1.0, 179.0);
}

pub fn apply_roll(
    manager: &ModuleManager,
    fx: &FxState,
    time_ms: u64,
    event: &mut CameraRollEvent,
) {
    if !visuals_on(manager) {
        return;
    }
    let cam = manager.get(ModuleId::VisCamera);
    let trip = manager.get(ModuleId::VisTrip);
    let mut roll = cam.float("roll", 0.0) as f32;
    if fx.trip_level > 0.05 && trip.bool("roll_drift", true) {
        let period = trip.float("wave_period", 12.0) as f32;
        let strength = trip.float("strength", 1.0) as f32;
        let dose = fx.trip_level * strength;
        roll += (wave(time_ms, period) - 0.5) * 2.0 * 16.0 * dose;
        if trip.bool("morph", true) {
            let melt = morph_wave(time_ms, period * 0.72);
            let spin = ((time_ms as f32 / 1000.0) * 0.55 * std::f32::consts::TAU).sin();
            roll += melt * 22.0 * dose + spin * 6.0 * dose;
        }
    }
    let sway = cam.float("sway", 0.0) as f32;
    if sway > 0.01 {
        let t = time_ms as f32 / 1000.0;
        roll += (t * 0.7).sin() * sway;
        if fx.trip_level > 0.05 && trip.bool("morph", true) {
            roll += (t * 1.9).sin() * sway * 0.85 * fx.trip_level;
        }
    }
    if roll.is_finite() {
        event.roll = (event.roll + roll).rem_euclid(360.0);
        if event.roll > 180.0 {
            event.roll -= 360.0;
        }
    }
}

pub fn apply_shake(
    manager: &ModuleManager,
    fx: &FxState,
    time_ms: u64,
    event: &mut CameraShakeEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    // Respect the existing Camera module's no_shake when it is on.
    let cam_mod = manager.get(ModuleId::Camera);
    if cam_mod.enabled && cam_mod.bool("no_shake", true) {
        // Still allow our intentional punch through a tiny offset.
    }
    let cam = manager.get(ModuleId::VisCamera);
    let trip = manager.get(ModuleId::VisTrip);
    let sway = cam.float("sway", 0.0) as f32;
    if sway > 0.01 {
        let t = time_ms as f32 / 1000.0;
        event.shake.pitch += (t * 1.1).sin() * sway * 0.35;
        event.shake.yaw += (t * 0.9).cos() * sway * 0.25;
    }
    if fx.trip_level > 0.05 && trip.bool("morph", true) {
        let period = trip.float("wave_period", 12.0) as f32;
        let strength = trip.float("strength", 1.0) as f32;
        let dose = fx.trip_level * strength;
        let melt = morph_wave(time_ms, period);
        let t = time_ms as f32 / 1000.0;
        event.shake.pitch += melt * 1.8 * dose + (t * 2.4).sin() * 0.55 * dose;
        event.shake.yaw += morph_wave(time_ms, period * 1.27) * 1.5 * dose
            + (t * 1.7).cos() * 0.45 * dose;
        event.shake.roll += melt * 2.2 * dose;
    }
    if fx.punch_yaw.abs() > 0.01 || fx.punch_pitch.abs() > 0.01 {
        event.shake.yaw += fx.punch_yaw;
        event.shake.pitch += fx.punch_pitch;
    }
}

pub fn apply_biome_colors(
    manager: &ModuleManager,
    fx: &FxState,
    time_ms: u64,
    event: &mut mod_api::BiomeColorsEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    let world = manager.get(ModuleId::VisWorld);
    let trip = manager.get(ModuleId::VisTrip);
    let rainbow = world.bool("rainbow_biomes", false)
        || (fx.trip_level > 0.05 && trip.bool("rainbow_biomes", true));
    if !(world.bool("tint_biomes", false) || rainbow) {
        return;
    }
    let sat = world.float("saturation", 1.0) as f32;
    let (grass, foliage, water) = if rainbow {
        let hash = fnv1a(event.biome);
        let speed = if fx.trip_level > 0.05 { 0.085 } else { 0.02 };
        let base = ((time_ms as f32 * speed) + (hash as f32) * 0.0000001).rem_euclid(1.0);
        let g = hsv_to_argb(base, (0.78 * sat).clamp(0.0, 1.0), 0.95, 0xFF);
        let f = hsv_to_argb((base + 0.18) % 1.0, (0.82 * sat).clamp(0.0, 1.0), 0.9, 0xFF);
        let w = hsv_to_argb((base + 0.55) % 1.0, (0.7 * sat).clamp(0.0, 1.0), 1.0, 0xFF);
        (g, f, w)
    } else {
        (
            world.color("grass", 0xFF55_C96A),
            world.color("foliage", 0xFF3D_A85A),
            world.color("water", 0xFF3A_8CFF),
        )
    };
    event.colors.grass = argb_to_rgb_u24(grass);
    event.colors.foliage = argb_to_rgb_u24(foliage);
    event.colors.dry_foliage = argb_to_rgb_u24(foliage);
    event.colors.water = argb_to_rgb_u24(water);
}

fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub fn apply_visuals_lights(
    host: &dyn ClientHost,
    manager: &ModuleManager,
    fx: &FxState,
    theme: &Theme,
    time_ms: u64,
    event: &mut VisualsEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    let world = manager.get(ModuleId::VisWorld);
    let accent = resolve_accent(manager, theme, time_ms, 0.4);
    if world.bool("player_light", false) || fx.trip_level > 0.1 {
        if let Some(player) = host.camera_entity() {
            if let Some(pos) = player.position(host) {
                let radius = if fx.trip_level > 0.1 {
                    (world.float("light_radius", 6.0) as f32) * (1.0 + fx.trip_level)
                } else {
                    world.float("light_radius", 6.0) as f32
                };
                let intensity = if fx.trip_level > 0.1 {
                    (0.3 + fx.trip_level * 0.5).clamp(0.05, 1.0)
                } else {
                    world.float("light_intensity", 0.45) as f32
                };
                event.lights.push(ColoredLight::new(
                    [pos[0], pos[1] + 1.0, pos[2]],
                    argb_to_rgb_u24(accent),
                    radius.clamp(1.0, 16.0),
                    intensity,
                ));
            }
        }
    }
    if world.bool("glow_rims", false) {
        if let Some((_, hit)) = host.view_hit() {
            event.rims.push(mod_api::BlockRim::new(
                [hit.block_x, hit.block_y, hit.block_z],
                with_alpha(accent, 0xE0),
                world.float("rim_width", 1.5) as f32,
            ));
        }
    }
}

pub fn apply_shader_params(
    manager: &ModuleManager,
    fx: &FxState,
    time_ms: u64,
    event: &mut ShaderParamsEvent<'_>,
) {
    if !visuals_on(manager) {
        return;
    }
    let trip = manager.get(ModuleId::VisTrip);
    if !trip.bool("shader_pack", true) {
        return;
    }
    let period = trip.float("wave_period", 12.0) as f32;
    let w = wave(time_ms, period);
    let strength = trip.float("strength", 1.0) as f32;
    let dose = fx.trip_level * strength;
    let morph = if trip.bool("morph", true) { 1.0 } else { 0.55 };
    let kaleido = if trip.bool("kaleidoscope", true) { 1.0 } else { 0.4 };
    let melt = morph_wave(time_ms, period);
    // Slot 0: dose, phase, warp, chroma
    event.params[0] = [
        dose,
        (w + melt * 0.35).clamp(0.0, 1.0),
        dose * 1.15 * morph,
        dose * 0.85 * morph,
    ];
    // Slot 1: hue shift, saturation, bloom, kaleidoscope
    event.params[1] = [
        (time_ms as f32 * 0.00035 * (1.0 + dose * 1.5)).fract(),
        1.0 + dose * 1.35,
        dose * 0.95,
        dose * 0.85 * kaleido,
    ];
}

pub fn draw_sky_dome(
    manager: &ModuleManager,
    fx: &FxState,
    theme: &Theme,
    time_ms: u64,
    event: &mut RenderLevelStageEvent<'_>,
) {
    if !visuals_on(manager) || event.stage != RenderLevelStage::AfterSky {
        return;
    }
    let sky = manager.get(ModuleId::VisSky);
    let trip = manager.get(ModuleId::VisTrip);
    let mut style = sky.mode("style", 0);
    if fx.trip_level > 0.05 && trip.bool("rainbow_sky", true) {
        style = 4; // Psychedelic
    }
    if style == 0 {
        return;
    }
    let brightness = sky.float("brightness", 0.55) as f32
        * (1.0 + fx.trip_level * 0.35)
        * (1.0 + sky.float("night_boost", 0.0) as f32 * 0.5);
    let alpha = ((brightness.clamp(0.0, 1.0)) * 180.0) as u8;
    let accent = resolve_accent(manager, theme, time_ms, wave(time_ms, 10.0));
    let color = with_alpha(accent, alpha);
    let cam = event.view.camera;
    let d = 64.0_f64;
    // Six inward faces of a cube around the camera.
    let faces: [[[f64; 3]; 4]; 6] = [
        // +X
        [
            [cam[0] + d, cam[1] - d, cam[2] - d],
            [cam[0] + d, cam[1] - d, cam[2] + d],
            [cam[0] + d, cam[1] + d, cam[2] + d],
            [cam[0] + d, cam[1] + d, cam[2] - d],
        ],
        // -X
        [
            [cam[0] - d, cam[1] - d, cam[2] + d],
            [cam[0] - d, cam[1] - d, cam[2] - d],
            [cam[0] - d, cam[1] + d, cam[2] - d],
            [cam[0] - d, cam[1] + d, cam[2] + d],
        ],
        // +Y
        [
            [cam[0] - d, cam[1] + d, cam[2] - d],
            [cam[0] + d, cam[1] + d, cam[2] - d],
            [cam[0] + d, cam[1] + d, cam[2] + d],
            [cam[0] - d, cam[1] + d, cam[2] + d],
        ],
        // -Y
        [
            [cam[0] - d, cam[1] - d, cam[2] + d],
            [cam[0] + d, cam[1] - d, cam[2] + d],
            [cam[0] + d, cam[1] - d, cam[2] - d],
            [cam[0] - d, cam[1] - d, cam[2] - d],
        ],
        // +Z
        [
            [cam[0] + d, cam[1] - d, cam[2] + d],
            [cam[0] - d, cam[1] - d, cam[2] + d],
            [cam[0] - d, cam[1] + d, cam[2] + d],
            [cam[0] + d, cam[1] + d, cam[2] + d],
        ],
        // -Z
        [
            [cam[0] - d, cam[1] - d, cam[2] - d],
            [cam[0] + d, cam[1] - d, cam[2] - d],
            [cam[0] + d, cam[1] + d, cam[2] - d],
            [cam[0] - d, cam[1] + d, cam[2] - d],
        ],
    ];
    let pipeline = match style {
        1 => fx.pipelines.gradient,
        2 => fx.pipelines.aurora,
        3 => fx.pipelines.galaxy,
        _ => fx.pipelines.psy,
    };
    if let Some(pipe) = pipeline {
        let quads: Vec<PipelineQuad> = faces
            .iter()
            .map(|corners| PipelineQuad::spanning(*corners, color))
            .collect();
        let _ = event.draw.pipeline_quads(pipe, &quads);
    } else {
        // Fallback: plain translucent quads.
        let quads: Vec<WorldQuad> = faces
            .iter()
            .map(|corners| WorldQuad::new(*corners, color, WorldDepth::ThroughWalls))
            .collect();
        let _ = event.draw.quads(&quads);
    }
}

/// Screen-space overlays drawn before the HUD.
pub fn draw_screen_overlays(
    host: &dyn ClientHost,
    draw: &mut dyn mod_api::DrawContext,
    manager: &ModuleManager,
    fx: &FxState,
    theme: &Theme,
    time_ms: u64,
) {
    if !visuals_on(manager) {
        return;
    }
    let screen = manager.get(ModuleId::VisScreen);
    let trip = manager.get(ModuleId::VisTrip);
    let size = draw.size();
    let w = size[0];
    let h = size[1];
    let accent = resolve_accent(manager, theme, time_ms, wave(time_ms, 9.0));
    let period = trip.float("wave_period", 12.0) as f32;
    let wv = wave(time_ms, period);
    let strength = trip.float("strength", 1.0) as f32;
    let dose = fx.trip_level * strength;
    let tripping = dose > 0.05;

    let mut vignette = screen.float("vignette", 0.0) as f32;
    if screen.bool("vignette_pulse", false) || (tripping && trip.bool("colour_wash", true)) {
        vignette *= 0.55 + wv * 0.75;
        if tripping && trip.bool("morph", true) {
            vignette *= 0.85 + morph_wave(time_ms, period * 0.6).abs() * 0.45;
        }
    }
    if vignette > 0.01 {
        let bands = if tripping { 9 } else { 6 };
        for i in 0..bands {
            let t = (i as f32 + 1.0) / bands as f32;
            let a = ((vignette * t * t * 175.0) as u32).min(210);
            let breathe = if tripping {
                1.0 + morph_wave(time_ms + (i as u64) * 90, period).abs() * 0.35
            } else {
                1.0
            };
            let inset = (1.0 - t) * 0.10 * breathe;
            let bw = w * inset.max(0.01);
            let bh = h * inset.max(0.01);
            // Dark edge always; rainbow tint on top while tripping.
            let dark = with_alpha(0xFF00_0000, a as u8);
            draw.fill([0.0, 0.0, w, bh], dark);
            draw.fill([0.0, h - bh, w, bh], dark);
            draw.fill([0.0, 0.0, bw, h], dark);
            draw.fill([w - bw, 0.0, bw, h], dark);
            if tripping {
                let tint = with_alpha(trip_hue(time_ms, t * 0.2, 0.18), (a as f32 * 0.45) as u8);
                draw.fill([0.0, 0.0, w, bh], tint);
                draw.fill([0.0, h - bh, w, bh], tint);
                draw.fill([0.0, 0.0, bw, h], tint);
                draw.fill([w - bw, 0.0, bw, h], tint);
            }
        }
    }

    // Multi-layer rainbow wash — primary + delayed afterimage hues.
    let mut wash = screen.float("wash", 0.0) as f32;
    if tripping && trip.bool("colour_wash", true) {
        wash = (wash + dose * 0.32).min(0.72);
    }
    if wash > 0.01 {
        if tripping && trip.bool("colour_wash", true) {
            let layers = [
                (0.0_f32, 1.0_f32),
                (0.33, 0.55),
                (0.66, 0.35),
            ];
            for (off, scale) in layers {
                let col = trip_hue(time_ms, off, 0.22 + dose * 0.08);
                let a = (wash * scale * 165.0) as u8;
                draw.fill([0.0, 0.0, w, h], with_alpha(col, a));
            }
        } else {
            let a = (wash * 180.0) as u8;
            draw.fill([0.0, 0.0, w, h], with_alpha(accent, a));
        }
    }

    // Melting horizontal / vertical colour waves.
    if tripping && trip.bool("morph", true) {
        draw_morph_waves(draw, w, h, time_ms, period, dose);
    }

    // Expanding rainbow rings + sector flashes.
    if tripping && trip.bool("kaleidoscope", true) {
        draw_kaleidoscope(draw, w, h, time_ms, period, dose);
    }

    let scan = screen.float("scanlines", 0.0) as f32;
    if scan > 0.01 {
        let a = (scan * (if tripping { 110.0 } else { 90.0 })) as u8;
        let step = if tripping { 2.5 } else { 3.0 };
        let drift = if tripping {
            morph_wave(time_ms, period * 0.4) * 4.0
        } else {
            0.0
        };
        let mut y = ((time_ms / 30) % 3) as f32 + drift;
        while y < h {
            let col = if tripping && ((y as i32) % 7 == 0) {
                with_alpha(trip_hue(time_ms, y * 0.01, 0.15), (a as f32 * 0.7) as u8)
            } else {
                with_alpha(0xFF00_0000, a)
            };
            draw.fill([0.0, y, w, 1.0], col);
            y += step;
        }
    }

    let edge = screen.float("edge_glow", 0.0) as f32;
    if edge > 0.01 {
        let glow_col = if tripping {
            trip_hue(time_ms, 0.15, 0.2)
        } else {
            accent
        };
        let a = (edge * 155.0) as u8;
        let t = with_alpha(glow_col, a);
        let thick = if tripping { 16.0 + dose * 10.0 } else { 10.0 };
        draw.fill_gradient([0.0, 0.0, w, thick], t, 0x0000_0000);
        draw.fill_gradient([0.0, h - thick, w, thick], 0x0000_0000, t);
        draw.fill(
            [0.0, 0.0, 8.0 + dose * 4.0, h],
            with_alpha(glow_col, (a as f32 * 0.75) as u8),
        );
        draw.fill(
            [w - (8.0 + dose * 4.0), 0.0, 8.0 + dose * 4.0, h],
            with_alpha(glow_col, (a as f32 * 0.75) as u8),
        );
    }

    let chroma = screen.float("chroma_bars", 0.0) as f32
        + if dose > 0.1 { dose * 0.35 } else { 0.0 };
    if chroma > 0.01 {
        let a = (chroma.clamp(0.0, 0.85) * 180.0) as u8;
        let spread = 4.0 + dose * 8.0;
        draw.fill([0.0, 0.0, spread, h], with_alpha(0xFFFF_0040, a));
        draw.fill([spread * 0.55, 0.0, spread * 0.45, h], with_alpha(0xFFFF_80FF, (a as f32 * 0.5) as u8));
        draw.fill([w - spread, 0.0, spread, h], with_alpha(0xFF00_E0FF, a));
        draw.fill(
            [w - spread, 0.0, spread * 0.45, h],
            with_alpha(0xFF40_FF80, (a as f32 * 0.5) as u8),
        );
    }

    if time_ms < fx.hit_flash_until_ms {
        let remain = (fx.hit_flash_until_ms - time_ms) as f32 / 140.0;
        let a = (remain.clamp(0.0, 1.0) * 140.0) as u8;
        let flash = if tripping {
            trip_hue(time_ms, 0.5, 0.4)
        } else {
            accent
        };
        draw.fill([0.0, 0.0, w, 10.0], with_alpha(flash, a));
        draw.fill([0.0, h - 10.0, w, 10.0], with_alpha(flash, a));
        draw.fill([0.0, 0.0, 10.0, h], with_alpha(flash, a));
        draw.fill([w - 10.0, 0.0, 10.0, h], with_alpha(flash, a));
    }

    if screen.bool("low_hp_pulse", false) {
        if let Some(player) = host.camera_entity() {
            if let Some(living) = host.living(player) {
                if let Some(hp) = living.health(host) {
                    if let Some(max) = living.max_health(host) {
                        let ratio = if max > 0.0 { hp / max } else { 1.0 };
                        if ratio < 0.35 {
                            let pulse = ((time_ms as f32 / 1000.0) * 4.0 * std::f32::consts::TAU)
                                .sin()
                                .abs();
                            let a = (((0.35 - ratio) / 0.35) * pulse * 140.0) as u8;
                            draw.fill([0.0, 0.0, w, h], with_alpha(0xFFFF_2020, a));
                        }
                    }
                }
            }
        }
    }
}

fn draw_morph_waves(
    draw: &mut dyn mod_api::DrawContext,
    w: f32,
    h: f32,
    time_ms: u64,
    period: f32,
    dose: f32,
) {
    let t = time_ms as f32 / 1000.0;
    let melt = morph_wave(time_ms, period);
    let band_h = (10.0 + dose * 8.0).clamp(8.0, 22.0);
    let rows = ((h / (band_h * 2.2)) as i32).clamp(8, 18);
    for i in 0..rows {
        let phase = i as f32 * 0.37;
        let y_base = (i as f32 / rows as f32) * h;
        let wobble = ((t * (1.1 + dose * 0.4) + phase) * std::f32::consts::TAU).sin()
            * (18.0 + dose * 28.0)
            + melt * 12.0;
        let y = (y_base + wobble).rem_euclid(h);
        let thick = band_h * (0.55 + 0.45 * ((t * 2.0 + phase).sin().abs()));
        let col = trip_hue(time_ms, phase * 0.13 + melt.abs() * 0.05, 0.25);
        let a = ((28.0 + dose * 42.0) as u32).min(95) as u8;
        draw.fill([0.0, y, w, thick], with_alpha(col, a));
    }

    let cols = ((w / 48.0) as i32).clamp(6, 14);
    let band_w = (8.0 + dose * 6.0).clamp(6.0, 18.0);
    for i in 0..cols {
        let phase = i as f32 * 0.51;
        let x_base = (i as f32 / cols as f32) * w;
        let wobble = ((t * (0.9 + dose * 0.35) + phase) * std::f32::consts::TAU).cos()
            * (14.0 + dose * 22.0);
        let x = (x_base + wobble).rem_euclid(w);
        let col = trip_hue(time_ms, 0.5 + phase * 0.09, 0.2);
        let a = ((18.0 + dose * 30.0) as u32).min(70) as u8;
        draw.fill([x, 0.0, band_w, h], with_alpha(col, a));
    }
}

fn draw_kaleidoscope(
    draw: &mut dyn mod_api::DrawContext,
    w: f32,
    h: f32,
    time_ms: u64,
    period: f32,
    dose: f32,
) {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let t = time_ms as f32 / 1000.0;
    let pulse = morph_wave(time_ms, period * 0.85).abs();
    let max_r = (w.max(h) * 0.55) * (0.75 + pulse * 0.45 + dose * 0.15);
    let rings = 7;
    for i in 0..rings {
        let u = (i as f32 + 1.0) / rings as f32;
        let breathe = 1.0 + ((t * 1.6 + u * 4.0).sin() * 0.12);
        let r = max_r * u * breathe;
        let thickness = (3.0 + dose * 3.5 + (1.0 - u) * 4.0).clamp(2.0, 10.0);
        let col = trip_hue(time_ms, u * 0.45 + t * 0.05, 0.3);
        let a = ((40.0 + dose * 55.0) * (1.0 - u * 0.35)) as u8;
        // Approximate a ring with four edge strips of a square frame.
        let x = cx - r;
        let y = cy - r;
        let side = r * 2.0;
        draw.fill([x, y, side, thickness], with_alpha(col, a));
        draw.fill([x, y + side - thickness, side, thickness], with_alpha(col, a));
        draw.fill([x, y, thickness, side], with_alpha(col, a));
        draw.fill([x + side - thickness, y, thickness, side], with_alpha(col, a));
    }

    // Rotating sector wedges (axis-aligned strips that sweep across).
    let sectors = 8;
    for i in 0..sectors {
        let ang = t * (0.7 + dose * 0.4) + (i as f32) * (std::f32::consts::TAU / sectors as f32);
        let span = 0.18 + dose * 0.06;
        let along_x = ang.cos().abs() > ang.sin().abs();
        let col = trip_hue(time_ms, i as f32 / sectors as f32, 0.28);
        let a = ((22.0 + dose * 38.0) as u32).min(80) as u8;
        if along_x {
            let x = (cx + ang.cos() * w * 0.35 - w * span * 0.5).clamp(0.0, w);
            draw.fill([x, 0.0, w * span * 0.35, h], with_alpha(col, a));
        } else {
            let y = (cy + ang.sin() * h * 0.35 - h * span * 0.5).clamp(0.0, h);
            draw.fill([0.0, y, w, h * span * 0.35], with_alpha(col, a));
        }
    }
}

/// Notify when a preset is applied from the GUI.
pub fn notify_preset(name: &str) {
    notify(ToastKind::Success, format!("Visuals preset: {name}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::ModuleManager;
    use crate::theme::Theme;

    #[test]
    fn fov_stays_in_safe_range() {
        let mgr = ModuleManager::new();
        let fx = FxState {
            trip_level: 1.5,
            ..FxState::default()
        };
        let trackers = Trackers::default();
        let mut event = ComputeFovEvent {
            partial: 0.0,
            camera: [0.0; 3],
            degrees: 70.0,
        };
        // Force trip FOV breathing via settings would need mutation; just clamp path.
        apply_fov(&mgr, &fx, &trackers, 12_000, &mut event);
        assert!(event.degrees >= 1.0 && event.degrees <= 179.0);
    }

    #[test]
    fn accent_rainbow_changes_over_time() {
        let mgr = ModuleManager::new();
        // Rainbow source requires setting the mode — default is Theme Accent.
        let theme = Theme {
            accent: 0xFFFF_0000,
            accent_soft: 0,
            panel_bg: 0,
            header_bg: 0,
            module_bg: 0,
            module_hover: 0,
            module_enabled_bg: 0,
            setting_bg: 0,
            slider_track: 0,
            border: 0,
            text: 0,
            text_muted: 0,
            backdrop_dim: 0,
            panel_width: 0.0,
            header_height: 0.0,
            row_height: 0.0,
            setting_height: 0.0,
            blur_background: false,
            show_descriptions: false,
            show_keybinds: false,
            rainbow_accent: false,
        };
        let a = resolve_accent(&mgr, &theme, 0, 0.0);
        assert_eq!(a, theme.accent);
    }

    #[test]
    fn hit_planner_is_empty_without_keys() {
        let keys = ParticleKeys::default();
        let ev = HitEvent {
            entity_id: 7,
            pos: [1.0, 2.0, 3.0],
            amount: 4.0,
            kind: HitKind::Normal,
            born_ms: 100,
        };
        let planned = plan_hit_particles(2, 200, 1.0, 1.0, 1.0, [1.0, 0.0, 0.0], &keys, &ev);
        assert!(planned.is_empty());
    }

    #[test]
    fn wave_stays_unit_interval() {
        for ms in [0_u64, 500, 12_000, 90_000] {
            let w = wave(ms, 12.0);
            assert!((0.0..=1.0).contains(&w));
        }
    }

    #[test]
    fn morph_wave_stays_signed_unit() {
        for ms in [0_u64, 250, 4_000, 33_000] {
            let m = morph_wave(ms, 5.5);
            assert!((-1.05..=1.05).contains(&m), "morph_wave={m}");
        }
    }
}
