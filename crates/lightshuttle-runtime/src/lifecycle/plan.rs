//! Resolved execution plan: topologically sorted nodes plus the dependency graph.
//!
//! The entry point is [`LifecyclePlan::from_manifest`], which converts a
//! parsed [`lightshuttle_manifest::Manifest`] into a [`LifecyclePlan`]. The
//! conversion resolves every resource to a [`lightshuttle_spec::ContainerSpec`]
//! and sorts the nodes using Kahn's topological sort algorithm. The resulting
//! order guarantees that a resource is always listed after all of its
//! dependencies, which allows the [`crate::LifecycleManager`] to start
//! independent branches in parallel.

use std::collections::{HashMap, HashSet};

use lightshuttle_manifest::{Manifest, ResourceKind};

use crate::lifecycle::error::LifecycleError;
use lightshuttle_spec::{ContainerSpec, ResolvedResource, ResourceOutputs, from_resource};

/// A single resource to manage, with its resolved [`ContainerSpec`],
/// its exposed outputs and its explicit dependencies.
#[derive(Debug, Clone)]
pub struct PlanNode {
    /// Resource name as declared in the manifest.
    pub name: String,
    /// Resource kind discriminant (`postgres`, `redis`, `container`,
    /// `dockerfile`), mirrored from the manifest.
    pub kind: String,
    /// Container specification derived from the manifest.
    pub spec: ContainerSpec,
    /// Outputs the resource exposes to its dependents (host, port,
    /// password, url, ...).
    pub outputs: ResourceOutputs,
    /// Names of resources this one depends on.
    pub depends_on: Vec<String>,
}

/// Topologically sorted execution plan.
///
/// Built by [`LifecyclePlan::from_manifest`] and consumed by
/// [`crate::LifecycleManager`]. The node order guarantees that every resource
/// appears after all of its direct and transitive dependencies.
///
/// # Example
///
/// ```rust,no_run
/// use lightshuttle_manifest::Manifest;
/// use lightshuttle_runtime::LifecyclePlan;
///
/// # fn example() -> Result<(), lightshuttle_runtime::LifecycleError> {
/// let yaml = r#"
/// project:
///   name: myapp
/// resources:
///   db:
///     postgres:
///       version: "16"
///   api:
///     container:
///       image: myapp:latest
///       depends_on: [db]
/// "#;
///
/// let manifest = Manifest::parse(yaml).expect("valid manifest");
/// let plan = LifecyclePlan::from_manifest(&manifest)?;
///
/// // Nodes are sorted: "db" appears before "api".
/// for node in plan.nodes() {
///     println!("{} ({}) -> {:?}", node.name, node.kind, node.depends_on);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct LifecyclePlan {
    nodes: Vec<PlanNode>,
    edges: HashMap<String, Vec<String>>,
}

impl LifecyclePlan {
    /// Build a plan from a parsed manifest.
    ///
    /// Resolves the dependency graph, performs a topological sort (Kahn's
    /// algorithm) and converts every resource to a [`ContainerSpec`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::LifecycleError::Cycle`] when the dependency graph
    /// contains a cycle, [`crate::LifecycleError::ResourceNotFound`] when a
    /// resource references an unknown dependency, and
    /// [`crate::LifecycleError::SpecBuild`] when a resource cannot be
    /// converted to a [`ContainerSpec`].
    pub fn from_manifest(manifest: &Manifest) -> Result<Self, LifecycleError> {
        let project = manifest.project.name.as_str();

        // Build edges and collect spec + outputs for every resource.
        let mut resolved: HashMap<String, ResolvedResource> = HashMap::new();
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        let mut kinds: HashMap<String, &'static str> = HashMap::new();
        for (name, kind) in &manifest.resources {
            let r =
                from_resource(project, name, kind).map_err(|source| LifecycleError::SpecBuild {
                    resource: name.clone(),
                    source,
                })?;
            resolved.insert(name.clone(), r);
            deps.insert(name.clone(), merged_dependencies(name, kind));
            kinds.insert(name.clone(), kind.kind_name());
        }

        // Verify every dependency points to an existing resource.
        for (name, dependencies) in &deps {
            for dependency in dependencies {
                if !resolved.contains_key(dependency) {
                    return Err(LifecycleError::ResourceNotFound(format!(
                        "`{dependency}` (depended on by `{name}`)"
                    )));
                }
            }
        }

        // Kahn's algorithm for topological sort.
        let mut in_degree: HashMap<String, usize> = resolved
            .keys()
            .map(|name| (name.clone(), 0_usize))
            .collect();
        for dependencies in deps.values() {
            for dependency in dependencies {
                *in_degree.entry(dependency.clone()).or_insert(0) += 1;
            }
        }

        // Build reverse adjacency: dependency → resources that depend on it.
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for (name, dependencies) in &deps {
            for dependency in dependencies {
                reverse
                    .entry(dependency.clone())
                    .or_default()
                    .push(name.clone());
            }
        }

        // Start with nodes that no one depends on (in_degree == 0 in the
        // reverse graph means "no dependent waits on this one"). For a
        // dependency edge dep → res, res is added to nodes that depend
        // on dep; topo order should yield deps first.
        //
        // We invert: deps come before their dependents.
        // in_degree counts how many incoming dependency edges (= how
        // many of my own dependencies) each node has.
        let mut in_count: HashMap<String, usize> = resolved
            .keys()
            .map(|name| (name.clone(), deps.get(name).map_or(0, Vec::len)))
            .collect();

        let mut ready: Vec<String> = in_count
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(name, _)| name.clone())
            .collect();
        // Deterministic order: sort by name.
        ready.sort();

