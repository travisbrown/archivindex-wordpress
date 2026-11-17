//! Property-testing strategies for `WordPress` capture values.

use jiff::Timestamp;
use proptest::prelude::*;

/// An instant, with sub-second precision.
pub fn datetime() -> impl Strategy<Value = Timestamp> {
    (0..=4_102_444_799_i64, 0..1_000_000_000_i32).prop_map(|(seconds, nanoseconds)| {
        Timestamp::new(seconds, nanoseconds)
            .expect("invariant violation: a generated instant is in range")
    })
}
