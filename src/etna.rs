//! ETNA benchmark harness for the `lru` crate.
//!
//! This module defines the framework-neutral `PropertyResult` enum plus one
//! `property_*` function per mined bug. Every framework adapter in
//! `src/bin/etna.rs` and every witness test calls into these functions.
//!
//! Several of the bugs are about missing destructor calls (memory leaks). To
//! detect them deterministically the properties build caches whose keys and
//! values are `Tracked` instances tied to a per-invocation `Arc<AtomicUsize>`.
//! Each `Drop` increments the counter, and the property compares the final
//! count against the expected number of drops. Using a per-invocation counter
//! (rather than a `static`) keeps the property safe to run concurrently from
//! proptest's parallel test runner.

#![allow(missing_docs)]

extern crate std;

use crate::LruCache;
use core::borrow::Borrow;
use core::hash::{Hash, Hasher};
use core::num::NonZeroUsize;
use std::format;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::String;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyResult {
    Pass,
    Fail(String),
    Discard,
}

// ---------------------------------------------------------------------------
// Drop-tracking helpers shared by the leak-detection properties.
// ---------------------------------------------------------------------------

/// A drop-counting key. Hash and Eq compare only the inner `id`, so it works
/// with `LruCache::pop(&u32)` via the `Borrow<u32>` impl below. Each `Drop`
/// increments the shared counter, letting a property assert the number of
/// destructor calls.
pub struct TrackedKey {
    id: u32,
    counter: Arc<AtomicUsize>,
}

impl TrackedKey {
    pub fn new(id: u32, counter: Arc<AtomicUsize>) -> Self {
        Self { id, counter }
    }
}

impl Hash for TrackedKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for TrackedKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TrackedKey {}

impl Borrow<u32> for TrackedKey {
    fn borrow(&self) -> &u32 {
        &self.id
    }
}

