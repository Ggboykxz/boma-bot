//! Integration test: the Docker computer supervisor against a real daemon, end to end.
//!
//! The daemon must be reachable (BOMA_TEST_DOCKER_HOST or DOCKER_HOST, defaulting to the socket);
//! a missing daemon fails loudly, never silently skips. The computer's container name is derived
//! from a freshly minted id, so two runs never collide.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use boma_app::{App, ports::ComputerRuntime, usecases};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, DockerComputerRuntime, IdGenerator, Postgres,
    PostgresAccountStore, PostgresBotStore, PostgresCardStore, PostgresComputerStore,
    PostgresEffectStore, PostgresEventStore, PostgresInvitationStore, PostgresMemoryStore,
    PostgresMessageStore, PostgresReactionStore, PostgresReadStateStore, PostgresRosterStore,
    PostgresRoutineStore, PostgresRunStore, PostgresScreenCapabilityStore, PostgresSessionStore,
    PostgresSpaceStore, PostgresThreadStore, PostgresToolStore, PostgresUserStore, SystemClock,
    TokenGenerator,
};
use common::{cleanup, fixture, fresh_database};

mod common;

/// Builds the app on the fresh database, with the docker runtime attached.
fn app(database: &Postgres, runtime: Arc<dyn ComputerRuntime>) -> App {
    let pool = database.pool().clone();
    let publisher = Arc::new(BroadcastPublisher::default());
    let services = boma_app::Services::new(
        publisher,
        Arc::new(Argon2idHasher::default()),
        Arc::new(TokenGenerator),
        Arc::new(IdGenerator),
        Arc::new(SystemClock),
        boma_domain::model::DeploymentModels {
            reachable: vec![
                "openai/gpt-5".to_owned(),
                "anthropic/claude-sonnet".to_owned(),
            ],
            default: "anthropic/claude-sonnet".to_owned(),
        },
    )
    .with_computers(runtime);
    App::new(
        boma_app::Stores::new(
            Arc::new(PostgresAccountStore::new(pool.clone())),
            Arc::new(PostgresUserStore::new(pool.clone())),
            Arc::new(PostgresSessionStore::new(pool.clone())),
            Arc::new(PostgresSpaceStore::new(pool.clone())),
            Arc::new(PostgresBotStore::new(pool.clone())),
            Arc::new(PostgresThreadStore::new(pool.clone())),
            Arc::new(PostgresMessageStore::new(pool.clone())),
            Arc::new(PostgresRunStore::new(pool.clone())),
            Arc::new(PostgresEventStore::new(pool.clone())),
            Arc::new(PostgresInvitationStore::new(pool.clone())),
            Arc::new(PostgresCardStore::new(pool.clone())),
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool.clone())),
            Arc::new(PostgresReactionStore::new(pool.clone())),
            Arc::new(PostgresReadStateStore::new(pool.clone())),
            Arc::new(PostgresRosterStore::new(pool.clone())),
            Arc::new(PostgresMemoryStore::new(pool.clone())),
            Arc::new(PostgresRoutineStore::new(pool.clone())),
            Arc::new(PostgresComputerStore::new(pool.clone())),
            Arc::new(PostgresScreenCapabilityStore::new(pool)),
        ),
        services,
    )
}

