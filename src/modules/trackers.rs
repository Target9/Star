//! Per-tick and per-frame state sampled for HUD widgets and world overlays:
//! frame rate, ground speed, click rate, session time, breadcrumb trails, damage
//! popups, projectile paths, and the storage-block cache.
//!
//! Everything here is sampled from `client_tick_pre` / `render_frame_pre` so the
//! render hooks only read finished data and never call the world repeatedly.

use std::collections::{HashMap, HashSet, VecDeque};

use mod_api::{ClientHost, ClipBlock, ClipFluid, EntityRef};

use crate::entity_filter::{EntityGroup, classify_entity_id};
use crate::module::{ModuleId, ModuleManager};

const MAX_TRAILS: usize = 24;
const TRAIL_STALE_TICKS: u64 = 40;
const MAX_POPUPS: usize = 32;
pub const POPUP_LIFETIME_MS: u64 = 1_100;
const MAX_PATHS: usize = 12;
/// A projectile first seen this close to the player is assumed to be the player's own.
const OWN_LAUNCH_RADIUS: f64 = 3.0;
const MAX_PATH_STEPS: usize = 90;
const STORAGE_REFRESH_TICKS: u64 = 20;
/// Largest storage scan radius; the API allows at most 64 chunks (an 8x8 grid) per query.
pub const MAX_STORAGE_RADIUS: f64 = 48.0;
const MAX_STORAGE_BLOCKS: usize = 600;

// ─────────────────────────────────────────────────────────────────────────────
// Plain data produced by the trackers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Trail {
    pub points: VecDeque<[f64; 3]>,
    pub group: EntityGroup,
    last_seen_tick: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct DamagePopup {
    pub pos: [f64; 3],
    pub amount: f32,
    pub heal: bool,
    pub born_ms: u64,
}

/// Kind of melee/projectile damage event for Visuals Hit FX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Normal,
    Crit,
    Kill,
}

/// One damage event attributed to the local player, for particle bursts.
#[derive(Clone, Copy, Debug)]
pub struct HitEvent {
    pub entity_id: u32,
    pub pos: [f64; 3],
    #[allow(dead_code)]
    pub amount: f32,
    pub kind: HitKind,
    pub born_ms: u64,
}

const MAX_HIT_EVENTS: usize = 24;

