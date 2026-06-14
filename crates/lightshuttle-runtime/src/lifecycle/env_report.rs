//! Classification of `${env.*}` references found in a [`LifecyclePlan`].
//!
//! Both `lightshuttle up` (its fail-fast preflight,
//! [`LifecycleManager::check_required_env`]) and `lightshuttle secrets
//! check` consume the report produced here, so the diagnostic command
//! predicts what the runtime will do, exactly. Only environment values and
//! command arguments are scanned, matching the sites the runtime actually
//! interpolates; a reference in an image tag or working directory is never
//! resolved and therefore never reported.
//!
//! [`LifecycleManager::check_required_env`]: crate::LifecycleManager::check_required_env

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lightshuttle_manifest::{InterpolationContext, Interpolator, Reference};

use crate::lifecycle::plan::LifecyclePlan;

/// Where a resolved variable's effective value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvSource {
    /// Supplied by the loaded `.env` file, which takes precedence over the
    /// ambient process environment.
    EnvFile,
    /// Inherited from the ambient process environment.
    Process,
}

/// Resolution status of a single referenced environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvVarStatus {
    /// Set to a non-empty value, resolved from the carried source.
    Resolved(EnvSource),
    /// Unset, but every reference supplies a default fallback.
    Defaulted {
        /// Distinct default fallbacks declared across references, sorted.
        defaults: Vec<String>,
    },
    /// Unset (or empty) and at least one reference has no default.
    Missing,
}

/// One referenced variable together with its resolution status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVarReport {
    /// Variable name as written inside `${env.NAME}`.
    pub name: String,
    /// Whether it resolves, falls back to a default, or is missing.
    pub status: EnvVarStatus,
}

/// Report over every `${env.*}` reference found in a plan's environment values
/// and command arguments.
///
/// Built by [`LifecyclePlan::env_report`] and consumed by
/// [`crate::LifecycleManager::check_required_env`] (fail-fast preflight) and
/// the `lightshuttle secrets check` subcommand (interactive diagnostic). There
/// is one entry per distinct variable name, sorted alphabetically.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvReport {
    /// One entry per distinct referenced variable, sorted alphabetically by name.
    pub vars: Vec<EnvVarReport>,
}

impl EnvReport {
    /// Returns `true` when no `${env.*}` reference was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Names of every variable whose status is [`EnvVarStatus::Missing`].
    ///
    /// The result is sorted and free of duplicates because the report holds
    /// at most one entry per name, kept in name order.
    #[must_use]
    pub fn missing(&self) -> Vec<String> {
        self.vars
            .iter()
            .filter(|v| v.status == EnvVarStatus::Missing)
            .map(|v| v.name.clone())
            .collect()
    }

    /// Returns `true` when at least one referenced variable is missing.
    #[must_use]
    pub fn has_missing(&self) -> bool {
        self.vars.iter().any(|v| v.status == EnvVarStatus::Missing)
    }
}

/// Aggregated facts about every reference to one variable name.
#[derive(Default)]
struct Aggregate {
    /// `true` when at least one reference omits a default fallback.
    required: bool,
    /// Distinct default fallbacks seen across references, sorted.
    defaults: BTreeSet<String>,
}

impl LifecyclePlan {
    /// Classify every `${env.*}` reference in this plan against the ambient
    /// process environment plus `extra_env` (which takes precedence).
    ///
    /// Only environment values and command arguments are scanned because those
    /// are the only sites the runtime interpolates at start time. References in
    /// image tags or working directories are intentionally excluded.
    ///
    /// The resolution logic delegates to the same [`Interpolator`] the runtime
    /// uses, so an empty value counts as unset and the report mirrors what a
    /// real `start_all` call would do. This makes the report suitable as both a
    /// preflight check (called by [`crate::LifecycleManager::check_required_env`])
    /// and a diagnostic tool (called by `lightshuttle secrets check`).
    ///
    /// The `extra_env` argument carries the contents of the loaded `.env` file.
    /// Entries in `extra_env` with an empty string value are treated as unset.
    #[must_use]
    pub fn env_report(&self, extra_env: &HashMap<String, String>) -> EnvReport {
        let ctx = InterpolationContext::from_env()
            .with_env(extra_env.iter().map(|(k, v)| (k.clone(), v.clone())));
        let interpolator = Interpolator::new(&ctx);

        let mut by_name: BTreeMap<String, Aggregate> = BTreeMap::new();
        for node in self.nodes() {
            for value in node.spec.env.values() {
                collect_env_refs(&interpolator, value, &mut by_name);
            }
            if let Some(args) = &node.spec.command {
                for arg in args {
                    collect_env_refs(&interpolator, arg, &mut by_name);
                }
            }
        }

        let vars = by_name
            .into_iter()
            .map(|(name, agg)| {
                let status = classify(&interpolator, &name, &agg, extra_env);
                EnvVarReport { name, status }
            })
            .collect();

        EnvReport { vars }
    }
}

