//! Integration test: the Docker computer supervisor against a real daemon, end to end.
//!
//! The daemon must be reachable (BOMA_TEST_DOCKER_HOST or DOCKER_HOST, defaulting to the socket);
//! a missing daemon fails loudly, never silently skips. The computer's container name is derived
//! from a freshly minted id, so two runs never collide.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use boma_app::{App, ports::ComputerRuntime, usecases};
use boma_domain::{SpaceId, TenantContext};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, DockerComputerRuntime, IdGenerator, Postgres,
    PostgresAccountStore, PostgresBotStore, PostgresCardStore, PostgresComputerStore,
    PostgresEffectStore, PostgresEventStore, PostgresInvitationStore, PostgresMemoryStore,
    PostgresMessageStore, PostgresPendingToolCallStore, PostgresRateLimitStore,
    PostgresReactionStore, PostgresReadStateStore, PostgresRosterStore, PostgresRoutineStore,
    PostgresRunStore, PostgresScreenCapabilityStore, PostgresSecretStore, PostgresSessionStore,
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
            Arc::new(PostgresPendingToolCallStore::new(pool.clone())),
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool.clone())),
            Arc::new(PostgresRateLimitStore::new(pool.clone())),
            Arc::new(PostgresSecretStore::new(
                pool.clone(),
                boma_infra::SecretBox::ephemeral(),
            )),
            Arc::new(boma_infra::PostgresPluginSubmissionStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresSkillStore::new(pool.clone())),
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
    let outcome = usecases::ensure_computer(&app, &context, fixture.bot_id.as_str(), true)
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
    let outcome = usecases::ensure_computer(&app, &context, fixture.bot_id.as_str(), true)
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

#[tokio::test]
async fn req_028_the_computer_runs_a_shell_and_keeps_files_inside_the_home() {
    let fresh = fresh_database().await;
    let database = &fresh.database;

    let host = host();
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    match docker.ping().await {
        Ok(_) => {}
        Err(error) => panic!(
            "req_028 needs a reachable docker daemon (BOMA_TEST_DOCKER_HOST, DOCKER_HOST, or the default socket): {error}"
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

    let runtime =
        DockerComputerRuntime::new(&host, "alpine:3.21", None).expect("the runtime builds");
    let app = app(database, Arc::new(runtime));
    let fixture = fixture(database.pool(), "shell").await;
    let context = fixture.context.clone();

    // The computer is up: the first shell starts it.
    usecases::ensure_computer(&app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");

    // A shell runs inside the container and answers on stdout.
    let out = usecases::computer_shell(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "echo hello-from-the-box",
    )
    .await
    .expect("the shell answers");
    assert_eq!(out.trim(), "hello-from-the-box");

    // The output is capped; a hostile flood of bytes cannot drown the caller.
    let out = usecases::computer_shell(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "yes | head -c 131072",
    )
    .await
    .expect("the shell answers");
    assert!(out.len() <= 64 * 1024, "the shell output is capped");

    // Files written inside the home are readable back.
    usecases::computer_write_file(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "/home/bot/notes.txt",
        "boma home marker\n",
    )
    .await
    .expect("the file is written");
    let read = usecases::computer_read_file(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "/home/bot/notes.txt",
    )
    .await
    .expect("the file reads back");
    assert!(read.contains("boma home marker"));

    // A path outside the home is refused, wherever the caller asks from.
    let refused =
        usecases::computer_read_file(&app, &context, fixture.bot_id.as_str(), "/etc/passwd").await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));

    let rel = usecases::computer_write_file(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "../outside.txt",
        "x",
    )
    .await;
    assert!(matches!(rel, Err(boma_app::AppError::Rule(_))));

    cleanup(database.pool(), "shell").await;
    fresh.dispose().await;
}

#[tokio::test]
async fn req_029_a_member_holds_the_screen_and_the_computer_refuses_other_input() {
    let fresh = fresh_database().await;
    let database = &fresh.database;

    let host = host();
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    match docker.ping().await {
        Ok(_) => {}
        Err(error) => panic!(
            "req_029 needs a reachable docker daemon (BOMA_TEST_DOCKER_HOST, DOCKER_HOST, or the default socket): {error}"
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

    let runtime =
        DockerComputerRuntime::new(&host, "alpine:3.21", None).expect("the runtime builds");
    let app = app(database, Arc::new(runtime));
    let fixture = fixture(database.pool(), "takeover").await;
    let context = fixture.context.clone();
    let other = boma_domain::UserId::new("takeover_otherusr".to_owned()).expect("an id is valid");

    usecases::ensure_computer(&app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");

    // The hold takes place, and while it holds, the computer refuses to act.
    let held = usecases::computer_takeover(&app, &context, fixture.bot_id.as_str())
        .await
        .expect("the takeover answers");
    assert_eq!(
        held.takeover_by_user
            .as_ref()
            .map(boma_domain::UserId::as_str),
        Some(fixture.user_id.as_str())
    );

    let refused = usecases::computer_shell(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "echo should_not_run",
    )
    .await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));

    // A different member may not break the hold.
    let other_context = TenantContext::Member {
        space_id: space_id_of(&fixture),
        user_id: other,
    };
    let outsider = usecases::computer_takeover(&app, &other_context, fixture.bot_id.as_str()).await;
    assert!(matches!(
        outsider,
        Err(boma_app::AppError::NotFound
            | boma_app::AppError::Conflict
            | boma_app::AppError::ScreenTaken,)
    ));

    // The holder releases; the computer acts again.
    let released = usecases::computer_release(&app, &context, fixture.bot_id.as_str())
        .await
        .expect("the release answers");
    assert!(released.takeover_by_user.is_none());
    let out = usecases::computer_shell(
        &app,
        &context,
        fixture.bot_id.as_str(),
        "echo after_release",
    )
    .await
    .expect("the computer acts after release");
    assert_eq!(out.trim(), "after_release");

    cleanup(database.pool(), "takeover").await;
    fresh.dispose().await;
}

/// Finds the space id of a fixture, for a context acting as another member.
fn space_id_of(fixture: &common::Fixture) -> SpaceId {
    fixture
        .context
        .space_id()
        .cloned()
        .expect("the fixture has a member context")
}
