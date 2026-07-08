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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// One preset: a display name and a map of `param_id -> plain value`. The same flat
/// struct backs BOTH factory presets (embedded JSON, keys are the plugin's pretty
/// preset ids) and user presets on disk (keys are the plugin's nih-plug param ids,
/// snapshotted generically — see [`crate::ui::snapshot_params`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Preset {
    pub name: String,
    /// Optional thematic-bank tag (OVERSEER-ENRICH: instrument type for the Node bar,
    /// session theme for the Master bar). `#[serde(default)]` keeps this back-compatible —
    /// existing factory JSON and all user presets simply omit it (→ `None`), and it is not
    /// serialized when absent, so user-preset files on disk are unchanged. Consumed as a
    /// typed field so it never lands in the numeric `values` map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
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

// ===========================================================================
// User preset disk tier (PRESET-SYSTEM, SPECS "POLISH phase")
// ===========================================================================
//
// User presets live at `[MyDocuments]/Qeynos/Presets/<plugin_id>/<name>.json` as the
// SAME flat JSON as factory presets. `[MyDocuments]` is resolved through the Windows
// known-folder API (`dirs::document_dir` → `SHGetKnownFolderPath(FOLDERID_Documents)`),
// which honours the OneDrive Documents redirection on this machine — NEVER a literal
// `%USERPROFILE%\Documents`.
//
// All IO is `Result`; the GUI shows the error inline and never panics. None of this is
// ever called from the audio thread — the preset bar drives it on the GUI thread only.

/// Characters that are illegal in a Windows path component, plus control chars, are
/// stripped from a user-supplied preset name.
fn is_illegal_name_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
}

/// Windows reserved device basenames (case-insensitive): `CON PRN AUX NUL`, `COM1..COM9`,
/// `LPT1..LPT9`. Windows treats a file whose base name (the part before the FIRST dot) matches
/// one of these as the *device*, even with an extension — so `NUL`, `nul.json`, and `COM5.foo`
/// all resolve to a device. Writing a preset to such a name silently targets the device: the
/// write "succeeds", the file never exists, and the preset is lost on the next listing. We
/// detect the reserved stem and suffix it so it lands on disk as an ordinary file.
fn is_reserved_stem(stem: &str) -> bool {
    let up = stem.to_ascii_uppercase();
    if matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    // COM1..9 / LPT1..9 — a 3-letter prefix plus a single 1..9 digit (COM0/LPT0 and COM10+
    // are NOT reserved).
    (up.starts_with("COM") || up.starts_with("LPT"))
        && up.len() == 4
        && matches!(up.as_bytes()[3], b'1'..=b'9')
}

/// Sanitize a user-supplied preset name into a safe file stem: drop illegal path
/// characters and control chars, collapse internal whitespace runs to single spaces,
/// trim surrounding whitespace and dots (Windows rejects trailing dots/spaces). The
/// result may be empty (caller treats an empty sanitized name as invalid). Idempotent:
/// `sanitize_name(sanitize_name(x)) == sanitize_name(x)`.
pub fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_space = false;
    for c in name.chars() {
        // Whitespace (incl. tab/newline, which are also control chars) collapses to a
        // single space; this is checked BEFORE the illegal/control strip so a newline
        // becomes a separator, not a deletion.
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else if is_illegal_name_char(c) {
            continue;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    let cleaned = out.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        return cleaned;
    }
    // Escape Windows reserved device basenames so the preset writes to a real file, not a device.
    let stem = cleaned.split('.').next().unwrap_or(cleaned.as_str());
    if is_reserved_stem(stem) {
        // Suffix the STEM with '_' (before any extension) so the base name is no longer a device:
        // "NUL" → "NUL_", "nul.json" → "nul_.json". Idempotent: "NUL_" is not reserved.
        let mut escaped = String::with_capacity(cleaned.len() + 1);
        escaped.push_str(stem);
        escaped.push('_');
        escaped.push_str(&cleaned[stem.len()..]); // remainder incl. the leading '.', if any
        return escaped;
    }
    cleaned
}

/// `[MyDocuments]` via the known-folder API. `None` if the shell can't resolve it.
pub fn documents_dir() -> Option<PathBuf> {
    dirs::document_dir()
}

/// Root of the user-preset tree: `[MyDocuments]/Qeynos/Presets`.
pub fn user_presets_root() -> Option<PathBuf> {
    documents_dir().map(|d| d.join("Qeynos").join("Presets"))
}

