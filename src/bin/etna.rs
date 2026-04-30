// ETNA workload runner for lru.
//
// Usage: cargo run --release --bin etna -- <tool> <property>
//   tool:     etna | proptest | quickcheck | crabcheck | hegel
//   property: CloneUnboundedNoPanic | PopIterConsistent | DropImplDropsAll
//             ClearResizeDropsAll | PopDropsKey | All
//
// Each run emits a single JSON line on stdout with fields:
//   status, tests, discards, time, counterexample, error, tool, property.
// Exit status is always 0 on completion; non-zero exit is reserved for
// adapter-level panics that escape the catch_unwind in main().

use crabcheck::quickcheck as crabcheck_qc;
use hegel::{generators as hgen, HealthCheck, Hegel, Settings as HegelSettings, TestCase};
use lru::etna::{
    property_clear_resize_drops_all, property_clone_unbounded_no_panic,
    property_drop_impl_drops_all, property_pop_drops_key, property_pop_iter_consistent,
    PropertyResult,
};
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, TestCaseError, TestRunner};
use quickcheck::{Arbitrary as QcArbitrary, Gen, QuickCheck, ResultStatus, TestResult};
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default, Clone, Copy)]
struct Metrics {
    inputs: u64,
    elapsed_us: u128,
}

impl Metrics {
    fn combine(self, other: Metrics) -> Metrics {
        Metrics {
            inputs: self.inputs + other.inputs,
            elapsed_us: self.elapsed_us + other.elapsed_us,
        }
    }
}

type Outcome = (Result<(), String>, Metrics);

fn to_err(r: PropertyResult) -> Result<(), String> {
    match r {
        PropertyResult::Pass | PropertyResult::Discard => Ok(()),
        PropertyResult::Fail(m) => Err(m),
    }
}

const ALL_PROPERTIES: &[&str] = &[
    "CloneUnboundedNoPanic",
    "PopIterConsistent",
    "DropImplDropsAll",
    "ClearResizeDropsAll",
    "PopDropsKey",
];

fn run_all<F: FnMut(&str) -> Outcome>(mut f: F) -> Outcome {
    let mut total = Metrics::default();
    let mut final_status: Result<(), String> = Ok(());
    for p in ALL_PROPERTIES {
        let (r, m) = f(p);
        total = total.combine(m);
        if r.is_err() && final_status.is_ok() {
            final_status = r;
        }
    }
    (final_status, total)
}

// Caps on generated input sizes. The drop-counting properties build a
// LruCache with one TrackedKey/TrackedVal per entry, so input lengths
// translate directly into per-call allocations. Keep them small enough to
// stay well under a 60-second wall-clock per `<tool> All` invocation while
// still producing meaningful drop counts.
const MAX_KEYS: usize = 24;

// ============================================================================
// Input wrappers (used so we can carry custom Arbitrary impls cleanly).
// ============================================================================

#[derive(Clone)]
struct ItemsInput {
    items: Vec<(u32, u32)>,
}

#[derive(Clone)]
struct KeysInput {
    keys: Vec<u32>,
}

#[derive(Clone)]
struct PopInput {
    keys: Vec<u32>,
    pop_pick: u8,
}

#[derive(Clone)]
struct ClearResizeInput {
    keys: Vec<u32>,
    use_clear: bool,
}

impl fmt::Debug for ItemsInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.items)
    }
}

impl fmt::Debug for KeysInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.keys)
    }
}

impl fmt::Debug for PopInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {}", self.keys, self.pop_pick)
    }
}

impl fmt::Debug for ClearResizeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {}", self.keys, self.use_clear)
    }
}

impl fmt::Display for ItemsInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for KeysInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for PopInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for ClearResizeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// ============================================================================
// etna (deterministic witness-shaped inputs)
// ============================================================================

