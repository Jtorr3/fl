//! Per-parameter "listen" layer (PRD §3 / SPECS "NERVE").
//!
//! The tier-2 [`crate::bus`] carries 8 modulation streams per source instance (NERVE
//! publishes them). This module lets *any* plugin param subscribe to one of those streams:
//! a persisted route `{param_id → (source_instance, source_index, depth, curve)}`, edited
//! from a small "MOD" section in the plugin GUI ([`crate::ui::mod_section`]), and applied
//! **at block rate as a normalized-value offset**.
//!
//! # How it is applied (important)
//! Writing a host parameter from the audio thread is wrong (it fights automation, undo and
//! the smoother). Instead the wrapper computes, per block:
//!
//! ```text
//! modulated_normalized = clamp(base_normalized + depth · curve(signal), 0, 1)
//! ```
//!
//! and feeds the plugin's DSP `configure` the *modulated* value **without touching host
//! param state**. The host, GUI and automation continue to see the unmodulated `base`
//! value; the modulation is a live additive offset, exactly like an internal LFO tool. The
//! plugin's own smoother (which glides toward the value the DSP is configured with)
//! removes any block-rate zipper.
//!
//! # RT-safety
//! [`ModRoutes`] is the persisted config, edited on the GUI thread and shared to the audio
//! thread behind an `RwLock`. The audio thread only ever `try_read`s it and looks routes up
//! by `&str` comparison (no allocation); on the rare block where the GUI holds the write
//! lock it simply applies the unmodulated base for that block. Bus reads use
//! [`crate::bus::Bus::read_mod_fast`] / [`crate::bus::Bus::resolve_instance`] (alloc-free).

use serde::{Deserialize, Serialize};

use crate::bus::{Bus, NUM_MOD_SIGNALS};

/// Shaping applied to a raw bus signal before it is scaled by `depth`. NERVE publishes LFO /
/// random / macro streams in roughly `[-1, 1]` (bipolar) and env-follower / macro streams in
/// `[0, 1]`; the curve decides how that maps onto a `[0,1]`-normalized parameter offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Curve {
    /// `signal` passed straight through (bipolar sources swing the param both ways).
    Linear,
    /// `-1..1 → 0..1` — a one-directional (upward-only) offset.
    Unipolar,
    /// `sign(signal)·signal²` — soft near 0, emphatic near the extremes; keeps sign.
    Squared,
    /// Smoothstep of the unipolar-mapped signal — gentle S-shaped one-directional offset.
    SmoothStep,
}

impl Curve {
    /// All curves, for GUI selectors.
    pub const ALL: [Curve; 4] = [
        Curve::Linear,
        Curve::Unipolar,
        Curve::Squared,
        Curve::SmoothStep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Curve::Linear => "Linear",
            Curve::Unipolar => "Unipolar",
            Curve::Squared => "Squared",
            Curve::SmoothStep => "S-Curve",
        }
    }

    /// Map a raw bus signal to a shaped factor (still pre-`depth`).
    #[inline]
    pub fn apply(self, signal: f32) -> f32 {
        let s = signal.clamp(-1.0, 1.0);
        match self {
            Curve::Linear => s,
            Curve::Unipolar => s * 0.5 + 0.5,
            Curve::Squared => s.signum() * s * s,
            Curve::SmoothStep => {
                let u = (s * 0.5 + 0.5).clamp(0.0, 1.0);
                u * u * (3.0 - 2.0 * u)
            }
        }
    }
}

/// One subscription: a param listens to `source_index` of `source_instance` on the bus.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Route {
    /// nih-plug param id this route modulates.
    pub param_id: String,
    /// Bus instance id of the source (e.g. a NERVE instance). 0 = unassigned.
    pub source_instance: u64,
    /// Which of the source's 8 streams (0..NUM_MOD_SIGNALS).
    pub source_index: u8,
    /// Modulation depth, `-1..=1` (normalized-param units).
    pub depth: f32,
    pub curve: Curve,
}

impl Route {
    pub fn new(param_id: impl Into<String>) -> Self {
        Self {
            param_id: param_id.into(),
            source_instance: 0,
            source_index: 0,
            depth: 0.0,
            curve: Curve::Linear,
        }
    }

    /// True if this route can produce a non-zero offset (assigned source + non-zero depth).
    #[inline]
    pub fn is_live(&self) -> bool {
        self.source_instance != 0
            && self.depth != 0.0
            && (self.source_index as usize) < NUM_MOD_SIGNALS
    }
}

/// The persisted set of routes for one plugin instance. Serialises to a compact JSON string
/// for a nih-plug `#[persist]` field. Edited on the GUI thread, read on the audio thread.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct ModRoutes {
    pub routes: Vec<Route>,
}

impl ModRoutes {
    pub fn new() -> Self {
        Self::default()
    }

