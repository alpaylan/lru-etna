# lru — ETNA Tasks

Total tasks: 20

## Task Index

| Task | Variant | Framework | Property | Witness |
|------|---------|-----------|----------|---------|
| 001 | `clear_resize_drops_all_4460158_1` | proptest | `ClearResizeDropsAll` | `witness_clear_resize_drops_all_case_clear` |
| 002 | `clear_resize_drops_all_4460158_1` | quickcheck | `ClearResizeDropsAll` | `witness_clear_resize_drops_all_case_clear` |
| 003 | `clear_resize_drops_all_4460158_1` | crabcheck | `ClearResizeDropsAll` | `witness_clear_resize_drops_all_case_clear` |
| 004 | `clear_resize_drops_all_4460158_1` | hegel | `ClearResizeDropsAll` | `witness_clear_resize_drops_all_case_clear` |
| 005 | `clone_unbounded_no_panic_3ec42b6_1` | proptest | `CloneUnboundedNoPanic` | `witness_clone_unbounded_no_panic_case_three_entries` |
| 006 | `clone_unbounded_no_panic_3ec42b6_1` | quickcheck | `CloneUnboundedNoPanic` | `witness_clone_unbounded_no_panic_case_three_entries` |
| 007 | `clone_unbounded_no_panic_3ec42b6_1` | crabcheck | `CloneUnboundedNoPanic` | `witness_clone_unbounded_no_panic_case_three_entries` |
| 008 | `clone_unbounded_no_panic_3ec42b6_1` | hegel | `CloneUnboundedNoPanic` | `witness_clone_unbounded_no_panic_case_three_entries` |
| 009 | `drop_impl_drops_all_37dbda0_1` | proptest | `DropImplDropsAll` | `witness_drop_impl_drops_all_case_many` |
| 010 | `drop_impl_drops_all_37dbda0_1` | quickcheck | `DropImplDropsAll` | `witness_drop_impl_drops_all_case_many` |
| 011 | `drop_impl_drops_all_37dbda0_1` | crabcheck | `DropImplDropsAll` | `witness_drop_impl_drops_all_case_many` |
| 012 | `drop_impl_drops_all_37dbda0_1` | hegel | `DropImplDropsAll` | `witness_drop_impl_drops_all_case_many` |
| 013 | `pop_drops_key_ea64c8f_1` | proptest | `PopDropsKey` | `witness_pop_drops_key_case_many` |
| 014 | `pop_drops_key_ea64c8f_1` | quickcheck | `PopDropsKey` | `witness_pop_drops_key_case_many` |
| 015 | `pop_drops_key_ea64c8f_1` | crabcheck | `PopDropsKey` | `witness_pop_drops_key_case_many` |
| 016 | `pop_drops_key_ea64c8f_1` | hegel | `PopDropsKey` | `witness_pop_drops_key_case_many` |
| 017 | `pop_iter_consistent_5f4a46a_1` | proptest | `PopIterConsistent` | `witness_pop_iter_consistent_case_pop_middle` |
| 018 | `pop_iter_consistent_5f4a46a_1` | quickcheck | `PopIterConsistent` | `witness_pop_iter_consistent_case_pop_middle` |
| 019 | `pop_iter_consistent_5f4a46a_1` | crabcheck | `PopIterConsistent` | `witness_pop_iter_consistent_case_pop_middle` |
| 020 | `pop_iter_consistent_5f4a46a_1` | hegel | `PopIterConsistent` | `witness_pop_iter_consistent_case_pop_middle` |

## Witness Catalog

- `witness_clear_resize_drops_all_case_clear` — base passes, variant fails
- `witness_clear_resize_drops_all_case_resize` — base passes, variant fails
- `witness_clear_resize_drops_all_case_clear_one` — base passes, variant fails
- `witness_clone_unbounded_no_panic_case_three_entries` — base passes, variant fails
- `witness_clone_unbounded_no_panic_case_one_entry` — base passes, variant fails
- `witness_clone_unbounded_no_panic_case_empty` — base passes, variant fails
- `witness_drop_impl_drops_all_case_many` — base passes, variant fails
- `witness_drop_impl_drops_all_case_one` — base passes, variant fails
- `witness_pop_drops_key_case_many` — base passes, variant fails
- `witness_pop_drops_key_case_one` — base passes, variant fails
- `witness_pop_iter_consistent_case_pop_middle` — base passes, variant fails
- `witness_pop_iter_consistent_case_pop_first` — base passes, variant fails
- `witness_pop_iter_consistent_case_pop_last` — base passes, variant fails