fn run_etna_property(property: &str) -> Outcome {
    if property == "All" {
        return run_all(run_etna_property);
    }
    let t0 = Instant::now();
    let result = match property {
        "CloneUnboundedNoPanic" => to_err(property_clone_unbounded_no_panic(vec![
            (1u32, 10u32),
            (2, 20),
            (3, 30),
        ])),
        "PopIterConsistent" => to_err(property_pop_iter_consistent((vec![1u32, 2, 3, 4, 5], 2))),
        "DropImplDropsAll" => to_err(property_drop_impl_drops_all(vec![1u32, 2, 3, 4, 5, 6, 7, 8])),
        "ClearResizeDropsAll" => to_err(property_clear_resize_drops_all((
            vec![1u32, 2, 3, 4],
            true,
        ))),
        "PopDropsKey" => to_err(property_pop_drops_key(vec![1u32, 2, 3, 4, 5])),
        _ => {
            return (
                Err(format!("Unknown property for etna: {property}")),
                Metrics::default(),
            )
        }
    };
    let elapsed_us = t0.elapsed().as_micros();
    (result, Metrics { inputs: 1, elapsed_us })
}

// ============================================================================
// proptest
// ============================================================================

fn items_strategy() -> BoxedStrategy<ItemsInput> {
    prop::collection::vec((any::<u32>(), any::<u32>()), 0..=MAX_KEYS)
        .prop_map(|items| ItemsInput { items })
        .boxed()
}

fn keys_strategy() -> BoxedStrategy<KeysInput> {
    prop::collection::vec(any::<u32>(), 0..=MAX_KEYS)
        .prop_map(|keys| KeysInput { keys })
        .boxed()
}

fn pop_strategy() -> BoxedStrategy<PopInput> {
    (
        prop::collection::vec(any::<u32>(), 2..=MAX_KEYS),
        any::<u8>(),
    )
        .prop_map(|(keys, pop_pick)| PopInput { keys, pop_pick })
        .boxed()
}

fn clear_resize_strategy() -> BoxedStrategy<ClearResizeInput> {
    (
        prop::collection::vec(any::<u32>(), 1..=MAX_KEYS),
        any::<bool>(),
    )
        .prop_map(|(keys, use_clear)| ClearResizeInput { keys, use_clear })
        .boxed()
}

