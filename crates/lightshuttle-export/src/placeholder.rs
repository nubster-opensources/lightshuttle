//! Deployment-time placeholder rendering.
//!
//! `lower` resolves every manifest resource through `lightshuttle-spec`, but
//! it never interprets the `${...}` text carried by an image reference, a
//! command, a working directory, or an environment value: those strings stay
//! verbatim in the [`crate::ExportModel`]. This module is where that text
//! finally gets interpreted, once per export target.
//!
//! A `${resources.<name>.<property>}` reference is resolved eagerly against a
//! [`ResourceDirectory`], because its value depends on nothing but the
//! target's host naming (see the design's rendering table). A
//! `${env.<NAME>}` reference is not: the value is only known when the
//! deployment starts, so it survives as a variable that
//! [`PlaceholderRenderer::render`] renders in the syntax of one target
//! (Compose `${NAME}`, a Kubernetes refusal, or a Helm `.Values.variables.NAME`
//! lookup).
//!
//! Every emitter builds one [`ResourceDirectory`], then calls
//! [`DeploymentText::parse`] for every interpolatable field of every
//! resource, then renders the result through the [`PlaceholderRenderer`] that
//! matches its target.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lightshuttle_manifest::Manifest;
use lightshuttle_manifest::interpolate::{Segment, segments};
use lightshuttle_spec::{ResourceOutputs, SENSITIVE_OUTPUTS, from_resource_on_host};

use crate::Target;
use crate::error::{ExportError, Result};
use crate::resolve::dns_name;

/// One piece of a deployment text once resource references are resolved:
/// either text the target takes verbatim, or a variable the target renders
/// in its own syntax.
#[derive(Debug, Clone)]
enum TextPart {
    /// Text to reproduce as-is, modulo the target's own escaping rules.
    Literal(String),
    /// A `${env.<NAME>}` reference left for the deployment target, with its
    /// default already split into parts of its own.
    Variable {
        /// Environment variable name, validated against the shell-safe
        /// identifier grammar every target requires.
        name: String,
        /// Fallback used when the variable is unset, itself a deployment
        /// text so a reference nested in a default renders recursively.
        default: Option<Vec<TextPart>>,
    },
}

/// A deployment-time string: literal text and variables left for the target.
///
/// Built by [`DeploymentText::parse`], which splits `raw` into the same
/// segments `lightshuttle-manifest` uses for runtime interpolation, resolves
/// every `${resources.<name>.<property>}` reference against a
/// [`ResourceDirectory`], and keeps every `${env.<NAME>}` reference as a
/// variable for a [`PlaceholderRenderer`] to render.
#[derive(Debug, Clone)]
pub struct DeploymentText {
    /// Parts after resource resolution, in source order.
    parts: Vec<TextPart>,
    /// Resource this text was parsed from, named by the errors the
    /// renderers raise.
    resource: String,
    /// Whether a `${resources...}` reference was resolved away while
    /// parsing. The resolved value is target specific, so the text is not a
    /// target-independent literal even though it now reads like one.
    has_resolved_reference: bool,
    /// Whether a resolved reference carried a property listed in
    /// [`lightshuttle_spec::SENSITIVE_OUTPUTS`]. The environment key this
    /// text feeds becomes a secret (design decision D4).
    carries_sensitive_output: bool,
}

/// Where a text lands, used for diagnostics and the D4 refusal (a sensitive
/// resource property may only be referenced from an environment value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField<'a> {
    /// The `image` reference of a resource.
    Image,
    /// The `entrypoint` override of a resource.
    Entrypoint,
    /// The `command` override of a resource.
    Command,
    /// The `working_dir` override of a resource.
    WorkingDir,
    /// One entry of a resource's environment map.
    Env {
        /// The environment key this text is assigned to.
        key: &'a str,
    },
    /// The `healthcheck` command of a resource.
    Healthcheck,
    /// A Dockerfile build input (build arg or build context) of a resource.
    BuildInput,
}

