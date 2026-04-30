# lru — Injected Bugs

An LRU cache implementation — ETNA workload.

Total mutations: 5

## Bug Index

| # | Variant | Name | Location | Injection | Fix Commit |
|---|---------|------|----------|-----------|------------|
| 1 | `clear_resize_drops_all_4460158_1` | `clear_resize_drops_all` | `src/lib.rs` | `patch` | `4460158fab9808001d8fad509615a4a8c2769e11` |
| 2 | `clone_unbounded_no_panic_3ec42b6_1` | `clone_unbounded_no_panic` | `src/lib.rs` | `marauders` | `3ec42b6369082c00da2836fe033db20cb52a36fc` |
| 3 | `drop_impl_drops_all_37dbda0_1` | `drop_impl_drops_all` | `src/lib.rs` | `marauders` | `37dbda01eb6820b86292c1f9d497d01d35287ad8` |
| 4 | `pop_drops_key_ea64c8f_1` | `pop_drops_key` | `src/lib.rs` | `marauders` | `ea64c8f932a45434cbc71d3843d28af7c1819864` |
| 5 | `pop_iter_consistent_5f4a46a_1` | `pop_iter_consistent` | `src/lib.rs` | `marauders` | `5f4a46a522b4e325cc4d56c1b737e44915921cca` |

## Property Mapping

| Variant | Property | Witness(es) |
|---------|----------|-------------|
| `clear_resize_drops_all_4460158_1` | `ClearResizeDropsAll` | `witness_clear_resize_drops_all_case_clear`, `witness_clear_resize_drops_all_case_resize`, `witness_clear_resize_drops_all_case_clear_one` |
| `clone_unbounded_no_panic_3ec42b6_1` | `CloneUnboundedNoPanic` | `witness_clone_unbounded_no_panic_case_three_entries`, `witness_clone_unbounded_no_panic_case_one_entry`, `witness_clone_unbounded_no_panic_case_empty` |
| `drop_impl_drops_all_37dbda0_1` | `DropImplDropsAll` | `witness_drop_impl_drops_all_case_many`, `witness_drop_impl_drops_all_case_one` |
| `pop_drops_key_ea64c8f_1` | `PopDropsKey` | `witness_pop_drops_key_case_many`, `witness_pop_drops_key_case_one` |
| `pop_iter_consistent_5f4a46a_1` | `PopIterConsistent` | `witness_pop_iter_consistent_case_pop_middle`, `witness_pop_iter_consistent_case_pop_first`, `witness_pop_iter_consistent_case_pop_last` |

## Framework Coverage

| Property | proptest | quickcheck | crabcheck | hegel |
|----------|---------:|-----------:|----------:|------:|
| `ClearResizeDropsAll` | ✓ | ✓ | ✓ | ✓ |
| `CloneUnboundedNoPanic` | ✓ | ✓ | ✓ | ✓ |
| `DropImplDropsAll` | ✓ | ✓ | ✓ | ✓ |
| `PopDropsKey` | ✓ | ✓ | ✓ | ✓ |
| `PopIterConsistent` | ✓ | ✓ | ✓ | ✓ |

## Bug Details

### 1. clear_resize_drops_all

- **Variant**: `clear_resize_drops_all_4460158_1`
- **Location**: `src/lib.rs` (inside `LruCache::clear`)
- **Property**: `ClearResizeDropsAll`
- **Witness(es)**:
  - `witness_clear_resize_drops_all_case_clear`
  - `witness_clear_resize_drops_all_case_resize`
  - `witness_clear_resize_drops_all_case_clear_one`