fn run_proptest_property(property: &str) -> Outcome {
    if property == "All" {
        return run_all(run_proptest_property);
    }
    let counter = Arc::new(AtomicU64::new(0));
    let t0 = Instant::now();
    let mut runner = TestRunner::new(ProptestConfig::default());
    let c = counter.clone();
    let result: Result<(), String> = match property {
        "CloneUnboundedNoPanic" => runner
            .run(&items_strategy(), move |args| {
                c.fetch_add(1, Ordering::Relaxed);
                let cex = format!("({:?})", args);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_clone_unbounded_no_panic(args.items.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => Ok(()),
                    Ok(PropertyResult::Fail(_)) | Err(_) => Err(TestCaseError::fail(cex)),
                }
            })
            .map_err(|e| match e {
                proptest::test_runner::TestError::Fail(r, _) => r.to_string(),
                other => other.to_string(),
            }),
        "PopIterConsistent" => runner
            .run(&pop_strategy(), move |args| {
                c.fetch_add(1, Ordering::Relaxed);
                let cex = format!("({:?})", args);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_pop_iter_consistent((args.keys.clone(), args.pop_pick))
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => Ok(()),
                    Ok(PropertyResult::Fail(_)) | Err(_) => Err(TestCaseError::fail(cex)),
                }
            })
            .map_err(|e| match e {
                proptest::test_runner::TestError::Fail(r, _) => r.to_string(),
                other => other.to_string(),
            }),
        "DropImplDropsAll" => runner
            .run(&keys_strategy(), move |args| {
                c.fetch_add(1, Ordering::Relaxed);
                let cex = format!("({:?})", args);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_drop_impl_drops_all(args.keys.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => Ok(()),
                    Ok(PropertyResult::Fail(_)) | Err(_) => Err(TestCaseError::fail(cex)),
                }
            })
            .map_err(|e| match e {
                proptest::test_runner::TestError::Fail(r, _) => r.to_string(),
                other => other.to_string(),
            }),
        "ClearResizeDropsAll" => runner
            .run(&clear_resize_strategy(), move |args| {
                c.fetch_add(1, Ordering::Relaxed);
                let cex = format!("({:?})", args);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_clear_resize_drops_all((args.keys.clone(), args.use_clear))
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => Ok(()),
                    Ok(PropertyResult::Fail(_)) | Err(_) => Err(TestCaseError::fail(cex)),
                }
            })
            .map_err(|e| match e {
                proptest::test_runner::TestError::Fail(r, _) => r.to_string(),
                other => other.to_string(),
            }),
        "PopDropsKey" => runner
            .run(&keys_strategy(), move |args| {
                c.fetch_add(1, Ordering::Relaxed);
                let cex = format!("({:?})", args);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_pop_drops_key(args.keys.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => Ok(()),
                    Ok(PropertyResult::Fail(_)) | Err(_) => Err(TestCaseError::fail(cex)),
                }
            })
            .map_err(|e| match e {
                proptest::test_runner::TestError::Fail(r, _) => r.to_string(),
                other => other.to_string(),
            }),
        _ => {
            return (
                Err(format!("Unknown property for proptest: {property}")),
                Metrics::default(),
            )
        }
    };
    let elapsed_us = t0.elapsed().as_micros();
    let inputs = counter.load(Ordering::Relaxed);
    (result, Metrics { inputs, elapsed_us })
}

// ============================================================================
// quickcheck (forked, fn-pointer based)
// ============================================================================

impl QcArbitrary for ItemsInput {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = (<u8 as QcArbitrary>::arbitrary(g) as usize) % (MAX_KEYS + 1);
        let mut items = Vec::with_capacity(len);
        for _ in 0..len {
            let k = <u32 as QcArbitrary>::arbitrary(g);
            let v = <u32 as QcArbitrary>::arbitrary(g);
            items.push((k, v));
        }
        ItemsInput { items }
    }
}

impl QcArbitrary for KeysInput {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = (<u8 as QcArbitrary>::arbitrary(g) as usize) % (MAX_KEYS + 1);
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(<u32 as QcArbitrary>::arbitrary(g));
        }
        KeysInput { keys }
    }
}

impl QcArbitrary for PopInput {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = 2 + (<u8 as QcArbitrary>::arbitrary(g) as usize) % (MAX_KEYS - 1);
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(<u32 as QcArbitrary>::arbitrary(g));
        }
        PopInput {
            keys,
            pop_pick: <u8 as QcArbitrary>::arbitrary(g),
        }
    }
}

impl QcArbitrary for ClearResizeInput {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = 1 + (<u8 as QcArbitrary>::arbitrary(g) as usize) % MAX_KEYS;
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(<u32 as QcArbitrary>::arbitrary(g));
        }
        ClearResizeInput {
            keys,
            use_clear: <bool as QcArbitrary>::arbitrary(g),
        }
    }
}

static QC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn qc_run<F>(prop: F) -> TestResult
where
    F: FnOnce() -> PropertyResult + std::panic::UnwindSafe,
{
    QC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let res = std::panic::catch_unwind(prop);
    match res {
        Ok(PropertyResult::Pass) => TestResult::passed(),
        Ok(PropertyResult::Discard) => TestResult::discard(),
        Ok(PropertyResult::Fail(_)) | Err(_) => TestResult::failed(),
    }
}

fn qc_clone_unbounded_no_panic(args: ItemsInput) -> TestResult {
    qc_run(move || property_clone_unbounded_no_panic(args.items))
}

