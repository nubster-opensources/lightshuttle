//! Resolved execution plan: topologically sorted nodes plus the
//! dependency graph.

use std::collections::{HashMap, HashSet};

use lightshuttle_manifest::Manifest;

use crate::lifecycle::error::LifecycleError;
use crate::spec::{ContainerSpec, ResolvedResource, ResourceOutputs, from_resource};

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
#[derive(Debug, Clone)]
pub struct LifecyclePlan {
    nodes: Vec<PlanNode>,
    edges: HashMap<String, Vec<String>>,
}

impl LifecyclePlan {
    /// Build a plan from a parsed manifest.
    ///
    /// Resolves the dependency graph, performs a topological sort and
    /// converts every resource to a [`ContainerSpec`].
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
            deps.insert(name.clone(), kind.depends_on().to_vec());
            kinds.insert(name.clone(), kind.kind_name());
        }

        // Verify every dependency points to an existing resource.
        for (name, dependencies) in &deps {
            for dependency in dependencies {
                if !resolved.contains_key(dependency) {
                    return Err(LifecycleError::UnknownResource(format!(
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

    /// The list of nodes in topological order.
    #[must_use]
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    /// Names of resources that depend on `name`.
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