- **Source**: [#101](https://github.com/jeromefroe/lru-rs/pull/101) — Fix memory leak when using clear and resize
  > Both `clear()` and `resize()` invoked the private `remove_last()` helper, which detaches a node from the linked list and frees its `Box<LruEntry>` shell but leaves the inner `MaybeUninit<K>` / `MaybeUninit<V>` undropped. Every entry the cache evicted via clear or resize was leaked. The fix routes both methods through `pop_lru()`, which `assume_init()`s the K/V out of the slot so they get destructed when the returned tuple is dropped.
- **Fix commit**: `4460158fab9808001d8fad509615a4a8c2769e11` — Fix memory leak when using clear and resize
- **Invariant violated**: Both `LruCache::clear()` and `LruCache::resize(small)` must drop every entry they remove from the cache; for N tracked entries cleared, the K/V destructor count must equal 2*N.
- **How the mutation triggers**: The buggy `clear` and `resize` discard the result of `self.remove_last()`. `remove_last` only frees the `Box<LruEntry>` heap allocation; the inner `MaybeUninit<K>` and `MaybeUninit<V>` are never dropped because `MaybeUninit` does not run its inner `T`'s destructor on drop. Every cleared / resized-out entry leaks both its key and value.

### 2. clone_unbounded_no_panic

- **Variant**: `clone_unbounded_no_panic_3ec42b6_1`
- **Location**: `src/lib.rs` (inside `LruCache::clone`)
- **Property**: `CloneUnboundedNoPanic`
- **Witness(es)**:
  - `witness_clone_unbounded_no_panic_case_three_entries`
  - `witness_clone_unbounded_no_panic_case_one_entry`
  - `witness_clone_unbounded_no_panic_case_empty`
- **Source**: [#219](https://github.com/jeromefroe/lru-rs/pull/219) — Fix clone implementation for unbounded cache
  > `<LruCache as Clone>::clone` forwarded `self.cap()` (which is `usize::MAX` for an unbounded cache) into `HashMap::with_capacity_and_hasher`, panicking with 'Hash table capacity overflow' the first time anyone cloned an unbounded cache. The fix special-cases `is_unbounded()` and uses `self.len()` for the new map's initial capacity instead.
- **Fix commit**: `3ec42b6369082c00da2836fe033db20cb52a36fc` — Fix clone implementation for unbounded cache
- **Invariant violated**: `LruCache::unbounded().clone()` must not panic regardless of how many entries the cache currently holds, and the clone must contain the same key/value pairs as the original.
- **How the mutation triggers**: The buggy clone allocates the new internal `HashMap` with capacity `self.cap().get()`. When the cache is unbounded `self.cap()` is `NonZeroUsize::MAX`, so `HashMap::with_capacity_and_hasher` is asked to reserve `usize::MAX` slots and hashbrown panics with 'Hash table capacity overflow' before a single entry is copied.

### 3. drop_impl_drops_all

- **Variant**: `drop_impl_drops_all_37dbda0_1`
- **Location**: `src/lib.rs` (inside `<LruCache as Drop>::drop`)
- **Property**: `DropImplDropsAll`
- **Witness(es)**:
  - `witness_drop_impl_drops_all_case_many`
  - `witness_drop_impl_drops_all_case_one`
- **Source**: [#79](https://github.com/jeromefroe/lru-rs/pull/79) — Use as_mut_ptr method to fix memory leak
  > The cache's `Drop` impl ran `ptr::drop_in_place(&mut e.key as *mut _)`. Type inference picked `*mut MaybeUninit<K>`, so the drop targeted the MaybeUninit wrapper (a no-op) rather than the inner `K`/`V`, leaking every entry's key and value when the cache itself was dropped. The fix uses `e.key.as_mut_ptr()` to obtain a `*mut K` so the inner destructor actually runs.
- **Fix commit**: `37dbda01eb6820b86292c1f9d497d01d35287ad8` — Use as_mut_ptr method to fix memory leak
- **Invariant violated**: Dropping an `LruCache` containing N entries must run `Drop` once for every key and once for every value (so 2*N destructor calls when both K and V track drops).
- **How the mutation triggers**: The buggy `Drop` impl casts `&mut node.key` (whose type is `&mut MaybeUninit<K>`) to `*mut _`, picking up `*mut MaybeUninit<K>` from inference. `ptr::drop_in_place` on a `*mut MaybeUninit<K>` is a no-op (MaybeUninit has no Drop), so the inner `K` is leaked; same for `V`. Dropping the cache therefore observes zero K/V destructor calls instead of the expected 2*N.

### 4. pop_drops_key

- **Variant**: `pop_drops_key_ea64c8f_1`
- **Location**: `src/lib.rs` (inside `LruCache::pop`)
- **Property**: `PopDropsKey`
- **Witness(es)**:
  - `witness_pop_drops_key_case_many`
  - `witness_pop_drops_key_case_one`
- **Source**: [#104](https://github.com/jeromefroe/lru-rs/pull/104) — Fix memory leak when using pop
  > `LruCache::pop` removed an entry from the hashmap and returned its value but never dropped the entry's key. The key sat in the freed slot's `MaybeUninit<K>` and was leaked, so any K with a non-trivial destructor (Box, Rc, custom Drop) leaked once per `pop` call. The fix adds an explicit `ptr::drop_in_place(old_node.key.as_mut_ptr())` before the slot is reused.
- **Fix commit**: `ea64c8f932a45434cbc71d3843d28af7c1819864` — Fix memory leak when using pop
- **Invariant violated**: `LruCache::pop(&q)` must drop the popped entry's key. After popping N tracked keys, the key destructor count must equal N.
- **How the mutation triggers**: The buggy `pop` extracts the entry's `V` via `assume_init` and returns it but never drops the entry's `K`. Because `K` lives inside a `MaybeUninit<K>` slot, dropping the surrounding `LruEntry` is a no-op for the inner key — every `pop` call therefore leaks one K.

### 5. pop_iter_consistent

- **Variant**: `pop_iter_consistent_5f4a46a_1`
- **Location**: `src/lib.rs` (inside `LruCache::pop`)
- **Property**: `PopIterConsistent`
- **Witness(es)**:
  - `witness_pop_iter_consistent_case_pop_middle`
  - `witness_pop_iter_consistent_case_pop_first`
  - `witness_pop_iter_consistent_case_pop_last`
- **Source**: [#29](https://github.com/jeromefroe/lru-rs/pull/29) — Fix bug in LruCache::pop().
  > `LruCache::pop` removed the entry from the underlying hashmap but never detached the corresponding node from the linked list. Subsequent operations (iter, pop_lru, evict-on-put) walked through the orphaned node, leading to memory unsafety and observable wrong-key-in-iter behaviour.
- **Fix commit**: `5f4a46a522b4e325cc4d56c1b737e44915921cca` — Fix bug in LruCache::pop().
- **Invariant violated**: After `pop(k)` removes a key, the cache's linked-list view (iter, pop_lru, evict-on-put) must agree with the map view. In particular, the popped key must not appear in `iter()` and must not be returned by any subsequent `pop_lru`.
- **How the mutation triggers**: The buggy `pop` removes the entry from the hashmap and frees its `Box<LruEntry>` but skips `self.detach(node)`, so the linked list still threads through the freed slot. iter() then visits the orphan (still holding the popped key), and pop_lru's `self.map.remove(&old_key).unwrap()` panics when it tries to remove a key the map no longer contains.
