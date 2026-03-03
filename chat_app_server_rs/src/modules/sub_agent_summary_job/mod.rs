mod config;
mod executor;
pub mod types;
mod worker;

use std::sync::Once;

static START_ONCE: Once = Once::new();

pub fn start_background() {
    START_ONCE.call_once(worker::start_worker);
}
