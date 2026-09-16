//! The placeholder pass: the stage between lowering and emission.
//!
//! Lowering is target agnostic, so it leaves every `${...}` expression in
//! place. Emission is target specific but structural: it decides which file
//! a value lands in, not what the value says. This pass sits between the
//! two and is the only place that reads the interpolation grammar during an
//! export.
//!
//! It walks every interpolatable field of every enabled resource exactly
//! once, and that single walk serves three purposes at the same time
//! (design decision 7):
//!
//! 1. it renders the text in the target's own syntax, through the matching
//!    [`PlaceholderRenderer`];
//! 2. it marks as secret any environment key that received a
//!    credential-bearing resource property, and refuses the export when such
//!    a property is referenced anywhere else (decision D4);
//! 3. it collects the variable names a Helm chart has to declare under
//!    `values.yaml`'s `variables:` key.
//!
//! Splitting those three across two walks would mean two readers of the same
//! rule, hence two places for it to drift.

use std::collections::{BTreeMap, BTreeSet};

use lightshuttle_spec::{ContainerSpec, ImageSource};

use crate::error::{ExportError, Result};
use crate::model::{ExportModel, Target};
use crate::placeholder::{
    ComposeRenderer, DeploymentText, HelmRenderer, KubernetesRenderer, PlaceholderRenderer,
    ResourceDirectory, TextField,
};
use crate::resolve::enabled_for;

/// One service with every text rendered for a single target.
pub(crate) struct RenderedService {
    /// The lowered specification, with every interpolatable field rendered
    /// and [`ContainerSpec::secret_env_keys`] extended by the D4
    /// propagation.
    pub(crate) spec: ContainerSpec,
    /// Names of the resources this service depends on, carried through from
    /// the lowered model.
    pub(crate) depends_on: Vec<String>,
    /// Whether the rendered image reference carries a deployment variable.
    ///
    /// Helm reads it to decide between publishing `repository`/`tag` and
    /// publishing the whole reference under one key: an image holding a
    /// variable cannot be split, because the analyser cannot tell a missing
    /// tag from a tag that is still a placeholder.
    pub(crate) image_has_variables: bool,
    /// Environment keys whose rendered value carries a deployment variable.
    ///
    /// Helm reads it to decide whether the chart has to run the service's
    /// environment through `tpl`.
    pub(crate) env_with_variables: BTreeSet<String>,
    /// Template actions held back from the fields a chart splices into
    /// hand-written YAML, keyed by the token standing in for each.
    ///
    /// Empty for every target but Helm. See [`HelmRenderer::shape`] for why
    /// those fields cannot carry their actions inline.
    pub(crate) template_actions: BTreeMap<String, String>,
}

/// Every enabled service of a model, rendered for a single target.
pub(crate) struct RenderedModel {
    /// Services in lowered order, filtered to the ones enabled for the
    /// target.
    pub(crate) services: Vec<RenderedService>,
    /// Every variable name referenced anywhere in `services`, sorted.
    pub(crate) variables: BTreeSet<String>,
}

/// Renders every enabled service of `model` for `target`.
///
/// # Errors
///
/// Returns whichever refusal the pass raises first: an unresolvable
/// reference, a sensitive property outside an environment value, an invalid
/// variable name, or (on Kubernetes) variables with no default.
pub(crate) fn render_for_target(model: &ExportModel, target: Target) -> Result<RenderedModel> {
    let directory = ResourceDirectory::for_target(&model.manifest, target)?;
    let renderer: &dyn PlaceholderRenderer = match target {
        Target::Compose => &ComposeRenderer,
        Target::Kubernetes => &KubernetesRenderer,
        Target::Helm => &HelmRenderer,
    };

    let mut variables = BTreeSet::new();
    let mut services = Vec::new();
    for service in &model.services {
        if !enabled_for(target, &service.spec.resource, model.export.as_ref()) {
            continue;
        }
        let mut spec = service.spec.clone();
        let token_prefix = if target == Target::Helm {
            let texts = spliced_texts(&spec);
            let borrowed: Vec<&str> = texts.iter().map(String::as_str).collect();
            Some(HelmRenderer::token_prefix(&borrowed))
        } else {
            None
        };
        let mut pass = ServicePass {
            resource: service.spec.resource.clone(),
            target,
            directory: &directory,
            renderer,
            variables: &mut variables,
            token_prefix,
            template_actions: BTreeMap::new(),
            unresolved: BTreeSet::new(),
            unresolved_target: None,
        };
        let image_has_variables = pass.render_image(&mut spec)?;
        let env_with_variables = pass.render_env(&mut spec)?;
        pass.render_argv(&mut spec)?;
        let template_actions = pass.into_unresolved()?;
        services.push(RenderedService {
            spec,
            depends_on: service.depends_on.clone(),
            image_has_variables,
            env_with_variables,
            template_actions,
        });
    }

    Ok(RenderedModel {
        services,
        variables,
    })
}

