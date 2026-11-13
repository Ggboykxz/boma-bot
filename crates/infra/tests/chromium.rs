//! Integration test: the computer against a real headless Chromium over CDP.
//!
//! This is the provider conformance for observe-and-act: it exercises observe (the PNG frame),
//! DOM snapshot, navigate, click, type and scroll against a real chromium image booted in the
//! local docker daemon. The image is public (zenika/alpine-chrome); pulling it requires network
//! access, like every other integration image here.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use boma_app::usecases;
use boma_infra::DockerComputerRuntime;
use common::{cleanup, computer_app};
use futures_util::TryStreamExt;
mod common;

const IMAGE: &str = "zenika/alpine-chrome:latest";
const PAGE: &str = "data:text/html,<html><body>\
<button id=b style=\"position:absolute;top:10px;left:10px;width:120px;height:40px\" onclick=\"window.__clicks=(window.__clicks||0)+1\">HIT</button>\
<input id=f style=\"position:absolute;top:80px;left:10px\" value=\"\">\
<div style=\"height:5000px\">tall</div>\
<script>window.__clicks=0;</script>\
</body></html>";

/// The daemon must answer and the image be pullable; a failure here is a loud failure.
async fn pull_image(docker: &bollard::Docker) {
    match docker.ping().await {
        Ok(_) => {}
        Err(error) => panic!(
            "the chromium integration test needs a reachable docker daemon (BOMA_TEST_DOCKER_HOST, DOCKER_HOST, or the default socket): {error}"
        ),
    }
    let pull = docker
        .create_image(
            Some(bollard::query_parameters::CreateImageOptions {
                from_image: Some(IMAGE.to_owned()),
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await;
    pull.expect("the chromium image is pulled");
}

#[tokio::test]
async fn req_028_the_computer_observes_acts_and_reads_back_through_a_real_browser() {
    let host = std::env::var("BOMA_TEST_DOCKER_HOST")
        .or_else(|_| std::env::var("DOCKER_HOST"))
        .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_owned());
    let docker = bollard::Docker::connect_with_local_defaults().expect("the docker defaults build");
    pull_image(&docker).await;

    let runtime = Arc::new(
        DockerComputerRuntime::new(&host, IMAGE, None)
            .expect("the docker client builds against the socket")
            .with_command(vec![
                "--headless".to_owned(),
                "--no-sandbox".to_owned(),
                "--disable-gpu".to_owned(),
                "--remote-debugging-port=9222".to_owned(),
                "--remote-debugging-address=0.0.0.0".to_owned(),
                "about:blank".to_owned(),
            ])
            .with_published_cdp_port(9222),
    );
    let fixture = computer_app(runtime as Arc<_>, "chromium").await;
    let context = fixture.context.clone();

    // The computer boots and reports running.
    let booted = usecases::ensure_computer(&fixture.app, &context, fixture.bot_id.as_str(), true)
        .await
        .expect("the computer boots");
    assert!(booted.started);
    assert_eq!(booted.computer.status.as_str(), "running");

    // Chromium needs a moment to bring its debug endpoint up; poll until it answers.
    let mut observation = None;
    for _ in 0..10 {
        match usecases::computer_observe(&fixture.app, &context, fixture.bot_id.as_str()).await {
            Ok(observed) => {
                observation = Some(observed);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
    let observation = observation.expect("the observation answers");
    assert_eq!(&observation.png[..4], b"\x89PNG");

    // Navigate to a data page with a button and a tall div.
    usecases::computer_navigate(&fixture.app, &context, fixture.bot_id.as_str(), PAGE)
        .await
        .expect("the navigate answers");

    // The DOM snapshot shows the button; the observation reflects a real page.
    let snapshot = usecases::computer_dom_snapshot(&fixture.app, &context, fixture.bot_id.as_str())
        .await
        .expect("the DOM answers");
    assert!(snapshot.html.contains("id=\"b\""), "{}", snapshot.html);

    // Click the input to focus it, then type; the value reads back through the DOM.
    usecases::computer_click(&fixture.app, &context, fixture.bot_id.as_str(), 40.0, 90.0)
        .await
        .expect("the click answers");
    usecases::computer_type(&fixture.app, &context, fixture.bot_id.as_str(), "boma")
        .await
        .expect("the typing answers");
    let typed = usecases::computer_evaluate(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "document.querySelector('#f').value",
    )
    .await
    .expect("the value reads back");
    assert_eq!(typed.value.as_str(), Some("boma"));

    // Click the button; its handler increments the counter.
    usecases::computer_click(&fixture.app, &context, fixture.bot_id.as_str(), 40.0, 20.0)
        .await
        .expect("the click answers");
    let clicks = usecases::computer_evaluate(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "window.__clicks",
    )
    .await
    .expect("the click count reads back");
    assert_eq!(clicks.value.as_i64(), Some(1));

    // Scroll to the bottom; the observation reflects motion through the tall page.
    usecases::computer_scroll(&fixture.app, &context, fixture.bot_id.as_str(), 5000.0)
        .await
        .expect("the scroll answers");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let scroll_y = usecases::computer_evaluate(
        &fixture.app,
        &context,
        fixture.bot_id.as_str(),
        "window.scrollY",
    )
    .await
    .expect("the scroll position reads back");
    assert_eq!(
        scroll_y.value.as_f64().map(|y| y > 1000.0),
        Some(true),
        "scrollY = {}",
        scroll_y.value
    );

    cleanup(fixture.fresh.database.pool(), "chromium").await;
    fixture.fresh.dispose().await;
}
