//! What the runtime is asked to start, by nature of resource.

use std::collections::HashMap;

use crate::process_spec::ProcessSpec;
use crate::spec::ContainerSpec;

/// What the runtime is asked to start, by nature of resource.
///
/// Deliberately **not** `#[non_exhaustive]`, unlike the specs it wraps. A
/// third kind of resource must make the compiler list every consumer that
/// routes on this type, exactly as `Segment` does for the placeholder
/// grammar. The cost of that choice is a breaking change for out-of-tree
/// consumers when a kind is added; the benefit is that no consumer can
/// silently fall through to a default that ignores the new kind.
///
/// Equality is not derived, because [`ContainerSpec`] does not implement it.
///
/// The two variants differ a lot in size, and the container one is deliberately
/// left unboxed. This enum is constructed a handful of times per `up` and passed
/// by reference everywhere after that, so the size difference costs a few stack
/// bytes at construction; boxing would instead add a heap allocation to the
/// common case in order to save them.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ResourceSpec {
    /// A container handed to the container daemon.
    Container(ContainerSpec),
    /// A command executed natively on the developer's machine.
    Process(ProcessSpec),
}

impl ResourceSpec {
    /// Stable identity of the resource, of the form `<project>_<resource>`,
    /// whatever its nature.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Container(spec) => &spec.name,
            Self::Process(spec) => &spec.name,
        }
    }

    /// Project the resource belongs to.
    #[must_use]
    pub fn project(&self) -> &str {
        match self {
            Self::Container(spec) => &spec.project,
            Self::Process(spec) => &spec.project,
        }
    }

    /// Resource name as declared in the manifest.
    #[must_use]
    pub fn resource(&self) -> &str {
        match self {
            Self::Container(spec) => &spec.resource,
            Self::Process(spec) => &spec.resource,
        }
    }

    /// Mutable access to the environment injected into the resource.
    ///
    /// Both kinds carry one, and the lifecycle manager fills it with the
    /// `LSH_*` variables derived from dependency outputs before starting the
    /// resource. Routing here rather than at the call site is what keeps a
    /// process from silently missing the variables a container receives.
    pub fn env_mut(&mut self) -> &mut HashMap<String, String> {
        match self {
            Self::Container(spec) => &mut spec.env,
            Self::Process(spec) => &mut spec.env,
        }
    }

    /// The container specification, when this resource is a container.
    ///
    /// Exists for consumers that handle containers and containers only, such
    /// as the export pipeline, which refuses a manifest holding a process
    /// rather than rendering one. Returning an `Option` makes that refusal an
    /// explicit branch instead of something a consumer can forget.
    #[must_use]
    pub fn as_container(&self) -> Option<&ContainerSpec> {
        match self {
            Self::Container(spec) => Some(spec),
            Self::Process(_) => None,
        }
    }

    /// The process specification, when this resource is a native process.
    #[must_use]
    pub fn as_process(&self) -> Option<&ProcessSpec> {
        match self {
            Self::Process(spec) => Some(spec),
            Self::Container(_) => None,
        }
    }
}

impl From<ContainerSpec> for ResourceSpec {
    fn from(spec: ContainerSpec) -> Self {
        Self::Container(spec)
    }
}

impl From<ProcessSpec> for ResourceSpec {
    fn from(spec: ProcessSpec) -> Self {
        Self::Process(spec)
    }
}
