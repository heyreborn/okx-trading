//! Low-cardinality in-process counters. Labels are enums, so account, order,
//! decision, and strategy instance IDs cannot become global time-series labels.

use crate::log::{Reason, Service};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Bounded product class label.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProductClass {
    Spot,
    Swap,
    None,
}

/// Counter families available before specialized adapters are implemented.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Counter {
    EventsRejected,
    QueueOverflow,
    DependencyFailure,
}

/// A fixed-cardinality metric key. All dimensions are enums.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Key {
    pub service: Service,
    pub product_class: ProductClass,
    pub counter: Counter,
    pub reason: Reason,
}

/// In-process counters. This is a diagnostic cache, not persistent audit state.
#[derive(Default)]
pub struct Registry(Mutex<BTreeMap<Key, u64>>);

impl Registry {
    /// Increments one counter without allocating an unbounded label string.
    /// Saturates at `u64::MAX`; a poisoned lock is recovered for diagnostics.
    pub fn increment(&self, key: Key) {
        let mut values = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let value = values.entry(key).or_default();
        *value = value.saturating_add(1);
    }

    /// Returns a point-in-time diagnostic snapshot with at most 6*3*3*6 keys.
    pub fn snapshot(&self) -> Vec<(Key, u64)> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .map(|(key, value)| (*key, *value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_remain_bounded_and_counters_accumulate() {
        let registry = Registry::default();
        let key = Key {
            service: Service::Observe,
            product_class: ProductClass::Spot,
            counter: Counter::QueueOverflow,
            reason: Reason::QueueFull,
        };
        registry.increment(key);
        registry.increment(key);
        assert_eq!(registry.snapshot(), vec![(key, 2)]);
    }
}
