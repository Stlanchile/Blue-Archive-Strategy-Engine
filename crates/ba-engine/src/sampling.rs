use rand_core::RngCore;

use crate::EngineError;

/// Samples uniformly from `0..bound` without modulo bias.
///
/// Keeping this primitive shared prevents the v2 binary and v3 categorical
/// samplers from drifting to different rejection rules.
pub(crate) fn uniform_below(rng: &mut impl RngCore, bound: u64) -> Result<u64, EngineError> {
    if bound == 0 {
        return Err(EngineError::InternalInvariantViolation {
            message: "cannot sample with a zero bound".to_owned(),
        });
    }
    if bound == 1 {
        return Ok(0);
    }
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return Ok(value % bound);
        }
    }
}