fn qc_pop_iter_consistent(args: PopInput) -> TestResult {
    qc_run(move || property_pop_iter_consistent((args.keys, args.pop_pick)))
}

fn qc_drop_impl_drops_all(args: KeysInput) -> TestResult {
    qc_run(move || property_drop_impl_drops_all(args.keys))
}

fn qc_clear_resize_drops_all(args: ClearResizeInput) -> TestResult {
    qc_run(move || property_clear_resize_drops_all((args.keys, args.use_clear)))
}

fn qc_pop_drops_key(args: KeysInput) -> TestResult {
    qc_run(move || property_pop_drops_key(args.keys))
}

fn run_quickcheck_property(property: &str) -> Outcome {
    if property == "All" {
        return run_all(run_quickcheck_property);
    }
    QC_COUNTER.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    let result = match property {
        "CloneUnboundedNoPanic" => QuickCheck::new()
            .tests(200)
            .max_tests(2000)
            .max_time(Duration::from_secs(86_400))
            .quicktest(qc_clone_unbounded_no_panic as fn(ItemsInput) -> TestResult),
        "PopIterConsistent" => QuickCheck::new()
            .tests(200)
            .max_tests(2000)
            .max_time(Duration::from_secs(86_400))
            .quicktest(qc_pop_iter_consistent as fn(PopInput) -> TestResult),
        "DropImplDropsAll" => QuickCheck::new()
            .tests(200)
            .max_tests(2000)
            .max_time(Duration::from_secs(86_400))
            .quicktest(qc_drop_impl_drops_all as fn(KeysInput) -> TestResult),
        "ClearResizeDropsAll" => QuickCheck::new()
            .tests(200)
            .max_tests(2000)
            .max_time(Duration::from_secs(86_400))
            .quicktest(qc_clear_resize_drops_all as fn(ClearResizeInput) -> TestResult),
        "PopDropsKey" => QuickCheck::new()
            .tests(200)
            .max_tests(2000)
            .max_time(Duration::from_secs(86_400))
            .quicktest(qc_pop_drops_key as fn(KeysInput) -> TestResult),
        _ => {
            return (
                Err(format!("Unknown property for quickcheck: {property}")),
                Metrics::default(),
            )
        }
    };
    let elapsed_us = t0.elapsed().as_micros();
    let inputs = QC_COUNTER.load(Ordering::Relaxed);
    let metrics = Metrics { inputs, elapsed_us };
    let status = match result.status {
        ResultStatus::Finished => Ok(()),
        ResultStatus::Failed { arguments } => Err(format!("({})", arguments.join(" "))),
        ResultStatus::Aborted { err } => Err(format!("aborted: {err:?}")),
        ResultStatus::TimedOut => Err("timed out".to_string()),
        ResultStatus::GaveUp => Err(format!(
            "gave up: passed={}, discarded={}",
            result.n_tests_passed, result.n_tests_discarded
        )),
    };
    (status, metrics)
}

// ============================================================================
// crabcheck
// ============================================================================

use crabcheck::quickcheck::Arbitrary as CcArbitrary;
use rand::Rng as CcRng;

impl<R: CcRng> CcArbitrary<R> for ItemsInput {
    fn generate(rng: &mut R, _n: usize) -> Self {
        let len = (rng.random::<u8>() as usize) % (MAX_KEYS + 1);
        let mut items = Vec::with_capacity(len);
        for _ in 0..len {
            items.push((rng.random::<u32>(), rng.random::<u32>()));
        }
        ItemsInput { items }
    }
}

impl<R: CcRng> CcArbitrary<R> for KeysInput {
    fn generate(rng: &mut R, _n: usize) -> Self {
        let len = (rng.random::<u8>() as usize) % (MAX_KEYS + 1);
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(rng.random::<u32>());
        }
        KeysInput { keys }
    }
}

