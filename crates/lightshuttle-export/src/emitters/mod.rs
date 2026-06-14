//! Concrete [`crate::Emitter`] implementations, one per export target.
//!
//! Each emitter is a zero-sized struct that implements [`crate::Emitter`].
//! All three are re-exported at the crate root for convenience.

mod compose;
mod helm;
mod kubernetes;

pub use compose::ComposeEmitter;
pub use helm::HelmEmitter;
pub use kubernetes::KubernetesEmitter;
