//! Counters for focused benchmark runs.
//!
//! The counter set is enabled by the `benchmark-counters` feature. Without
//! that feature, the record functions do nothing.

#[cfg(feature = "benchmark-counters")]
use std::cell::Cell;

/// Counts collected during one adaptive selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdaptiveCounters {
    /// Calls to the greedy agent that attempt to select one action.
    pub greedy_actions: u64,
    /// Calls to the attack-target search.
    pub attack_target_calls: u64,
    /// In-range board cells inspected by the attack-target search.
    pub destinations_inspected: u64,
    /// Valid unit targets found by the attack-target search.
    pub unit_targets_found: u64,
    /// Valid destructible tile targets found by the attack-target search.
    pub tile_targets_found: u64,
    /// Unit targets passed to the final sort, counted as elements.
    pub candidate_units_sorted: u64,
    /// Completed attack-target searches that found no target.
    pub empty_target_searches: u64,
    /// Forecast requests made for accepted attack candidates.
    pub forecasts_calculated: u64,
}

#[cfg(feature = "benchmark-counters")]
struct CounterCells {
    greedy_actions: Cell<u64>,
    attack_target_calls: Cell<u64>,
    destinations_inspected: Cell<u64>,
    unit_targets_found: Cell<u64>,
    tile_targets_found: Cell<u64>,
    candidate_units_sorted: Cell<u64>,
    empty_target_searches: Cell<u64>,
    forecasts_calculated: Cell<u64>,
}

#[cfg(feature = "benchmark-counters")]
impl CounterCells {
    const fn new() -> Self {
        Self {
            greedy_actions: Cell::new(0),
            attack_target_calls: Cell::new(0),
            destinations_inspected: Cell::new(0),
            unit_targets_found: Cell::new(0),
            tile_targets_found: Cell::new(0),
            candidate_units_sorted: Cell::new(0),
            empty_target_searches: Cell::new(0),
            forecasts_calculated: Cell::new(0),
        }
    }

    fn reset(&self) {
        self.greedy_actions.set(0);
        self.attack_target_calls.set(0);
        self.destinations_inspected.set(0);
        self.unit_targets_found.set(0);
        self.tile_targets_found.set(0);
        self.candidate_units_sorted.set(0);
        self.empty_target_searches.set(0);
        self.forecasts_calculated.set(0);
    }

    fn snapshot(&self) -> AdaptiveCounters {
        AdaptiveCounters {
            greedy_actions: self.greedy_actions.get(),
            attack_target_calls: self.attack_target_calls.get(),
            destinations_inspected: self.destinations_inspected.get(),
            unit_targets_found: self.unit_targets_found.get(),
            tile_targets_found: self.tile_targets_found.get(),
            candidate_units_sorted: self.candidate_units_sorted.get(),
            empty_target_searches: self.empty_target_searches.get(),
            forecasts_calculated: self.forecasts_calculated.get(),
        }
    }
}

#[cfg(feature = "benchmark-counters")]
thread_local! {
    static COUNTERS: CounterCells = const { CounterCells::new() };
}

/// Clear the counters for the current thread.
#[inline]
pub fn reset_adaptive_counters() {
    #[cfg(feature = "benchmark-counters")]
    COUNTERS.with(CounterCells::reset);
}

/// Read the counters for the current thread.
#[inline]
pub fn adaptive_counters() -> AdaptiveCounters {
    #[cfg(feature = "benchmark-counters")]
    return COUNTERS.with(CounterCells::snapshot);

    #[cfg(not(feature = "benchmark-counters"))]
    AdaptiveCounters::default()
}

#[cfg(feature = "benchmark-counters")]
#[inline]
fn add(counter: impl FnOnce(&CounterCells)) {
    COUNTERS.with(counter);
}

/// Count one greedy action selection.
#[inline]
pub fn record_greedy_action() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| c.greedy_actions.set(c.greedy_actions.get() + 1));
}

/// Count one attack-target search.
#[inline]
pub fn record_attack_target_call() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| c.attack_target_calls.set(c.attack_target_calls.get() + 1));
}

/// Count one in-range board cell inspected by an attack-target search.
#[inline]
pub fn record_destination_inspected() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| {
        c.destinations_inspected
            .set(c.destinations_inspected.get() + 1)
    });
}

/// Count one valid unit target found by an attack-target search.
#[inline]
pub fn record_unit_target_found() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| c.unit_targets_found.set(c.unit_targets_found.get() + 1));
}

/// Count one valid tile target found by an attack-target search.
#[inline]
pub fn record_tile_target_found() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| c.tile_targets_found.set(c.tile_targets_found.get() + 1));
}

/// Count the unit elements passed to the final candidate sort.
#[inline]
pub fn record_candidate_units_sorted(count: u64) {
    #[cfg(feature = "benchmark-counters")]
    add(|c| {
        c.candidate_units_sorted
            .set(c.candidate_units_sorted.get() + count)
    });
    #[cfg(not(feature = "benchmark-counters"))]
    let _ = count;
}

/// Count one completed search that found no target.
#[inline]
pub fn record_empty_target_search() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| {
        c.empty_target_searches
            .set(c.empty_target_searches.get() + 1)
    });
}

/// Count one forecast request for an accepted attack candidate.
#[inline]
pub fn record_forecast_calculated() {
    #[cfg(feature = "benchmark-counters")]
    add(|c| c.forecasts_calculated.set(c.forecasts_calculated.get() + 1));
}