#[derive(Clone, Debug)]
pub struct ProjectilePath {
    pub points: Vec<[f64; 3]>,
    pub landing: Option<[f64; 3]>,
    pub own: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorageBlock {
    pub pos: [i32; 3],
    pub kind: StorageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageKind {
    Chest,
    EnderChest,
    Shulker,
    Barrel,
    Furnace,
    Hopper,
    Dispenser,
    Other,
}

impl StorageKind {
    /// Maps a block entity registry id to a storage kind, or `None` if it isn't storage.
    pub fn from_block_entity(id: &str) -> Option<Self> {
        let path = id.split_once(':').map_or(id, |(_, p)| p);
        Some(match path {
            "chest" | "trapped_chest" => Self::Chest,
            "ender_chest" => Self::EnderChest,
            "shulker_box" => Self::Shulker,
            "barrel" => Self::Barrel,
            "furnace" | "blast_furnace" | "smoker" => Self::Furnace,
            "hopper" => Self::Hopper,
            "dispenser" | "dropper" | "crafter" => Self::Dispenser,
            "brewing_stand" | "chiseled_bookshelf" | "decorated_pot" => Self::Other,
            _ => return None,
        })
    }

    pub fn color(self) -> u32 {
        match self {
            Self::Chest => 0xFFF5_A623,
            Self::EnderChest => 0xFFB0_4CFF,
            Self::Shulker => 0xFFE0_6BFF,
            Self::Barrel => 0xFFC8_8A4C,
            Self::Furnace => 0xFF9A_A4B5,
            Self::Hopper => 0xFF6B_B8FF,
            Self::Dispenser => 0xFF4C_E08A,
            Self::Other => 0xFFFF_7A9C,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClickRate {
    pub left: u32,
    pub right: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// The tracker bundle
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Trackers {
    tick: u64,

    // Frame rate (exponential moving average of frame time).
    frame_secs_avg: f32,

    // Ground speed in blocks per second.
    prev_pos: Option<[f64; 3]>,
    speed_bps: f32,

    // Clicks per second, sampled as rising edges of the attack / use mappings.
    attack_was_down: bool,
    use_was_down: bool,
    left_clicks: VecDeque<u64>,
    right_clicks: VecDeque<u64>,

    session_start_ms: Option<u64>,

    // Breadcrumbs.
    self_trail: VecDeque<[f64; 3]>,
    trails: HashMap<u32, Trail>,

    // Damage indicators.
    health_prev: HashMap<u32, f32>,
    popups: Vec<DamagePopup>,
    /// Recent hits for Visuals Hit FX (drained each tick by `vis_fx`).
    hits: Vec<HitEvent>,
    /// Last time the attack key went down, for fallback attribution.
    last_attack_ms: u64,

    // Trajectories.
    paths: Vec<ProjectilePath>,

    // Trajectories: whether each projectile id appeared next to the player.
    proj_owner: HashMap<u32, bool>,

    // Storage ESP.
    storage: Vec<StorageBlock>,
    storage_scan_tick: Option<u64>,
    storage_center: [f64; 3],

    // Latency by lower-cased player name, refreshed from the tab list.
    ping_map: HashMap<String, i32>,
}

impl Trackers {
    // ── accessors ───────────────────────────────────────────────────────────

    pub fn fps(&self) -> u32 {
        if self.frame_secs_avg <= 1e-4 {
            0
        } else {
            (1.0 / self.frame_secs_avg).round().clamp(0.0, 9999.0) as u32
        }
    }

    pub fn speed_bps(&self) -> f32 {
        self.speed_bps
    }

    pub fn clicks_per_second(&self) -> ClickRate {
        ClickRate {
            left: self.left_clicks.len() as u32,
            right: self.right_clicks.len() as u32,
        }
    }

    pub fn session_secs(&self, now_ms: u64) -> u64 {
        self.session_start_ms
            .map_or(0, |start| now_ms.saturating_sub(start) / 1000)
    }

    pub fn self_trail(&self) -> &VecDeque<[f64; 3]> {
        &self.self_trail
    }

    pub fn trails(&self) -> impl Iterator<Item = &Trail> {
        self.trails.values()
    }

    pub fn popups(&self) -> &[DamagePopup] {
        &self.popups
    }

    /// Take and clear queued Visuals hit events.
    pub fn drain_hits(&mut self) -> Vec<HitEvent> {
        std::mem::take(&mut self.hits)
    }

    pub fn paths(&self) -> &[ProjectilePath] {
        &self.paths
    }

    pub fn storage(&self) -> &[StorageBlock] {
        &self.storage
    }

    /// Latency in milliseconds of the player called `name`, if the tab list reported one.
    pub fn ping_of(&self, name: &str) -> Option<i32> {
        self.ping_map.get(&name.to_ascii_lowercase()).copied()
    }

    // ── lifecycle ───────────────────────────────────────────────────────────

    /// Drops everything that depends on the current world (called when leaving a world).
    pub fn clear_world_state(&mut self) {
        self.prev_pos = None;
        self.speed_bps = 0.0;
        self.self_trail.clear();
        self.trails.clear();
        self.health_prev.clear();
        self.popups.clear();
        self.hits.clear();
        self.paths.clear();
        self.proj_owner.clear();
        self.storage.clear();
        self.storage_scan_tick = None;
        self.ping_map.clear();
    }

    // ── per frame ───────────────────────────────────────────────────────────

    /// Called from `render_frame_pre` with the real frame time.
    pub fn on_frame(&mut self, host: &dyn ClientHost, elapsed_secs: f32) {
        let dt = elapsed_secs.clamp(0.0005, 0.5);
        self.frame_secs_avg = if self.frame_secs_avg <= 0.0 {
            dt
        } else {
            self.frame_secs_avg + (dt - self.frame_secs_avg) * 0.08
        };

        let now = host.millis();
        if self.session_start_ms.is_none() {
            self.session_start_ms = Some(now);
        }

        let down = |name: &str| {
            host.mapping(name)
                .ok()
                .flatten()
                .is_some_and(|m| m.down)
        };
        let attack = down("key.attack");
        let use_item = down("key.use");
        if attack && !self.attack_was_down {
            self.left_clicks.push_back(now);
            self.last_attack_ms = now;
        }
        if use_item && !self.use_was_down {
            self.right_clicks.push_back(now);
        }
        self.attack_was_down = attack;
        self.use_was_down = use_item;
        for queue in [&mut self.left_clicks, &mut self.right_clicks] {
            while queue.front().is_some_and(|&t| now.saturating_sub(t) > 1000) {
                queue.pop_front();
            }
        }
    }

    // ── per tick ────────────────────────────────────────────────────────────

    /// Samples the world once per client tick for every enabled tracker.
    pub fn on_tick(&mut self, host: &dyn ClientHost, manager: &ModuleManager) {
        self.tick = self.tick.wrapping_add(1);
        if !host.playing() {
            self.clear_world_state();
            return;
        }
        let Some(player) = host.camera_entity() else {
            return;
        };
        let Some(pos) = player.position(host) else {
            return;
        };

        self.sample_speed(pos);

        let breadcrumbs = manager.get(ModuleId::Breadcrumbs);
        let damage = manager.get(ModuleId::DamageIndicators);
        let hit_fx = manager.get(ModuleId::VisHit);
        let trajectories = manager.get(ModuleId::Trajectories);
        let storage = manager.get(ModuleId::StorageEsp);

        let trail_self = breadcrumbs.enabled && breadcrumbs.mode("source", 0) == 0;
        let trail_others = breadcrumbs.enabled && breadcrumbs.mode("source", 0) == 1;
        let want_hits = hit_fx.enabled && hit_fx.bool("enabled_fx", false);
        if breadcrumbs.enabled {
            let max_len = breadcrumbs.int("length", 100).clamp(10, 600) as usize;
            if trail_self {
                self.sample_self_trail(pos, max_len);
            } else {
                self.self_trail.clear();
            }
        } else {
            self.self_trail.clear();
            self.trails.clear();
        }

        let need_entities = trail_others || damage.enabled || trajectories.enabled || want_hits;
        if need_entities {
            let mut pad: f64 = 0.0;
            if trail_others {
                pad = pad.max(breadcrumbs.float("range", 48.0));
            }
            if damage.enabled {
                pad = pad.max(damage.float("range", 32.0));
            }
            if want_hits {
                pad = pad.max(hit_fx.float("range", 12.0));
            }
            if trajectories.enabled {
                pad = pad.max(trajectories.float("range", 64.0));
            }
            let entities = scan_entities(host, player, pos, pad + 2.0);

            if trail_others {
                self.sample_trails(host, manager, player, pos, &entities);
            }
            if damage.enabled || want_hits {
                self.sample_damage(host, manager, player, pos, &entities);
            } else {
                self.health_prev.clear();
                self.popups.clear();
            }
            if trajectories.enabled {
                self.sample_paths(host, manager, player, pos, &entities);
            } else {
                self.paths.clear();
            }
        } else {
            self.trails.clear();
            self.health_prev.clear();
            self.popups.clear();
            self.paths.clear();
        }

        if storage.enabled {
            self.sample_storage(host, manager, player, pos);
        } else {
            self.storage.clear();
            self.storage_scan_tick = None;
        }

        let hud = manager.get(ModuleId::Hud);
        let nametags = manager.get(ModuleId::Nametags);
        let need_ping = (hud.enabled && hud.bool("show_ping", false))
            || (nametags.enabled && nametags.bool("show_ping", false));
        if need_ping {
            if self.tick.is_multiple_of(10) {
                self.sample_ping(host);
            }
        } else {
            self.ping_map.clear();
        }
    }

    fn sample_ping(&mut self, host: &dyn ClientHost) {
        let Some(tab) = host.client_session().client_tab_list() else {
            return;
        };
        self.ping_map = tab
            .entries
            .iter()
            .filter(|e| e.latency >= 0)
            .map(|e| (e.name.to_ascii_lowercase(), e.latency))
            .collect();
    }

    fn sample_speed(&mut self, pos: [f64; 3]) {
        if let Some(prev) = self.prev_pos {
            let dx = pos[0] - prev[0];
            let dz = pos[2] - prev[2];
            let raw = ((dx * dx + dz * dz).sqrt() * 20.0) as f32;
            // Ignore teleports and dimension changes.
            if raw < 80.0 {
                self.speed_bps += (raw - self.speed_bps) * 0.35;
            } else {
                self.speed_bps = 0.0;
            }
        }
        self.prev_pos = Some(pos);
    }

    fn sample_self_trail(&mut self, pos: [f64; 3], max_len: usize) {
        let p = [pos[0], pos[1] + 0.08, pos[2]];
        let moved = self
            .self_trail
            .back()
            .is_none_or(|last| sq_dist(*last, p) > 0.01);
        if moved {
            if self
                .self_trail
                .back()
                .is_some_and(|last| sq_dist(*last, p) > 64.0 * 64.0)
            {
                self.self_trail.clear();
            }
            self.self_trail.push_back(p);
        }
        while self.self_trail.len() > max_len {
            self.self_trail.pop_front();
        }
    }

    fn sample_trails(
        &mut self,
        host: &dyn ClientHost,
        manager: &ModuleManager,
        player: EntityRef,
        origin: [f64; 3],
        entities: &[EntityRef],
    ) {
        let module = manager.get(ModuleId::Breadcrumbs);
        let Some(filter) = module.entity_filter("entities") else {
            return;
        };
        let range = module.float("range", 48.0);
        let max_len = module.int("length", 100).clamp(10, 600) as usize;
        let tick = self.tick;

        for entity in entities {
            if entity.id() == player.id() {
                continue;
            }
            let Some(kind) = entity.kind(host) else {
                continue;
            };
            let kind_id = kind.id(host);
            if !filter.contains(&kind_id) {
                continue;
            }
            let Some(pos) = entity.position(host) else {
                continue;
            };
            if sq_dist(origin, pos) > range * range {
                continue;
            }
            if self.trails.len() >= MAX_TRAILS && !self.trails.contains_key(&entity.id()) {
                continue;
            }
            let trail = self.trails.entry(entity.id()).or_insert_with(|| Trail {
                points: VecDeque::new(),
                group: classify_entity_id(&kind_id),
                last_seen_tick: tick,
            });
            trail.last_seen_tick = tick;
            let p = [pos[0], pos[1] + 0.08, pos[2]];
            if trail.points.back().is_none_or(|last| sq_dist(*last, p) > 0.01) {
                if trail
                    .points
                    .back()
                    .is_some_and(|last| sq_dist(*last, p) > 16.0 * 16.0)
                {
                    trail.points.clear();
                }
                trail.points.push_back(p);
            }
            while trail.points.len() > max_len {
                trail.points.pop_front();
            }
        }
        self.trails
            .retain(|_, t| tick.saturating_sub(t.last_seen_tick) <= TRAIL_STALE_TICKS);
    }

    fn sample_damage(
        &mut self,
        host: &dyn ClientHost,
        manager: &ModuleManager,
        player: EntityRef,
        origin: [f64; 3],
        entities: &[EntityRef],
    ) {
        let indicators = manager.get(ModuleId::DamageIndicators);
        let hit_fx = manager.get(ModuleId::VisHit);
        let want_popups = indicators.enabled;
        let want_hits = hit_fx.enabled && hit_fx.bool("enabled_fx", false);
        if !want_popups && !want_hits {
            return;
        }

        let filter = indicators.entity_filter("entities");
        let popup_range = indicators.float("range", 32.0);
        let hit_range = hit_fx.float("range", 12.0);
        let only_mine_popup = indicators.bool("only_mine", false);
        let only_mine_hit = hit_fx.bool("only_mine", true);
        let show_heal = indicators.bool("show_healing", false);
        let now = host.millis();
        let scan_range = popup_range.max(hit_range);

        let mut seen: Vec<u32> = Vec::with_capacity(entities.len());
        for entity in entities {
            if entity.id() == player.id() {
                continue;
            }
            let Some(living) = host.living(*entity) else {
                continue;
            };
            let Some(hp) = living.health(host) else {
                continue;
            };
            let Some(kind) = entity.kind(host) else {
                continue;
            };
            let kind_id = kind.id(host);
            let passes_filter = filter.is_none_or(|f| f.contains(&kind_id));
            let Some(bb) = entity.bounding_box(host) else {
                continue;
            };
            let center = [
                (bb[0][0] + bb[1][0]) * 0.5,
                bb[1][1],
                (bb[0][2] + bb[1][2]) * 0.5,
            ];
            let dist_sq = sq_dist(origin, center);
            if dist_sq > scan_range * scan_range {
                continue;
            }
            seen.push(entity.id());

            let id = entity.id();
            let prev = self.health_prev.insert(id, hp);
            let Some(prev) = prev else {
                continue;
            };
            let delta = hp - prev;
            let is_damage = delta < -0.05;
            let is_heal = delta > 0.5;
            if !is_damage && !(is_heal && show_heal && want_popups) {
                continue;
            }

            let mine = living
                .last_attacker(host)
                .is_some_and(|a| a.id() == player.id())
                || now.saturating_sub(self.last_attack_ms) < 350;

            if want_popups && passes_filter && dist_sq <= popup_range * popup_range {
                let allow = !is_damage || !only_mine_popup || mine;
                if allow && (is_damage || is_heal) {
                    if self.popups.len() >= MAX_POPUPS {
                        self.popups.remove(0);
                    }
                    let jitter =
                        ((id.wrapping_mul(2654435761) ^ (now as u32)) % 100) as f64 / 100.0 - 0.5;
                    self.popups.push(DamagePopup {
                        pos: [center[0] + jitter * 0.6, center[1] + 0.25, center[2]],
                        amount: delta.abs(),
                        heal: is_heal && !is_damage,
                        born_ms: now,
                    });
                }
            }

            if want_hits && is_damage && dist_sq <= hit_range * hit_range {
                let allow = !only_mine_hit || mine;
                if allow {
                    if self.hits.len() >= MAX_HIT_EVENTS {
                        self.hits.remove(0);
                    }
                    let kind = if hp <= 0.0 {
                        HitKind::Kill
                    } else if delta.abs() >= 6.0 {
                        HitKind::Crit
                    } else {
                        HitKind::Normal
                    };
                    self.hits.push(HitEvent {
                        entity_id: id,
                        pos: center,
                        amount: delta.abs(),
                        kind,
                        born_ms: now,
                    });
                }
            }
        }
        self.health_prev.retain(|id, _| seen.contains(id));
        self.popups
            .retain(|p| now.saturating_sub(p.born_ms) < POPUP_LIFETIME_MS);
    }

    fn sample_paths(
        &mut self,
        host: &dyn ClientHost,
        manager: &ModuleManager,
        player: EntityRef,
        origin: [f64; 3],
        entities: &[EntityRef],
    ) {
        self.paths.clear();
        let module = manager.get(ModuleId::Trajectories);
        let range = module.float("range", 64.0);
        let owner_mode = module.mode("owner", 0); // 0 All, 1 Mine, 2 Others
        let Some(level) = player.level(host) else {
            return;
        };

        // Forget projectiles that no longer exist so ids can be reused.
        let live: HashSet<u32> = entities.iter().map(|e| e.id()).collect();
        self.proj_owner.retain(|id, _| live.contains(id));

        for entity in entities {
            if self.paths.len() >= MAX_PATHS {
                break;
            }
            let Some(kind) = entity.kind(host) else {
                continue;
            };
            let kind_id = kind.id(host);
            let Some(physics) = ProjectilePhysics::for_kind(&kind_id) else {
                continue;
            };
            let Some(pos) = entity.position(host) else {
                continue;
            };
            if sq_dist(origin, pos) > range * range {
                continue;
            }
            let Some(vel) = entity.velocity(host) else {
                continue;
            };
            // Stuck arrows and resting projectiles have no velocity worth predicting.
            if vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2] < 0.0025 {
                continue;
            }
            // The client has no projectile-owner query, so a projectile counts as yours
            // when it first shows up right next to you (you just launched it).
            let id = entity.id();
            let own = *self
                .proj_owner
                .entry(id)
                .or_insert_with(|| sq_dist(origin, pos) <= OWN_LAUNCH_RADIUS * OWN_LAUNCH_RADIUS);
            match owner_mode {
                1 if !own => continue,
                2 if own => continue,
                _ => {}
            }

            let (points, landing) = simulate_path(
                pos,
                vel,
                physics,
                MAX_PATH_STEPS,
                |from, to| {
                    level
                        .clip(host, from, to, ClipBlock::Collider, ClipFluid::None)
                        .filter(|hit| !hit.miss)
                        .map(|hit| hit.location)
                },
            );
            if points.len() >= 2 {
                self.paths.push(ProjectilePath {
                    points,
                    landing,
                    own,
                });
            }
        }
    }

    fn sample_storage(
        &mut self,
        host: &dyn ClientHost,
        manager: &ModuleManager,
        player: EntityRef,
        pos: [f64; 3],
    ) {
        let module = manager.get(ModuleId::StorageEsp);
        let radius = module.float("range", 48.0).clamp(8.0, MAX_STORAGE_RADIUS);
        let moved = sq_dist(pos, self.storage_center) > 8.0 * 8.0;
        let due = self
            .storage_scan_tick
            .is_none_or(|t| self.tick.saturating_sub(t) >= STORAGE_REFRESH_TICKS);
        if !(due || moved) {
            return;
        }
        let Some(level) = player.level(host) else {
            return;
        };
        let min = [
            (pos[0] - radius).floor() as i32,
            (pos[1] - radius).floor() as i32,
            (pos[2] - radius).floor() as i32,
        ];
        let max = [
            (pos[0] + radius).ceil() as i32,
            (pos[1] + radius).ceil() as i32,
            (pos[2] + radius).ceil() as i32,
        ];
        self.storage_scan_tick = Some(self.tick);
        self.storage_center = pos;
        let Some(found) = level.block_entities(host, min, max) else {
            return;
        };
        self.storage = found
            .into_iter()
            .filter_map(|b| {
                StorageKind::from_block_entity(&b.id).map(|kind| StorageBlock { pos: b.pos, kind })
            })
            .take(MAX_STORAGE_BLOCKS)
            .collect();
    }
}

fn scan_entities(
    host: &dyn ClientHost,
    player: EntityRef,
    center: [f64; 3],
    pad: f64,
) -> Vec<EntityRef> {
    let Some(level) = player.level(host) else {
        return Vec::new();
    };
    level
        .entities_in(
            host,
            [center[0] - pad, center[1] - pad, center[2] - pad],
            [center[0] + pad, center[1] + pad, center[2] + pad],
        )
        .unwrap_or_default()
}

fn sq_dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

// ─────────────────────────────────────────────────────────────────────────────
// Projectile physics (pure, unit-tested)
// ─────────────────────────────────────────────────────────────────────────────

/// Per-tick gravity and drag of a projectile type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectilePhysics {
    pub gravity: f64,
    pub drag: f64,
}

impl ProjectilePhysics {
    pub fn for_kind(kind_id: &str) -> Option<Self> {
        let path = kind_id.split_once(':').map_or(kind_id, |(_, p)| p);
        let (gravity, drag) = match path {
            "arrow" | "spectral_arrow" | "trident" => (0.05, 0.99),
            "ender_pearl" | "snowball" | "egg" => (0.03, 0.99),
            "splash_potion" | "lingering_potion" | "potion" => (0.05, 0.99),
            "experience_bottle" => (0.07, 0.99),
            _ => return None,
        };
        Some(Self { gravity, drag })
    }
}

/// Integrates a projectile forward one tick at a time (move, then drag, then gravity, as
/// vanilla does) and returns the visited points plus where it stops.
///
/// `clip(from, to)` must return the first solid hit point on the segment, if any. It is
/// asked about two-tick segments to halve the number of world queries; a hit truncates the
/// path at the returned point.
pub fn simulate_path(
    start: [f64; 3],
    velocity: [f64; 3],
    physics: ProjectilePhysics,
    max_steps: usize,
    mut clip: impl FnMut([f64; 3], [f64; 3]) -> Option<[f64; 3]>,
) -> (Vec<[f64; 3]>, Option<[f64; 3]>) {
    let mut points = Vec::with_capacity(max_steps + 1);
    let mut pos = start;
    let mut vel = velocity;
    points.push(pos);

    // Positions after each of the next ticks, computed two at a time.
    let mut step = 0;
    while step < max_steps {
        let mut segment = [pos; 2];
        let mut sim_pos = pos;
        let mut sim_vel = vel;
        for slot in &mut segment {
            for axis in 0..3 {
                sim_pos[axis] += sim_vel[axis];
            }
            for v in &mut sim_vel {
                *v *= physics.drag;
            }
            sim_vel[1] -= physics.gravity;
            *slot = sim_pos;
        }

        if let Some(hit) = clip(pos, segment[1]) {
            points.push(hit);
            return (points, Some(hit));
        }
        points.push(segment[0]);
        points.push(segment[1]);
        pos = segment[1];
        vel = sim_vel;
        step += 2;

        // Far below the world: nothing left to predict.
        if pos[1] < -128.0 {
            break;
        }
    }
    (points, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARROW: ProjectilePhysics = ProjectilePhysics {
        gravity: 0.05,
        drag: 0.99,
    };

    #[test]
    fn physics_table_recognises_projectiles_only() {
        assert_eq!(ProjectilePhysics::for_kind("minecraft:arrow"), Some(ARROW));
        assert!(ProjectilePhysics::for_kind("minecraft:ender_pearl").is_some());
        assert!(ProjectilePhysics::for_kind("minecraft:zombie").is_none());
    }

    #[test]
    fn free_flight_arcs_downward_without_hitting_anything() {
        let (points, landing) =
            simulate_path([0.0, 100.0, 0.0], [1.0, 0.0, 0.0], ARROW, 40, |_, _| None);
        assert!(landing.is_none());
        assert!(points.len() > 30);
        // Moves forward and gravity bends the path down monotonically.
        assert!(points.last().unwrap()[0] > points[0][0]);
        assert!(points.last().unwrap()[1] < points[0][1]);
        for pair in points.windows(2) {
            assert!(pair[1][1] <= pair[0][1] + 1e-9);
        }
    }

    #[test]
    fn ground_plane_ends_the_path_at_the_hit_point() {
        let ground = |from: [f64; 3], to: [f64; 3]| {
            if to[1] <= 64.0 && from[1] > 64.0 {
                let t = (from[1] - 64.0) / (from[1] - to[1]);
                Some([
                    from[0] + (to[0] - from[0]) * t,
                    64.0,
                    from[2] + (to[2] - from[2]) * t,
                ])
            } else {
                None
            }
        };
        let (points, landing) = simulate_path([0.0, 70.0, 0.0], [0.8, 0.2, 0.0], ARROW, 90, ground);
        let landing = landing.expect("arrow lands");
        assert!((landing[1] - 64.0).abs() < 1e-9);
        assert_eq!(*points.last().unwrap(), landing);
        assert!(landing[0] > 0.0);
    }

    #[test]
    fn path_length_is_bounded() {
        let (points, _) = simulate_path([0.0, 100.0, 0.0], [0.0, 0.5, 0.0], ARROW, 10, |_, _| None);
        assert!(points.len() <= 11);
    }

    #[test]
    fn storage_kinds_cover_common_containers() {
        assert_eq!(
            StorageKind::from_block_entity("minecraft:chest"),
            Some(StorageKind::Chest)
        );
        assert_eq!(
            StorageKind::from_block_entity("minecraft:trapped_chest"),
            Some(StorageKind::Chest)
        );
        assert_eq!(
            StorageKind::from_block_entity("minecraft:shulker_box"),
            Some(StorageKind::Shulker)
        );
        assert_eq!(StorageKind::from_block_entity("minecraft:sign"), None);
    }

    #[test]
    fn speed_ignores_teleports_and_smooths() {
        let mut t = Trackers::default();
        t.sample_speed([0.0, 64.0, 0.0]);
        for i in 1..=30 {
            t.sample_speed([i as f64 * 0.215, 64.0, 0.0]);
        }
        // 0.215 blocks per tick = 4.3 blocks per second.
        assert!((t.speed_bps() - 4.3).abs() < 0.2, "got {}", t.speed_bps());
        t.sample_speed([5000.0, 64.0, 0.0]);
        assert_eq!(t.speed_bps(), 0.0);
    }

    #[test]
    fn self_trail_samples_only_when_moving_and_respects_length() {
        let mut t = Trackers::default();
        t.sample_self_trail([0.0, 64.0, 0.0], 5);
        t.sample_self_trail([0.0, 64.0, 0.0], 5);
        assert_eq!(t.self_trail().len(), 1, "standing still adds nothing");
        for i in 1..20 {
            t.sample_self_trail([i as f64, 64.0, 0.0], 5);
        }
        assert_eq!(t.self_trail().len(), 5);
    }

    #[test]
    fn fps_follows_frame_time() {
        let mut t = Trackers::default();
        t.frame_secs_avg = 1.0 / 120.0;
        assert_eq!(t.fps(), 120);
        assert_eq!(Trackers::default().fps(), 0);
    }
}