impl Drop for TrackedKey {
    fn drop(&mut self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

/// A drop-counting value. Used as the `V` parameter of an `LruCache` to
/// observe how many values are destructed.
pub struct TrackedVal {
    counter: Arc<AtomicUsize>,
}

impl TrackedVal {
    pub fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl Drop for TrackedVal {
    fn drop(&mut self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

// Deduplicate while preserving order so generated keys map cleanly onto
// distinct cache entries.
fn unique_keys(keys: &[u32]) -> Vec<u32> {
    let mut seen: Vec<u32> = Vec::with_capacity(keys.len());
    for &k in keys {
        if !seen.iter().any(|&s| s == k) {
            seen.push(k);
        }
    }
    seen
}

// ---------------------------------------------------------------------------
// Bug 1: clone implementation panics for unbounded caches.
// (3ec42b6 — Fix clone implementation for unbounded cache.)
// ---------------------------------------------------------------------------

/// `LruCache::unbounded().clone()` must succeed regardless of how many entries
/// the cache currently holds. The buggy implementation forwards the cache's
/// capacity (`usize::MAX` for unbounded) into `HashMap::with_capacity`, which
/// panics with "Hash table capacity overflow".
pub fn property_clone_unbounded_no_panic(items: Vec<(u32, u32)>) -> PropertyResult {
    let mut cache: LruCache<u32, u32> = LruCache::unbounded();
    for (k, v) in &items {
        cache.put(*k, *v);
    }
    let cloned = catch_unwind(AssertUnwindSafe(|| cache.clone()));
    match cloned {
        Err(_) => PropertyResult::Fail(format!(
            "LruCache::unbounded().clone() panicked with {} entries",
            cache.len()
        )),
        Ok(c) => {
            if c.len() != cache.len() {
                return PropertyResult::Fail(format!(
                    "clone len mismatch: original={}, clone={}",
                    cache.len(),
                    c.len()
                ));
            }
            // Spot-check that clone preserves the entries.
            for (k, v) in &items {
                if cache.peek(k) != c.peek(k) {
                    return PropertyResult::Fail(format!(
                        "clone diverges on key {}: orig={:?}, clone={:?}",
                        k,
                        cache.peek(k),
                        c.peek(k)
                    ));
                }
                let _ = v;
            }
            PropertyResult::Pass
        }
    }
}

// ---------------------------------------------------------------------------
// Bug 2: pop did not detach the linked-list node.
// (5f4a46a — Fix bug in LruCache::pop().)
// ---------------------------------------------------------------------------

/// After `LruCache::pop(k)` removes a key, the iter view of the cache must
/// not include the popped key, and successive pops must continue returning
/// the right keys. The buggy `pop` removed the entry from the hashmap but
/// left the corresponding linked-list node attached, so a subsequent iter
/// would still walk through the orphaned (now-freed) node — observable as
/// `iter()` yielding the popped key, or as a `pop_lru` returning a key that
/// is no longer in the map (`unwrap()` on a missing entry panics).
pub fn property_pop_iter_consistent(args: (Vec<u32>, u8)) -> PropertyResult {
    let (raw_keys, pop_pick) = args;
    let keys = unique_keys(&raw_keys);
    if keys.len() < 2 {
        return PropertyResult::Discard;
    }
    let cap = NonZeroUsize::new(keys.len()).unwrap();
    let mut cache: LruCache<u32, u32> = LruCache::new(cap);
    // Insert keys in order; after this the MRU-first iter order is
    // keys reversed.
    for k in &keys {
        cache.put(*k, *k);
    }
    let pop_idx = (pop_pick as usize) % keys.len();
    let pop_key = keys[pop_idx];

    // The buggy pop leaves the linked-list node attached. Subsequent
    // operations that walk the list (iter / pop_lru / capturing_put on a
    // full cache) dereference the orphan or its dangling neighbours.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let removed = cache.pop(&pop_key);
        // Snapshot iter immediately, before the freed slot is reused by any
        // allocator. With the bug the orphan still holds the popped key.
        let iter_keys: Vec<u32> = cache.iter().map(|(&k, _)| k).collect();
        // pop_lru repeatedly: with the bug the linked-list head/tail still
        // points at the orphaned node and pop_lru tries to remove a key from
        // the map that is no longer there, which `.unwrap()`s to a panic.
        let mut drained = Vec::new();
        while let Some((k, _)) = cache.pop_lru() {
            drained.push(k);
        }
        (removed, iter_keys, drained)
    }));

    let (removed, iter_keys, drained) = match outcome {
        Ok(o) => o,
        Err(_) => {
            return PropertyResult::Fail(format!(
                "pop({}) on cache of {} keys panicked while replaying through the linked list",
                pop_key,
                keys.len()
            ));
        }
    };

    if removed != Some(pop_key) {
        return PropertyResult::Fail(format!(
            "pop({}) returned {:?}, expected Some({})",
            pop_key, removed, pop_key
        ));
    }
    if iter_keys.iter().any(|&k| k == pop_key) {
        return PropertyResult::Fail(format!(
            "popped key {} still appears in iter() after pop: iter={:?}",
            pop_key, iter_keys
        ));
    }
    if drained.iter().any(|&k| k == pop_key) {
        return PropertyResult::Fail(format!(
            "popped key {} reappeared during pop_lru drain: drained={:?}",
            pop_key, drained
        ));
    }
    let expected_remaining = keys.len() - 1;
    if drained.len() != expected_remaining {
        return PropertyResult::Fail(format!(
            "drain after pop({}) yielded {} keys, expected {} (drained={:?})",
            pop_key,
            drained.len(),
            expected_remaining,
            drained
        ));
    }
    PropertyResult::Pass
}

// ---------------------------------------------------------------------------
// Bug 3: Drop impl uses the wrong pointer cast and skips inner drops.
// (37dbda0 — Use as_mut_ptr method to fix memory leak.)
// ---------------------------------------------------------------------------

/// Dropping an `LruCache` containing N entries must run `Drop` once for every
/// key and once for every value (so `2 * N` total destructor calls when both
/// `K` and `V` track drops). The buggy `Drop` impl casts a `&mut MaybeUninit<K>`
/// directly to `*mut _`, so `ptr::drop_in_place` runs against the
/// `MaybeUninit` wrapper (a no-op) instead of the inner `K` / `V`. Result: the
/// keys and values are leaked on cache drop.
pub fn property_drop_impl_drops_all(keys: Vec<u32>) -> PropertyResult {
    let unique = unique_keys(&keys);
    if unique.is_empty() {
        return PropertyResult::Discard;
    }
    let n = unique.len();
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let cap = NonZeroUsize::new(n).unwrap();
        let mut cache: LruCache<TrackedKey, TrackedVal> = LruCache::new(cap);
        for &id in &unique {
            cache.put(
                TrackedKey::new(id, counter.clone()),
                TrackedVal::new(counter.clone()),
            );
        }
        // Cache drops here.
    }
    let observed = counter.load(Ordering::SeqCst);
    let expected = 2 * n;
    if observed != expected {
        return PropertyResult::Fail(format!(
            "drop count after dropping cache of {} entries: got {}, expected {}",
            n, observed, expected
        ));
    }
    PropertyResult::Pass
}

// ---------------------------------------------------------------------------
// Bug 4: clear() / resize() leak the values they evict.
// (4460158 — Fix memory leak when using clear and resize.)
// ---------------------------------------------------------------------------

/// `LruCache::clear()` and `LruCache::resize(small)` must drop every entry
/// they remove. The buggy implementations called the private `remove_last()`
/// helper, which detaches and frees the `Box<LruEntry>` shell but leaves the
/// inner `MaybeUninit<K>` / `MaybeUninit<V>` undropped. This property fills
/// the cache, then either clears it or resizes it to a single slot, and
/// asserts the expected number of `Drop` invocations.
pub fn property_clear_resize_drops_all(args: (Vec<u32>, bool)) -> PropertyResult {
    let (keys, use_clear) = args;
    let unique = unique_keys(&keys);
    if unique.is_empty() {
        return PropertyResult::Discard;
    }
    let n = unique.len();
    let counter = Arc::new(AtomicUsize::new(0));
    let cap = NonZeroUsize::new(n).unwrap();
    let mut cache: LruCache<TrackedKey, TrackedVal> = LruCache::new(cap);
    for &id in &unique {
        cache.put(
            TrackedKey::new(id, counter.clone()),
            TrackedVal::new(counter.clone()),
        );
    }
    counter.store(0, Ordering::SeqCst);

    let expected_evictions = if use_clear {
        cache.clear();
        n
    } else {
        // Resize to 1 slot: should evict (n - 1) entries; allow 0 if n == 1.
        cache.resize(NonZeroUsize::new(1).unwrap());
        n.saturating_sub(1)
    };

    let observed = counter.load(Ordering::SeqCst);
    let expected = 2 * expected_evictions;
    if observed != expected {
        return PropertyResult::Fail(format!(
            "{} on {} entries: got {} drops, expected {}",
            if use_clear { "clear()" } else { "resize(1)" },
            n,
            observed,
            expected
        ));
    }
    drop(cache);
    PropertyResult::Pass
}

// ---------------------------------------------------------------------------
// Bug 5: pop leaks the popped key.
// (ea64c8f — Fix memory leak when using pop.)
// ---------------------------------------------------------------------------

/// `LruCache::pop(&q)` must drop the popped entry's `K` (the value is returned
/// to the caller, who drops it). The buggy `pop` only returned the `V` and
/// left the `K` in the now-freed `LruEntry`'s `MaybeUninit` slot — leaking
/// every key that flows through `pop`.
///
/// The property fills a cache with `TrackedKey` keys and `()` values, pops
/// each key, and asserts that the key drop count equals the number of pops.
pub fn property_pop_drops_key(keys: Vec<u32>) -> PropertyResult {
    let unique = unique_keys(&keys);
    if unique.is_empty() {
        return PropertyResult::Discard;
    }
    let n = unique.len();
    let counter = Arc::new(AtomicUsize::new(0));
    let cap = NonZeroUsize::new(n).unwrap();
    let mut cache: LruCache<TrackedKey, ()> = LruCache::new(cap);
    for &id in &unique {
        cache.put(TrackedKey::new(id, counter.clone()), ());
    }
    counter.store(0, Ordering::SeqCst);

    for &id in &unique {
        let _ = cache.pop(&id);
    }

    let observed = counter.load(Ordering::SeqCst);
    if observed != n {
        return PropertyResult::Fail(format!(
            "pop on {} entries dropped {} keys, expected {}",
            n, observed, n
        ));
    }
    drop(cache);
    PropertyResult::Pass
}