    /// The route for `param_id`, if any.
    pub fn get(&self, param_id: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.param_id == param_id)
    }

    pub fn get_mut(&mut self, param_id: &str) -> Option<&mut Route> {
        self.routes.iter_mut().find(|r| r.param_id == param_id)
    }

    /// Insert or replace the route for its `param_id`.
    pub fn set(&mut self, route: Route) {
        if let Some(existing) = self.get_mut(&route.param_id) {
            *existing = route;
        } else {
            self.routes.push(route);
        }
    }

    /// Remove any route for `param_id`.
    pub fn clear(&mut self, param_id: &str) {
        self.routes.retain(|r| r.param_id != param_id);
    }

    /// Get the route for `param_id`, creating an unassigned one if absent (GUI editing).
    pub fn entry(&mut self, param_id: &str) -> &mut Route {
        if self.get(param_id).is_none() {
            self.routes.push(Route::new(param_id));
        }
        self.get_mut(param_id).unwrap()
    }

    /// Normalized offset for `param_id` given the current bus state. `0.0` when unrouted,
    /// depth-0, or the source slot is not live. Alloc-free — safe on the audio thread.
    #[inline]
    pub fn offset_for(&self, param_id: &str, bus: Option<&Bus>) -> f32 {
        let route = match self.get(param_id) {
            Some(r) if r.is_live() => r,
            _ => return 0.0,
        };
        let bus = match bus {
            Some(b) => b,
            None => return 0.0,
        };
        let idx = match bus.resolve_instance(route.source_instance) {
            Some(i) => i,
            None => return 0.0,
        };
        match bus.read_mod_fast(idx, route.source_index as usize) {
            Some(sig) => route.depth * route.curve.apply(sig),
            None => 0.0,
        }
    }

    /// Compute the modulated PLAIN value for a nih-plug [`nih_plug::prelude::FloatParam`]:
    /// `base_normalized + offset`, clamped to `0..1`, mapped back to plain through the
    /// param's own range. The host still sees the unmodulated base — see the module docs.
    /// Alloc-free; the standard one-line retrofit at a plugin's block-rate configure site.
    #[cfg(feature = "gui")]
    #[inline]
    pub fn modulated_float(
        &self,
        param_id: &str,
        param: &nih_plug::prelude::FloatParam,
        bus: Option<&Bus>,
    ) -> f32 {
        use nih_plug::prelude::Param;
        let off = self.offset_for(param_id, bus);
        if off == 0.0 {
            return param.value();
        }
        let base = param.unmodulated_normalized_value();
        let modded = (base + off).clamp(0.0, 1.0);
        param.preview_plain(modded)
    }

    /// Serialize to the compact JSON string a `#[persist]` field stores.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"routes\":[]}".to_string())
    }

    /// Parse from a persisted JSON string (empty / invalid → default, never panics).
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{new_instance_id, Bus, PluginKind, NUM_MOD_SIGNALS};
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qeynos-bus-modlisten-{}-{}-{}",
            tag,
            std::process::id(),
            new_instance_id()
        ))
    }

    #[test]
    fn curve_shapes() {
        assert!((Curve::Linear.apply(0.5) - 0.5).abs() < 1e-6);
        assert!((Curve::Unipolar.apply(-1.0) - 0.0).abs() < 1e-6);
        assert!((Curve::Unipolar.apply(1.0) - 1.0).abs() < 1e-6);
        assert!((Curve::Squared.apply(-0.5) + 0.25).abs() < 1e-6); // keeps sign
        assert!((Curve::SmoothStep.apply(-1.0)).abs() < 1e-6);
        assert!((Curve::SmoothStep.apply(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn offset_tracks_bus_signal() {
        let path = temp_path("offset");
        let writer = Bus::open_or_create(&path).unwrap();
        let reader = Bus::open_or_create(&path).unwrap();
        let src = new_instance_id();
        let idx = writer.claim(src, PluginKind::Nerve, "LFO").unwrap();

        let mut routes = ModRoutes::new();
        routes.set(Route {
            param_id: "drive".into(),
            source_instance: src,
            source_index: 2,
            depth: 0.5,
            curve: Curve::Linear,
        });

        // No signal yet published beyond 0 → offset 0.
        assert!(routes.offset_for("drive", Some(&reader)).abs() < 1e-6);

        // Publish signal 2 = 0.8 → offset = depth(0.5) * 0.8 = 0.4.
        let mut mods = [0.0f32; NUM_MOD_SIGNALS];
        mods[2] = 0.8;
        writer.publish_mods(idx, &mods);
        writer.beat(idx);
        let off = routes.offset_for("drive", Some(&reader));
        assert!((off - 0.4).abs() < 1e-4, "offset was {off}");

        // Unrouted param → 0. Missing bus → 0.
        assert_eq!(routes.offset_for("mix", Some(&reader)), 0.0);
        assert_eq!(routes.offset_for("drive", None), 0.0);

        writer.release(idx, src);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dead_source_yields_zero_offset() {
        let path = temp_path("dead");
        let bus = Bus::open_or_create(&path).unwrap();
        let mut routes = ModRoutes::new();
        // Points at an instance that was never claimed.
        routes.set(Route {
            param_id: "cutoff".into(),
            source_instance: 999_999,
            source_index: 0,
            depth: 1.0,
            curve: Curve::Linear,
        });
        assert_eq!(routes.offset_for("cutoff", Some(&bus)), 0.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn serde_round_trip() {
        let mut r = ModRoutes::new();
        r.set(Route {
            param_id: "drive".into(),
            source_instance: 12345,
            source_index: 3,
            depth: -0.75,
            curve: Curve::Squared,
        });
        let js = r.to_json();
        let back = ModRoutes::from_json(&js);
        assert_eq!(r, back);
        // Garbage → default, no panic.
        assert_eq!(ModRoutes::from_json("not json"), ModRoutes::default());
    }
}
