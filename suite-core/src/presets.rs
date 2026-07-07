//! Factory-preset support for the suite (PRD §1.4 step 6). A preset is embedded as a
//! flat JSON object: a `"name"` string plus one numeric field per parameter id, e.g.
//!
//! ```json
//! { "name": "Kick Bass Grit", "drive": 8.0, "mix": 100.0 }
//! ```
//!
//! Plugins embed their presets as `&'static str` JSON blobs, parse them once at load
//! with [`Preset::parse`] / [`load_all`], and apply the `values` to their nih-plug
//! params. Keeping presets as data (not code) lets the same list drive both the GUI
//! selector and the offline render tests.

use serde::Deserialize;
use std::collections::BTreeMap;

/// One factory preset: a display name and a map of `param_id -> plain value`.
#[derive(Debug, Clone, Deserialize)]
pub struct Preset {
    pub name: String,
    /// Remaining JSON fields (all numeric) collected as parameter values.
    #[serde(flatten)]
    pub values: BTreeMap<String, f32>,
}

impl Preset {
    /// Parse a single embedded JSON preset blob.
    pub fn parse(json: &str) -> Result<Preset, String> {
        serde_json::from_str(json).map_err(|e| format!("preset JSON parse error: {e}"))
    }

    /// Look up a parameter value by id.
    pub fn get(&self, id: &str) -> Option<f32> {
        self.values.get(id).copied()
    }
}

/// Parse every embedded JSON blob into a `Vec<Preset>`. Panics with a descriptive
/// message if any blob is malformed — presets are compile-time constants, so a bad
/// blob is a build/author error that must surface loudly (in tests), not silently.
pub fn load_all(blobs: &[&str]) -> Vec<Preset> {
    blobs
        .iter()
        .map(|b| Preset::parse(b).expect("embedded preset JSON must be valid"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_values() {
        let p = Preset::parse(r#"{ "name": "Test", "drive": 8.0, "mix": 100.0 }"#).unwrap();
        assert_eq!(p.name, "Test");
        assert_eq!(p.get("drive"), Some(8.0));
        assert_eq!(p.get("mix"), Some(100.0));
        assert_eq!(p.get("missing"), None);
    }

    #[test]
    fn load_all_parses_many() {
        let blobs = [
            r#"{ "name": "A", "x": 1.0 }"#,
            r#"{ "name": "B", "x": 2.0, "y": -3.5 }"#,
        ];
        let presets = load_all(&blobs);
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[1].get("y"), Some(-3.5));
    }
}