/// Every text of `spec` that an emitter splices into hand-written YAML
/// rather than handing to a serialiser.
fn spliced_texts(spec: &ContainerSpec) -> Vec<String> {
    let mut texts = Vec::new();
    texts.extend(spec.entrypoint.iter().flatten().cloned());
    texts.extend(spec.command.iter().flatten().cloned());
    texts.extend(spec.working_dir.iter().cloned());
    if let Some(healthcheck) = &spec.healthcheck {
        texts.extend(healthcheck.test.iter().cloned());
    }
    texts
}

/// The pass applied to one service, holding what every field render needs.
struct ServicePass<'a> {
    /// Resource being rendered, named by every refusal it raises.
    resource: String,
    /// Target being rendered.
    target: Target,
    /// Resource outputs addressed for the target being rendered.
    directory: &'a ResourceDirectory,
    /// Renderer of the target being rendered.
    renderer: &'a dyn PlaceholderRenderer,
    /// Accumulator of every variable name seen across the whole model.
    variables: &'a mut BTreeSet<String>,
    /// Token prefix reserved for the spliced fields, set on Helm only.
    token_prefix: Option<String>,
    /// Template actions held back from the spliced fields, keyed by token.
    template_actions: BTreeMap<String, String>,
    /// Variables this target refuses to export without, gathered across
    /// every field of the resource rather than raised on the first one.
    unresolved: BTreeSet<String>,
    /// Target that refused them, kept so the aggregated refusal names it.
    unresolved_target: Option<&'static str>,
}

