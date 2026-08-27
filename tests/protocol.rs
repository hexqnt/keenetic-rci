use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderValue, Method, Response, StatusCode, header},
};
use keenetic_rci::{
    AuthenticationError, CiPath, CliCommand, Dbm, Error, FirmwareChannel, HardwareType,
    KeeneticClient, NetworkTestOutput, RciPath,
    request::{
        Iperf3, IperfLimit, Ping, ShowAssociations, ShowIdentification, ShowInterface,
        ShowInterfaceStat, ShowInterfaces, ShowInternetStatus, ShowIpArp, ShowIpDhcpBindings,
        ShowIpHotspotHosts, ShowIpNameServers, ShowIpRoute, ShowIpv6Route, ShowLteInterface,
        ShowMedia, ShowMwsMembers, ShowMwsStatus, ShowNtpStatus, ShowPingCheck, ShowSystem,
        ShowSystemMode, ShowUsb, ShowVersion,
    },
};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use tokio::{sync::Notify, task::JoinHandle, time::sleep};

type BoxResponseFuture = Pin<Box<dyn Future<Output = Response<Body>> + Send>>;
type Handler = Arc<dyn Fn(Request) -> BoxResponseFuture + Send + Sync>;

struct TestServer {
    origin: String,
    task: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Counting<'a, T> {
    calls: &'a AtomicUsize,
    value: &'a T,
}

impl<T: Serialize> Serialize for Counting<'_, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.value.serialize(serializer)
    }
}

struct Failing;

impl Serialize for Failing {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(serde::ser::Error::custom("fixture serialization failure"))
    }
}

#[derive(Debug, serde::Deserialize, PartialEq)]
struct RequiredReply {
    required: bool,
}

fn response(status: StatusCode, body: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(body.into())
        .unwrap()
}

