//! Tier-2 cross-DLL / cross-process suite bus (PRD §3).
//!
//! Where the OVERSEER tier-1 bus (`plugins/overseer/src/bus.rs`) is a same-DLL
//! `OnceLock<Registry>` — genuinely shared only when the linked plugins load into one
//! address space — this **tier-2** bus is file-backed shared memory that links *any*
//! Qeynos instance in *any* process. NERVE publishes its 8 modulation streams here; the
//! per-param "listen" layer ([`crate::modlisten`]) and X-RAY (spectrum, later) read it.
//!
//! # Mechanism
//! A single fixed-size file at `%TEMP%\qeynos-bus` is mapped (`memmap2::MmapMut`,
//! `MAP_SHARED`/`PAGE_READWRITE` — the canonical Windows cross-process page-sharing
//! mechanism, no named kernel object needed). The file is created **at its full fixed
//! size before mapping and never grown live**. A header carries a magic + version + slot
//! count; a stale or incompatible file (older layout / different slot count / wrong size)
//! is recreated. All live data is accessed through raw pointers into the map as atomics.
//!
//! # Per-slot SEQLOCK
//! Each slot's payload (mod signals, spectrum, peak/RMS, label, kind, instance id) is
//! guarded by a per-slot generation counter: the writer bumps it to **odd**, writes the
//! payload, bumps it to **even**; a reader loads it (must be even), reads, re-loads, and
//! retries if it changed or was odd. This gives a wait-free, alloc-free reader that never
//! observes a torn / mixed-generation snapshot. Heartbeat and staleness timestamps are
//! independent monotonic atomics (torn-read irrelevant), so they live outside the seqlock.
//!
//! # GC
//! There is no cross-process `Arc` strong-count, so liveness is a wall-clock heartbeat:
//! the owner stamps `last_beat_ms` every block; a slot unbeaten for [`STALE_MS`] is
//! reclaimable and any [`Bus::claim`] first GCs it (CAS `instance_id` back to the 0 =
//! free sentinel). A monotonic block `heartbeat` counter is also published for liveness
//! display.
//!
//! # Safety
//! The mapped region outlives every borrow (it is owned by the [`Bus`]); the base address
//! is stable for the life of the mapping. Every field is an `Atomic*` accessed through a
//! shared reference synthesised from the base pointer, so all reads/writes are atomic and
//! interior-mutable — no `&mut` aliasing, no data race even across processes (x86-64 pages
//! are cache-coherent). The raw base pointer makes [`Bus`] `!Send`/`!Sync` by default; we
//! `unsafe impl` both because access is exclusively through these atomics.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use memmap2::{MmapMut, MmapOptions};

/// File magic: "QVSB" (Qeynos suite bus).
const MAGIC: u32 = 0x5156_5342;
/// Layout version — bump on any field/size change so old files are recreated.
const VERSION: u32 = 1;
/// Fixed number of slots. Every plugin instance in the whole session claims one.
pub const NUM_SLOTS: usize = 64;
/// Modulation streams per slot (the NERVE outputs).
pub const NUM_MOD_SIGNALS: usize = 8;
/// Spectrum bands per slot (published for X-RAY; unused by NERVE, left at 0).
pub const NUM_SPECTRUM: usize = 32;
/// Max bytes of a user label held in-slot (UTF-8, truncated on overflow).
pub const LABEL_CAP: usize = 32;
/// A slot unbeaten for longer than this (wall-clock) is reclaimable by GC.
pub const STALE_MS: u64 = 3_000;
/// Free-slot sentinel for `instance_id`.
const FREE: u64 = 0;

/// Plugin-kind tag published in a slot (for reader UIs to label sources).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PluginKind {
    Generic = 0,
    Nerve = 1,
    Xray = 2,
}
impl PluginKind {
    fn from_u32(v: u32) -> Self {
        match v {
            1 => PluginKind::Nerve,
            2 => PluginKind::Xray,
            _ => PluginKind::Generic,
        }
    }
}

#[inline]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[inline]
fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}
#[inline]
fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Raw shared-memory layout (repr(C), accessed in place through the mmap)
// ---------------------------------------------------------------------------

#[repr(C)]
struct RawHeader {
    magic: AtomicU32,
    version: AtomicU32,
    num_slots: AtomicU32,
    _reserved: AtomicU32,
}