/// Scan `value` for `${env.*}` references and fold them into `by_name`.
fn collect_env_refs(
    interpolator: &Interpolator<'_>,
    value: &str,
    by_name: &mut BTreeMap<String, Aggregate>,
) {
    let Ok(refs) = interpolator.scan(value) else {
        return;
    };
    for reference in refs {
        if let Reference::Env { name, default } = reference {
            let agg = by_name.entry(name).or_default();
            match default {
                None => agg.required = true,
                Some(d) => {
                    agg.defaults.insert(d);
                }
            }
        }
    }
}

/// Decide the status of one variable, deferring the resolved-or-not call to
/// the interpolator so it never diverges from runtime resolution.
fn classify(
    interpolator: &Interpolator<'_>,
    name: &str,
    agg: &Aggregate,
    extra_env: &HashMap<String, String>,
) -> EnvVarStatus {
    let probe = format!("${{env.{name}}}");
    if interpolator.resolve(&probe).is_ok() {
        // `extra_env` overrides the ambient environment, so a non-empty
        // entry there is the value actually used; otherwise resolution can
        // only have come from the process environment.
        let source = if extra_env.get(name).is_some_and(|v| !v.is_empty()) {
            EnvSource::EnvFile
        } else {
            EnvSource::Process
        };
        EnvVarStatus::Resolved(source)
    } else if agg.required {
        EnvVarStatus::Missing
    } else {
        EnvVarStatus::Defaulted {
            defaults: agg.defaults.iter().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use lightshuttle_manifest::Manifest;

    use super::*;

    fn plan_with_env(token: &str, level: &str) -> LifecyclePlan {
        let yaml = format!(
            "project:\n  name: app\nresources:\n  app:\n    container:\n      image: myapp:latest\n      env:\n        API_TOKEN: \"{token}\"\n        LOG_LEVEL: \"{level}\"\n"
        );
        let manifest = Manifest::parse(&yaml).expect("valid manifest");
        LifecyclePlan::from_manifest(&manifest).expect("valid plan")
    }

    fn plan_with_raw_env(env_block: &str) -> LifecyclePlan {
        let yaml = format!(
            "project:\n  name: app\nresources:\n  app:\n    container:\n      image: myapp:latest\n      env:\n{env_block}"
        );
        let manifest = Manifest::parse(&yaml).expect("valid manifest");
        LifecyclePlan::from_manifest(&manifest).expect("valid plan")
    }

    fn status_of<'a>(report: &'a EnvReport, name: &str) -> &'a EnvVarStatus {
        &report
            .vars
            .iter()
            .find(|v| v.name == name)
            .expect("variable present")
            .status
    }

    #[test]
    fn env_file_value_resolves_with_env_file_source() {
        let plan = plan_with_env("${env.API_TOKEN}", "${env.LOG_LEVEL:-info}");
        let mut env = HashMap::new();
        env.insert("API_TOKEN".to_owned(), "secret".to_owned());
        let report = plan.env_report(&env);
        assert_eq!(
            status_of(&report, "API_TOKEN"),
            &EnvVarStatus::Resolved(EnvSource::EnvFile)
        );
    }

    #[test]
    fn unset_with_default_is_defaulted() {
        let plan = plan_with_env("${env.API_TOKEN}", "${env.LOG_LEVEL:-info}");
        let mut env = HashMap::new();
        env.insert("API_TOKEN".to_owned(), "secret".to_owned());
        let report = plan.env_report(&env);
        assert_eq!(
            status_of(&report, "LOG_LEVEL"),
            &EnvVarStatus::Defaulted {
                defaults: vec!["info".to_owned()]
            }
        );
    }

    #[test]
    fn empty_env_file_value_counts_as_missing() {
        let plan = plan_with_env("${env.API_TOKEN}", "${env.LOG_LEVEL:-info}");
        let mut env = HashMap::new();
        // Empty value overrides the ambient environment and is treated as
        // unset by the interpolator, so the required var stays missing.
        env.insert("API_TOKEN".to_owned(), String::new());
        let report = plan.env_report(&env);
        assert_eq!(status_of(&report, "API_TOKEN"), &EnvVarStatus::Missing);
        assert!(report.has_missing());
        assert_eq!(report.missing(), vec!["API_TOKEN".to_owned()]);
    }

    #[test]
    fn divergent_defaults_are_all_reported_sorted() {
        let plan = plan_with_raw_env(
            "        LOG_A: \"${env.LOG_LEVEL:-info}\"\n        LOG_B: \"${env.LOG_LEVEL:-debug}\"\n",
        );
        let report = plan.env_report(&HashMap::new());
        assert_eq!(
            status_of(&report, "LOG_LEVEL"),
            &EnvVarStatus::Defaulted {
                defaults: vec!["debug".to_owned(), "info".to_owned()]
            }
        );
    }
}