fn json_response(value: impl Into<Value>) -> Response<Body> {
    let value = value.into();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

fn unauthorized_challenge(realm: &'static str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("x-ndm-realm", realm)
        .header("x-ndm-challenge", "0123456789abcdef0123456789abcdef")
        .body(Body::empty())
        .unwrap()
}

fn client(origin: &str) -> KeeneticClient {
    KeeneticClient::builder()
        .base_url(origin)
        .credentials("admin", "secret")
        .build()
        .unwrap()
}

fn raw_path(path: &str) -> RciPath {
    path.parse().unwrap()
}

fn ci_path(path: &str) -> CiPath {
    path.parse().unwrap()
}

#[tokio::test]
async fn raw_paths_preserve_canonical_percent_encoding() {
    let server = spawn(|request| async move {
        assert_eq!(request.uri().path(), "/rci/show/interface%20name");
        json_response(json!({}))
    })
    .await;

    client(&server.origin)
        .get_raw(&raw_path("show/interface name"))
        .await
        .unwrap();
}

async fn dispatch(State(handler): State<Handler>, request: Request) -> Response<Body> {
    handler(request).await
}

async fn spawn<F, Fut>(handler: F) -> TestServer
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response<Body>> + Send + 'static,
{
    let handler: Handler = Arc::new(move |request| Box::pin(handler(request)));
    let app = Router::new().fallback(dispatch).with_state(handler);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    TestServer {
        origin: format!("http://{address}"),
        task,
    }
}

#[tokio::test]
async fn a_successful_first_request_does_not_authenticate() {
    let auth_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let auth_calls = Arc::clone(&auth_calls);
        move |request| {
            let auth_calls = Arc::clone(&auth_calls);
            async move {
                if request.uri().path() == "/auth" {
                    auth_calls.fetch_add(1, Ordering::SeqCst);
                }
                json_response(json!({}))
            }
        }
    })
    .await;

    let value = client(&server.origin)
        .get_raw(&raw_path("show/ping"))
        .await
        .unwrap();
    assert_eq!(value, json!({}));
    assert_eq!(auth_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn dynamic_cookies_from_unauthorized_and_auth_responses_are_replayed() {
    let target_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let target_calls = Arc::clone(&target_calls);
        move |request| {
            let target_calls = Arc::clone(&target_calls);
            async move {
                let path = request.uri().path();
                let method = request.method().clone();
                let cookie = request
                    .headers()
                    .get(header::COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                match (method, path) {
                    (Method::GET, "/rci/show/ping") => {
                        if target_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                            Response::builder()
                                .status(StatusCode::UNAUTHORIZED)
                                .header(header::SET_COOKIE, "from_target=seed; Path=/")
                                .body(Body::empty())
                                .unwrap()
                        } else {
                            assert!(cookie.contains("from_target=seed"));
                            assert!(cookie.contains("random_session=sid"));
                            assert!(cookie.contains("from_auth_post=accepted"));
                            json_response(json!({"ok": true}))
                        }
                    }
                    (Method::GET, "/auth") => {
                        assert!(cookie.contains("from_target=seed"));
                        Response::builder()
                            .status(StatusCode::UNAUTHORIZED)
                            .header("x-ndm-realm", "Dynamic Realm")
                            .header("x-ndm-challenge", "0123456789abcdef0123456789abcdef")
                            .header(header::SET_COOKIE, "random_session=sid; Path=/")
                            .body(Body::empty())
                            .unwrap()
                    }
                    (Method::POST, "/auth") => {
                        assert!(cookie.contains("from_target=seed"));
                        assert!(cookie.contains("random_session=sid"));
                        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                        let payload: Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(payload["login"], "admin");
                        assert_eq!(
                            payload["password"],
                            "b2ecb85016e0b8c80be6d99c2355826a75120d3e70fd05ff8167ae708e2ffbfb"
                        );
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::SET_COOKIE, "from_auth_post=accepted; Path=/")
                            .body(Body::empty())
                            .unwrap()
                    }
                    _ => response(StatusCode::NOT_FOUND, Body::empty()),
                }
            }
        }
    })
    .await;

    let value = client(&server.origin)
        .get_raw(&raw_path("show/ping"))
        .await
        .unwrap();
    assert_eq!(value, json!({"ok": true}));
    assert_eq!(target_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cookies_from_non_success_responses_are_retained() {
    let server = spawn(|request| async move {
        match request.uri().path() {
            "/rci/missing" => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::SET_COOKIE, "error_cookie=kept; Path=/")
                .body(Body::empty())
                .unwrap(),
            "/rci/check" => {
                let cookie = request
                    .headers()
                    .get(header::COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("");
                assert!(cookie.contains("error_cookie=kept"));
                json_response(json!({"ok": true}))
            }
            _ => response(StatusCode::NOT_FOUND, Body::empty()),
        }
    })
    .await;
    let client = client(&server.origin);

    let error = client.get_raw(&raw_path("missing")).await.unwrap_err();
    assert!(matches!(error, Error::Http(_)));
    client.get_raw(&raw_path("check")).await.unwrap();
}

#[tokio::test]
async fn authentication_failures_are_distinct() {
    let no_credentials_server =
        spawn(|_| async { response(StatusCode::UNAUTHORIZED, Body::empty()) }).await;
    let anonymous = KeeneticClient::builder()
        .base_url(&no_credentials_server.origin)
        .build()
        .unwrap();
    assert!(matches!(
        anonymous.get_raw(&raw_path("show/ping")).await,
        Err(Error::Authentication(AuthenticationError::Required))
    ));

    let unsupported_server =
        spawn(|_| async { response(StatusCode::UNAUTHORIZED, Body::empty()) }).await;
    assert!(matches!(
        client(&unsupported_server.origin)
            .get_raw(&raw_path("show/ping"))
            .await,
        Err(Error::Authentication(AuthenticationError::Unsupported))
    ));

    let malformed_server = spawn(|request| async move {
        if request.uri().path() == "/auth" {
            let mut response = unauthorized_challenge("valid");
            response
                .headers_mut()
                .insert("x-ndm-challenge", HeaderValue::from_bytes(&[0xff]).unwrap());
            response
        } else {
            response(StatusCode::UNAUTHORIZED, Body::empty())
        }
    })
    .await;
    assert!(matches!(
        client(&malformed_server.origin)
            .get_raw(&raw_path("show/ping"))
            .await,
        Err(Error::Authentication(
            AuthenticationError::MalformedResponse(_)
        ))
    ));
}

#[tokio::test]
async fn rejected_credentials_and_unconfirmed_auth_statuses_differ() {
    for (post_status, rejected) in [
        (StatusCode::UNAUTHORIZED, true),
        (StatusCode::FORBIDDEN, false),
    ] {
        let server = spawn(move |request| async move {
            match (request.method(), request.uri().path()) {
                (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                (&Method::POST, "/auth") => response(post_status, Body::empty()),
                _ => response(StatusCode::UNAUTHORIZED, Body::empty()),
            }
        })
        .await;
        let error = client(&server.origin)
            .get_raw(&raw_path("show/ping"))
            .await
            .unwrap_err();
        if rejected {
            assert!(matches!(
                error,
                Error::Authentication(AuthenticationError::RejectedCredentials)
            ));
        } else {
            assert!(
                matches!(error, Error::Http(ref http) if http.status() == StatusCode::FORBIDDEN && http.context().endpoint() == "/auth")
            );
        }
    }
}

#[tokio::test]
async fn a_second_endpoint_401_stops_after_one_authentication() {
    let auth_posts = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let auth_posts = Arc::clone(&auth_posts);
        move |request| {
            let auth_posts = Arc::clone(&auth_posts);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => {
                        auth_posts.fetch_add(1, Ordering::SeqCst);
                        response(StatusCode::OK, Body::empty())
                    }
                    _ => response(StatusCode::UNAUTHORIZED, Body::empty()),
                }
            }
        }
    })
    .await;

    let error = client(&server.origin)
        .get_raw(&raw_path("show/ping"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Authentication(AuthenticationError::UnauthorizedAfterAuthentication)
    ));
    assert_eq!(auth_posts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_unauthorized_requests_share_one_successful_authentication() {
    let auth_posts = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let auth_posts = Arc::clone(&auth_posts);
        move |request| {
            let auth_posts = Arc::clone(&auth_posts);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => {
                        auth_posts.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(100)).await;
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::SET_COOKIE, "logged_in=yes; Path=/")
                            .body(Body::empty())
                            .unwrap()
                    }
                    _ => {
                        let authenticated = request
                            .headers()
                            .get(header::COOKIE)
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value.contains("logged_in=yes"));
                        if authenticated {
                            json_response(json!({"ok": true}))
                        } else {
                            response(StatusCode::UNAUTHORIZED, Body::empty())
                        }
                    }
                }
            }
        }
    })
    .await;
    let client = client(&server.origin);
    let start = Arc::new(tokio::sync::Barrier::new(13));
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let client = client.clone();
        let start = Arc::clone(&start);
        tasks.push(tokio::spawn(async move {
            start.wait().await;
            client.get_raw(&raw_path("show/ping")).await
        }));
    }
    start.wait().await;
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap(), json!({"ok": true}));
    }
    assert_eq!(auth_posts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_auth_errors_are_shared_but_not_permanently_cached() {
    let auth_posts = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let auth_posts = Arc::clone(&auth_posts);
        move |request| {
            let auth_posts = Arc::clone(&auth_posts);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => {
                        auth_posts.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(100)).await;
                        response(StatusCode::UNAUTHORIZED, Body::empty())
                    }
                    _ => response(StatusCode::UNAUTHORIZED, Body::empty()),
                }
            }
        }
    })
    .await;
    let client = client(&server.origin);
    let start = Arc::new(tokio::sync::Barrier::new(9));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let client = client.clone();
        let start = Arc::clone(&start);
        tasks.push(tokio::spawn(async move {
            start.wait().await;
            client.get_raw(&raw_path("show/ping")).await
        }));
    }
    start.wait().await;
    for task in tasks {
        assert!(matches!(
            task.await.unwrap(),
            Err(Error::Authentication(
                AuthenticationError::RejectedCredentials
            ))
        ));
    }
    assert_eq!(auth_posts.load(Ordering::SeqCst), 1);

    assert!(matches!(
        client.get_raw(&raw_path("show/ping")).await,
        Err(Error::Authentication(
            AuthenticationError::RejectedCredentials
        ))
    ));
    assert_eq!(auth_posts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancelling_authentication_does_not_leave_single_flight_locked() {
    let auth_gets = Arc::new(AtomicUsize::new(0));
    let first_auth_started = Arc::new(Notify::new());
    let server = spawn({
        let auth_gets = Arc::clone(&auth_gets);
        let first_auth_started = Arc::clone(&first_auth_started);
        move |request| {
            let auth_gets = Arc::clone(&auth_gets);
            let first_auth_started = Arc::clone(&first_auth_started);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => {
                        if auth_gets.fetch_add(1, Ordering::SeqCst) == 0 {
                            first_auth_started.notify_one();
                            sleep(Duration::from_secs(5)).await;
                        }
                        unauthorized_challenge("Fixture Realm")
                    }
                    (&Method::POST, "/auth") => Response::builder()
                        .status(StatusCode::OK)
                        .header(header::SET_COOKIE, "logged_in=yes; Path=/")
                        .body(Body::empty())
                        .unwrap(),
                    _ => {
                        let authenticated = request
                            .headers()
                            .get(header::COOKIE)
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value.contains("logged_in=yes"));
                        if authenticated {
                            json_response(json!({"ok": true}))
                        } else {
                            response(StatusCode::UNAUTHORIZED, Body::empty())
                        }
                    }
                }
            }
        }
    })
    .await;
    let client = client(&server.origin);
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.get_raw(&raw_path("show/ping")).await }
    });
    first_auth_started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());

    let result = tokio::time::timeout(
        Duration::from_secs(1),
        client.get_raw(&raw_path("show/ping")),
    )
    .await
    .expect("auth mutex remained locked")
    .unwrap();
    assert_eq!(result, json!({"ok": true}));
    assert_eq!(auth_gets.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn independent_clients_share_transport_but_not_sessions() {
    let auth_posts = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let auth_posts = Arc::clone(&auth_posts);
        move |request| {
            let auth_posts = Arc::clone(&auth_posts);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => {
                        let number = auth_posts.fetch_add(1, Ordering::SeqCst) + 1;
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::SET_COOKIE, format!("session={number}; Path=/"))
                            .body(Body::empty())
                            .unwrap()
                    }
                    _ => {
                        let authenticated = request.headers().contains_key(header::COOKIE);
                        if authenticated {
                            json_response(json!({"ok": true}))
                        } else {
                            response(StatusCode::UNAUTHORIZED, Body::empty())
                        }
                    }
                }
            }
        }
    })
    .await;
    let transport = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .unwrap();
    let first = KeeneticClient::builder()
        .base_url(&server.origin)
        .credentials("admin", "secret")
        .http_client(transport.clone())
        .build()
        .unwrap();
    let second = KeeneticClient::builder()
        .base_url(&server.origin)
        .credentials("admin", "secret")
        .http_client(transport)
        .build()
        .unwrap();

    first.get_raw(&raw_path("show/ping")).await.unwrap();
    first.clone().get_raw(&raw_path("show/ping")).await.unwrap();
    assert_eq!(auth_posts.load(Ordering::SeqCst), 1);
    second.get_raw(&raw_path("show/ping")).await.unwrap();
    assert_eq!(auth_posts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn transport_modes_disable_redirects_and_honor_injected_defaults() {
    let final_calls = Arc::new(AtomicUsize::new(0));
    let unavailable_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let final_calls = Arc::clone(&final_calls);
        let unavailable_calls = Arc::clone(&unavailable_calls);
        move |request| {
            let final_calls = Arc::clone(&final_calls);
            let unavailable_calls = Arc::clone(&unavailable_calls);
            async move {
                match request.uri().path() {
                    "/rci/redirect" => Response::builder()
                        .status(StatusCode::FOUND)
                        .header(header::LOCATION, "/rci/final")
                        .body(Body::empty())
                        .unwrap(),
                    "/rci/final" => {
                        final_calls.fetch_add(1, Ordering::SeqCst);
                        json_response(json!({}))
                    }
                    "/rci/header" => {
                        assert_eq!(request.headers()["x-fixture"], "observed");
                        json_response(json!({"ok": true}))
                    }
                    "/rci/unavailable" => {
                        unavailable_calls.fetch_add(1, Ordering::SeqCst);
                        response(StatusCode::SERVICE_UNAVAILABLE, Body::empty())
                    }
                    _ => response(StatusCode::SERVICE_UNAVAILABLE, Body::empty()),
                }
            }
        }
    })
    .await;
    let internal = client(&server.origin);
    let error = internal.get_raw(&raw_path("redirect")).await.unwrap_err();
    assert!(matches!(error, Error::Http(ref http) if http.status() == StatusCode::FOUND));
    assert_eq!(final_calls.load(Ordering::SeqCst), 0);
    let error = internal
        .get_raw(&raw_path("unavailable"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Http(ref http) if http.status() == StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(unavailable_calls.load(Ordering::SeqCst), 1);

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-fixture", "observed".parse().unwrap());
    let transport = reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .unwrap();
    let injected = KeeneticClient::builder()
        .base_url(&server.origin)
        .http_client(transport)
        .build()
        .unwrap();
    injected.get_raw(&raw_path("header")).await.unwrap();
}

#[tokio::test]
async fn internal_transport_ignores_system_proxy() {
    const CHILD_ORIGIN: &str = "KEENETIC_RCI_PROXY_TEST_ORIGIN";

    if let Ok(origin) = std::env::var(CHILD_ORIGIN) {
        let value = KeeneticClient::builder()
            .base_url(origin)
            .request_timeout(Duration::from_secs(1))
            .build()
            .unwrap()
            .get_raw(&raw_path("show/ping"))
            .await
            .unwrap();
        assert_eq!(value, json!({"direct": true}));
        return;
    }

    let origin = spawn(|_| async { json_response(json!({"direct": true})) }).await;
    let proxy_calls = Arc::new(AtomicUsize::new(0));
    let proxy = spawn({
        let proxy_calls = Arc::clone(&proxy_calls);
        move |_| {
            let proxy_calls = Arc::clone(&proxy_calls);
            async move {
                proxy_calls.fetch_add(1, Ordering::SeqCst);
                response(StatusCode::BAD_GATEWAY, Body::empty())
            }
        }
    })
    .await;
    let executable = std::env::current_exe().unwrap();
    let origin_url = origin.origin.clone();
    let proxy_url = proxy.origin.clone();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(executable)
            .args([
                "--exact",
                "internal_transport_ignores_system_proxy",
                "--nocapture",
            ])
            .env(CHILD_ORIGIN, origin_url)
            .env("HTTP_PROXY", &proxy_url)
            .env("http_proxy", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env("all_proxy", proxy_url)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "proxy child failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(proxy_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_timeout_applies_to_injected_transport_and_raw_post_is_not_retried() {
    let post_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let post_calls = Arc::clone(&post_calls);
        move |request| {
            let post_calls = Arc::clone(&post_calls);
            async move {
                if request.method() == Method::POST {
                    post_calls.fetch_add(1, Ordering::SeqCst);
                }
                sleep(Duration::from_millis(200)).await;
                json_response(json!({}))
            }
        }
    })
    .await;
    let client = KeeneticClient::builder()
        .base_url(&server.origin)
        .http_client(
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .no_proxy()
                .build()
                .unwrap(),
        )
        .request_timeout(Duration::from_millis(30))
        .build()
        .unwrap();
    let error = client
        .post_raw(&json!({"possibly": "applied"}))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Transport(ref error) if error.is_timeout()));
    sleep(Duration::from_millis(250)).await;
    assert_eq!(post_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn transport_error_debug_omits_query_values() {
    let sensitive = "unique-query-value-never-log";
    let server = spawn(|_| async move {
        sleep(Duration::from_millis(200)).await;
        json_response(json!({}))
    })
    .await;
    let client = KeeneticClient::builder()
        .base_url(&server.origin)
        .request_timeout(Duration::from_millis(20))
        .build()
        .unwrap();
    let error = client
        .get_with_query::<Value, _>(&raw_path("show/filter"), &[("token", sensitive)])
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Transport(_)));
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains(sensitive));
    assert!(!rendered.contains("token="));
}