impl<R: CcRng> CcArbitrary<R> for PopInput {
    fn generate(rng: &mut R, _n: usize) -> Self {
        let len = 2 + (rng.random::<u8>() as usize) % (MAX_KEYS - 1);
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(rng.random::<u32>());
        }
        PopInput {
            keys,
            pop_pick: rng.random::<u8>(),
        }
    }
}

impl<R: CcRng> CcArbitrary<R> for ClearResizeInput {
    fn generate(rng: &mut R, _n: usize) -> Self {
        let len = 1 + (rng.random::<u8>() as usize) % MAX_KEYS;
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(rng.random::<u32>());
        }
        ClearResizeInput {
            keys,
            use_clear: rng.random::<bool>(),
        }
    }
}

static CC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn cc_clone_unbounded_no_panic(v: ItemsInput) -> Option<bool> {
    CC_COUNTER.fetch_add(1, Ordering::Relaxed);
    match property_clone_unbounded_no_panic(v.items) {
        PropertyResult::Pass => Some(true),
        PropertyResult::Fail(_) => Some(false),
        PropertyResult::Discard => None,
    }
}

fn cc_pop_iter_consistent(v: PopInput) -> Option<bool> {
    CC_COUNTER.fetch_add(1, Ordering::Relaxed);
    match property_pop_iter_consistent((v.keys, v.pop_pick)) {
        PropertyResult::Pass => Some(true),
        PropertyResult::Fail(_) => Some(false),
        PropertyResult::Discard => None,
    }
}

fn cc_drop_impl_drops_all(v: KeysInput) -> Option<bool> {
    CC_COUNTER.fetch_add(1, Ordering::Relaxed);
    match property_drop_impl_drops_all(v.keys) {
        PropertyResult::Pass => Some(true),
        PropertyResult::Fail(_) => Some(false),
        PropertyResult::Discard => None,
    }
}

fn cc_clear_resize_drops_all(v: ClearResizeInput) -> Option<bool> {
    CC_COUNTER.fetch_add(1, Ordering::Relaxed);
    match property_clear_resize_drops_all((v.keys, v.use_clear)) {
        PropertyResult::Pass => Some(true),
        PropertyResult::Fail(_) => Some(false),
        PropertyResult::Discard => None,
    }
}

fn cc_pop_drops_key(v: KeysInput) -> Option<bool> {
    CC_COUNTER.fetch_add(1, Ordering::Relaxed);
    match property_pop_drops_key(v.keys) {
        PropertyResult::Pass => Some(true),
        PropertyResult::Fail(_) => Some(false),
        PropertyResult::Discard => None,
    }
}

fn run_crabcheck_property(property: &str) -> Outcome {
    if property == "All" {
        return run_all(run_crabcheck_property);
    }
    CC_COUNTER.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    let cfg = crabcheck_qc::Config { tests: 200 };
    let result = match property {
        "CloneUnboundedNoPanic" => crabcheck_qc::quickcheck_with_config(
            cfg,
            cc_clone_unbounded_no_panic as fn(ItemsInput) -> Option<bool>,
        ),
        "PopIterConsistent" => crabcheck_qc::quickcheck_with_config(
            cfg,
            cc_pop_iter_consistent as fn(PopInput) -> Option<bool>,
        ),
        "DropImplDropsAll" => crabcheck_qc::quickcheck_with_config(
            cfg,
            cc_drop_impl_drops_all as fn(KeysInput) -> Option<bool>,
        ),
        "ClearResizeDropsAll" => crabcheck_qc::quickcheck_with_config(
            cfg,
            cc_clear_resize_drops_all as fn(ClearResizeInput) -> Option<bool>,
        ),
        "PopDropsKey" => crabcheck_qc::quickcheck_with_config(
            cfg,
            cc_pop_drops_key as fn(KeysInput) -> Option<bool>,
        ),
        _ => {
            return (
                Err(format!("Unknown property for crabcheck: {property}")),
                Metrics::default(),
            )
        }
    };
    let elapsed_us = t0.elapsed().as_micros();
    let inputs = CC_COUNTER.load(Ordering::Relaxed);
    let metrics = Metrics { inputs, elapsed_us };
    let status = match result.status {
        crabcheck_qc::ResultStatus::Finished => Ok(()),
        crabcheck_qc::ResultStatus::Failed { arguments } => {
            Err(format!("({})", arguments.join(" ")))
        }
        crabcheck_qc::ResultStatus::TimedOut => Err("timed out".to_string()),
        crabcheck_qc::ResultStatus::GaveUp => Err(format!(
            "gave up: passed={}, discarded={}",
            result.passed, result.discarded
        )),
        crabcheck_qc::ResultStatus::Aborted { error } => Err(format!("aborted: {error}")),
    };
    (status, metrics)
}

