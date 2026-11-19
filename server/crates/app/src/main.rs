//! Server binary: reads the configuration once, wires the adapters into
//! the use cases and runs the background tasks.

fn main() {
    println!("unified-mailbox {}", env!("CARGO_PKG_VERSION"));
}
