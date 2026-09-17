//! Property-testing strategies for `WordPress` capture values.

use chrono::{DateTime, Utc};
use proptest::prelude::*;

/// An instant, with sub-second precision.
pub fn datetime() -> impl Strategy<Value = DateTime<Utc>> {
    (0..=4_102_444_799_i64, 0..1_000_000_000_u32).prop_map(|(seconds, nanoseconds)| {
        DateTime::from_timestamp(seconds, nanoseconds)
            .expect("invariant violation: a generated instant is in range")
    })
}
