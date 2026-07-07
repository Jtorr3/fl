//! The OVERSEER "bus": a same-DLL shared registry linking every Node instance to the
//! Master. Both plugins are exported from this one cdylib, so a process-wide
//! `static BUS: OnceLock<Bus>` is genuinely shared between them (PRD §3, tier 1).
//!
//! # Locking discipline
//! No lock is ever held across audio processing. The only mutex guards the *structure* of
//! the registry (the `Vec` of slots) and is taken only at block boundaries — on register,
//! on GC, and when the Master snapshots the live slot list for its GUI/overrides. Each
//! [`Slot`]'s live data (meters, param mirror, the override area, heartbeat) is all
//! atomics, so a Node touches only lock-free atomics on its own `Arc<Slot>` during
//! `process` and the Master writes overrides through the same atomics.
//!
//! # GC
//! Every Node owns an `Arc<Slot>`; the registry holds another. A slot is dead once no Node
//! references it (`Arc::strong_count == 1`). GC (run on any registry access) drops those,
//! so crashed/removed Node instances never linger. A heartbeat block-counter additionally
//! lets the Master show which slots are actively streaming audio.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Overridable Node-strip parameters the Master can remote-control. Order is the index
/// space used across the override area, the param mirror, and the Master GUI grid.
pub const OVR_THRESHOLD: usize = 0;
pub const OVR_RATIO: usize = 1;
pub const OVR_DRIVE: usize = 2;
pub const OVR_WIDTH: usize = 3;
pub const OVR_TRIM: usize = 4;
pub const NUM_OVERRIDES: usize = 5;

/// Human-readable names for the overridable params (Master GUI labels).
pub const OVR_NAMES: [&str; NUM_OVERRIDES] = ["THRESH", "RATIO", "DRIVE", "WIDTH", "TRIM"];

#[inline]
fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}
#[inline]
fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// One registry slot, owned jointly by a Node and the registry.
pub struct Slot {
    pub id: u64,
    /// User-editable instance label ("KICK"). Touched only by the GUI thread + registration.
    label: Mutex<String>,

    // Meters (written by the Node each block, read by the Master GUI).
    peak_db: AtomicU32,
    rms_db: AtomicU32,
    lufs_m: AtomicU32,

    // Param mirror: the Node's *effective* values, so the Master GUI shows the truth even
    // when the Node is running on its own local params.
    mirror: [AtomicU32; NUM_OVERRIDES],

    // Override area: the Master writes here; the Node reads it each block.
    ovr_val: [AtomicU32; NUM_OVERRIDES],
    ovr_active: [AtomicBool; NUM_OVERRIDES],
    /// Timestamp of the Master's most recent override write.
    ovr_ts: AtomicU64,
    /// Timestamp of the Node's most recent *local* param touch (steal-back).
    local_ts: AtomicU64,

    /// Block counter; advances every Node `process` block (liveness).
    heartbeat: AtomicU64,
}

impl Slot {
    fn new(id: u64, label: String) -> Self {
        Self {
            id,
            label: Mutex::new(label),
            peak_db: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            rms_db: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            lufs_m: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            mirror: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            ovr_val: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            ovr_active: std::array::from_fn(|_| AtomicBool::new(false)),
            ovr_ts: AtomicU64::new(0),
            local_ts: AtomicU64::new(0),
            heartbeat: AtomicU64::new(0),
        }
    }

