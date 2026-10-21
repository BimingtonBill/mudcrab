//! Live controls for the package's native distant object and tree inventories.

use bevy::prelude::*;

/// Per-knob flags, without the leading `--`.
pub const KNOBS: [&str; 2] = ["lod-objects", "lod-trees"];

/// Whether a command-line flag names a native LOD control.
pub fn is_knob(name: &str) -> bool {
    KNOBS.contains(&name)
}

/// Live native LOD visibility controls, read by streaming on every frame.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource)]
pub struct GraphicsSettings {
    /// Draw distant object blocks from the package's converted inventory.
    pub lod_objects: bool,
    /// Draw distant tree billboards from the package's converted inventory.
    pub lod_trees: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            lod_objects: true,
            lod_trees: true,
        }
    }
}

impl GraphicsSettings {
    /// Apply a native LOD control, refusing unknown keys and invalid values.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let target = match name {
            "lod-objects" => &mut self.lod_objects,
            "lod-trees" => &mut self.lod_trees,
            _ => return Err(format!("unknown graphics setting {name:?}")),
        };
        *target = switch(value)
            .map_err(|expected| format!("invalid --{name} value {value:?}; expected {expected}"))?;
        Ok(())
    }
}

fn switch(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err("on or off".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lod_controls_are_independent_and_reversible() {
        let mut settings = GraphicsSettings::default();
        settings.set("lod-objects", "off").unwrap();
        assert!(!settings.lod_objects);
        assert!(settings.lod_trees);
        settings.set("lod-trees", "false").unwrap();
        assert!(!settings.lod_trees);
        settings.set("lod-objects", "on").unwrap();
        settings.set("lod-trees", "yes").unwrap();
        assert_eq!(settings, GraphicsSettings::default());
    }

    #[test]
    fn invalid_controls_do_not_mutate_settings() {
        let mut settings = GraphicsSettings::default();
        assert!(settings.set("lod-objects", "sometimes").is_err());
        assert!(settings.set("unknown", "off").is_err());
        assert_eq!(settings, GraphicsSettings::default());
    }
}