impl TextField<'_> {
    /// Names this field the way the manifest does, for diagnostics.
    fn label(self) -> String {
        match self {
            Self::Image => "image".to_owned(),
            Self::Entrypoint => "entrypoint".to_owned(),
            Self::Command => "command".to_owned(),
            Self::WorkingDir => "working_dir".to_owned(),
            Self::Env { key } => format!("env.{key}"),
            Self::Healthcheck => "healthcheck".to_owned(),
            Self::BuildInput => "build".to_owned(),
        }
    }

    /// Returns `true` when this field is an environment value, the only
    /// place a sensitive resource property may be referenced from.
    fn is_env(self) -> bool {
        matches!(self, Self::Env { .. })
    }
}

/// Resource outputs addressed for one target, keyed by resource name.
///
/// Built once per export target by [`ResourceDirectory::for_target`], then
/// passed to every [`DeploymentText::parse`] call so a
/// `${resources.<name>.<property>}` reference resolves to the value the
/// target actually reaches a service through: the raw manifest name for
/// Compose, the generated DNS name for Kubernetes and Helm.
#[derive(Debug, Clone)]
pub struct ResourceDirectory {
    /// Resolved output properties, keyed by resource name.
    outputs: BTreeMap<String, ResourceOutputs>,
    /// Target the outputs were addressed for, named by the errors raised
    /// while parsing a text against this directory.
    target: Target,
}

impl ResourceDirectory {
    /// Builds the directory for `target` (host naming per the design's
    /// rendering table).
    ///
    /// Resolves every resource declared in `manifest` through
    /// [`lightshuttle_spec::from_resource_on_host`] with the hostname
    /// `target` reaches that resource through.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ExportError::Spec`] when a resource cannot be
    /// resolved, and [`crate::ExportError::Unsupported`] when a resource
    /// name has no DNS label for a target that addresses services by one.
    pub fn for_target(manifest: &Manifest, target: Target) -> Result<Self> {
        let mut outputs = BTreeMap::new();
        for (name, kind) in &manifest.resources {
            let host = match target {
                Target::Compose => name.clone(),
                Target::Kubernetes | Target::Helm => dns_name(name)?,
            };
            let resolved = from_resource_on_host(&manifest.project.name, name, kind, &host)
                .map_err(|source| ExportError::Spec {
                    resource: name.clone(),
                    source,
                })?;
            outputs.insert(name.clone(), resolved.outputs);
        }
        Ok(Self { outputs, target })
    }

    /// Looks up one output property of one resource.
    fn output(&self, resource: &str, name: &str, property: &str) -> Result<&str> {
        let outputs = self
            .outputs
            .get(name)
            .ok_or_else(|| self.unaddressable(resource, format!("unknown resource `{name}`")))?;
        outputs.get(property).map(String::as_str).ok_or_else(|| {
            self.unaddressable(
                resource,
                format!("resource `{name}` exposes no property `{property}`"),
            )
        })
    }

    /// Builds the refusal raised when a reference cannot be addressed.
    fn unaddressable(&self, resource: &str, reason: String) -> ExportError {
        ExportError::Unsupported {
            resource: resource.to_owned(),
            target: self.target.label(),
            reason,
        }
    }
}

/// Renders the source form of a resource reference, for diagnostics.
fn resource_reference(name: &str, property: &str) -> String {
    format!("${{resources.{name}.{property}}}")
}

/// Returns `true` when `name` matches the shell-safe identifier grammar
/// `[A-Za-z_][A-Za-z0-9_]*` that Compose interpolation and Helm value keys
/// both require.
fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Appends `text` to `parts`, merging it into the literal already there so a
/// resolved reference and the text around it form a single part.
fn push_literal(parts: &mut Vec<TextPart>, text: &str) {
    if let Some(TextPart::Literal(last)) = parts.last_mut() {
        last.push_str(text);
    } else {
        parts.push(TextPart::Literal(text.to_owned()));
    }
}

