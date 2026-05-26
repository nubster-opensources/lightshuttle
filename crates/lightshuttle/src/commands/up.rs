//! `lightshuttle up`.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use lightshuttle_control::{ControlServer, ControlState};
use lightshuttle_runtime::{DockerRuntime, LifecycleManager, LifecyclePlan};
use owo_colors::OwoColorize;
use tokio::sync::oneshot;
use tracing::{info, warn};

use super::{ExitOutcome, load_manifest};
use crate::control_url;

/// Boot the stack, supervise it, and stop it cleanly on signal.
pub(crate) async fn run(
    file: &Path,
    grace: Duration,
    control_port_override: Option<u16>,
) -> Result<ExitOutcome> {
    let manifest = load_manifest(file)?;
    let plan = LifecyclePlan::from_manifest(&manifest)?;
    let runtime = DockerRuntime::connect()?;
    let (manager, _events) = LifecycleManager::new(plan, runtime);

    let port = control_port_override
        .or_else(|| manifest.dashboard.as_ref().and_then(|d| d.port))
        .unwrap_or(0);
    let listener = ControlServer::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .context("failed to bind control plane socket")?;
    let local_addr = listener
        .local_addr()
        .context("failed to read control plane local addr")?;
    let url = format!("http://{local_addr}/");

    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    let url_path =
        control_url::write(&cwd, &url).context("failed to write .lightshuttle/control.url")?;

    println!(
        "{} {}",
        "LightShuttle dashboard ready at".green().bold(),
        url.cyan().bold()
    );

    let project = manifest.project.name.clone();
    let server = ControlServer::new(ControlState::new(project.clone()));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let _ = server
            .serve(listener, async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    info!(project = %project, "stack starting");
    let stack_result = manager.run_until_signal(grace).await;
    info!(project = %project, "stack stopped");

    let _ = shutdown_tx.send(());
    if let Err(join_err) = server_task.await {
        warn!(error = %join_err, "control plane task did not join cleanly");
    }
    if let Err(remove_err) = control_url::remove(&cwd) {
        warn!(
            error = %remove_err,
            path = %url_path.display(),
            "failed to remove discovery file",
        );
    }

    stack_result?;
    Ok(ExitOutcome::Success)
}