#[repr(C)]
struct RawSlot {
    /// Seqlock generation: even = stable, odd = write in progress.
    seq: AtomicU32,
    /// [`PluginKind`] tag (written under the seqlock).
    kind: AtomicU32,
    /// Owner instance id; `FREE` (0) means the slot is unclaimed. Claim/release CAS this.
    instance_id: AtomicU64,
    /// Wall-clock ms of the last heartbeat (GC staleness signal; outside the seqlock).
    last_beat_ms: AtomicU64,
    /// Monotonic block counter (liveness display; outside the seqlock).
    heartbeat: AtomicU64,
    /// Valid label byte count (0..=LABEL_CAP), under the seqlock.
    label_len: AtomicU32,
    _pad: AtomicU32,
    /// Label bytes, under the seqlock.
    label: [AtomicU8; LABEL_CAP],
    /// The 8 modulation streams (f32 bits), under the seqlock.
    mod_signals: [AtomicU32; NUM_MOD_SIGNALS],
    /// 32-band spectrum (f32 bits), under the seqlock.
    spectrum: [AtomicU32; NUM_SPECTRUM],
    /// Peak / RMS (f32 bits), under the seqlock.
    peak: AtomicU32,
    rms: AtomicU32,
}

const HEADER_SIZE: usize = std::mem::size_of::<RawHeader>();
const SLOT_SIZE: usize = std::mem::size_of::<RawSlot>();
/// Total fixed file size.
const FILE_SIZE: usize = HEADER_SIZE + NUM_SLOTS * SLOT_SIZE;

// ---------------------------------------------------------------------------
// Snapshot (owned, seqlock-consistent copy handed to readers)
// ---------------------------------------------------------------------------

/// A coherent, owned copy of one slot's payload, read atomically under the seqlock.
#[derive(Clone, Debug)]
pub struct SlotSnapshot {
    pub index: usize,
    pub instance_id: u64,
    pub kind: PluginKind,
    pub label: String,
    pub mods: [f32; NUM_MOD_SIGNALS],
    pub spectrum: [f32; NUM_SPECTRUM],
    pub peak: f32,
    pub rms: f32,
    pub heartbeat: u64,
    /// Age of the last heartbeat in ms (`now - last_beat_ms`).
    pub age_ms: u64,
}

// ---------------------------------------------------------------------------
// Bus handle
// ---------------------------------------------------------------------------

/// A mapped handle onto the shared bus file. One per process (see [`bus`]) — or several
/// in one test process to simulate several DLLs.
pub struct Bus {
    _map: MmapMut,
    base: *mut u8,
}

// SAFETY: all access to the mapped region is through `Atomic*` (interior-mutable, atomic);
// `base` is a stable address for the life of `_map`. No non-atomic aliasing occurs.
unsafe impl Send for Bus {}
unsafe impl Sync for Bus {}

impl Bus {
    /// Open (or create + initialise) the bus file at `path` and map it. Recreates the
    /// file if it is missing, the wrong size, or carries an incompatible magic/version.
    pub fn open_or_create(path: &Path) -> std::io::Result<Bus> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Decide whether the existing file is a compatible bus; if not, (re)initialise it
        // to the fixed size and write a fresh header. A brand-new file is zero-filled by
        // `set_len`, so every slot starts FREE (instance_id 0) and stable (seq 0 = even).
        let meta = file.metadata()?;
        let needs_init = meta.len() as usize != FILE_SIZE || {
            // Peek the header of an existing right-sized file.
            let peek = unsafe { MmapOptions::new().len(FILE_SIZE).map(&file)? };
            let hdr = unsafe { &*(peek.as_ptr() as *const RawHeader) };
            hdr.magic.load(Ordering::Relaxed) != MAGIC
                || hdr.version.load(Ordering::Relaxed) != VERSION
                || hdr.num_slots.load(Ordering::Relaxed) != NUM_SLOTS as u32
        };
        if needs_init {
            file.set_len(0)?;
            file.set_len(FILE_SIZE as u64)?; // zero-fills
        }

