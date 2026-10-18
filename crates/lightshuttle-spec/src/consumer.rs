//! Rendering a resource's outputs from the point of view of what consumes them.

use lightshuttle_manifest::ResourceKind;

use crate::error::{Result, SpecError};
use crate::spec::{ResourceOutputs, from_resource, from_resource_on_host};

/// Hostname through which a container reaches the machine hosting it, and
/// therefore the native processes running there.
///
/// Docker Desktop resolves it on its own; on a bare Linux engine the runtime
/// makes it resolvable by setting `host.docker.internal:host-gateway` on every
/// container it starts.
pub const HOST_GATEWAY_NAME: &str = "host.docker.internal";

/// Address through which a native process reaches every resource of its
/// project.
///
/// Always the loopback, never the project gateway and never `0.0.0.0`.
/// Measured on 2026-09-18 and recorded in `docs/design/process-networking.md`:
/// the loopback is the only address that works on both Docker Desktop and a
/// Linux engine without publishing a development service to the local network.
pub const LOOPBACK_ADDRESS: &str = "127.0.0.1";

/// What kind of resource is reading a `${resources.<name>.<property>}`
/// reference.
///
/// Introduced because a reference stops having one value. A container and a
/// native process do not reach the same service by the same address, so the
/// same reference in the same manifest renders differently depending on which
/// side asks. That is a real loss of simplicity, accepted deliberately: the
/// alternative was to publish development services on the local network, or
/// to make one of the two sides unable to reach the other at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerKind {
    /// A container attached to the project network.
    Container,
    /// A process running natively on the developer's machine.
    Process,
}

/// Outputs of `resource_name` as `consumer` would see them.
///
/// The four cases, which are the network design of #171 made executable:
///
/// | Consumer | Target | Host rendered |
/// |---|---|---|
/// | container | container | the resource name, resolved by the bridge DNS |
/// | container | process | `host.docker.internal` |
/// | process | container | `127.0.0.1` and the port published on the host |
/// | process | process | `127.0.0.1` and the process port |
///
/// Note what this is not: [`crate::from_resource_on_host`] answers a
/// different question, the hostname a *deployment target* reaches a service
/// through, and belongs to the export pipeline. Local execution and export
/// must not share one notion of "host": they disagree on every line above.
///
/// # Errors
///
/// Returns a [`crate::SpecError`] when the declaration is structurally
/// invalid, or when the target is a process declaring no port and the
/// reference therefore has no address to render.
pub fn outputs_for_consumer(
    project: &str,
    resource_name: &str,
    kind: &ResourceKind,
    consumer: ConsumerKind,
) -> Result<ResourceOutputs> {
    // Matched on the kind first, and exhaustively, so that a fifth resource
    // kind makes the compiler list this table among the places that must
    // decide what address it exposes.
    match kind {
        ResourceKind::Process(config) => {
            let host = match consumer {
                ConsumerKind::Container => HOST_GATEWAY_NAME,
                ConsumerKind::Process => LOOPBACK_ADDRESS,
            };
            process_outputs(resource_name, host, config.port)
        }

        // A managed resource publishes its declared port on the host under the
        // same number, so the `port` output already names a port a native
        // process can reach. Only the host changes.
        ResourceKind::Postgres(_) | ResourceKind::Redis(_) => match consumer {
            ConsumerKind::Container => Ok(from_resource(project, resource_name, kind)?.outputs),
            ConsumerKind::Process => {
                Ok(from_resource_on_host(project, resource_name, kind, LOOPBACK_ADDRESS)?.outputs)
            }
        },

        // A container's own `ports` output holds *container* ports, which a
        // native process cannot reach: it is not on the project network. The
        // published host ports replace them, and a container publishing none
        // is refused rather than rendered as an address that resolves to
        // nothing.
        ResourceKind::Container(_) | ResourceKind::Dockerfile(_) => match consumer {
            ConsumerKind::Container => Ok(from_resource(project, resource_name, kind)?.outputs),
            ConsumerKind::Process => {
                let resolved =
                    from_resource_on_host(project, resource_name, kind, LOOPBACK_ADDRESS)?;
                let host_ports: Vec<u16> = resolved
                    .spec
                    .as_container()
                    .map(|spec| spec.ports.iter().map(|port| port.host_port).collect())
                    .unwrap_or_default();
                let Some(first) = host_ports.first() else {
                    return Err(SpecError::NoPublishedPort {
                        resource: resource_name.to_owned(),
                    });
                };
                let mut outputs = resolved.outputs;
                outputs.insert("port".to_owned(), first.to_string());
                outputs.insert(
                    "ports".to_owned(),
                    host_ports
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
                Ok(outputs)
            }
        },
    }
}

/// Outputs of a `process` resource seen from `host`.
///
/// No `url` output, deliberately. A process declares a port, never a
/// protocol: rendering `http://` would be a guess, and a guess that lands in
/// an environment variable is discovered by the service that reads it, not by
/// the developer who wrote the manifest. A `${resources.<name>.url}` reference
/// to a process is refused as an unknown property instead.
///
/// # Errors
///
/// Returns [`SpecError::ProcessWithoutPort`] when the process declares no
/// port, because there is then no address to render.
pub(crate) fn process_outputs(
    resource_name: &str,
    host: &str,
    port: Option<u16>,
) -> Result<ResourceOutputs> {
    let port = port.ok_or_else(|| SpecError::ProcessWithoutPort {
        resource: resource_name.to_owned(),
    })?;
    let mut outputs = ResourceOutputs::new();
    outputs.insert("host".to_owned(), host.to_owned());
    outputs.insert("port".to_owned(), port.to_string());
    Ok(outputs)
}
