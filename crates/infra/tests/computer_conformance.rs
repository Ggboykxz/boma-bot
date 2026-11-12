//! Provider conformance suite: every computer runtime must pass this battery.
//!
//! The tests here are the contract every provider in this deployment answers to: boot, reuse,
//! shell inside, confinement, takeover exclusion, capability sealing, release. They run against a
//! real rootless Docker daemon because that is the provider this deployment ships; a daemon that
//! cannot answer fails loudly, never silently skips.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use boma_app::usecases;
use common::{cleanup, computer_app};
mod common;

fn host() -> String {
    std::env::var("BOMA_TEST_DOCKER_HOST")
        .or_else(|_| std::env::var("DOCKER_HOST"))
        .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_owned())
}

async fn docker_ready(docker: &bollard::Docker) {
    match docker.ping().await {
        Ok(_) => {}
        Err(error) => panic!(
            "the computer conformance suite needs a reachable docker daemon (BOMA_TEST_DOCKER_HOST, DOCKER_HOST, or the default socket): {error}"
        ),
    }
    {
        use futures_util::TryStreamExt;
        let pull = docker
            .create_image(
                Some(bollard::query_parameters::CreateImageOptions {
                    from_image: Some("alpine:3.21".to_owned()),
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await;
        pull.expect("the image is pulled");
    }
}

fn docker_runtime() -> Arc<boma_infra::DockerComputerRuntime> {
    let host = host();
    Arc::new(
        boma_infra::DockerComputerRuntime::new(&host, "alpine:3.21", None)
            .expect("the docker client builds against the socket"),
    )
}

#[tokio::test]
async fn conformance_start_boots_and_is_idempotent() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf1").await;
    let context = fixture.context.clone();

    let booted = usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the first ensure boots");
    assert!(booted.started, "the first ensure starts the computer");
    assert_eq!(booted.computer.status.as_str(), "running");
    assert_eq!(booted.computer.screen_generation, 1);

    let reused = usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the second ensure reuses");
    assert!(!reused.started, "the second ensure does not reboot");
    assert_eq!(reused.computer.screen_generation, 1);

    usecases::stop_computer(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the stop answers");
    cleanup(fixture.fresh.database.pool(), "conf1").await;
    fixture.fresh.dispose().await;
}

#[tokio::test]
async fn conformance_shell_runs_inside_and_is_confined() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf2").await;
    let context = fixture.context.clone();

    usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");
    let out = usecases::computer_shell(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "echo conformance-ok",
    )
    .await
    .expect("the shell answers");
    assert_eq!(out.trim(), "conformance-ok");

    // The home is confined: a path outside it is refused.
    let refused = usecases::computer_read_file(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "/etc/passwd",
    )
    .await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));
    cleanup(fixture.fresh.database.pool(), "conf2").await;
    fixture.fresh.dispose().await;
}

#[tokio::test]
async fn conformance_release_re_enables_the_computer() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf3").await;
    let context = fixture.context.clone();

    usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");
    usecases::computer_takeover(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the takeover answers");
    let refused = usecases::computer_shell(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "echo should_not_run",
    )
    .await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));

    usecases::computer_release(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the release answers");
    let out = usecases::computer_shell(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "echo after-release",
    )
    .await
    .expect("the computer acts after release");
    assert_eq!(out.trim(), "after-release");

    cleanup(fixture.fresh.database.pool(), "conf3").await;
    fixture.fresh.dispose().await;
}

#[tokio::test]
async fn conformance_a_stale_screen_capability_generation_is_refused() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf4").await;
    let context = fixture.context.clone();

    usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the first boot answers");
    let issued = usecases::issue_screen_capability(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("a capability is issued");
    assert_eq!(issued.screen_generation, 1);

    usecases::stop_computer(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the stop answers");
    usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the second boot answers");

    // The restart moved the generation: the capability no longer opens the screen.
    match usecases::resolve_screen_capability(&fixture.app, &issued.token).await {
        Ok(Some(_)) => panic!("a stale capability must not resolve"),
        Ok(None) => {}
        Err(error) => panic!("the capability resolution errored: {error}"),
    }
    cleanup(fixture.fresh.database.pool(), "conf4").await;
    fixture.fresh.dispose().await;
}

#[tokio::test]
async fn conformance_idle_sweep_releases_the_computer() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf5").await;
    let context = fixture.context.clone();

    let booted = usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");
    let mut stale = booted.computer.clone();
    stale.last_used_at = Some("2025-12-31T00:00:00Z".to_owned());
    fixture
        .app
        .stores
        .computers
        .update(&context, &stale)
        .await
        .expect("the row is marked stale");
    let released = usecases::sweep_idle_computers(&fixture.app)
        .await
        .expect("the sweeper answers");
    assert_eq!(released, 1);
    let found = fixture
        .app
        .stores
        .computers
        .find(&context, &fixture.bot_id)
        .await
        .expect("the store answers")
        .expect("the computer persists");
    assert_eq!(found.status.as_str(), "stopped");

    cleanup(fixture.fresh.database.pool(), "conf5").await;
    fixture.fresh.dispose().await;
}

#[tokio::test]
async fn conformance_private_computers_do_not_see_each_other() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    docker_ready(&docker).await;
    let fixture = computer_app(docker_runtime(), "conf6").await;
    let context = fixture.context.clone();

    // A second bot in the same space: it gets its own computer, not the first bot's.
    let other_bot = boma_app::usecases::create_bot(&fixture.app, &context, "Other", None, None)
        .await
        .expect("the second bot is created");

    let mine = usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the private computer boots");
    let theirs = usecases::ensure_computer(&fixture.app, &context, other_bot.bot.id.as_str(), true)
        .await
        .expect("their computer boots");
    assert_ne!(mine.computer.id, theirs.computer.id);
    assert_ne!(
        mine.computer.external_id.as_deref(),
        theirs.computer.external_id.as_deref()
    );
    assert_ne!(
        mine.computer.home_id.as_deref(),
        theirs.computer.home_id.as_deref()
    );

    // The first bot still resolves to its own screen, not the second bot's.
    let first_read = usecases::computer_status(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the status answers")
        .expect("the computer is found");
    assert_eq!(first_read.id, mine.computer.id);

    cleanup(fixture.fresh.database.pool(), "conf6").await;
    fixture.fresh.dispose().await;
}
