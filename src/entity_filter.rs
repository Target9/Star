//! Reusable entity catalog & exact entity-type selector filter.
//!
//! Populated dynamically from Horizon's `minecraft:entity_type` registry during
//! registration (with a built-in Minecraft 26.3 fallback for offline tests).

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// High-level category used to organize entities in the Entity Selector modal
/// and color-code entities in ESP / Tracers / Nametags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityGroup {
    Players,
    Hostile,
    Neutral,
    Passive,
    Misc,
}

impl EntityGroup {
    pub fn color(self, accent: u32) -> u32 {
        match self {
            Self::Players => accent,
            Self::Hostile => 0xFFFF_4D5E,
            Self::Neutral => 0xFFFF_C84D,
            Self::Passive => 0xFF4C_E08A,
            Self::Misc => 0xFF6B_B8FF,
        }
    }
}

/// One selectable entity type entry in the catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityEntry {
    pub id: String,
    pub display: String,
    pub group: EntityGroup,
}

static CATALOG: OnceLock<Vec<EntityEntry>> = OnceLock::new();

/// Seeds the global entity catalog from Horizon's live `minecraft:entity_type` registry.
pub fn init_catalog(registry_ids: &[String]) {
    if registry_ids.is_empty() {
        return;
    }
    let _ = CATALOG.set(build_entries(registry_ids.iter().map(String::as_str)));
}

/// Returns the sorted catalog of all known Minecraft entity types.
pub fn catalog() -> &'static [EntityEntry] {
    CATALOG.get_or_init(|| build_entries(FALLBACK_ENTITIES.iter().copied()))
}

fn build_entries<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<EntityEntry> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for raw in ids {
        let id = normalize_id(raw);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let group = classify_entity_id(&id);
        let display = pretty_entity_id(&id);
        entries.push(EntityEntry { id, display, group });
    }
    // Guarantee `minecraft:player` is always present at the top.
    if seen.insert("minecraft:player".to_owned()) {
        entries.push(EntityEntry {
            id: "minecraft:player".to_owned(),
            display: "Player".to_owned(),
            group: EntityGroup::Players,
        });
    }
    entries.sort_by(|a, b| a.group.cmp(&b.group).then_with(|| a.display.cmp(&b.display)));
    entries
}

/// Normalizes an entity ID to lowercase with a `minecraft:` namespace if omitted.
pub fn normalize_id(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains(':') {
        trimmed
    } else {
        format!("minecraft:{trimmed}")
    }
}

/// Formats `minecraft:wither_skeleton` -> `Wither Skeleton`.
pub fn pretty_entity_id(id: &str) -> String {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    let mut out = String::with_capacity(path.len());
    for (i, part) in path.split('_').filter(|s| !s.is_empty()).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        id.to_owned()
    } else {
        out
    }
}

/// Classifies a namespaced entity type ID into Players / Hostile / Neutral / Passive / Misc.
pub fn classify_entity_id(id: &str) -> EntityGroup {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    if path == "player" {
        return EntityGroup::Players;
    }
    if HOSTILE_PATHS.contains(&path) {
        return EntityGroup::Hostile;
    }
    if NEUTRAL_PATHS.contains(&path) {
        return EntityGroup::Neutral;
    }
    if PASSIVE_PATHS.contains(&path) {
        return EntityGroup::Passive;
    }
    EntityGroup::Misc
}

/// Set of exact entity type IDs selected for a module (Aim Assist, ESP, Tracers, Nametags, etc.).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityFilter {
    pub ids: BTreeSet<String>,
}

