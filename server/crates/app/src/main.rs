//! Server binary: reads the configuration once, wires the adapters into
//! the use cases and runs the background tasks.

// `main` uses the configuration from the next commit on.
#[cfg_attr(not(test), allow(dead_code))]
mod config;

fn main() {
    println!("unified-mailbox {}", env!("CARGO_PKG_VERSION"));
}
