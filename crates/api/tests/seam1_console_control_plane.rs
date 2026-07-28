//! Seam 1 — Console control-plane expansions: inbox, memory, skills, artifacts.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router_with_artifact_put, AppState, OperatorTokenTable};
use keryx_app::ControlPlane;
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

fn harness() -> axum::Router {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content("ok"));
    let control = Arc::new(ControlPlane::new(store, model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let tmp = tempfile::tempdir().unwrap();
    let skills = tmp.path().join("skills");
    let arts = tmp.path().join("artifacts");
    std::fs::create_dir_all(skills.join("demo")).unwrap();
    std::fs::write(
        skills.join("demo").join("SKILL.md"),
        "# Demo skill\nDo things.\n",
    )
    .unwrap();
    // leak tempdir for test process lifetime
    let skills = Box::leak(Box::new(skills));
    let arts = Box::leak(Box::new(arts));
    let state =
        AppState::new(control, tokens).with_console_paths(Some(skills.clone()), Some(arts.clone()));
    // keep tmp alive by forgetting
    std::mem::forget(tmp);
    router_with_artifact_put(state)
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn memory_crud_and_search() {
    let app = harness();
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memory")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"content":"prefers dark mode","label":"prefs"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let id = body_json(create).await["id"].as_str().unwrap().to_string();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        body_json(list).await["entries"].as_array().unwrap().len(),
        1
    );

    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory?q=dark")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        body_json(search).await["entries"].as_array().unwrap().len(),
        1
    );

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/memory/{id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn inbox_lists_empty_when_idle() {
    let app = harness();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/inbox")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn skills_list_and_get() {
    let app = harness();
    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/skills")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let skills = body_json(list).await["skills"].as_array().unwrap().clone();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["name"], "demo");

    let get = app
        .oneshot(
            Request::builder()
                .uri("/v1/skills/demo")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    assert!(body_json(get).await["content"]
        .as_str()
        .unwrap()
        .contains("Demo skill"));
}

#[tokio::test]
async fn artifact_put_and_get_meta() {
    let app = harness();
    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/artifacts/new")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kind": "terminal",
                        "summary": "shell out",
                        "content_text": "hello terminal\n"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::CREATED);
    let id = body_json(put).await["id"].as_str().unwrap().to_string();

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/artifacts/{id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let body = body_json(get).await;
    assert_eq!(body["kind"], "terminal");
    assert!(body["byte_len"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn unauth_inbox_fails_closed() {
    let app = harness();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/inbox")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