impl DeploymentText {
    /// Parses `raw`, resolves every resource reference against `directory`,
    /// and keeps every environment reference as a variable.
    ///
    /// `field` and `resource` are carried for diagnostics only: they name
    /// the offending field and resource in
    /// [`crate::ExportError::SensitiveReferenceOutsideEnv`] and
    /// [`crate::ExportError::InvalidVariableName`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::ExportError::SensitiveReferenceOutsideEnv`] when a
    /// credential-bearing property is referenced anywhere but an
    /// environment value, [`crate::ExportError::InvalidVariableName`] when a
    /// variable name is not a shell-safe identifier, and
    /// [`crate::ExportError::Unsupported`] when `raw` is not valid
    /// interpolation syntax or names a resource property that does not
    /// exist.
    pub fn parse(
        raw: &str,
        field: TextField<'_>,
        resource: &str,
        directory: &ResourceDirectory,
    ) -> Result<Self> {
        let parsed = segments(raw).map_err(|source| ExportError::Unsupported {
            resource: resource.to_owned(),
            target: directory.target.label(),
            reason: source.to_string(),
        })?;

        let mut text = Self {
            parts: Vec::new(),
            resource: resource.to_owned(),
            has_resolved_reference: false,
            carries_sensitive_output: false,
        };
        text.parts = text.convert(&parsed, field, directory)?;
        Ok(text)
    }

    /// Converts manifest segments into deployment parts, resolving resource
    /// references and validating variable names along the way.
    fn convert(
        &mut self,
        parsed: &[Segment],
        field: TextField<'_>,
        directory: &ResourceDirectory,
    ) -> Result<Vec<TextPart>> {
        let mut parts: Vec<TextPart> = Vec::new();

        for segment in parsed {
            match segment {
                Segment::Literal(text) => push_literal(&mut parts, text),
                Segment::Resource { name, property } => {
                    if SENSITIVE_OUTPUTS.contains(&property.as_str()) {
                        if !field.is_env() {
                            return Err(ExportError::SensitiveReferenceOutsideEnv {
                                resource: self.resource.clone(),
                                field: field.label(),
                                reference: resource_reference(name, property),
                            });
                        }
                        self.carries_sensitive_output = true;
                    }
                    let value = directory.output(&self.resource, name, property)?.to_owned();
                    self.has_resolved_reference = true;
                    push_literal(&mut parts, &value);
                }
                Segment::Env { name, default } => {
                    if !is_valid_variable_name(name) {
                        return Err(ExportError::InvalidVariableName {
                            resource: self.resource.clone(),
                            name: name.clone(),
                        });
                    }
                    let default = match default {
                        Some(inner) => Some(self.convert(inner, field, directory)?),
                        None => None,
                    };
                    parts.push(TextPart::Variable {
                        name: name.clone(),
                        default,
                    });
                }
            }
        }

        Ok(parts)
    }

    /// Variables referenced by this text, defaults included, in first-seen
    /// order and without duplicates.
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        collect_variables(&self.parts, &mut out);
        out
    }

    /// Returns `true` when this text carries no variable: it renders
    /// identically on every target.
    #[must_use]
    pub fn is_literal(&self) -> bool {
        !self.has_resolved_reference && self.variables().is_empty()
    }

    /// Returns `true` when this text resolved a credential-bearing resource
    /// property, so the environment key it feeds must be exported as a
    /// secret (design decision D4).
    pub(crate) fn carries_sensitive_output(&self) -> bool {
        self.carries_sensitive_output
    }
}

/// Walks `parts` in source order, appending every variable name not already
/// present. A variable surfaces before the variables nested in its own
/// default, mirroring how `lightshuttle-manifest` nests a default's segments
/// under the reference that carries it.
fn collect_variables<'a>(parts: &'a [TextPart], out: &mut Vec<&'a str>) {
    for part in parts {
        match part {
            TextPart::Literal(_) => {}
            TextPart::Variable { name, default } => {
                if !out.contains(&name.as_str()) {
                    out.push(name.as_str());
                }
                if let Some(inner) = default {
                    collect_variables(inner, out);
                }
            }
        }
    }
}

/// Renders a [`DeploymentText`] in the syntax of one export target.
///
/// One implementation per [`Target`]: [`ComposeRenderer`],
/// [`KubernetesRenderer`], and [`HelmRenderer`].
pub trait PlaceholderRenderer {
    /// Renders `text` in this renderer's target syntax.
    ///
    /// Returns [`crate::ExportError::UnresolvedVariables`] when `text`
    /// carries a variable with no default and this target refuses to export
    /// without one (Kubernetes).
    ///
    /// # Errors
    ///
    /// Returns [`crate::ExportError::UnresolvedVariables`] for a target that
    /// cannot defer a variable to deployment time.
    fn render(&self, text: &DeploymentText) -> Result<String>;
}