        let mut map = unsafe { MmapOptions::new().len(FILE_SIZE).map_mut(&file)? };
        let base = map.as_mut_ptr();
        let bus = Bus { _map: map, base };
        if needs_init {
            let hdr = bus.header();
            hdr.magic.store(MAGIC, Ordering::Relaxed);
            hdr.version.store(VERSION, Ordering::Relaxed);
            hdr.num_slots.store(NUM_SLOTS as u32, Ordering::Relaxed);
        }
        Ok(bus)
    }

    /// Default location: `%TEMP%\qeynos-bus`.
    pub fn default_path() -> PathBuf {
        std::env::temp_dir().join("qeynos-bus")
    }

    /// Open the process-default bus, or `None` if it cannot be mapped (bus is best-effort;
    /// plugins degrade gracefully to "no cross-plugin modulation").
    pub fn open_default() -> Option<Bus> {
        Bus::open_or_create(&Bus::default_path()).ok()
    }

    #[inline]
    fn header(&self) -> &RawHeader {
        // SAFETY: base points at a valid, mapped, correctly-aligned RawHeader for _map's life.
        unsafe { &*(self.base as *const RawHeader) }
    }

    #[inline]
    fn slot(&self, i: usize) -> &RawSlot {
        debug_assert!(i < NUM_SLOTS);
        // SAFETY: i < NUM_SLOTS so the offset is inside the mapping; RawSlot is 8-aligned and
        // the header is a multiple of 8, so each slot is correctly aligned.
        unsafe { &*(self.base.add(HEADER_SIZE + i * SLOT_SIZE) as *const RawSlot) }
    }

    // ---- writer (seqlock) --------------------------------------------------

    /// Run `f` inside a seqlock write section on slot `s` (bump odd → write → bump even).
    #[inline]
    fn write_locked(s: &RawSlot, f: impl FnOnce()) {
        s.seq.fetch_add(1, Ordering::Release); // -> odd
        f();
        s.seq.fetch_add(1, Ordering::Release); // -> even
    }

    // ---- claim / release ---------------------------------------------------

    /// Claim a free slot for `instance_id` (must be non-zero). GCs stale slots first, then
    /// CAS-grabs the first free one and publishes kind + label. Returns the slot index, or
    /// `None` if the bus is full.
    pub fn claim(&self, instance_id: u64, kind: PluginKind, label: &str) -> Option<usize> {
        debug_assert!(instance_id != FREE, "instance_id 0 is the free sentinel");
        self.gc();
        for i in 0..NUM_SLOTS {
            let s = self.slot(i);
            if s.instance_id
                .compare_exchange(FREE, instance_id, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                Self::write_locked(s, || {
                    s.kind.store(kind as u32, Ordering::Relaxed);
                    Self::store_label(s, label);
                    for a in &s.mod_signals {
                        a.store(0, Ordering::Relaxed);
                    }
                    for a in &s.spectrum {
                        a.store(0, Ordering::Relaxed);
                    }
                    s.peak.store(0, Ordering::Relaxed);
                    s.rms.store(0, Ordering::Relaxed);
                });
                s.heartbeat.store(0, Ordering::Relaxed);
                s.last_beat_ms.store(now_ms(), Ordering::Relaxed);
                return Some(i);
            }
        }
        None
    }

    /// Release a slot previously claimed by `instance_id` (no-op if ownership does not
    /// match, so a stale index from a reused id can't stomp a live slot).
    pub fn release(&self, idx: usize, instance_id: u64) {
        if idx >= NUM_SLOTS {
            return;
        }
        let s = self.slot(idx);
        let _ = s.instance_id.compare_exchange(
            instance_id,
            FREE,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
    }

    #[inline]
    fn store_label(s: &RawSlot, label: &str) {
        let bytes = label.as_bytes();
        let n = bytes.len().min(LABEL_CAP);
        for i in 0..n {
            s.label[i].store(bytes[i], Ordering::Relaxed);
        }
        s.label_len.store(n as u32, Ordering::Relaxed);
    }

    // ---- publish (owner, block-rate) ---------------------------------------

    /// Publish the 8 modulation streams for slot `idx` (seqlock write). No alloc; call from
    /// `process()`.
    pub fn publish_mods(&self, idx: usize, mods: &[f32; NUM_MOD_SIGNALS]) {
        if idx >= NUM_SLOTS {
            return;
        }
        let s = self.slot(idx);
        Self::write_locked(s, || {
            for (a, &v) in s.mod_signals.iter().zip(mods.iter()) {
                store_f32(a, v);
            }
        });
    }

    /// Publish the 32-band spectrum + peak/RMS for slot `idx` (seqlock write; X-RAY consumer).
    pub fn publish_spectrum(
        &self,
        idx: usize,
        spectrum: &[f32; NUM_SPECTRUM],
        peak: f32,
        rms: f32,
    ) {
        if idx >= NUM_SLOTS {
            return;
        }
        let s = self.slot(idx);
        Self::write_locked(s, || {
            for (a, &v) in s.spectrum.iter().zip(spectrum.iter()) {
                store_f32(a, v);
            }
            store_f32(&s.peak, peak);
            store_f32(&s.rms, rms);
        });
    }

    /// Update the user label for slot `idx` (owner GUI/init; seqlock write).
    pub fn set_label(&self, idx: usize, instance_id: u64, label: &str) {
        if idx >= NUM_SLOTS {
            return;
        }
        let s = self.slot(idx);
        if s.instance_id.load(Ordering::Relaxed) != instance_id {
            return;
        }
        Self::write_locked(s, || Self::store_label(s, label));
    }

    /// Stamp the heartbeat (block counter + wall-clock ms). Call once per `process()` block.
    pub fn beat(&self, idx: usize) {
        if idx >= NUM_SLOTS {
            return;
        }
        let s = self.slot(idx);
        s.heartbeat.fetch_add(1, Ordering::Relaxed);
        s.last_beat_ms.store(now_ms(), Ordering::Relaxed);
    }

    // ---- reader (seqlock) --------------------------------------------------

    /// Read a coherent snapshot of slot `idx`. Returns `None` if the slot is free or GC-stale.
    /// Wait-free: retries the seqlock until it reads a stable even generation.
    pub fn read_slot(&self, idx: usize) -> Option<SlotSnapshot> {
        if idx >= NUM_SLOTS {
            return None;
        }
        let s = self.slot(idx);
        let now = now_ms();
        loop {
            let s1 = s.seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                std::hint::spin_loop();
                continue; // write in progress
            }
            let instance_id = s.instance_id.load(Ordering::Relaxed);
            let kind = PluginKind::from_u32(s.kind.load(Ordering::Relaxed));
            let len = (s.label_len.load(Ordering::Relaxed) as usize).min(LABEL_CAP);
            let mut lbytes = [0u8; LABEL_CAP];
            for i in 0..len {
                lbytes[i] = s.label[i].load(Ordering::Relaxed);
            }
            let mut mods = [0.0f32; NUM_MOD_SIGNALS];
            for (m, a) in mods.iter_mut().zip(s.mod_signals.iter()) {
                *m = load_f32(a);
            }
            let mut spectrum = [0.0f32; NUM_SPECTRUM];
            for (m, a) in spectrum.iter_mut().zip(s.spectrum.iter()) {
                *m = load_f32(a);
            }
            let peak = load_f32(&s.peak);
            let rms = load_f32(&s.rms);
            let heartbeat = s.heartbeat.load(Ordering::Relaxed);
            let last_beat = s.last_beat_ms.load(Ordering::Relaxed);

            let s2 = s.seq.load(Ordering::Acquire);
            if s1 == s2 {
                if instance_id == FREE {
                    return None;
                }
                let age_ms = now.saturating_sub(last_beat);
                if age_ms > STALE_MS {
                    return None; // present but dead
                }
                let label = String::from_utf8_lossy(&lbytes[..len]).into_owned();
                return Some(SlotSnapshot {
                    index: idx,
                    instance_id,
                    kind,
                    label,
                    mods,
                    spectrum,
                    peak,
                    rms,
                    heartbeat,
                    age_ms,
                });
            }
            // Torn / changed: retry.
            std::hint::spin_loop();
        }
    }

    /// Snapshot every live (claimed, non-stale) slot.
    pub fn snapshot_live(&self) -> Vec<SlotSnapshot> {
        (0..NUM_SLOTS).filter_map(|i| self.read_slot(i)).collect()
    }

    /// Find a live slot by owner instance id.
    pub fn find_by_instance(&self, instance_id: u64) -> Option<SlotSnapshot> {
        (0..NUM_SLOTS).find_map(|i| {
            self.read_slot(i)
                .filter(|snap| snap.instance_id == instance_id)
        })
    }

    /// Read just one modulation signal of a live slot (fast path for the listen layer).
    /// `None` if the slot is free/stale or the index is out of range.
    pub fn read_mod(&self, idx: usize, signal: usize) -> Option<f32> {
        if signal >= NUM_MOD_SIGNALS {
            return None;
        }
        self.read_slot(idx).map(|snap| snap.mods[signal])
    }

    /// Alloc-free single-signal read under the seqlock — for the RT listen path (no
    /// `String`/`Vec` built). Validates liveness (free/stale → `None`).
    pub fn read_mod_fast(&self, idx: usize, signal: usize) -> Option<f32> {
        if idx >= NUM_SLOTS || signal >= NUM_MOD_SIGNALS {
            return None;
        }
        let s = self.slot(idx);
        let now = now_ms();
        loop {
            let s1 = s.seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let id = s.instance_id.load(Ordering::Relaxed);
            let last = s.last_beat_ms.load(Ordering::Relaxed);
            let v = load_f32(&s.mod_signals[signal]);
            let s2 = s.seq.load(Ordering::Acquire);
            if s1 == s2 {
                if id == FREE || now.saturating_sub(last) > STALE_MS {
                    return None;
                }
                return Some(v);
            }
            std::hint::spin_loop();
        }
    }

    /// Alloc-free: index of the live slot owned by `instance_id`, if any. For the listen
    /// layer to cache a source's slot index (re-resolved when the cache misses).
    pub fn resolve_instance(&self, instance_id: u64) -> Option<usize> {
        if instance_id == FREE {
            return None;
        }
        let now = now_ms();
        for i in 0..NUM_SLOTS {
            let s = self.slot(i);
            if s.instance_id.load(Ordering::Relaxed) == instance_id
                && now.saturating_sub(s.last_beat_ms.load(Ordering::Relaxed)) <= STALE_MS
            {
                return Some(i);
            }
        }
        None
    }

    // ---- GC ----------------------------------------------------------------

    /// Reclaim slots whose heartbeat has gone stale (owner crashed / bridged-away process
    /// removed). Idempotent; safe to call from any handle.
    pub fn gc(&self) {
        let now = now_ms();
        for i in 0..NUM_SLOTS {
            let s = self.slot(i);
            let id = s.instance_id.load(Ordering::Relaxed);
            if id == FREE {
                continue;
            }
            let last = s.last_beat_ms.load(Ordering::Relaxed);
            if now.saturating_sub(last) > STALE_MS {
                // Only reclaim if it is still the same dead owner (avoid racing a fresh claim).
                let _ = s.instance_id.compare_exchange(
                    id,
                    FREE,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
            }
        }
    }

    /// Number of currently-claimed (non-stale) slots — test/introspection helper.
    pub fn live_count(&self) -> usize {
        self.snapshot_live().len()
    }
}

