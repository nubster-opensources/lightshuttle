//! Canonical parsers for the normalised grammars the project relies on.
//!
//! Several grammars used across LightShuttle look simple enough to split by
//! hand, and are not: an OCI image reference gives `:` three distinct roles,
//! a DNS label has a normalisation that must stay injective, and a volume
//! mapping shares its separator with a Windows drive letter. Each of those
//! grammars was reimplemented independently in `lightshuttle-export` and in
//! `lightshuttle-runtime`, each time approximately, and each time
//! differently.
//!
//! This module holds one parser per grammar. Downstream crates consume the
//! resulting types and take no parsing decision of their own, so a grammar
//! is understood in exactly one place.
//!
//! `lightshuttle-manifest` is the only common ancestor of `lightshuttle-export`
//! and `lightshuttle-runtime` in the workspace dependency graph, which is why
//! the canonical types live here.

pub mod dns_name;
pub mod duration;
pub mod image_reference;

pub use dns_name::{DnsName, DnsNameError, is_dns_label};
pub use duration::{DurationError, parse_duration, to_whole_seconds};
pub use image_reference::{ImageReference, ImageReferenceError};
