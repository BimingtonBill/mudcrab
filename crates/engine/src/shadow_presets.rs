//! Skyrim's own shadow settings per quality preset, as data the Vanilla presets read.
//!
//! Evidence (tier 3J, the shipped preset INIs; `local/research/INDEX-skyrim.md`, rows
//! `iShadowMapResolution` and `fShadowDistance`, and `lighting-weather.md` section 5.10):
//! `iShadowMapResolution` is 1024 / 2048 / 2048 / 4096 and `fShadowDistance` 3000 / 3000 / 8000 /
//! 10000 game units for Low / Medium / High / Ultra, and `iNumFocusShadow` is 1 / 2 / 4 / 4.
//!
//! Not known from any source: Skyrim's cascade count and its depth bias. They are not part of this
//! table on purpose: a value here without evidence would be a guess presented as vanilla.
//! [`VanillaShadows::cascades`] is `None` for that reason, and a preset that applies this table
//! keeps the engine's own cascade count (a Bevy limit, not a Skyrim value).
//!
//! `iNumFocusShadow` (how many actors Skyrim gives their own focus shadow) has no counterpart in
//! the sun's cascades, so it is carried for the Vanilla presets' report and is not applied.
//!
//! The Vanilla presets (OP4) own choosing the preset and the menu; this module only holds the
//! numbers and applies the two that map onto the graphics settings.

use crate::graphics_settings::GraphicsSettings;

/// One Skyrim quality level's shadow values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VanillaShadows {
    pub name: &'static str,
    /// `iShadowMapResolution`, texels a side.
    pub map_size: usize,
    /// `fShadowDistance`, game (Creation) units: the engine renders one Creation unit as one world
    /// unit, so this is also the sun's last cascade reach.
    pub distance: f32,
    /// `iNumFocusShadow`.
    pub focus_shadows: u32,
    /// Skyrim's cascade count: vanilla value unknown.
    pub cascades: Option<usize>,
}

/// Low, Medium, High and Ultra, in that order.
pub const VANILLA_SHADOWS: [VanillaShadows; 4] = [
    VanillaShadows {
        name: "low",
        map_size: 1024,
        distance: 3000.0,
        focus_shadows: 1,
        cascades: None,
    },
    VanillaShadows {
        name: "medium",
        map_size: 2048,
        distance: 3000.0,
        focus_shadows: 2,
        cascades: None,
    },
    VanillaShadows {
        name: "high",
        map_size: 2048,
        distance: 8000.0,
        focus_shadows: 4,
        cascades: None,
    },
    VanillaShadows {
        name: "ultra",
        map_size: 4096,
        distance: 10000.0,
        focus_shadows: 4,
        cascades: None,
    },
];

impl VanillaShadows {
    /// The level named `low`, `medium`, `high` or `ultra` (any case).
    pub fn named(name: &str) -> Option<&'static Self> {
        let name = name.trim().to_ascii_lowercase();
        VANILLA_SHADOWS.iter().find(|level| level.name == name)
    }

    /// Sets the shadow map size and the sun's reach on `settings`; every other knob, the cascade
    /// count included, is left as it is.
    pub fn apply(&self, settings: &mut GraphicsSettings) {
        settings.shadow_map_size = self.map_size;
        settings.shadow_distance = Some(self.distance);
        if let Some(cascades) = self.cascades {
            settings.shadow_cascades = cascades;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_the_shipped_ini_values() {
        let sizes: Vec<_> = VANILLA_SHADOWS.iter().map(|l| l.map_size).collect();
        let distances: Vec<_> = VANILLA_SHADOWS.iter().map(|l| l.distance).collect();
        let focus: Vec<_> = VANILLA_SHADOWS.iter().map(|l| l.focus_shadows).collect();
        assert_eq!(sizes, [1024, 2048, 2048, 4096]);
        assert_eq!(distances, [3000.0, 3000.0, 8000.0, 10000.0]);
        assert_eq!(focus, [1, 2, 4, 4]);
    }

    #[test]
    fn apply_sets_size_and_reach_and_keeps_the_cascade_count() {
        let mut settings = GraphicsSettings::current();
        let cascades = settings.shadow_cascades;
        VanillaShadows::named("High").unwrap().apply(&mut settings);
        assert_eq!(settings.shadow_map_size, 2048);
        assert_eq!(settings.shadow_distance, Some(8000.0));
        assert_eq!(settings.shadow_cascades, cascades);
    }

    #[test]
    fn unknown_level_is_none() {
        assert!(VanillaShadows::named("showcase").is_none());
    }

    #[test]
    fn every_vanilla_map_size_passes_the_settings_check() {
        for level in VANILLA_SHADOWS {
            let mut settings = GraphicsSettings::current();
            settings
                .set("shadow-map-size", &level.map_size.to_string())
                .unwrap();
            assert_eq!(settings.shadow_map_size, level.map_size);
        }
    }
}