#[tokio::test]
async fn connection_failures_are_transport_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = KeeneticClient::builder()
        .base_url(format!("http://{address}"))
        .connect_timeout(Duration::from_millis(50))
        .request_timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    assert!(matches!(
        client.get_raw(&raw_path("show/ping")).await,
        Err(Error::Transport(_))
    ));
}

#[tokio::test]
async fn query_and_body_are_serialized_once_across_auth_retry() {
    async fn exercise(is_post: bool) {
        let server = spawn(move |request| async move {
            match (request.method(), request.uri().path()) {
                (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                (&Method::POST, "/auth") => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::SET_COOKIE, "logged_in=yes; Path=/")
                    .body(Body::empty())
                    .unwrap(),
                _ => {
                    let authenticated = request
                        .headers()
                        .get(header::COOKIE)
                        .and_then(|value| value.to_str().ok())
                        .is_some_and(|value| value.contains("logged_in=yes"));
                    if !authenticated {
                        return response(StatusCode::UNAUTHORIZED, Body::empty());
                    }
                    if is_post {
                        assert_eq!(request.method(), Method::POST);
                    } else {
                        assert_eq!(request.uri().query(), Some("name=two+words%2Fvalue"));
                    }
                    json_response(json!({"ok": true}))
                }
            }
        })
        .await;
        let client = client(&server.origin);
        let calls = AtomicUsize::new(0);
        if is_post {
            client
                .post_raw(&Counting {
                    calls: &calls,
                    value: &json!({"write": true}),
                })
                .await
                .unwrap();
        } else {
            client
                .get_with_query::<Value, _>(
                    &raw_path("show/filter"),
                    &Counting {
                        calls: &calls,
                        value: &[("name", "two words/value")],
                    },
                )
                .await
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    exercise(false).await;
    exercise(true).await;
}

#[tokio::test]
async fn typed_interface_stat_encodes_query_once_across_auth_retry() {
    let target_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let target_calls = Arc::clone(&target_calls);
        move |request| {
            let target_calls = Arc::clone(&target_calls);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => Response::builder()
                        .status(StatusCode::OK)
                        .header(header::SET_COOKIE, "logged_in=yes; Path=/")
                        .body(Body::empty())
                        .unwrap(),
                    (&Method::GET, "/rci/show/interface/stat") => {
                        target_calls.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(
                            request.uri().query(),
                            Some("name=WifiMaster0%2FAccessPoint0")
                        );
                        let authenticated = request
                            .headers()
                            .get(header::COOKIE)
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value.contains("logged_in=yes"));
                        if authenticated {
                            response(
                                StatusCode::OK,
                                include_str!("fixtures/show_interface_stat.json"),
                            )
                        } else {
                            response(StatusCode::UNAUTHORIZED, Body::empty())
                        }
                    }
                    _ => response(StatusCode::NOT_FOUND, Body::empty()),
                }
            }
        }
    })
    .await;

    client(&server.origin)
        .execute(ShowInterfaceStat::new("WifiMaster0/AccessPoint0").unwrap())
        .await
        .unwrap();
    assert_eq!(target_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn typed_get_requests_use_verified_endpoints() {
    let server = spawn(|request| async move {
        assert_eq!(request.method(), Method::GET);
        let fixture = match request.uri().path() {
            "/rci/show/version" => include_str!("fixtures/show_version.json"),
            "/rci/show/system" => include_str!("fixtures/show_system.json"),
            "/rci/show/internet/status" => include_str!("fixtures/show_internet_status.json"),
            "/rci/show/interface" => include_str!("fixtures/show_interfaces.json"),
            "/rci/show/associations" => include_str!("fixtures/show_associations.json"),
            "/rci/show/ip/hotspot" => include_str!("fixtures/show_ip_hotspot.json"),
            "/rci/show/identification" => include_str!("fixtures/show_identification.json"),
            "/rci/show/system/mode" => include_str!("fixtures/show_system_mode.json"),
            "/rci/show/ip/arp" => include_str!("fixtures/show_ip_arp.json"),
            "/rci/show/ip/dhcp/bindings" => {
                include_str!("fixtures/show_ip_dhcp_bindings.json")
            }
            "/rci/show/ip/route" => include_str!("fixtures/show_ip_route.json"),
            "/rci/show/ipv6/route" => include_str!("fixtures/show_ipv6_route.json"),
            "/rci/show/ping-check" => include_str!("fixtures/show_ping_check.json"),
            "/rci/show/ip/name-server" => include_str!("fixtures/show_ip_name_server.json"),
            "/rci/show/ntp/status" => include_str!("fixtures/show_ntp_status.json"),
            "/rci/show/mws/status" => include_str!("fixtures/show_mws_status.json"),
            "/rci/show/mws/member" => include_str!("fixtures/show_mws_member.json"),
            "/rci/show/usb" => include_str!("fixtures/show_usb.json"),
            "/rci/show/media" => include_str!("fixtures/show_media.json"),
            _ => return response(StatusCode::NOT_FOUND, Body::empty()),
        };
        response(StatusCode::OK, fixture)
    })
    .await;
    let client = client(&server.origin);

    client.execute(ShowVersion).await.unwrap();
    client.execute(ShowSystem).await.unwrap();
    client.execute(ShowInternetStatus).await.unwrap();
    client.execute(ShowInterfaces).await.unwrap();
    client.execute(ShowAssociations).await.unwrap();
    client.execute(ShowIpHotspotHosts).await.unwrap();
    client.execute(ShowIdentification).await.unwrap();
    client.execute(ShowSystemMode).await.unwrap();
    client.execute(ShowIpArp).await.unwrap();
    client.execute(ShowIpDhcpBindings).await.unwrap();
    client.execute(ShowIpRoute).await.unwrap();
    client.execute(ShowIpv6Route).await.unwrap();
    client.execute(ShowPingCheck).await.unwrap();
    client.execute(ShowIpNameServers).await.unwrap();
    client.execute(ShowNtpStatus).await.unwrap();
    client.execute(ShowMwsStatus).await.unwrap();
    client.execute(ShowMwsMembers).await.unwrap();
    client.execute(ShowUsb).await.unwrap();
    client.execute(ShowMedia).await.unwrap();
}

#[tokio::test]
async fn serialization_errors_happen_before_network_io() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let calls = Arc::clone(&calls);
        move |_| {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                json_response(json!({}))
            }
        }
    })
    .await;
    let client = client(&server.origin);
    assert!(matches!(
        client
            .get_with_query::<Value, _>(&raw_path("show/filter"), &Failing)
            .await,
        Err(Error::QuerySerialization(_))
    ));
    assert!(matches!(
        client.post_raw(&Failing).await,
        Err(Error::JsonSerialization(_))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn response_pipeline_separates_http_json_rci_and_model_errors() {
    let sensitive = "unique-sensitive-response-body";
    let server = spawn(move |request| async move {
        match request.uri().path() {
            "/rci/not-found" => response(StatusCode::NOT_FOUND, sensitive),
            "/rci/malformed" => response(StatusCode::OK, sensitive),
            "/rci/rci-errors" => json_response(json!({
                "status": [{"status":"error","code":"1","message":"first"}],
                "nested": {"items": [{
                    "status": [
                        {"status":"message","message":"success"},
                        {"status":"error","ident":"Fixture","message":"second"}
                    ]
                }]}
            })),
            "/rci/escaped-rci-error" => response(
                StatusCode::OK,
                r#"{"sta\u0074us":[{"status":"\u0065rror","code":"2"}]}"#,
            ),
            "/rci/message" => json_response(json!({
                "status": [{"status":"message","message":"applied"}],
                "required": true
            })),
            "/rci/wrong-shape" => json_response(json!({"required": sensitive})),
            "/rci/text" => response(StatusCode::OK, "plain fixture text"),
            "/ci/running-config.txt" => response(StatusCode::OK, "system fixture\n"),
            _ => response(StatusCode::NOT_FOUND, Body::empty()),
        }
    })
    .await;
    let client = client(&server.origin);

    let http = client.get_raw(&raw_path("not-found")).await.unwrap_err();
    assert!(matches!(http, Error::Http(ref error) if error.status() == StatusCode::NOT_FOUND));
    assert!(!format!("{http:?} {http}").contains(sensitive));

    let json = client.get_raw(&raw_path("malformed")).await.unwrap_err();
    assert!(matches!(json, Error::ResponseJson(_)));
    assert!(!format!("{json:?} {json}").contains(sensitive));

    let rci = client.get_raw(&raw_path("rci-errors")).await.unwrap_err();
    let Error::Rci(ref rci_error) = rci else {
        panic!("expected RCI error")
    };
    assert_eq!(rci_error.entries().len(), 2);
    assert!(
        rci_error
            .entries()
            .iter()
            .any(|entry| entry.code.as_deref() == Some("1"))
    );
    assert!(
        rci_error
            .entries()
            .iter()
            .any(|entry| entry.ident.as_deref() == Some("Fixture"))
    );
    assert!(!format!("{rci:?} {rci}").contains("second"));

    let escaped = client
        .get_raw(&raw_path("escaped-rci-error"))
        .await
        .unwrap_err();
    assert!(matches!(
        escaped,
        Error::Rci(ref error) if error.entries()[0].code.as_deref() == Some("2")
    ));

    assert_eq!(
        client
            .get::<RequiredReply>(&raw_path("message"))
            .await
            .unwrap(),
        RequiredReply { required: true }
    );
    let model = client
        .get::<RequiredReply>(&raw_path("wrong-shape"))
        .await
        .unwrap_err();
    assert!(matches!(model, Error::ResponseDeserialization(_)));
    assert!(!format!("{model:?} {model}").contains(sensitive));
    assert_eq!(
        client.get_text(&raw_path("text")).await.unwrap(),
        "plain fixture text"
    );
    assert_eq!(
        client
            .get_ci_text(&ci_path("running-config.txt"))
            .await
            .unwrap(),
        "system fixture\n"
    );
}

#[tokio::test]
async fn typed_interface_and_raw_posts_have_no_hidden_side_effects() {
    let requests = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let requests = Arc::clone(&requests);
        move |request| {
            let requests = Arc::clone(&requests);
            async move {
                requests.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.method(), Method::POST);
                assert_eq!(request.uri().path(), "/rci/");
                let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                let command: Value = serde_json::from_slice(&body).unwrap();
                let name = command
                    .pointer("/show/interface/name")
                    .and_then(Value::as_str)
                    .unwrap_or("Fixture0");
                json_response(json!({
                    "show": {"interface": {
                        "id": name,
                        "index": 1,
                        "interface-name": name,
                        "type": "Fixture",
                        "traits": [],
                        "link": "up",
                        "admin-only": false,
                        "summary": {"layer": {"conf":"running", "link":"running"}}
                    }}
                }))
            }
        }
    })
    .await;
    let client = client(&server.origin);

    let reply = client
        .execute(ShowInterface::new("WifiMaster0/AccessPoint0").unwrap())
        .await
        .unwrap();
    assert_eq!(
        reply.interface().interface_name.as_str(),
        "WifiMaster0/AccessPoint0"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);

    let typed: Value = client.post(&json!({"fixture": true})).await.unwrap();
    let raw = client.post_raw(&json!({"fixture": true})).await.unwrap();
    assert_eq!(typed, raw);
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn post_at_uses_the_validated_rci_endpoint() {
    let requests = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let requests = Arc::clone(&requests);
        move |request| {
            let requests = Arc::clone(&requests);
            async move {
                requests.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.method(), Method::POST);
                assert_eq!(request.uri().path(), "/rci/interface/description");
                assert_eq!(
                    request.headers().get(header::CONTENT_TYPE),
                    Some(&HeaderValue::from_static("application/json"))
                );
                let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&body).unwrap(),
                    json!({"name": "Bridge0", "description": "Home"})
                );
                json_response(json!({"required": true}))
            }
        }
    })
    .await;
    let client = client(&server.origin);
    let path = raw_path("interface/description");
    let body = json!({"name": "Bridge0", "description": "Home"});

    assert_eq!(
        client
            .post_at::<RequiredReply, _>(&path, &body)
            .await
            .unwrap(),
        RequiredReply { required: true }
    );
    assert_eq!(
        client.post_at_raw(&path, &body).await.unwrap(),
        json!({"required": true})
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cli_execution_uses_parse_and_preserves_command_specific_output() {
    let server = spawn(|request| async move {
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.uri().path(), "/rci/parse");
        assert_eq!(
            request.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
        let command: String = serde_json::from_slice(&body).unwrap();
        assert_eq!(command, "interface UsbLte1 tty send AT+GTCAINFO?");
        json_response(json!({
            "tty-out": ["PCC:103,489,1275", "OK"],
            "prompt": "(config)",
            "status": [{
                "status": "message",
                "code": "73141676",
                "ident": "Mobile::Interface",
                "message": "got expected response"
            }],
            "future-field": {"preserved": true}
        }))
    })
    .await;
    let client = client(&server.origin);
    let command = CliCommand::new("interface UsbLte1 tty send AT+GTCAINFO?").unwrap();

    let reply = client.execute_cli(&command).await.unwrap();

    assert_eq!(
        reply.tty_output().collect::<Vec<_>>(),
        ["PCC:103,489,1275", "OK"]
    );
    assert_eq!(reply.prompt(), Some("(config)"));
    assert_eq!(
        reply.raw().pointer("/future-field/preserved"),
        Some(&Value::Bool(true))
    );
}

#[tokio::test]
async fn cli_supports_typed_replies() {
    let server = spawn(|request| async move {
        assert_eq!(request.uri().path(), "/rci/parse");
        json_response(json!({"required": true}))
    })
    .await;
    let client = client(&server.origin);
    let command = CliCommand::new("show fixture").unwrap();

    assert_eq!(
        client
            .execute_cli_as::<RequiredReply>(&command)
            .await
            .unwrap(),
        RequiredReply { required: true }
    );
}

#[tokio::test]
async fn cli_errors_use_the_parse_context_without_exposing_the_command() {
    let server = spawn(|request| async move {
        assert_eq!(request.uri().path(), "/rci/parse");
        json_response(json!({
            "status": [{
                "status": "error",
                "code": "73140786",
                "ident": "Mobile::Interface",
                "message": "fixture failure"
            }]
        }))
    })
    .await;
    let client = client(&server.origin);
    let sensitive = "user admin password unique-sensitive-command-value";
    let command = CliCommand::new(sensitive).unwrap();

    let error = client.execute_cli(&command).await.unwrap_err();
    let Error::Rci(ref rci) = error else {
        panic!("expected an RCI error")
    };
    assert_eq!(rci.context().endpoint(), "/rci/parse");
    assert_eq!(rci.entries()[0].code.as_deref(), Some("73140786"));
    assert!(!format!("{command:?} {error:?} {error}").contains(sensitive));
}

#[tokio::test]
async fn network_test_streams_post_and_polled_output_in_order() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let calls = Arc::clone(&calls);
        move |request| {
            let calls = Arc::clone(&calls);
            async move {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.uri().path(), "/rci/tools/ping");
                match call {
                    0 => {
                        assert_eq!(request.method(), Method::POST);
                        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                        assert_eq!(
                            serde_json::from_slice::<Value>(&body).unwrap(),
                            json!({"host": "example.com", "count": 2})
                        );
                        // The web protocol uses field presence, even for null.
                        json_response(json!({
                            "message": ["starting ping", "reply 1"],
                            "continued": null
                        }))
                    }
                    1 => {
                        assert_eq!(request.method(), Method::GET);
                        json_response(json!({"message": ["reply 2", "finished"]}))
                    }
                    _ => panic!("unexpected network-test request"),
                }
            }
        }
    })
    .await;

    let request = Ping::new("example.com").unwrap().count(2).unwrap();
    let mut session = client(&server.origin)
        .start_network_test(request)
        .await
        .unwrap();
    assert!(!session.is_finished());
    let first = session.next_chunk().await.unwrap().unwrap();
    assert_eq!(first.lines(), ["starting ping", "reply 1"]);
    assert!(!session.is_finished());
    let remaining = session.collect().await.unwrap();
    assert_eq!(remaining.lines(), ["reply 2", "finished"]);
    assert_eq!(remaining.to_string(), "reply 2\nfinished");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn run_network_test_collects_a_fresh_session_and_cancel_uses_delete() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let calls = Arc::clone(&calls);
        move |request| {
            let calls = Arc::clone(&calls);
            async move {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.uri().path(), "/rci/tools/iperf3");
                match *request.method() {
                    Method::POST => {
                        let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                        assert_eq!(
                            serde_json::from_slice::<Value>(&body).unwrap(),
                            json!({
                                "host": "server.example",
                                "ipv4": true,
                                "tcp": true,
                                "time": 1
                            })
                        );
                        if call == 0 {
                            json_response(json!({
                                "message": ["started"],
                                "continued": true
                            }))
                        } else {
                            json_response(json!({"message": ["complete"]}))
                        }
                    }
                    Method::DELETE => json_response(json!({"message": ["cancelled"]})),
                    _ => panic!("unexpected network-test method"),
                }
            }
        }
    })
    .await;

    let request = Iperf3::new("server.example", IperfLimit::time(1).unwrap()).unwrap();
    let session = client(&server.origin)
        .start_network_test(request)
        .await
        .unwrap();
    let output = session.cancel().await.unwrap();
    assert_eq!(output.lines(), ["started", "cancelled"]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let request = Iperf3::new("server.example", IperfLimit::time(1).unwrap()).unwrap();
    let session = client(&server.origin)
        .start_network_test(request)
        .await
        .unwrap();
    let output = session.cancel().await.unwrap();
    assert_eq!(output.lines(), ["complete"]);
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    let request = Iperf3::new("server.example", IperfLimit::time(1).unwrap()).unwrap();
    let output = client(&server.origin)
        .run_network_test(request)
        .await
        .unwrap();
    assert_eq!(output.lines(), ["complete"]);
    assert_eq!(calls.load(Ordering::SeqCst), 4);

    let empty = NetworkTestOutput::default();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.to_string(), "");
}