// ============================================================================
// hegel
// ============================================================================

static HG_COUNTER: AtomicU64 = AtomicU64::new(0);

fn hegel_settings() -> HegelSettings {
    HegelSettings::new()
        .test_cases(200)
        .suppress_health_check(HealthCheck::all())
}

fn hg_draw_u8(tc: &TestCase) -> u8 {
    tc.draw(hgen::integers::<u32>().min_value(0).max_value(255)) as u8
}

fn hg_draw_u32(tc: &TestCase) -> u32 {
    tc.draw(hgen::integers::<u32>())
}

fn hg_draw_bool(tc: &TestCase) -> bool {
    tc.draw(hgen::booleans())
}

fn hg_draw_keys(tc: &TestCase, min: usize) -> Vec<u32> {
    let extra = (hg_draw_u8(tc) as usize) % (MAX_KEYS - min + 1);
    let len = min + extra;
    let mut keys = Vec::with_capacity(len);
    for _ in 0..len {
        keys.push(hg_draw_u32(tc));
    }
    keys
}

fn run_hegel_property(property: &str) -> Outcome {
    if property == "All" {
        return run_all(run_hegel_property);
    }
    HG_COUNTER.store(0, Ordering::Relaxed);
    let t0 = Instant::now();
    let settings = hegel_settings();
    let run_result = std::panic::catch_unwind(AssertUnwindSafe(|| match property {
        "CloneUnboundedNoPanic" => {
            Hegel::new(|tc: TestCase| {
                HG_COUNTER.fetch_add(1, Ordering::Relaxed);
                let len = (hg_draw_u8(&tc) as usize) % (MAX_KEYS + 1);
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push((hg_draw_u32(&tc), hg_draw_u32(&tc)));
                }
                let cex = format!("({:?})", items);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_clone_unbounded_no_panic(items.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => {}
                    Ok(PropertyResult::Fail(_)) | Err(_) => panic!("{cex}"),
                }
            })
            .settings(settings.clone())
            .run();
        }
        "PopIterConsistent" => {
            Hegel::new(|tc: TestCase| {
                HG_COUNTER.fetch_add(1, Ordering::Relaxed);
                let keys = hg_draw_keys(&tc, 2);
                let pop_pick = hg_draw_u8(&tc);
                let cex = format!("({:?} {})", keys, pop_pick);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_pop_iter_consistent((keys.clone(), pop_pick))
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => {}
                    Ok(PropertyResult::Fail(_)) | Err(_) => panic!("{cex}"),
                }
            })
            .settings(settings.clone())
            .run();
        }
        "DropImplDropsAll" => {
            Hegel::new(|tc: TestCase| {
                HG_COUNTER.fetch_add(1, Ordering::Relaxed);
                let keys = hg_draw_keys(&tc, 0);
                let cex = format!("({:?})", keys);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_drop_impl_drops_all(keys.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => {}
                    Ok(PropertyResult::Fail(_)) | Err(_) => panic!("{cex}"),
                }
            })
            .settings(settings.clone())
            .run();
        }
        "ClearResizeDropsAll" => {
            Hegel::new(|tc: TestCase| {
                HG_COUNTER.fetch_add(1, Ordering::Relaxed);
                let keys = hg_draw_keys(&tc, 1);
                let use_clear = hg_draw_bool(&tc);
                let cex = format!("({:?} {})", keys, use_clear);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_clear_resize_drops_all((keys.clone(), use_clear))
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => {}
                    Ok(PropertyResult::Fail(_)) | Err(_) => panic!("{cex}"),
                }
            })
            .settings(settings.clone())
            .run();
        }
        "PopDropsKey" => {
            Hegel::new(|tc: TestCase| {
                HG_COUNTER.fetch_add(1, Ordering::Relaxed);
                let keys = hg_draw_keys(&tc, 0);
                let cex = format!("({:?})", keys);
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    property_pop_drops_key(keys.clone())
                }));
                match res {
                    Ok(PropertyResult::Pass) | Ok(PropertyResult::Discard) => {}
                    Ok(PropertyResult::Fail(_)) | Err(_) => panic!("{cex}"),
                }
            })
            .settings(settings.clone())
            .run();
        }
        _ => panic!("__unknown_property:{property}"),
    }));
    let elapsed_us = t0.elapsed().as_micros();
    let inputs = HG_COUNTER.load(Ordering::Relaxed);
    let metrics = Metrics { inputs, elapsed_us };
    let status = match run_result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "hegel panicked with non-string payload".to_string()
            };
            if let Some(rest) = msg.strip_prefix("__unknown_property:") {
                return (
                    Err(format!("Unknown property for hegel: {rest}")),
                    Metrics::default(),
                );
            }
            Err(msg
                .strip_prefix("Property test failed: ")
                .unwrap_or(&msg)
                .to_string())
        }
    };
    (status, metrics)
}

