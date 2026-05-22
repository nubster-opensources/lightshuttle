//! LightShuttle CLI binary.
//!
//! This is a placeholder for the v0.0.1 bootstrap release. The real
//! `clap`-based command surface (`up`, `down`, `ps`, `logs`, `validate`,
//! `manifest`) lands in a follow-up pull request.

fn main() {
    println!(
        "lightshuttle {} — placeholder bootstrap, see https://github.com/nubster-opensources/lightshuttle",
        env!("CARGO_PKG_VERSION"),
    );
}