/// Renders a [`DeploymentText`] in Docker Compose interpolation syntax
/// (`${NAME}`, `${NAME:-default}`).
#[derive(Debug, Clone, Copy)]
pub struct ComposeRenderer;

/// Renders a [`DeploymentText`] for plain Kubernetes manifests: a variable
/// with a default is figured in place, a variable with no default is
/// refused.
#[derive(Debug, Clone, Copy)]
pub struct KubernetesRenderer;

/// Renders a [`DeploymentText`] for a Helm chart: a variable becomes a
/// `.Values.variables.NAME` lookup, wrapped in `required` or `default`
/// depending on whether the source carried a default.
#[derive(Debug, Clone, Copy)]
pub struct HelmRenderer;

impl PlaceholderRenderer for ComposeRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        Ok(compose_parts(&text.parts))
    }
}

/// Renders parts in Compose syntax. Literal text is escaped so that a `$`
/// the manifest meant literally survives Compose's own interpolation pass:
/// without it, `costs $5` is read as a reference and silently emptied.
fn compose_parts(parts: &[TextPart]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Literal(text) => out.push_str(&text.replace('$', "$$")),
            TextPart::Variable {
                name,
                default: None,
            } => {
                let _ = write!(out, "${{{name}}}");
            }
            TextPart::Variable {
                name,
                default: Some(inner),
            } => {
                let _ = write!(out, "${{{name}:-{}}}", compose_parts(inner));
            }
        }
    }
    out
}

impl PlaceholderRenderer for KubernetesRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        let mut missing: Vec<String> = Vec::new();
        collect_defaultless(&text.parts, &mut missing);
        if !missing.is_empty() {
            missing.sort_unstable();
            missing.dedup();
            return Err(ExportError::UnresolvedVariables {
                resource: text.resource.clone(),
                target: Target::Kubernetes.label(),
                variables: missing,
            });
        }
        Ok(kubernetes_parts(&text.parts))
    }
}

/// Collects every variable that plain Kubernetes cannot resolve: one with no
/// default at all, including inside the default of another variable, since a
/// frozen default is itself rendered into the manifest.
fn collect_defaultless(parts: &[TextPart], out: &mut Vec<String>) {
    for part in parts {
        match part {
            TextPart::Literal(_) => {}
            TextPart::Variable {
                name,
                default: None,
            } => out.push(name.clone()),
            TextPart::Variable {
                default: Some(inner),
                ..
            } => collect_defaultless(inner, out),
        }
    }
}

/// Renders parts for plain Kubernetes: every variable is replaced by its
/// default, since nothing substitutes at deploy time.
fn kubernetes_parts(parts: &[TextPart]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Literal(text) => out.push_str(text),
            TextPart::Variable { default, .. } => {
                if let Some(inner) = default {
                    out.push_str(&kubernetes_parts(inner));
                }
            }
        }
    }
    out
}

impl PlaceholderRenderer for HelmRenderer {
    fn render(&self, text: &DeploymentText) -> Result<String> {
        Ok(helm_parts(&text.parts))
    }
}

impl HelmRenderer {
    /// Renders `text` for splicing into a chart template, where Go's
    /// templater runs before any YAML parser.
    ///
    /// [`PlaceholderRenderer::render`] is right for a value that travels
    /// through `values.yaml`: YAML is parsed first there, then `tpl` runs
    /// over the parsed bytes, so escaping before serialisation is what makes
    /// the value come back intact. Under `templates/` the order is reversed,
    /// and a caller splicing into hand-written YAML has to settle quoting on
    /// the text as it will read *after* Go renders. So this form hands back
    /// the literal text unescaped, with every template action replaced by an
    /// opaque token: the caller quotes and escapes, then substitutes
    /// `actions` back once its decisions are made.
    pub(crate) fn shape(
        text: &DeploymentText,
        prefix: &str,
        actions: &mut BTreeMap<String, String>,
    ) -> String {
        let mut out = String::new();
        shape_parts(&text.parts, prefix, actions, &mut out);
        out
    }