/// The daemon host: env var, else DOCKER_HOST, else the default socket.
fn host() -> String {
    std::env::var("BOMA_TEST_DOCKER_HOST")
        .or_else(|_| std::env::var("DOCKER_HOST"))
        .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_owned())
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // one linear boot-to-release scenario; splitting it loses the story
async fn req_026_a_computer_boots_stops_and_its_home_survives_in_docker() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
    let fresh = fresh_database().await;
    let database = &fresh.database;

    // The daemon must answer; a missing one is a loud failure, never a silent skip.
    let host = host();
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    match docker.ping().await {
        Ok(_) => {}
        Err(error) => panic!(
            "req_026 needs a reachable docker daemon (BOMA_TEST_DOCKER_HOST, DOCKER_HOST, or the default socket): {error}"
        ),
    }
    // The image the computer boots from; pulled here so the test is self-sufficient.
    let image = "alpine:3.21";
    {
        use futures_util::TryStreamExt;
        let pull = docker
            .create_image(
                Some(bollard::query_parameters::CreateImageOptions {
                    from_image: Some(image.to_owned()),
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await;
        pull.expect("the image is pulled");
    }

    let runtime = DockerComputerRuntime::new(&host, image, None).expect("the runtime builds");
    let app = app(database, Arc::new(runtime));
    let fixture = fixture(database.pool(), "computers").await;
    let context = fixture.context.clone();

    // The first ensure boots the computer; the handle comes back running.
    let outcome = usecases::ensure_computer(&app, &context, fixture.bot_id.as_str())
        .await
        .expect("the computer boots");
    assert!(outcome.started);
    assert_eq!(outcome.computer.status.as_str(), "running");
    assert_eq!(outcome.computer.screen_generation, 1);
    let external_id = outcome
        .computer
        .external_id
        .clone()
        .expect("the handle exists");
    let home_id = outcome.computer.home_id.clone().expect("the home exists");
    assert!(external_id.len() > 8);
    assert!(home_id.starts_with("boma-home-"));

    // The container is really running.
    let inspection = docker
        .inspect_container(
            &external_id,
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .expect("the container answers");
    assert_eq!(inspection.state.and_then(|state| state.running), Some(true));

    // The home volume exists; the bot's files outlive the container.
    let volumes = docker
        .list_volumes(Some(bollard::query_parameters::ListVolumesOptions {
            filters: Some(std::collections::HashMap::from([(
                "name".to_owned(),
                vec![home_id.clone()],
            )])),
        }))
        .await
        .expect("the volume list answers");
    assert_eq!(volumes.volumes.map(|volumes| volumes.len()), Some(1));

    // A second ensure reuses the computer: same container, nothing rebooted.
    let outcome = usecases::ensure_computer(&app, &context, fixture.bot_id.as_str())
        .await
        .expect("the computer is reused");
    assert!(!outcome.started);
    assert_eq!(
        outcome.computer.external_id.as_deref(),
        Some(external_id.as_str())
    );
    assert_eq!(outcome.computer.screen_generation, 1);

    // The sweeper releases it past the idle window; the home survives.
    let mut stale = outcome.computer.clone();
    stale.last_used_at = Some("2025-12-31T00:00:00Z".to_owned());
    app.stores
        .computers
        .update(&context, &stale)
        .await
        .expect("the row is marked stale");
    let released = usecases::sweep_idle_computers(&app)
        .await
        .expect("the sweeper answers");
    assert_eq!(released, 1);
    let inspection = docker
        .inspect_container(
            &external_id,
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .expect("the container answers");
    assert_eq!(
        inspection.state.and_then(|state| state.running),
        Some(false)
    );
    let volumes = docker
        .list_volumes(Some(bollard::query_parameters::ListVolumesOptions {
            filters: Some(std::collections::HashMap::from([(
                "name".to_owned(),
                vec![home_id.clone()],
            )])),
        }))
        .await
        .expect("the volume list answers");
    assert_eq!(volumes.volumes.map(|volumes| volumes.len()), Some(1));

    // The released computer row reports stopped, with no timestamps.
    let found = app
        .stores
        .computers
        .find(&context, &fixture.bot_id)
        .await
        .expect("the row is read")
        .expect("the row persists");
    assert_eq!(found.status.as_str(), "stopped");
    assert!(found.started_at.is_none());

    // Cleanup: remove the container and its home volume, then the tenant.
    let _ = docker
        .remove_container(
            &external_id,
            Some(bollard::query_parameters::RemoveContainerOptions {
                v: true,
                force: true,
                link: false,
            }),
        )
        .await;
    let _ = docker
        .remove_volume(
            &home_id,
            Some(bollard::query_parameters::RemoveVolumeOptions { force: true }),
        )
        .await;
    cleanup(database.pool(), "computers").await;
    fresh.dispose().await;
}
