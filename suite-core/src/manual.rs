//! Built-in usage manual — the tiny section parser behind the in-GUI '?' panel.
//!
//! Every plugin's `docs/<PLUGIN>.md` is embedded at compile time (`include_str!`) and
//! parsed by [`Manual::parse`] into `## `-delimited sections. The renderer lives in
//! [`crate::ui::manual_button`] (gui-only); this module is pure so the cross-check test
//! (every param display name appears in the Controls section) can run without the `gui`
//! feature getting in the way — one source of truth, readable both on GitHub and in-GUI.
//!
//! Section convention appended to each docs file (parser is tolerant — a missing section
//! simply renders as absent, never panics):
//! * `## What It Is`   — 2-3 sentences.
//! * `## Signal Flow`  — the ASCII diagram (rendered monospace).
//! * `## Controls`     — every param: name — what it does musically.
//! * `## Recipes`      — concrete workflow recipes with real settings.

/// One `## Heading` section of a manual: its title and the raw markdown body beneath it
/// (up to the next `## ` heading or end of file).
#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub body: String,
}

/// A parsed plugin manual: the ordered list of its `## ` sections.
#[derive(Clone, Debug, Default)]
pub struct Manual {
    pub sections: Vec<Section>,
}

impl Manual {
    /// Split a markdown document into its `## ` sections. Text before the first `## `
    /// heading (the title line and intro) is ignored — the manual panel renders only the
    /// canonical sections. Tolerant by construction: no headings ⇒ empty manual, never a
    /// panic.
    pub fn parse(doc: &str) -> Manual {
        let mut sections: Vec<Section> = Vec::new();
        let mut cur: Option<Section> = None;
        for line in doc.lines() {
            // A level-2 ATX heading opens a new section. Deeper headings (`### `) stay
            // in the current section's body so sub-structure survives.
            if let Some(rest) = line.strip_prefix("## ") {
                if let Some(s) = cur.take() {
                    sections.push(s);
                }
                cur = Some(Section {
                    title: rest.trim().to_string(),
                    body: String::new(),
                });
            } else if let Some(s) = cur.as_mut() {
                s.body.push_str(line);
                s.body.push('\n');
            }
        }
        if let Some(s) = cur.take() {
            sections.push(s);
        }
        Manual { sections }
    }

    /// The trimmed body of the section whose title matches `title` case-insensitively,
    /// or `None` if absent.
    pub fn section(&self, title: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| s.title.eq_ignore_ascii_case(title))
            .map(|s| s.body.trim_matches(['\n', ' ', '\t'].as_ref()))
    }

    /// Whether a section exists and has non-whitespace content.
    pub fn has_content(&self, title: &str) -> bool {
        self.section(title).map_or(false, |b| !b.trim().is_empty())
    }
}

/// The display names of every param in a `Params` set, in declaration order. Used by the
/// per-plugin cross-check test to assert the Controls section documents every param.
///
/// Gui-gated because it depends on `nih_plug` (only compiled with the `gui` feature),
/// which is on by default for plugins and their tests.
#[cfg(feature = "gui")]
pub fn param_names(params: &dyn nih_plug::prelude::Params) -> Vec<String> {
    params
        .param_map()
        .into_iter()
        // SAFETY: `param_map` returns live `ParamPtr`s into `params`, which outlives this
        // call; `name()` only reads the param's static display name.
        .map(|(_, ptr, _)| unsafe { ptr.name().to_string() })
        .collect()
}

/// Assert a plugin's manual documents every param and has a non-empty Recipes section —
/// the BUILT-IN-MANUALS done-bar, called from each plugin's tests with its embedded doc
/// and default params. Panics with a precise message on the first gap.
#[cfg(feature = "gui")]
pub fn assert_manual_covers_params(doc: &str, params: &dyn nih_plug::prelude::Params) {
    let manual = Manual::parse(doc);
    let controls = manual
        .section("Controls")
        .expect("manual is missing a `## Controls` section");
    for name in param_names(params) {
        assert!(
            controls.contains(name.as_str()),
            "param `{name}` is not documented in the manual's Controls section"
        );
    }
    assert!(
        manual.has_content("Recipes"),
        "manual is missing a non-empty `## Recipes` section"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# TITLE — foo\n\nintro para\n\n## What It Is\nA thing.\nTwo lines.\n\n## Signal Flow\n```\nin -> core -> out\n```\n\n## Controls\n- Drive — pushes it.\n- Mix — blends.\n\n## Recipes\n1. Do a thing.\n";

    #[test]
    fn parses_canonical_sections() {
        let m = Manual::parse(DOC);
        assert_eq!(m.sections.len(), 4);
        assert!(m.section("What It Is").unwrap().starts_with("A thing."));
        assert!(m.section("signal flow").unwrap().contains("in -> core -> out"));
        assert!(m.section("Controls").unwrap().contains("Drive"));
        assert!(m.has_content("Recipes"));
    }

    #[test]
    fn missing_section_is_none_not_panic() {
        let m = Manual::parse("# just a title\nno sections here\n");
        assert!(m.section("Controls").is_none());
        assert!(!m.has_content("Recipes"));
        assert!(m.sections.is_empty());
    }

    #[test]
    fn empty_input_is_empty_manual() {
        let m = Manual::parse("");
        assert!(m.sections.is_empty());
        assert!(m.section("What It Is").is_none());
    }
}