/// Process-wide default bus handle (lazily mapped). `None` if the file can't be mapped.
pub fn bus() -> Option<&'static Bus> {
    static BUS: OnceLock<Option<Bus>> = OnceLock::new();
    BUS.get_or_init(Bus::open_default).as_ref()
}

/// A small, non-zero, process-unique-ish instance id source for slot ownership. Combines
/// the process id with a monotonic counter so ids don't collide within or across processes
/// for the life of a session.
pub fn new_instance_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let pid = std::process::id() as u64;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // High 32 bits = pid, low 32 = counter; never zero.
    ((pid << 32) | (n & 0xFFFF_FFFF)) | 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn temp_bus_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qeynos-bus-test-{}-{}-{}",
            tag,
            std::process::id(),
            new_instance_id()
        ))
    }

    /// Two handles onto ONE file in one process simulate two DLLs: writer publishes,
    /// reader sees the values.
    #[test]
    fn two_handles_publish_and_read() {
        let path = temp_bus_path("pubread");
        let writer = Bus::open_or_create(&path).unwrap();
        let reader = Bus::open_or_create(&path).unwrap();

        let id = new_instance_id();
        let idx = writer.claim(id, PluginKind::Nerve, "LFO Desk").unwrap();

        let mods = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        writer.publish_mods(idx, &mods);
        writer.beat(idx);

        let snap = reader.find_by_instance(id).expect("reader sees the slot");
        assert_eq!(snap.kind, PluginKind::Nerve);
        assert_eq!(snap.label, "LFO Desk");
        for (a, b) in snap.mods.iter().zip(mods.iter()) {
            assert!((a - b).abs() < 1e-6, "mod mismatch {a} vs {b}");
        }
        // A second reader handle indexing the same signal directly.
        assert!((reader.read_mod(idx, 4).unwrap() - 0.5).abs() < 1e-6);

        writer.release(idx, id);
        let _ = std::fs::remove_file(&path);
    }

    /// Seqlock torn-read guarantee: a writer thrashes all 8 signals to a common,
    /// ever-incrementing value; a reader must NEVER observe a slot where the 8 signals
    /// disagree (which would be a mixed-generation / torn read).
    #[test]
    fn seqlock_never_tears() {
        let path = temp_bus_path("torn");
        let writer = Arc::new(Bus::open_or_create(&path).unwrap());
        let reader = Bus::open_or_create(&path).unwrap();
        let id = new_instance_id();
        let idx = writer.claim(id, PluginKind::Nerve, "torn").unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let w = writer.clone();
        let stop_w = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut gen = 0.0f32;
            while !stop_w.load(Ordering::Relaxed) {
                gen += 1.0;
                // All 8 equal to `gen` — any reader that sees them unequal has torn.
                w.publish_mods(idx, &[gen; NUM_MOD_SIGNALS]);
            }
        });

        let mut reads = 0u64;
        for _ in 0..2_000_000 {
            if let Some(snap) = reader.read_slot(idx) {
                let first = snap.mods[0];
                for (k, m) in snap.mods.iter().enumerate() {
                    assert_eq!(
                        *m, first,
                        "TORN READ: signal {k} = {m} but signal 0 = {first}"
                    );
                }
                reads += 1;
            }
        }
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
        assert!(reads > 0, "reader never got a coherent snapshot");
        writer.release(idx, id);
        let _ = std::fs::remove_file(&path);
    }

    /// A slot whose heartbeat has gone stale is reclaimed and re-claimable.
    #[test]
    fn stale_slot_reclaimed() {
        let path = temp_bus_path("stale");
        let bus = Bus::open_or_create(&path).unwrap();
        let id = new_instance_id();
        let idx = bus.claim(id, PluginKind::Generic, "ghost").unwrap();

        // Force the heartbeat into the deep past → GC must reclaim it.
        let s = bus.slot(idx);
        s.last_beat_ms
            .store(now_ms().saturating_sub(STALE_MS + 1000), Ordering::Relaxed);

        // Reader sees it as dead...
        assert!(bus.read_slot(idx).is_none(), "stale slot must read as dead");
        // ...and GC frees it so a new claim can take (possibly) the same index.
        bus.gc();
        assert_eq!(s.instance_id.load(Ordering::Relaxed), FREE, "GC must free it");

        let id2 = new_instance_id();
        let idx2 = bus.claim(id2, PluginKind::Nerve, "fresh").unwrap();
        assert!(bus.find_by_instance(id2).is_some());
        bus.release(idx2, id2);
        let _ = std::fs::remove_file(&path);
    }

    /// A header with the wrong version is recreated (not trusted) on open.
    #[test]
    fn header_version_mismatch_recreates() {
        let path = temp_bus_path("version");
        {
            let bus = Bus::open_or_create(&path).unwrap();
            let id = new_instance_id();
            let idx = bus.claim(id, PluginKind::Nerve, "old").unwrap();
            bus.publish_mods(idx, &[9.0; NUM_MOD_SIGNALS]);
            // Corrupt the version in place.
            bus.header().version.store(VERSION + 1, Ordering::Relaxed);
        }
        // Re-open: mismatch → file reinitialised → no live slots, valid header.
        let bus = Bus::open_or_create(&path).unwrap();
        assert_eq!(bus.header().magic.load(Ordering::Relaxed), MAGIC);
        assert_eq!(bus.header().version.load(Ordering::Relaxed), VERSION);
        assert_eq!(bus.live_count(), 0, "recreated bus must start empty");
        let _ = std::fs::remove_file(&path);
    }

    /// The bus fills at NUM_SLOTS and reports full.
    #[test]
    fn bus_fills_and_reports_full() {
        let path = temp_bus_path("full");
        let bus = Bus::open_or_create(&path).unwrap();
        let mut ids = Vec::new();
        for _ in 0..NUM_SLOTS {
            let id = new_instance_id();
            assert!(bus.claim(id, PluginKind::Generic, "x").is_some());
            ids.push(id);
        }
        // One past capacity → None.
        assert!(bus.claim(new_instance_id(), PluginKind::Generic, "over").is_none());
        assert_eq!(bus.live_count(), NUM_SLOTS);
        let _ = std::fs::remove_file(&path);
    }
}