// ============================================================================
// dispatch + main
// ============================================================================

fn run(tool: &str, property: &str) -> Outcome {
    match tool {
        "etna" => run_etna_property(property),
        "proptest" => run_proptest_property(property),
        "quickcheck" => run_quickcheck_property(property),
        "crabcheck" => run_crabcheck_property(property),
        "hegel" => run_hegel_property(property),
        _ => (Err(format!("Unknown tool: {tool}")), Metrics::default()),
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit_json(
    tool: &str,
    property: &str,
    status: &str,
    metrics: Metrics,
    counterexample: Option<&str>,
    error: Option<&str>,
) {
    let cex = counterexample.map_or("null".to_string(), json_str);
    let err = error.map_or("null".to_string(), json_str);
    println!(
        "{{\"status\":{},\"tests\":{},\"discards\":0,\"time\":{},\"counterexample\":{},\"error\":{},\"tool\":{},\"property\":{}}}",
        json_str(status),
        metrics.inputs,
        json_str(&format!("{}us", metrics.elapsed_us)),
        cex,
        err,
        json_str(tool),
        json_str(property),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <tool> <property>", args[0]);
        eprintln!("Tools: etna | proptest | quickcheck | crabcheck | hegel");
        eprintln!(
            "Properties: CloneUnboundedNoPanic | PopIterConsistent | DropImplDropsAll | ClearResizeDropsAll | PopDropsKey | All"
        );
        std::process::exit(2);
    }
    let (tool, property) = (args[1].as_str(), args[2].as_str());

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(AssertUnwindSafe(|| run(tool, property)));
    std::panic::set_hook(previous_hook);

    let (result, metrics) = match caught {
        Ok(outcome) => outcome,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "panic with non-string payload".to_string()
            };
            emit_json(
                tool,
                property,
                "aborted",
                Metrics::default(),
                None,
                Some(&format!("adapter panic: {msg}")),
            );
            return;
        }
    };

    match result {
        Ok(()) => emit_json(tool, property, "passed", metrics, None, None),
        Err(msg) => emit_json(tool, property, "failed", metrics, Some(&msg), None),
    }
}