impl ServicePass<'_> {
    /// Parses one text, recording the variables it references.
    fn parse(&mut self, raw: &str, field: TextField<'_>) -> Result<DeploymentText> {
        let text = DeploymentText::parse(raw, field, &self.resource, self.directory)?;
        for variable in text.variables() {
            self.variables.insert(variable.to_owned());
        }
        Ok(text)
    }

    /// Renders one text that reaches its target through a serialiser,
    /// given whether the target will run a template engine over it.
    ///
    /// The distinction exists for Helm alone, and it is not cosmetic. Helm
    /// escapes a literal `{{` to `{{ "{{" }}`, and that escape is itself a
    /// template action: it comes back as a literal `{{` only because
    /// something renders it. A chart runs `tpl` over a value exactly when
    /// that value has a placeholder to substitute, so escaping a value
    /// nothing will render would not protect it, it would corrupt it. The
    /// two decisions are therefore taken on one predicate rather than two.
    fn render_value(&mut self, text: &DeploymentText, templated: bool) -> Result<String> {
        if self.target == Target::Helm
            && !templated
            && let Some(plain) = text.as_plain_text()
        {
            return Ok(plain);
        }
        self.capture_unresolved(self.renderer.render(text))
    }

    /// Parses and renders one text, recording the variables it references.
    ///
    /// Returns the rendered text and the parsed form, which carries the D4
    /// marking the caller needs for an environment value.
    fn render(&mut self, raw: &str, field: TextField<'_>) -> Result<(String, DeploymentText)> {
        let text = self.parse(raw, field)?;
        let templated = !text.variables().is_empty();
        let rendered = self.render_value(&text, templated)?;
        Ok((rendered, text))
    }

    /// Renders one text destined for a field the emitter splices into
    /// hand-written YAML.
    ///
    /// On Helm that is not the same rendering as everywhere else: the chart
    /// gets the literal shape plus a token per template action, so the
    /// emitter can settle YAML quoting before the actions go back in.
    fn render_spliced(&mut self, raw: &str, field: TextField<'_>) -> Result<String> {
        let text = self.parse(raw, field)?;
        match &self.token_prefix {
            Some(prefix) => Ok(HelmRenderer::shape(
                &text,
                prefix,
                &mut self.template_actions,
            )),
            None => self.capture_unresolved(self.renderer.render(&text)),
        }
    }

    /// Holds back a target's refusal to export a variable with no default,
    /// so the resource is walked to the end before the refusal is raised.
    ///
    /// Reporting the first field that comes up short would make fixing a
    /// manifest an exercise in re-running the export: a resource that
    /// references three undeclared variables would take three passes to
    /// fix, each one revealing exactly one more. The remaining fields are
    /// still rendered, and the text they produce is discarded by
    /// [`Self::into_unresolved`] raising instead of returning.
    fn capture_unresolved(&mut self, rendered: Result<String>) -> Result<String> {
        match rendered {
            Err(ExportError::UnresolvedVariables {
                target, variables, ..
            }) => {
                self.unresolved_target = Some(target);
                self.unresolved.extend(variables);
                Ok(String::new())
            }
            other => other,
        }
    }

    /// The single refusal owed for this resource, once every field has been
    /// walked, or `Ok(())` when every variable was resolvable.
    fn into_unresolved(self) -> Result<BTreeMap<String, String>> {
        if let Some(target) = self.unresolved_target {
            return Err(ExportError::UnresolvedVariables {
                resource: self.resource,
                target,
                variables: self.unresolved.into_iter().collect(),
            });
        }
        Ok(self.template_actions)
    }

    /// Renders every spliced text in place inside `values`.
    fn render_each(&mut self, values: &mut [String], field: TextField<'_>) -> Result<()> {
        for value in values {
            *value = self.render_spliced(value, field)?;
        }
        Ok(())
    }

    /// Renders the image reference and every build input.
    ///
    /// Returns whether the rendered image reference carries a variable.
    fn render_image(&mut self, spec: &mut ContainerSpec) -> Result<bool> {
        match &mut spec.image {
            ImageSource::Pull(image) => {
                let (rendered, text) = self.render(image, TextField::Image)?;
                *image = rendered;
                Ok(!text.variables().is_empty())
            }
            ImageSource::Build {
                context,
                dockerfile,
                build_args,
                target,
                tag,
            } => {
                let (rendered_tag, text) = self.render(tag, TextField::Image)?;
                *tag = rendered_tag;
                let (rendered_context, _) = self.render(context, TextField::BuildInput)?;
                *context = rendered_context;
                let (rendered_dockerfile, _) = self.render(dockerfile, TextField::BuildInput)?;
                *dockerfile = rendered_dockerfile;
                if let Some(stage) = target {
                    let (rendered_stage, _) = self.render(stage, TextField::BuildInput)?;
                    *stage = rendered_stage;
                }
                for value in build_args.values_mut() {
                    let (rendered, _) = self.render(value, TextField::BuildInput)?;
                    *value = rendered;
                }
                Ok(!text.variables().is_empty())
            }
        }
    }

    /// Renders every environment value and applies the D4 propagation.
    ///
    /// Returns the keys whose rendered value still carries a variable.
    ///
    /// The whole map is parsed before any of it is rendered, because a
    /// chart templates a service's environment as a block: one key holding
    /// a placeholder puts every value of that service through `tpl`, and
    /// the escaping has to follow that same decision rather than be taken
    /// key by key.
    fn render_env(&mut self, spec: &mut ContainerSpec) -> Result<BTreeSet<String>> {
        let mut with_variables = BTreeSet::new();
        let mut rendered_env = BTreeMap::new();
        let mut secret_keys = BTreeSet::new();

        let mut keys: Vec<String> = spec.env.keys().cloned().collect();
        keys.sort();

        let mut parsed = Vec::with_capacity(keys.len());
        for key in keys {
            let Some(raw) = spec.env.get(&key).cloned() else {
                continue;
            };
            let text = self.parse(&raw, TextField::Env { key: &key })?;
            if text.carries_sensitive_output() {
                secret_keys.insert(key.clone());
            }
            if !text.variables().is_empty() {
                with_variables.insert(key.clone());
            }
            parsed.push((key, text));
        }

        let templated = !with_variables.is_empty();
        for (key, text) in parsed {
            let rendered = self.render_value(&text, templated)?;
            rendered_env.insert(key, rendered);
        }

        for (key, value) in rendered_env {
            spec.env.insert(key, value);
        }
        spec.secret_env_keys.extend(secret_keys);
        Ok(with_variables)
    }

    /// Renders the entrypoint, the command, the working directory and the
    /// healthcheck command.
    fn render_argv(&mut self, spec: &mut ContainerSpec) -> Result<()> {
        if let Some(entrypoint) = &mut spec.entrypoint {
            self.render_each(entrypoint, TextField::Entrypoint)?;
        }
        if let Some(command) = &mut spec.command {
            self.render_each(command, TextField::Command)?;
        }
        if let Some(working_dir) = &mut spec.working_dir {
            *working_dir = self.render_spliced(working_dir, TextField::WorkingDir)?;
        }
        if let Some(healthcheck) = &mut spec.healthcheck {
            self.render_each(&mut healthcheck.test, TextField::Healthcheck)?;
        }
        Ok(())
    }
}