/// Per-plugin user-preset directory: `[MyDocuments]/Qeynos/Presets/<plugin_id>`.
/// `plugin_id` is a stable short slug (e.g. `"grit"`, `"overseer-node"`).
pub fn user_plugin_dir(plugin_id: &str) -> Option<PathBuf> {
    user_presets_root().map(|r| r.join(plugin_id))
}

/// List the user presets for a plugin, sorted by name (case-insensitive). Missing
/// directory ⇒ empty list. Unreadable/malformed individual files are skipped rather
/// than failing the whole listing. Only a failure to *resolve Documents* is an error.
pub fn list_user(plugin_id: &str) -> Result<Vec<Preset>, String> {
    let dir = user_plugin_dir(plugin_id)
        .ok_or_else(|| "could not resolve the Documents folder".to_string())?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("reading {}: {e}", dir.display())),
    };
    let mut presets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(p) = Preset::parse(&text) {
                presets.push(p);
            }
        }
    }
    presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(presets)
}

/// Save a user preset. The name is sanitized ([`sanitize_name`]); an empty sanitized
/// name is rejected. Overwrite is allowed. The sanitized name is stored as the preset's
/// display name so the file stem and the shown name always agree. Returns the path.
pub fn save_user(
    plugin_id: &str,
    name: &str,
    values: &BTreeMap<String, f32>,
) -> Result<PathBuf, String> {
    let clean = sanitize_name(name);
    if clean.is_empty() {
        return Err("preset name is empty after removing illegal characters".to_string());
    }
    let dir = user_plugin_dir(plugin_id)
        .ok_or_else(|| "could not resolve the Documents folder".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let path = dir.join(format!("{clean}.json"));
    let preset = Preset {
        name: clean,
        category: None,
        values: values.clone(),
    };
    let json = serde_json::to_string_pretty(&preset)
        .map_err(|e| format!("serializing preset: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(path)
}

/// Load a single user preset by (unsanitized) name.
pub fn load_user(plugin_id: &str, name: &str) -> Result<Preset, String> {
    let clean = sanitize_name(name);
    if clean.is_empty() {
        return Err("preset name is empty".to_string());
    }
    let dir = user_plugin_dir(plugin_id)
        .ok_or_else(|| "could not resolve the Documents folder".to_string())?;
    let path = dir.join(format!("{clean}.json"));
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Preset::parse(&text)
}

/// Delete a user preset by (unsanitized) name. Missing file is treated as success.
pub fn delete_user(plugin_id: &str, name: &str) -> Result<(), String> {
    let clean = sanitize_name(name);
    if clean.is_empty() {
        return Err("preset name is empty".to_string());
    }
    let dir = user_plugin_dir(plugin_id)
        .ok_or_else(|| "could not resolve the Documents folder".to_string())?;
    let path = dir.join(format!("{clean}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("deleting {}: {e}", path.display())),
    }
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
    fn category_is_backcompat_and_optional() {
        // Legacy factory/user JSON with no category → None, and no numeric leakage.
        let p = Preset::parse(r#"{ "name": "Legacy", "drive": 8.0 }"#).unwrap();
        assert_eq!(p.category, None);
        assert_eq!(p.get("drive"), Some(8.0));

        // Tagged factory JSON → category is consumed as a typed field, not a value.
        let t = Preset::parse(
            r#"{ "name": "Warehouse Thump", "category": "KICK", "drive": 4.0, "width": 0.0 }"#,
        )
        .unwrap();
        assert_eq!(t.category.as_deref(), Some("KICK"));
        assert_eq!(t.get("category"), None, "category must not leak into values");
        assert_eq!(t.get("drive"), Some(4.0));

        // Round-trips through serde, and a None category is omitted from the output.
        let none = Preset {
            name: "N".into(),
            category: None,
            values: vals(&[("x", 1.0)]),
        };
        let js = serde_json::to_string(&none).unwrap();
        assert!(!js.contains("category"), "None category must not serialize: {js}");
        let some = Preset {
            name: "S".into(),
            category: Some("PAD".into()),
            values: vals(&[("x", 1.0)]),
        };
        let js2 = serde_json::to_string(&some).unwrap();
        assert!(js2.contains("\"category\":\"PAD\""));
        assert_eq!(Preset::parse(&js2).unwrap().category.as_deref(), Some("PAD"));
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

    // --- Name sanitization --------------------------------------------------

    #[test]
    fn sanitize_strips_illegal_path_chars() {
        assert_eq!(sanitize_name(r#"Warehouse<>:"/\|?*Thump"#), "WarehouseThump");
        assert_eq!(sanitize_name("a/b\\c"), "abc");
    }

    #[test]
    fn sanitize_trims_and_collapses_whitespace() {
        assert_eq!(sanitize_name("  Last   Train  Home  "), "Last Train Home");
        assert_eq!(sanitize_name("\tTabbed\nName\r"), "Tabbed Name");
    }

    #[test]
    fn sanitize_trims_dots_and_control() {
        assert_eq!(sanitize_name("...Ghost..."), "Ghost");
        assert_eq!(sanitize_name("Bell\u{0007}\u{0000}"), "Bell");
    }

    #[test]
    fn sanitize_empty_and_illegal_only() {
        assert_eq!(sanitize_name(""), "");
        assert_eq!(sanitize_name(r#"<>:"/\|?*"#), "");
        assert_eq!(sanitize_name("   "), "");
    }

    #[test]
    fn sanitize_preserves_unicode() {
        // Non-ASCII letters are kept; only path-illegal + control chars are dropped.
        assert_eq!(sanitize_name("Drowned Ghøst · 亡霊"), "Drowned Ghøst · 亡霊");
    }

    #[test]
    fn sanitize_is_idempotent() {
        for s in [
            "  Foo/Bar  ",
            "亡霊",
            "...x...",
            "a\tb\nc",
            "NUL",
            "com5",
            "nul.json",
            "Con.Preset",
        ] {
            let once = sanitize_name(s);
            assert_eq!(sanitize_name(&once), once, "not idempotent for {s:?}");
        }
    }

    // --- Windows reserved device basenames -----------------------------------

    #[test]
    fn sanitize_escapes_reserved_device_names() {
        // Bare reserved stems, all cases → suffixed so they are no longer devices.
        assert_eq!(sanitize_name("NUL"), "NUL_");
        assert_eq!(sanitize_name("nul"), "nul_");
        assert_eq!(sanitize_name("NuL"), "NuL_");
        assert_eq!(sanitize_name("CON"), "CON_");
        assert_eq!(sanitize_name("prn"), "prn_");
        assert_eq!(sanitize_name("Aux"), "Aux_");
        assert_eq!(sanitize_name("COM5"), "COM5_");
        assert_eq!(sanitize_name("lpt9"), "lpt9_");

        // A reserved stem with an extension is STILL the device on Windows → the STEM is escaped
        // (not the whole filename), keeping the base name safe: "nul.json" → "nul_.json".
        assert_eq!(sanitize_name("nul.json"), "nul_.json");
        assert_eq!(sanitize_name("Con.Preset"), "Con_.Preset");
        assert_eq!(sanitize_name("COM1.bank.json"), "COM1_.bank.json");
    }

    #[test]
    fn sanitize_leaves_non_reserved_lookalikes() {
        // Not devices: COM0/LPT0, COM10+, and names that merely start with a reserved word.
        for s in ["COM0", "LPT0", "COM10", "COM12", "CONSOLE", "Nullify", "Auxiliary", "Prne"] {
            assert_eq!(sanitize_name(s), s, "{s:?} must not be escaped");
        }
    }

    #[test]
    fn reserved_name_round_trips_save_list_load() {
        // A user naming a preset "NUL" used to silently vanish (write hit the null device).
        // After escaping, it must save, appear in the listing, and load back.
        let plugin = "test-reserved";
        let path = save_user(plugin, "NUL", &vals(&[("x", 1.0), ("y", 2.0)])).expect("save NUL");
        assert!(path.exists(), "escaped reserved-name preset must exist on disk");
        assert!(
            path.file_name().unwrap().to_string_lossy().starts_with("NUL_"),
            "file stem must be escaped: {}",
            path.display()
        );

        let names: Vec<_> = list_user(plugin)
            .expect("list")
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert!(names.contains(&"NUL_".to_string()), "listing must include NUL_: {names:?}");

        let loaded = load_user(plugin, "NUL").expect("load by original name");
        assert_eq!(loaded.name, "NUL_");
        assert_eq!(loaded.get("y"), Some(2.0));

        delete_user(plugin, "NUL").expect("delete");
        assert!(load_user(plugin, "NUL").is_err(), "deleted reserved-name preset must not load");
        let _ = std::fs::remove_dir(user_plugin_dir(plugin).unwrap());
    }

    // --- Disk-tier round trips (GUI-less). These use the SAME code path the preset
    //     bar drives on the GUI thread. A unique plugin_id per test keeps them
    //     isolated under [MyDocuments]/Qeynos/Presets/<id> and they clean up after
    //     themselves. -----------------------------------------------------------

    fn vals(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn save_mutate_load_restores_exactly() {
        let plugin = "test-roundtrip";
        let original = vals(&[("drive", 8.0), ("mix", 100.0), ("hz", 12345.0), ("neg", -3.5)]);
        let path = save_user(plugin, "Round Trip", &original).expect("save");

        // Mutate the in-memory map; the on-disk file must be unaffected.
        let mut mutated = original.clone();
        mutated.insert("drive".into(), -99.0);
        mutated.insert("mix".into(), 0.0);

        let loaded = load_user(plugin, "Round Trip").expect("load");
        assert_eq!(loaded.name, "Round Trip");
        assert_eq!(loaded.values, original, "loaded values must match what was saved");
        assert_ne!(loaded.values, mutated);

        // Overwrite is allowed and replaces content.
        save_user(plugin, "Round Trip", &mutated).expect("overwrite");
        let reloaded = load_user(plugin, "Round Trip").expect("reload");
        assert_eq!(reloaded.values, mutated);

        delete_user(plugin, "Round Trip").expect("delete");
        assert!(load_user(plugin, "Round Trip").is_err(), "deleted preset must not load");
        let _ = std::fs::remove_dir(user_plugin_dir(plugin).unwrap());
        let _ = path; // path returned points inside the plugin dir
    }

    #[test]
    fn list_user_sorts_and_ignores_non_json() {
        let plugin = "test-list";
        let dir = user_plugin_dir(plugin).unwrap();
        let _ = std::fs::create_dir_all(&dir);
        // A stray non-preset file must be ignored, not crash the listing.
        let _ = std::fs::write(dir.join("notes.txt"), "ignore me");
        save_user(plugin, "Beta", &vals(&[("x", 1.0)])).unwrap();
        save_user(plugin, "alpha", &vals(&[("x", 2.0)])).unwrap();

        let list = list_user(plugin).unwrap();
        let names: Vec<_> = list.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, vec!["alpha".to_string(), "Beta".to_string()]);

        for n in ["Beta", "alpha"] {
            delete_user(plugin, n).unwrap();
        }
        let _ = std::fs::remove_file(dir.join("notes.txt"));
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn list_user_missing_dir_is_empty_not_error() {
        assert_eq!(list_user("test-definitely-absent-plugin").unwrap().len(), 0);
    }

    #[test]
    fn save_rejects_empty_sanitized_name() {
        assert!(save_user("test-empty", r#"<>:"/\|?*"#, &vals(&[("x", 1.0)])).is_err());
    }

    #[test]
    fn sanitized_name_and_file_stem_agree() {
        // A name with illegal chars saves to the sanitized stem and reloads by the
        // original (unsanitized) name.
        let plugin = "test-sanitize-file";
        save_user(plugin, "Deep/Sub :Bass", &vals(&[("x", 1.0)])).unwrap();
        let clean = sanitize_name("Deep/Sub :Bass");
        assert_eq!(clean, "DeepSub Bass");
        let loaded = load_user(plugin, "Deep/Sub :Bass").unwrap();
        assert_eq!(loaded.name, clean);
        delete_user(plugin, "Deep/Sub :Bass").unwrap();
        let _ = std::fs::remove_dir(user_plugin_dir(plugin).unwrap());
    }

    /// Verifies the REAL Documents path resolves (OneDrive-redirected on this machine)
    /// and is writable — writes a sample `Test Save.json` under a `_selftest` plugin dir
    /// and LEAVES it as a documented artifact (see docs/PRESETS.md). Idempotent.
    #[test]
    fn real_documents_path_resolves_and_is_writable() {
        let root = user_presets_root().expect("resolve [MyDocuments]/Qeynos/Presets");
        // On this build machine Documents is under OneDrive; assert we did NOT fall back
        // to a bare %USERPROFILE%\Documents when a redirect is in effect is not possible
        // to check portably, so we only assert the path is absolute and writable.
        assert!(root.is_absolute(), "documents root must be absolute: {}", root.display());
        let path = save_user(
            "_selftest",
            "Test Save",
            &vals(&[("marker", 1.0), ("value", 42.0)]),
        )
        .expect("must be able to write a user preset to the real Documents path");
        assert!(path.exists());
        let back = load_user("_selftest", "Test Save").expect("read back sample");
        assert_eq!(back.get("value"), Some(42.0));
        // Intentionally left on disk as a human-visible sample (documented).
    }
}