#[tokio::test]
async fn network_test_poll_errors_use_the_get_endpoint_context() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let calls = Arc::clone(&calls);
        move |request| {
            let calls = Arc::clone(&calls);
            async move {
                assert_eq!(request.uri().path(), "/rci/tools/ping");
                match calls.fetch_add(1, Ordering::SeqCst) {
                    0 => json_response(json!({"continued": true})),
                    1 => json_response(json!({
                        "status": [{
                            "status": "error",
                            "code": "fixture",
                            "message": "poll failed"
                        }]
                    })),
                    _ => panic!("unexpected network-test request"),
                }
            }
        }
    })
    .await;

    let error = client(&server.origin)
        .run_network_test(Ping::new("example.com").unwrap())
        .await
        .unwrap_err();
    let Error::Rci(error) = error else {
        panic!("expected an RCI error");
    };
    assert_eq!(error.context().method(), Method::GET);
    assert_eq!(error.context().endpoint(), "/rci/tools/ping");
}

#[tokio::test]
async fn network_test_start_uses_the_shared_authentication_pipeline() {
    let target_calls = Arc::new(AtomicUsize::new(0));
    let server = spawn({
        let target_calls = Arc::clone(&target_calls);
        move |request| {
            let target_calls = Arc::clone(&target_calls);
            async move {
                match (request.method(), request.uri().path()) {
                    (&Method::POST, "/rci/tools/ping") => {
                        if target_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                            response(StatusCode::UNAUTHORIZED, Body::empty())
                        } else {
                            let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                            assert_eq!(
                                serde_json::from_slice::<Value>(&body).unwrap(),
                                json!({"host": "example.com", "count": 5})
                            );
                            json_response(json!({"message": ["authenticated"]}))
                        }
                    }
                    (&Method::GET, "/auth") => unauthorized_challenge("Fixture Realm"),
                    (&Method::POST, "/auth") => json_response(json!({})),
                    _ => response(StatusCode::NOT_FOUND, Body::empty()),
                }
            }
        }
    })
    .await;

    let output = client(&server.origin)
        .run_network_test(Ping::new("example.com").unwrap())
        .await
        .unwrap();
    assert_eq!(output.lines(), ["authenticated"]);
    assert_eq!(target_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn typed_lte_interface_uses_the_verified_json_command() {
    let requests = Arc::new(AtomicUsize::new(0));
    let fixture = include_str!("fixtures/show_lte_interface.json");
    let server = spawn({
        let requests = Arc::clone(&requests);
        move |request| {
            let requests = Arc::clone(&requests);
            async move {
                requests.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.method(), Method::POST);
                assert_eq!(request.uri().path(), "/rci/");
                let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&body).unwrap(),
                    json!({"show":{"interface":{"name":"UsbLte1"}}})
                );
                response(StatusCode::OK, fixture)
            }
        }
    })
    .await;

    let reply = client(&server.origin)
        .execute(ShowLteInterface::new("UsbLte1").unwrap())
        .await
        .unwrap();
    assert_eq!(
        reply.interface().status().signal.rsrp.map(Dbm::get),
        Some(-79.0)
    );
    assert_eq!(reply.interface().status().reported_carriers.len(), 1);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn debug_and_errors_do_not_expose_credentials_or_auth_material() {
    let password = "unique-password-never-log";
    let cookie = "unique-cookie-never-log";
    let hash = "unique-auth-hash-never-log";
    let server = spawn(move |request| async move {
        if request.uri().path() == "/auth" {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("x-ndm-realm", "Fixture Realm")
                .header("x-ndm-challenge", hash)
                .header(header::SET_COOKIE, format!("session={cookie}; Path=/"))
                .body(Body::empty())
                .unwrap()
        } else {
            response(StatusCode::UNAUTHORIZED, Body::empty())
        }
    })
    .await;
    let client = KeeneticClient::builder()
        .base_url(&server.origin)
        .credentials("admin", password)
        .build()
        .unwrap();
    let debug = format!("{client:?}");
    assert!(!debug.contains(password));

    let error = client.get_raw(&raw_path("show/ping")).await.unwrap_err();
    let rendered = format!("{error:?} {error}");
    for secret in [password, cookie, hash] {
        assert!(!rendered.contains(secret));
    }
    assert!(!rendered.contains('?'));
}

#[tokio::test]
async fn typed_version_uses_the_shared_response_pipeline() {
    let fixture = include_str!("fixtures/show_version.json");
    let server = spawn(move |request| async move {
        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.uri().path(), "/rci/show/version");
        response(StatusCode::OK, fixture)
    })
    .await;
    let version = client(&server.origin).execute(ShowVersion).await.unwrap();
    assert_eq!(version.release.as_ref(), "5.x");
    assert_eq!(version.arch.as_str(), "mips");
    assert_eq!(
        version.sandbox.as_ref().map(FirmwareChannel::as_str),
        Some("stable")
    );
    assert_eq!(version.series.as_str(), "KN");
    assert_eq!(version.hw_version.as_str(), "10000000");
    assert_eq!(
        version.hw_type.as_ref().map(HardwareType::as_str),
        Some("router")
    );
    assert_eq!(version.hw_id.to_string(), "KN-0000");
    assert_eq!(version.region.as_str(), "EA");
}