    /// A token prefix that appears in none of `texts`, so substituting a
    /// token back can never rewrite text the manifest wrote.
    pub(crate) fn token_prefix(texts: &[&str]) -> String {
        let mut prefix = String::from("LSHPLACEHOLDER");
        while texts.iter().any(|text| text.contains(&prefix)) {
            prefix.push('X');
        }
        prefix
    }
}

/// Walks `parts`, appending literal text verbatim and registering one token
/// per template action.
fn shape_parts(
    parts: &[TextPart],
    prefix: &str,
    actions: &mut BTreeMap<String, String>,
    out: &mut String,
) {
    for part in parts {
        match part {
            TextPart::Literal(text) => out.push_str(text),
            TextPart::Variable { name, default } => {
                // The trailing underscore keeps a token from being a prefix
                // of another: without it, substituting `...1` first would
                // rewrite the head of `...10` and corrupt both.
                let token = format!("{prefix}{}_", actions.len());
                let action = format!("{{{{ {} }}}}", helm_expression(name, default.as_deref()));
                actions.insert(token.clone(), action);
                out.push_str(&token);
            }
        }
    }
}

/// Renders parts as Helm chart text: literal `{{` is escaped to the Go
/// template literal that renders back to `{{`, and every variable becomes a
/// `.Values.variables.NAME` action.
fn helm_parts(parts: &[TextPart]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Literal(text) => out.push_str(&escape_template_braces(text)),
            TextPart::Variable { name, default } => {
                let _ = write!(
                    out,
                    "{{{{ {} }}}}",
                    helm_expression(name, default.as_deref())
                );
            }
        }
    }
    out
}

/// Escapes every `{{` opener so Go's templater renders it back as a literal
/// `{{` instead of reading it as a template action.
///
/// Helm runs `text/template` over every file under `templates/`, and over
/// any value passed through `tpl`, before a YAML parser ever sees it: YAML
/// quoting alone does nothing against `{{`.
pub(crate) fn escape_template_braces(text: &str) -> String {
    text.replace("{{", "{{ \"{{\" }}")
}

/// Builds the Go template pipeline that yields one variable's value.
///
/// Written as an expression rather than a complete `{{ ... }}` action so it
/// composes: a nested default embeds its own expression inside the
/// surrounding `default` call, which a nested action could not do.
fn helm_expression(name: &str, default: Option<&[TextPart]>) -> String {
    match default {
        None => {
            format!("required \"variables.{name} is required\" .Values.variables.{name}")
        }
        Some(parts) => format!(
            ".Values.variables.{name} | default {}",
            helm_default_expression(parts)
        ),
    }
}

/// Builds the Go template expression that yields a default's value.
///
/// A default made of literal text is a quoted string. A default that itself
/// references variables is assembled with `printf`, the only sprig form that
/// concatenates a mix of literals and pipelines inside a single expression.
fn helm_default_expression(parts: &[TextPart]) -> String {
    if parts
        .iter()
        .all(|part| matches!(part, TextPart::Literal(_)))
    {
        let literal: String = parts
            .iter()
            .map(|part| match part {
                TextPart::Literal(text) => text.as_str(),
                TextPart::Variable { .. } => "",
            })
            .collect();
        return quote_go_string(&literal);
    }

    if let [TextPart::Variable { name, default }] = parts {
        return format!("({})", helm_expression(name, default.as_deref()));
    }

    let mut format_string = String::new();
    let mut arguments = String::new();
    for part in parts {
        match part {
            TextPart::Literal(text) => format_string.push_str(&text.replace('%', "%%")),
            TextPart::Variable { name, default } => {
                format_string.push_str("%s");
                let _ = write!(
                    arguments,
                    " ({})",
                    helm_expression(name, default.as_deref())
                );
            }
        }
    }
    format!("(printf {}{arguments})", quote_go_string(&format_string))
}

/// Quotes `value` as a Go template string literal.
fn quote_go_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