        let mut sorted: Vec<String> = Vec::with_capacity(resolved.len());
        while let Some(node) = ready.pop() {
            sorted.push(node.clone());
            if let Some(dependents) = reverse.get(&node) {
                let mut newly_ready: Vec<String> = Vec::new();
                for dependent in dependents {
                    let count = in_count.get_mut(dependent).expect("dependent indexed");
                    *count -= 1;
                    if *count == 0 {
                        newly_ready.push(dependent.clone());
                    }
                }
                newly_ready.sort();
                ready.extend(newly_ready);
            }
        }

        if sorted.len() != resolved.len() {
            let unresolved: Vec<&String> = resolved
                .keys()
                .filter(|name| !sorted.contains(name))
                .collect();
            return Err(LifecycleError::Cycle(format!(
                "{unresolved:?} involved in a cycle"
            )));
        }

        let _ = in_degree; // silence unused warning for the alternate counter
        let _ = HashSet::<&str>::new();

        // Snapshot the edges before draining deps into the nodes.
        let edges = deps.clone();

        let nodes: Vec<PlanNode> = sorted
            .into_iter()
            .map(|name| {
                let ResolvedResource { spec, outputs } =
                    resolved.remove(&name).expect("spec indexed by name");
                let dependencies = deps.remove(&name).unwrap_or_default();
                let kind = kinds
                    .remove(&name)
                    .expect("kind indexed by name")
                    .to_owned();
                PlanNode {
                    name,
                    kind,
                    spec,
                    outputs,
                    depends_on: dependencies,
                }
            })
            .collect();

        Ok(Self { nodes, edges })
    }

    /// Returns the nodes in topological order.
    ///
    /// A node is guaranteed to appear after every node it depends on.
    /// The [`crate::LifecycleManager`] iterates this slice to start resources
    /// in order, spawning independent branches concurrently.
    #[must_use]
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    /// Returns the names of resources that directly depend on `name`,
    /// sorted alphabetically.
    ///
    /// Used during teardown to determine which downstream resources must be
    /// stopped before their dependency can be removed.
    #[must_use]
    pub fn dependents_of(&self, name: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for (resource, deps) in &self.edges {
            if deps.iter().any(|d| d == name) {
                out.push(resource.as_str());
            }
        }
        out.sort_unstable();
        out
    }
}

/// Merge a resource's explicit `depends_on` with the implicit dependencies
/// derived from its `${resources.<name>.*}` interpolations.
///
/// Explicit entries keep their declared position, implicit ones are appended
/// in first-occurrence order, and the whole list is de-duplicated. The
/// resource's own name is skipped so that a self-referencing interpolation
/// does not turn into a spurious cycle.
fn merged_dependencies(name: &str, kind: &ResourceKind) -> Vec<String> {
    let mut dependencies = kind.depends_on().to_vec();
    for implicit in kind.implicit_dependencies() {
        if implicit != name && !dependencies.contains(&implicit) {
            dependencies.push(implicit);
        }
    }
    dependencies
}
