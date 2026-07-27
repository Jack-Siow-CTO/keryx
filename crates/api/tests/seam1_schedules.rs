//! Seam 1 — Schedules: CRUD/pause/resume, fire with origin=schedule, durability, missed/double-fire.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ControlPlaneService, SessionStore};
use keryx_domain::{Policy, Principal, RunOrigin, ScheduleStatus};
use keryx_model::FakeModelProvider;
use keryx_storage::{InMemorySessionStore, SqliteSessionStore};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

fn harness() -> (
    axum::Router,
    Arc<InMemorySessionStore>,
    Arc<ControlPlane<InMemorySessionStore, FakeModelProvider>>,
) {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content("schedule fire ok"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));
    (app, store, control)
}

#[tokio::test]
async fn schedule_crud_pause_resume_delete() {
    let (app, _, _) = harness();
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "goal": "morning briefing",
                        "interval_secs": 60,
                        "next_fire_at": 1_000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body = body_json(create).await;
    assert_eq!(body["status"], "active");
    assert_eq!(body["goal"], "morning briefing");
    let id = body["id"].as_str().unwrap().to_string();

    let pause = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/pause"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(pause).await["status"], "paused");

    let resume = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/resume"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(resume).await["status"], "active");

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        body_json(list).await["schedules"].as_array().unwrap().len(),
        1
    );

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/delete"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(del).await["status"], "deleted");
}

#[tokio::test]
async fn tick_fires_run_with_origin_schedule_reduced_policy() {
    let (app, store, control) = harness();
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let schedule = control
        .create_schedule(principal, "do work".into(), 60, 1_000, None)
        .await
        .unwrap();
    assert!(Policy::for_origin(&RunOrigin::Schedule).allows_tool("read_file"));
    assert!(!Policy::for_origin(&RunOrigin::Schedule).allows_tool("write_file"));

    let tick = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 1_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick.status(), StatusCode::OK);
    let body = body_json(tick).await;
    let runs = body["started_runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["origin"], "schedule");
    assert_eq!(runs[0]["goal"], "do work");

    // Double-fire same second: no second start.
    let tick2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 1_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(body_json(tick2).await["started_runs"]
        .as_array()
        .unwrap()
        .is_empty());

    // Wait for run to finish so next fire can create another root.
    let run_id = runs[0]["id"].as_str().unwrap();
    for _ in 0..50 {
        let r = store
            .get_run(run_id.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let updated = store.get_schedule(schedule.id).await.unwrap().unwrap();
    assert_eq!(updated.last_fired_at, Some(1_000));
    assert!(updated.next_fire_at > 1_000);
}

#[tokio::test]
async fn missed_fire_advances_without_storm() {
    let mut s = keryx_domain::Schedule::new(
        keryx_domain::PrincipalId::new("p"),
        "g",
        60,
        100,
        vec!["read_file".into()],
    );
    // far past due
    s.record_fire(10_000);
    assert!(s.next_fire_at > 10_000 || s.next_fire_at == 10_000 + 60);
    // only one step — not a catch-up loop of many intervals
    assert!(s.next_fire_at <= 10_000 + 120);
}

#[tokio::test]
async fn schedule_survives_sqlite_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::with_fixed_content("x"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let schedule = control
        .create_schedule(principal, "persist".into(), 30, 500, None)
        .await
        .unwrap();
    let id = schedule.id;
    drop(control);
    drop(store);

    let reopened = SqliteSessionStore::open(dir.path()).unwrap();
    let got = reopened.get_schedule(id).await.unwrap().expect("row");
    assert_eq!(got.goal, "persist");
    assert_eq!(got.status, ScheduleStatus::Active);
    assert_eq!(got.interval_secs, 30);
}

#[tokio::test]
async fn unauthenticated_schedule_fail_closed() {
    let (app, _, _) = harness();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"goal":"x","interval_secs":1}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
