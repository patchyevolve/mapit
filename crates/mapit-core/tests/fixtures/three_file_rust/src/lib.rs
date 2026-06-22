// Fixture: lib.rs — defines compute(), calls into utils::double()
use crate::utils;

pub fn compute(x: u32) -> u32 {
    let doubled = utils::double(x);
    doubled + 1
}

// This function has no callers — it is a dead_code candidate.
fn never_called() -> u32 {
    99
}