    // ---- label -------------------------------------------------------------
    pub fn label(&self) -> String {
        self.label.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn set_label(&self, s: &str) {
        if let Ok(mut g) = self.label.lock() {
            *g = s.to_string();
        }
    }

    // ---- meters ------------------------------------------------------------
    pub fn set_meters(&self, peak_db: f32, rms_db: f32, lufs_m: f32) {
        store_f32(&self.peak_db, peak_db);
        store_f32(&self.rms_db, rms_db);
        store_f32(&self.lufs_m, lufs_m);
    }
    pub fn meters(&self) -> (f32, f32, f32) {
        (
            load_f32(&self.peak_db),
            load_f32(&self.rms_db),
            load_f32(&self.lufs_m),
        )
    }

    // ---- param mirror ------------------------------------------------------
    pub fn set_mirror(&self, idx: usize, v: f32) {
        if idx < NUM_OVERRIDES {
            store_f32(&self.mirror[idx], v);
        }
    }
    pub fn mirror(&self, idx: usize) -> f32 {
        if idx < NUM_OVERRIDES {
            load_f32(&self.mirror[idx])
        } else {
            0.0
        }
    }

    // ---- override area -----------------------------------------------------
    /// Master writes an override value for param `idx` (bumps the override timestamp so a
    /// fresh write wins over any earlier local touch).
    pub fn write_override(&self, idx: usize, v: f32) {
        if idx >= NUM_OVERRIDES {
            return;
        }
        store_f32(&self.ovr_val[idx], v);
        self.ovr_active[idx].store(true, Ordering::Relaxed);
        self.ovr_ts.store(bus().tick(), Ordering::Relaxed);
    }
    /// Master releases its override on param `idx`.
    pub fn clear_override(&self, idx: usize) {
        if idx < NUM_OVERRIDES {
            self.ovr_active[idx].store(false, Ordering::Relaxed);
        }
    }
    pub fn is_override_active(&self, idx: usize) -> bool {
        idx < NUM_OVERRIDES && self.ovr_active[idx].load(Ordering::Relaxed)
    }
    /// Current override value for param `idx` (meaningful when the override is active).
    pub fn override_value(&self, idx: usize) -> f32 {
        if idx < NUM_OVERRIDES {
            load_f32(&self.ovr_val[idx])
        } else {
            0.0
        }
    }
    /// True if the Master currently holds *any* param (badge on the Node GUI).
    pub fn override_held(&self) -> bool {
        self.ovr_ts.load(Ordering::Relaxed) > self.local_ts.load(Ordering::Relaxed)
            && (0..NUM_OVERRIDES).any(|i| self.ovr_active[i].load(Ordering::Relaxed))
    }
    /// The Node records a local param touch; this steals control back from the Master
    /// (write-wins by timestamp).
    pub fn note_local_touch(&self) {
        self.local_ts.store(bus().tick(), Ordering::Relaxed);
    }
    /// Resolve the effective value of param `idx`: the Master override wins iff it is active
    /// and newer than the last local touch; otherwise the Node's `local_val`.
    #[inline]
    pub fn effective(&self, idx: usize, local_val: f32) -> f32 {
        if idx < NUM_OVERRIDES
            && self.ovr_active[idx].load(Ordering::Relaxed)
            && self.ovr_ts.load(Ordering::Relaxed) > self.local_ts.load(Ordering::Relaxed)
        {
            load_f32(&self.ovr_val[idx])
        } else {
            local_val
        }
    }

    // ---- heartbeat ---------------------------------------------------------
    pub fn beat(&self) {
        self.heartbeat.fetch_add(1, Ordering::Relaxed);
    }
    pub fn heartbeat(&self) -> u64 {
        self.heartbeat.load(Ordering::Relaxed)
    }
}

/// The process-wide registry.
pub struct Bus {
    slots: Mutex<Vec<Arc<Slot>>>,
    next_id: AtomicU64,
    clock: AtomicU64,
}

static BUS: OnceLock<Bus> = OnceLock::new();

/// Access the process-wide bus (lazily created on first use).
pub fn bus() -> &'static Bus {
    BUS.get_or_init(|| Bus {
        slots: Mutex::new(Vec::new()),
        next_id: AtomicU64::new(1),
        clock: AtomicU64::new(1),
    })
}

impl Bus {
    /// Monotonic timestamp source for override/local-touch ordering.
    #[inline]
    pub fn tick(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Register a new Node slot and return its shared handle. Runs GC first.
    pub fn register(&self, label: &str) -> Arc<Slot> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let slot = Arc::new(Slot::new(id, label.to_string()));
        if let Ok(mut slots) = self.slots.lock() {
            slots.retain(|s| Arc::strong_count(s) > 1);
            slots.push(slot.clone());
        }
        slot
    }

    /// Snapshot the currently-live slots (Node still referencing them), GC'ing dead ones.
    pub fn live_slots(&self) -> Vec<Arc<Slot>> {
        if let Ok(mut slots) = self.slots.lock() {
            slots.retain(|s| Arc::strong_count(s) > 1);
            slots.clone()
        } else {
            Vec::new()
        }
    }

    /// Number of currently-registered (pre-GC) slots — test/introspection helper.
    pub fn slot_count(&self) -> usize {
        self.slots.lock().map(|s| s.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_gc_by_strong_count() {
        let b = bus();
        let before = b.live_slots().len();
        let s = b.register("KICK");
        assert!(b.live_slots().iter().any(|x| x.id == s.id));
        let id = s.id;
        drop(s);
        // After dropping the Node handle, the slot GCs on a subsequent access. Retry to
        // tolerate a concurrent test transiently cloning the slot vec (which briefly bumps
        // every slot's strong count) — the global BUS is shared across all tests in this
        // binary, so they run against it in parallel.
        let mut gone = false;
        for _ in 0..10_000 {
            if !b.live_slots().iter().any(|x| x.id == id) {
                gone = true;
                break;
            }
            std::thread::yield_now();
        }
        assert!(gone, "dead slot was not GC'd");
        assert!(b.live_slots().len() >= before.saturating_sub(0));
    }

    #[test]
    fn override_wins_then_local_steals_back() {
        let b = bus();
        let s = b.register("VOX");
        // No override yet → effective is the local value.
        assert_eq!(s.effective(OVR_DRIVE, 3.0), 3.0);
        // Master writes an override → it wins.
        s.write_override(OVR_DRIVE, 9.0);
        assert_eq!(s.effective(OVR_DRIVE, 3.0), 9.0);
        assert!(s.override_held());
        // Local touch (newer timestamp) steals control back.
        s.note_local_touch();
        assert_eq!(s.effective(OVR_DRIVE, 3.0), 3.0);
        assert!(!s.override_held());
    }
}