impl EntityFilter {
    #[cfg(test)]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_ids(ids: &[&str]) -> Self {
        let mut set = BTreeSet::new();
        for id in ids {
            let norm = normalize_id(id);
            if !norm.is_empty() {
                set.insert(norm);
            }
        }
        Self { ids: set }
    }

    pub fn players_only() -> Self {
        Self::from_ids(&["minecraft:player"])
    }

    pub fn players_and_hostile() -> Self {
        let mut set = BTreeSet::new();
        set.insert("minecraft:player".to_owned());
        for path in HOSTILE_PATHS {
            set.insert(format!("minecraft:{path}"));
        }
        Self { ids: set }
    }

    pub fn contains(&self, id: &str) -> bool {
        if self.ids.contains(id) {
            return true;
        }
        let norm = normalize_id(id);
        self.ids.contains(&norm)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn toggle(&mut self, id: &str) -> bool {
        let norm = normalize_id(id);
        if norm.is_empty() {
            return false;
        }
        if !self.ids.remove(&norm) {
            self.ids.insert(norm);
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        for entry in catalog() {
            self.ids.insert(entry.id.clone());
        }
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    pub fn toggle_group(&mut self, group: EntityGroup) {
        let group_ids: Vec<&str> = catalog()
            .iter()
            .filter(|e| e.group == group)
            .map(|e| e.id.as_str())
            .collect();
        let all_selected = !group_ids.is_empty() && group_ids.iter().all(|id| self.ids.contains(*id));
        if all_selected {
            for id in group_ids {
                self.ids.remove(id);
            }
        } else {
            for id in group_ids {
                self.ids.insert(id.to_owned());
            }
        }
    }

    pub fn group_fully_selected(&self, group: EntityGroup) -> bool {
        let mut any = false;
        for entry in catalog().iter().filter(|e| e.group == group) {
            any = true;
            if !self.ids.contains(&entry.id) {
                return false;
            }
        }
        any
    }

    /// Short compact button label for the setting row.
    pub fn summary(&self) -> String {
        match self.ids.len() {
            0 => "None".to_owned(),
            1 => {
                let first = self.ids.iter().next().map(String::as_str).unwrap_or("");
                pretty_entity_id(first)
            }
            2 => {
                if self.ids.contains("minecraft:player") {
                    "Player + 1".to_owned()
                } else {
                    "2 types".to_owned()
                }
            }
            n => {
                if self.ids.contains("minecraft:player") && n <= 4 {
                    format!("Player + {}", n - 1)
                } else {
                    format!("{n} types")
                }
            }
        }
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for (i, id) in self.ids.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(id);
        }
        out
    }

    pub fn deserialize(raw: &str) -> Self {
        let mut set = BTreeSet::new();
        for part in raw.split(',') {
            let norm = normalize_id(part);
            if !norm.is_empty() {
                set.insert(norm);
            }
        }
        Self { ids: set }
    }
}

const HOSTILE_PATHS: &[&str] = &[
    "blaze",
    "bogged",
    "breeze",
    "creaking",
    "creeper",
    "drowned",
    "elder_guardian",
    "ender_dragon",
    "endermite",
    "evoker",
    "ghast",
    "giant",
    "guardian",
    "hoglin",
    "husk",
    "illusioner",
    "magma_cube",
    "parched",
    "phantom",
    "piglin_brute",
    "pillager",
    "ravager",
    "shulker",
    "silverfish",
    "skeleton",
    "slime",
    "stray",
    "vex",
    "vindicator",
    "warden",
    "witch",
    "wither",
    "wither_skeleton",
    "zoglin",
    "zombie",
    "zombie_villager",
];

const NEUTRAL_PATHS: &[&str] = &[
    "bee",
    "cave_spider",
    "dolphin",
    "enderman",
    "fox",
    "goat",
    "iron_golem",
    "llama",
    "nautilus",
    "panda",
    "piglin",
    "polar_bear",
    "pufferfish",
    "spider",
    "trader_llama",
    "wolf",
    "zombified_piglin",
];

const PASSIVE_PATHS: &[&str] = &[
    "allay",
    "armadillo",
    "axolotl",
    "bat",
    "camel",
    "cat",
    "chicken",
    "cod",
    "copper_golem",
    "cow",
    "donkey",
    "frog",
    "glow_squid",
    "happy_ghast",
    "horse",
    "mooshroom",
    "mule",
    "ocelot",
    "parrot",
    "pig",
    "rabbit",
    "salmon",
    "sheep",
    "skeleton_horse",
    "sniffer",
    "snow_golem",
    "squid",
    "strider",
    "tadpole",
    "tropical_fish",
    "turtle",
    "villager",
    "wandering_trader",
    "zombie_horse",
];

const FALLBACK_ENTITIES: &[&str] = &[
    "minecraft:player",
    // Hostile
    "minecraft:blaze",
    "minecraft:bogged",
    "minecraft:breeze",
    "minecraft:creaking",
    "minecraft:creeper",
    "minecraft:drowned",
    "minecraft:elder_guardian",
    "minecraft:ender_dragon",
    "minecraft:endermite",
    "minecraft:evoker",
    "minecraft:ghast",
    "minecraft:guardian",
    "minecraft:hoglin",
    "minecraft:husk",
    "minecraft:magma_cube",
    "minecraft:phantom",
    "minecraft:piglin_brute",
    "minecraft:pillager",
    "minecraft:ravager",
    "minecraft:shulker",
    "minecraft:silverfish",
    "minecraft:skeleton",
    "minecraft:slime",
    "minecraft:stray",
    "minecraft:vex",
    "minecraft:vindicator",
    "minecraft:warden",
    "minecraft:witch",
    "minecraft:wither",
    "minecraft:wither_skeleton",
    "minecraft:zoglin",
    "minecraft:zombie",
    "minecraft:zombie_villager",
    // Neutral
    "minecraft:bee",
    "minecraft:cave_spider",
    "minecraft:dolphin",
    "minecraft:enderman",
    "minecraft:fox",
    "minecraft:goat",
    "minecraft:iron_golem",
    "minecraft:llama",
    "minecraft:panda",
    "minecraft:piglin",
    "minecraft:polar_bear",
    "minecraft:pufferfish",
    "minecraft:spider",
    "minecraft:trader_llama",
    "minecraft:wolf",
    "minecraft:zombified_piglin",
    // Passive
    "minecraft:allay",
    "minecraft:armadillo",
    "minecraft:axolotl",
    "minecraft:bat",
    "minecraft:camel",
    "minecraft:cat",
    "minecraft:chicken",
    "minecraft:cod",
    "minecraft:cow",
    "minecraft:donkey",
    "minecraft:frog",
    "minecraft:glow_squid",
    "minecraft:horse",
    "minecraft:mooshroom",
    "minecraft:mule",
    "minecraft:ocelot",
    "minecraft:parrot",
    "minecraft:pig",
    "minecraft:rabbit",
    "minecraft:salmon",
    "minecraft:sheep",
    "minecraft:sniffer",
    "minecraft:snow_golem",
    "minecraft:squid",
    "minecraft:strider",
    "minecraft:tadpole",
    "minecraft:tropical_fish",
    "minecraft:turtle",
    "minecraft:villager",
    "minecraft:wandering_trader",
    // Misc
    "minecraft:armor_stand",
    "minecraft:arrow",
    "minecraft:chest_minecart",
    "minecraft:end_crystal",
    "minecraft:experience_orb",
    "minecraft:fireball",
    "minecraft:item",
    "minecraft:item_frame",
    "minecraft:minecart",
    "minecraft:oak_boat",
    "minecraft:tnt",
    "minecraft:trident",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_round_trips_and_summarizes() {
        let mut f = EntityFilter::players_only();
        assert!(f.contains("minecraft:player"));
        assert!(!f.contains("minecraft:zombie"));
        assert_eq!(f.summary(), "Player");

        f.toggle("minecraft:zombie");
        f.toggle("minecraft:skeleton");
        assert_eq!(f.summary(), "Player + 2");

        let encoded = f.serialize();
        let decoded = EntityFilter::deserialize(&encoded);
        assert_eq!(f, decoded);
    }

    #[test]
    fn group_toggle_selects_and_deselects_group() {
        let mut f = EntityFilter::empty();
        f.toggle_group(EntityGroup::Hostile);
        assert!(f.group_fully_selected(EntityGroup::Hostile));
        assert!(f.contains("minecraft:zombie"));
        assert!(!f.contains("minecraft:player"));

        f.toggle_group(EntityGroup::Hostile);
        assert!(!f.group_fully_selected(EntityGroup::Hostile));
        assert!(!f.contains("minecraft:zombie"));
    }
}
