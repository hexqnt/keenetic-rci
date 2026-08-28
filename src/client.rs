use std::{
    fmt,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use reqwest::{
    Method, Response, StatusCode, Url,
    cookie::{CookieStore, Jar},
    header::{CONTENT_TYPE, COOKIE, HeaderMap, SET_COOKIE},
    redirect::Policy,
    retry,
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned, de::IgnoredAny};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep_until};

use crate::{
    auth::{AuthPayload, Credentials, Secret, response_hash},
    cli::{CliCommand, CliReply},
    error::{
        AuthenticationError, ConfigError, Error, HttpError, JsonSerializationError,
        MalformedAuthReason, QuerySerializationError, RciError, RciStatusEntry, RequestContext,
        ResponseDeserializationError, ResponseJsonError, TransportError,
    },
    path::{CiPath, RciPath},
    request::{NetworkTestRequest, RciRequest, private::Mode},
};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const NETWORK_TEST_POLL_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_ENDPOINT: &str = "/auth";
const REALM_HEADER: &str = "x-ndm-realm";
const CHALLENGE_HEADER: &str = "x-ndm-challenge";

/// Builder for an independent [`KeeneticClient`] session.
///
/// Building is synchronous and never performs a network request.
pub struct KeeneticClientBuilder {
    base_url: Option<String>,
    credentials: Option<Credentials>,
    http_client: Option<reqwest::Client>,
    request_timeout: Duration,
    connect_timeout: Duration,
    connect_timeout_explicit: bool,
}

impl KeeneticClientBuilder {
    /// Sets the router origin, for example `http://192.168.1.1`.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets optional LAN challenge-response credentials.
    #[must_use]
    pub fn credentials(mut self, login: impl Into<String>, password: impl Into<String>) -> Self {
        self.credentials = Some(Credentials {
            login: login.into().into_boxed_str(),
            password: Secret::new(password.into()),
        });
        self
    }

    /// Uses an existing transport and its connection pool.
    ///
    /// The supplied client must have redirects, automatic retries, its cookie
    /// store, and default `Cookie`/`Authorization` headers disabled. Transport
    /// TLS, DNS, proxy, pool, and connect-timeout settings remain owned by the
    /// caller. This library still applies [`Self::request_timeout`] per request.
    #[must_use]
    pub fn http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Sets the timeout for every request, including authentication requests.
    #[must_use]
    pub const fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Sets the internal transport's connect timeout.
    ///
    /// This cannot be combined with [`Self::http_client`].
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self.connect_timeout_explicit = true;
        self
    }

    /// Validates the configuration and creates a client without network I/O.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the origin, credentials, timeout settings,
    /// or internal HTTP transport configuration is invalid.
    pub fn build(self) -> Result<KeeneticClient, ConfigError> {
        if self.request_timeout.is_zero() {
            return Err(ConfigError::ZeroTimeout("request_timeout"));
        }
        if self.connect_timeout.is_zero() {
            return Err(ConfigError::ZeroTimeout("connect_timeout"));
        }
        if self.http_client.is_some() && self.connect_timeout_explicit {
            return Err(ConfigError::ConnectTimeoutWithHttpClient);
        }
        if self
            .credentials
            .as_ref()
            .is_some_and(|credentials| credentials.login.is_empty())
        {
            return Err(ConfigError::EmptyLogin);
        }

        let base_url = normalize_base_url(
            self.base_url
                .as_deref()
                .ok_or(ConfigError::MissingBaseUrl)?,
        )?;
        let (http, request_timeout_per_request) = match self.http_client {
            Some(client) => (client, true),
            None => (
                reqwest::Client::builder()
                    .redirect(Policy::none())
                    .retry(retry::never())
                    .no_proxy()
                    .connect_timeout(self.connect_timeout)
                    .timeout(self.request_timeout)
                    .build()
                    .map_err(|source| ConfigError::HttpClient(Arc::new(source)))?,
                false,
            ),
        };

        Ok(KeeneticClient {
            inner: Arc::new(Inner {
                http,
                base_url,
                credentials: self.credentials,
                cookies: Jar::default(),
                request_timeout: self.request_timeout,
                request_timeout_per_request,
                auth_generation: AtomicU64::new(0),
                auth: Mutex::new(AuthState::default()),
            }),
        })
    }
}

impl Default for KeeneticClientBuilder {
    fn default() -> Self {
        Self {
            base_url: None,
            credentials: None,
            http_client: None,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            connect_timeout_explicit: false,
        }
    }
}

/// An asynchronous client with one independent cookie session and auth state.
///
/// Cloning this value is cheap and shares the transport, cookie jar, and
/// single-flight authentication state. Building another client with a clone of
/// the same `reqwest::Client` shares only the transport connection pool.
#[derive(Clone)]
pub struct KeeneticClient {
    inner: Arc<Inner>,
}

impl KeeneticClient {
    /// Starts a new client builder.
    #[must_use]
    pub fn builder() -> KeeneticClientBuilder {
        KeeneticClientBuilder::default()
    }

    /// Executes a supported typed request.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for transport, authentication, HTTP, RCI-status, JSON
    /// syntax, or response-model failures.
    pub async fn execute<R>(&self, request: R) -> Result<R::Response, Error>
    where
        R: RciRequest,
    {
        let query = request.query();
        let prepared = match request.mode() {
            Mode::Get => match query {
                Some((key, value)) => self.inner.prepare_rci_get_pair(R::ENDPOINT, key, value),
                None => self.inner.prepare_rci_get(R::ENDPOINT, None),
            },
            Mode::PostJson(body) => self.inner.prepare_rci_post(body),
        };
        self.inner.send_json(prepared).await
    }

    /// Starts a continued Network Connection Test operation.
    ///
    /// The initial output is buffered in the returned session. Use
    /// [`NetworkTestSession::next_chunk`] to read output incrementally,
    /// [`NetworkTestSession::collect`] to await all remaining output, or
    /// [`NetworkTestSession::cancel`] to stop an active router command.
    ///
    /// Dropping the session does not send a cancellation request.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when request serialization, transport,
    /// authentication, HTTP, RCI-status, or response decoding fails.
    pub async fn start_network_test<R>(&self, request: R) -> Result<NetworkTestSession, Error>
    where
        R: NetworkTestRequest,
    {
        let endpoint = R::ENDPOINT;
        let body = request.body().map_err(|source| {
            Error::from(JsonSerializationError::new(
                Inner::rci_context(Method::POST, endpoint),
                source,
            ))
        })?;
        let response = self
            .inner
            .send_json::<NetworkTestResponse>(self.inner.prepare_rci_post_at(endpoint, body))
            .await?;
        Ok(NetworkTestSession::new(self.clone(), endpoint, response))
    }

    /// Runs a Network Connection Test and collects its complete output.
    ///
    /// Unlike the per-request transport timeout, this method has no overall
    /// operation timeout. Every supported request is finite by construction.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] from either the initial request or a later poll.
    pub async fn run_network_test<R>(&self, request: R) -> Result<NetworkTestOutput, Error>
    where
        R: NetworkTestRequest,
    {
        let session = self.start_network_test(request).await?;
        session.collect().await
    }

    /// Performs a raw typed JSON GET below `/rci/`.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the request cannot be sent or its response cannot
    /// be checked and decoded as `T`.
    pub async fn get<T>(&self, path: &RciPath) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.inner
            .send_json(self.inner.prepare_rci_get(path.as_str(), None))
            .await
    }

    /// Performs a raw JSON GET and returns the checked JSON value.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the request cannot be sent or its response fails
    /// HTTP, RCI-status, or JSON checks.
    pub async fn get_raw(&self, path: &RciPath) -> Result<Value, Error> {
        self.get(path).await
    }

    /// Performs a typed JSON GET with one-time URL query serialization.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when query serialization fails, the request cannot be
    /// sent, or its response cannot be checked and decoded as `T`.
    pub fn get_with_query<T, Q>(
        &self,
        path: &RciPath,
        query: &Q,
    ) -> impl Future<Output = Result<T, Error>> + Send + '_
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let query = serde_urlencoded::to_string(query).map_err(|source| {
            Error::from(QuerySerializationError::new(
                Inner::rci_context(Method::GET, path.as_str()),
                source,
            ))
        });
        let prepared = query.map(|query| self.inner.prepare_rci_get(path.as_str(), Some(&query)));
        async move { self.inner.send_json(prepared?).await }
    }

    /// Performs a plain-text GET below `/rci/`.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for transport, authentication, or HTTP failures.
    pub async fn get_text(&self, path: &RciPath) -> Result<String, Error> {
        self.inner
            .send_text(self.inner.prepare_rci_get(path.as_str(), None))
            .await
    }

    /// Performs a plain-text GET below `/ci/`.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for transport, authentication, or HTTP failures.
    pub async fn get_ci_text(&self, path: &CiPath) -> Result<String, Error> {
        self.inner
            .send_text(self.inner.prepare_ci_get(path.as_str()))
            .await
    }

    /// Executes one validated command through `POST /rci/parse`.
    ///
    /// This is a single request rather than an interactive or streaming CLI
    /// session. Command-specific fields are retained in [`CliReply`].
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the request cannot be sent or its response fails
    /// HTTP, RCI-status, or JSON checks.
    pub async fn execute_cli(&self, command: &CliCommand) -> Result<CliReply, Error> {
        self.execute_cli_as(command).await
    }

    /// Executes one validated CLI command and decodes its command-specific reply.
    ///
    /// The configured request timeout must be long enough for the router command
    /// to finish. This method does not retry a transport failure.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when request serialization, transport, authentication,
    /// HTTP, RCI-status, or response decoding fails.
    pub fn execute_cli_as<T>(
        &self,
        command: &CliCommand,
    ) -> impl Future<Output = Result<T, Error>> + Send + '_
    where
        T: DeserializeOwned,
    {
        self.post_at_path("parse", command)
    }

    /// Serializes and sends a raw JSON command to `POST /rci/`.
    ///
    /// RCI commands can modify the running configuration. This method does not
    /// save it, read it back, poll for completion, or infer idempotency.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when JSON serialization fails, the request cannot be
    /// sent, or its response cannot be checked and decoded as `T`.
    pub fn post<T, B>(&self, body: &B) -> impl Future<Output = Result<T, Error>> + Send + '_
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_at_path("", body)
    }

    /// Serializes and sends a raw JSON command, returning checked raw JSON.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when JSON serialization fails or the request/response
    /// pipeline fails.
    pub fn post_raw<B>(&self, body: &B) -> impl Future<Output = Result<Value, Error>> + Send + '_
    where
        B: Serialize + ?Sized,
    {
        self.post(body)
    }

    /// Serializes and sends JSON to a specific endpoint below `/rci/`.
    ///
    /// This method does not retry a transport failure or infer whether the
    /// endpoint changes router state.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when JSON serialization fails, the request cannot be
    /// sent, or its response cannot be checked and decoded as `T`.
    pub fn post_at<T, B>(
        &self,
        path: &RciPath,
        body: &B,
    ) -> impl Future<Output = Result<T, Error>> + Send + '_
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_at_path(path.as_str(), body)
    }

    /// Sends JSON to a specific endpoint below `/rci/`, returning checked raw JSON.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when JSON serialization fails or the request/response
    /// pipeline fails.
    pub fn post_at_raw<B>(
        &self,
        path: &RciPath,
        body: &B,
    ) -> impl Future<Output = Result<Value, Error>> + Send + '_
    where
        B: Serialize + ?Sized,
    {
        self.post_at(path, body)
    }

    fn post_at_path<T, B>(
        &self,
        path: &str,
        body: &B,
    ) -> impl Future<Output = Result<T, Error>> + Send + '_
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let body = serde_json::to_vec(body).map_err(|source| {
            Error::from(JsonSerializationError::new(
                Inner::rci_context(Method::POST, path),
                source,
            ))
        });
        let prepared = body.map(|body| self.inner.prepare_rci_post_at(path, body));
        async move { self.inner.send_json(prepared?).await }
    }
}

impl fmt::Debug for KeeneticClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeeneticClient")
            .field("base_url", &self.inner.base_url.as_str())
            .field("request_timeout", &self.inner.request_timeout)
            .field(
                "credentials",
                &self.inner.credentials.as_ref().map(|_| "[REDACTED]"),
            )
            .finish_non_exhaustive()
    }
}

/// One ordered chunk of console output from a Network Connection Test.
///
/// The router's lines are intentionally not parsed because their text and
/// localization are firmware-defined. The same type represents a collected
/// full response.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[must_use = "network-test output should be inspected"]
pub struct NetworkTestOutput {
    lines: Box<[String]>,
}

impl NetworkTestOutput {
    fn new(lines: Vec<String>) -> Self {
        Self {
            lines: lines.into_boxed_slice(),
        }
    }

    /// Returns output lines in router-provided order.
    #[must_use]
    pub const fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Iterates over output lines.
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.lines.iter()
    }

    /// Reports whether this output contains no lines.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Returns the number of output lines.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.lines.len()
    }

    fn append_to(self, target: &mut Vec<String>) {
        target.extend(self.lines.into_vec());
    }
}

impl IntoIterator for NetworkTestOutput {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a NetworkTestOutput {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.iter()
    }
}

impl fmt::Display for NetworkTestOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut lines = self.lines.iter();
        if let Some(first) = lines.next() {
            formatter.write_str(first)?;
            for line in lines {
                formatter.write_str("\n")?;
                formatter.write_str(line)?;
            }
        }
        Ok(())
    }
}

/// A pull-based continued Network Connection Test session.
///
/// The router identifies a continued operation only by its command endpoint;
/// it does not issue a per-session identifier. Concurrent commands of the same
/// type are therefore arbitrated by the router.
#[must_use = "retain the session to read its output or cancel the router command"]
pub struct NetworkTestSession {
    client: KeeneticClient,
    endpoint: &'static str,
    pending: Option<NetworkTestOutput>,
    continued: bool,
    next_poll: Instant,
}

impl NetworkTestSession {
    fn new(client: KeeneticClient, endpoint: &'static str, response: NetworkTestResponse) -> Self {
        let (pending, continued) = response.into_parts();
        Self {
            client,
            endpoint,
            pending,
            continued,
            next_poll: Instant::now() + NETWORK_TEST_POLL_INTERVAL,
        }
    }

    /// Reads the next non-empty output chunk.
    ///
    /// The initial POST response is returned without delay. Empty continued
    /// responses are hidden while polling proceeds.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when a polling request fails.
    pub async fn next_chunk(&mut self) -> Result<Option<NetworkTestOutput>, Error> {
        loop {
            if let Some(output) = self.pending.take() {
                return Ok(Some(output));
            }
            if !self.continued {
                return Ok(None);
            }

            sleep_until(self.next_poll).await;
            let response = self
                .client
                .inner
                .send_json::<NetworkTestResponse>(
                    self.client.inner.prepare_rci_get(self.endpoint, None),
                )
                .await?;
            let (pending, continued) = response.into_parts();
            self.pending = pending;
            self.continued = continued;
            self.next_poll = Instant::now() + NETWORK_TEST_POLL_INTERVAL;
        }
    }

    /// Collects all unread output until the router reports completion.
    ///
    /// If chunks have already been consumed, only the remaining output is
    /// returned.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when a polling request fails.
    pub async fn collect(mut self) -> Result<NetworkTestOutput, Error> {
        let mut lines = Vec::new();
        while let Some(output) = self.next_chunk().await? {
            output.append_to(&mut lines);
        }
        Ok(NetworkTestOutput::new(lines))
    }

    /// Stops an active command and returns all currently unread output.
    ///
    /// No DELETE is sent after the router has already reported completion.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the cancellation request fails.
    pub async fn cancel(self) -> Result<NetworkTestOutput, Error> {
        let Self {
            client,
            endpoint,
            pending,
            continued,
            ..
        } = self;
        let mut lines = Vec::new();
        if let Some(output) = pending {
            output.append_to(&mut lines);
        }
        if continued {
            let response = client
                .inner
                .send_json::<NetworkTestResponse>(client.inner.prepare_rci_delete(endpoint))
                .await?;
            let (pending, _) = response.into_parts();
            if let Some(output) = pending {
                output.append_to(&mut lines);
            }
        }
        Ok(NetworkTestOutput::new(lines))
    }

    /// Reports whether no unread output or further polls remain.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.pending.is_none() && !self.continued
    }
}

impl fmt::Debug for NetworkTestSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkTestSession")
            .field("endpoint", &self.endpoint)
            .field("has_pending_output", &self.pending.is_some())
            .field("continued", &self.continued)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct NetworkTestResponse {
    #[serde(default)]
    message: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_presence")]
    continued: bool,
}

impl NetworkTestResponse {
    fn into_parts(self) -> (Option<NetworkTestOutput>, bool) {
        let output = (!self.message.is_empty()).then(|| NetworkTestOutput::new(self.message));
        (output, self.continued)
    }
}

struct Inner {
    http: reqwest::Client,
    base_url: Url,
    credentials: Option<Credentials>,
    cookies: Jar,
    request_timeout: Duration,
    request_timeout_per_request: bool,
    auth_generation: AtomicU64,
    auth: Mutex<AuthState>,
}

impl Inner {
    fn rci_context(method: Method, path: &str) -> RequestContext {
        RequestContext::new(method, endpoint("rci", path))
    }

    fn prepare_rci_get(&self, path: &str, query: Option<&str>) -> PreparedRequest {
        self.prepare_get("rci", path, query)
    }

    fn prepare_rci_get_pair(&self, path: &str, key: &str, value: &str) -> PreparedRequest {
        let mut prepared = self.prepare_rci_get(path, None);
        prepared.url.query_pairs_mut().append_pair(key, value);
        prepared
    }

    fn prepare_ci_get(&self, path: &str) -> PreparedRequest {
        self.prepare_get("ci", path, None)
    }

    fn prepare_get(&self, namespace: &str, path: &str, query: Option<&str>) -> PreparedRequest {
        let endpoint = endpoint(namespace, path);
        let mut url = self.endpoint_url(&endpoint);
        url.set_query(query);
        PreparedRequest {
            context: RequestContext::new(Method::GET, endpoint),
            url,
            body: None,
        }
    }

    fn prepare_rci_post(&self, body: impl Into<Bytes>) -> PreparedRequest {
        self.prepare_rci_post_at("", body)
    }

    fn prepare_rci_post_at(&self, path: &str, body: impl Into<Bytes>) -> PreparedRequest {
        let endpoint = endpoint("rci", path);
        let url = self.endpoint_url(&endpoint);
        PreparedRequest {
            context: RequestContext::new(Method::POST, endpoint),
            url,
            body: Some(body.into()),
        }
    }

    fn prepare_rci_delete(&self, path: &str) -> PreparedRequest {
        let endpoint = endpoint("rci", path);
        let url = self.endpoint_url(&endpoint);
        PreparedRequest {
            context: RequestContext::new(Method::DELETE, endpoint),
            url,
            body: None,
        }
    }

    async fn send_json<T>(&self, request: PreparedRequest) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let response = self.send_authenticated(&request).await?;
        if !response.status().is_success() {
            return Err(HttpError::new(request.context.clone(), response.status()).into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|source| TransportError::new(request.context.clone(), source))?;
        decode_json_response(&bytes, &request.context)
    }

    async fn send_text(&self, request: PreparedRequest) -> Result<String, Error> {
        let response = self.send_authenticated(&request).await?;
        if !response.status().is_success() {
            return Err(HttpError::new(request.context.clone(), response.status()).into());
        }
        response
            .text()
            .await
            .map_err(|source| TransportError::new(request.context, source).into())
    }

    async fn send_authenticated(&self, request: &PreparedRequest) -> Result<Response, Error> {
        let observed_generation = self.auth_generation.load(Ordering::Acquire);
        let response = self.send_once(request).await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        if self.credentials.is_none() {
            return Err(AuthenticationError::Required.into());
        }

        self.authenticate(observed_generation).await?;
        let response = self.send_once(request).await?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(AuthenticationError::UnauthorizedAfterAuthentication.into());
        }
        Ok(response)
    }

    async fn authenticate(&self, observed_generation: u64) -> Result<(), Error> {
        let mut state = self.auth.lock().await;
        if state.generation != observed_generation {
            return state
                .last_result
                .clone()
                .expect("a completed generation must retain its result");
        }

        let result = self.perform_authentication().await;
        state.generation = state.generation.wrapping_add(1);
        state.last_result = Some(result.clone());
        self.auth_generation
            .store(state.generation, Ordering::Release);
        result
    }

    async fn perform_authentication(&self) -> Result<(), Error> {
        let credentials = self
            .credentials
            .as_ref()
            .expect("authentication is only attempted with credentials");
        let get = self.prepare_auth(Method::GET, None);
        let response = self.send_once(&get).await?;
        if response.status() == StatusCode::OK {
            return Ok(());
        }
        if response.status() != StatusCode::UNAUTHORIZED {
            return Err(HttpError::new(get.context, response.status()).into());
        }

        let (realm, challenge) = parse_challenge(response.headers())?;
        let password = response_hash(&credentials.login, realm, &credentials.password, challenge);
        let body = serde_json::to_vec(&AuthPayload {
            login: &credentials.login,
            password: password.as_str(),
        })
        .map_err(|source| JsonSerializationError::new(Self::auth_context(Method::POST), source))?;
        let post = self.prepare_auth(Method::POST, Some(body.into()));
        let response = self.send_once(&post).await?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(AuthenticationError::RejectedCredentials.into());
        }
        if !response.status().is_success() {
            return Err(HttpError::new(post.context, response.status()).into());
        }
        Ok(())
    }

    fn auth_context(method: Method) -> RequestContext {
        RequestContext::new(method, AUTH_ENDPOINT)
    }

    fn prepare_auth(&self, method: Method, body: Option<Bytes>) -> PreparedRequest {
        PreparedRequest {
            context: Self::auth_context(method),
            url: self.endpoint_url(AUTH_ENDPOINT),
            body,
        }
    }

    fn endpoint_url(&self, endpoint: &str) -> Url {
        let mut url = self.base_url.clone();
        url.set_path(endpoint);
        url
    }

    async fn send_once(&self, request: &PreparedRequest) -> Result<Response, Error> {
        let mut builder = self
            .http
            .request(request.context.method().clone(), request.url.clone());
        if self.request_timeout_per_request {
            builder = builder.timeout(self.request_timeout);
        }
        if let Some(cookie) = self.cookies.cookies(&request.url) {
            builder = builder.header(COOKIE, cookie);
        }
        if let Some(body) = &request.body {
            builder = builder
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        let response = builder
            .send()
            .await
            .map_err(|source| TransportError::new(request.context.clone(), source))?;
        if response.headers().contains_key(SET_COOKIE) {
            let mut set_cookies = response.headers().get_all(SET_COOKIE).iter();
            self.cookies.set_cookies(&mut set_cookies, response.url());
        }
        Ok(response)
    }
}

#[derive(Default)]
struct AuthState {
    generation: u64,
    last_result: Option<Result<(), Error>>,
}

struct PreparedRequest {
    context: RequestContext,
    url: Url,
    body: Option<Bytes>,
}

fn deserialize_presence<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    IgnoredAny::deserialize(deserializer).map(|_| true)
}

fn decode_json_response<T>(bytes: &[u8], context: &RequestContext) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    if needs_rci_inspection(bytes) {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|source| ResponseJsonError::new(context.clone(), source))?;
        let mut entries = Vec::new();
        collect_rci_errors(&value, &mut entries);
        if !entries.is_empty() {
            return Err(RciError::new(context.clone(), entries).into());
        }
        return serde_json::from_value(value)
            .map_err(|source| ResponseDeserializationError::new(context.clone(), source).into());
    }

    serde_json::from_slice(bytes).map_err(|source| match source.classify() {
        serde_json::error::Category::Data => {
            ResponseDeserializationError::new(context.clone(), source).into()
        }
        serde_json::error::Category::Io
        | serde_json::error::Category::Syntax
        | serde_json::error::Category::Eof => {
            ResponseJsonError::new(context.clone(), source).into()
        }
    })
}

fn needs_rci_inspection(bytes: &[u8]) -> bool {
    // Modem fields commonly contain escaped newlines. Match exact JSON string
    // tokens so unrelated escapes stay on the direct typed-decoding path.
    contains_json_string(bytes, b"status", br#""status""#)
        && contains_json_string(bytes, b"error", br#""error""#)
}

fn contains_json_string(bytes: &[u8], expected: &[u8], literal: &[u8]) -> bool {
    if memchr::memmem::find(bytes, literal).is_some() {
        return true;
    }
    if memchr::memchr(b'\\', bytes).is_none() {
        return false;
    }

    let mut remaining = bytes;
    while let Some(quote) = memchr::memchr(b'"', remaining) {
        remaining = &remaining[quote + 1..];
        if json_string_matches(remaining, expected) {
            return true;
        }
    }
    false
}

fn json_string_matches(mut bytes: &[u8], expected: &[u8]) -> bool {
    for &expected_byte in expected {
        if bytes.first() == Some(&expected_byte) {
            bytes = &bytes[1..];
            continue;
        }
        let Some(digits) = bytes.strip_prefix(br"\u").and_then(|bytes| bytes.get(..4)) else {
            return false;
        };
        let Some(decoded) = digits.iter().try_fold(0_u16, |value, &digit| {
            json_hex_value(digit).map(|digit| (value << 4) | digit)
        }) else {
            return false;
        };
        if decoded != u16::from(expected_byte) {
            return false;
        }
        bytes = &bytes[6..];
    }
    bytes.first() == Some(&b'"')
}

const fn json_hex_value(byte: u8) -> Option<u16> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u16),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u16),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u16),
        _ => None,
    }
}

fn normalize_base_url(value: &str) -> Result<Url, ConfigError> {
    let (_, remainder) = value
        .split_once("://")
        .ok_or_else(|| ConfigError::InvalidBaseUrl("relative URL without a base".into()))?;
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    if remainder[..authority_end].contains('@') {
        return Err(ConfigError::EmbeddedCredentials);
    }
    if !matches!(&remainder[authority_end..], "" | "/") {
        return Err(ConfigError::NotAnOrigin);
    }

    let mut url =
        Url::parse(value).map_err(|error| ConfigError::InvalidBaseUrl(error.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(ConfigError::UnsupportedScheme(scheme.to_owned())),
    }
    if url.host().is_none() {
        return Err(ConfigError::MissingHost);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ConfigError::EmbeddedCredentials);
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return Err(ConfigError::NotAnOrigin);
    }
    url.set_path("/");
    Ok(url)
}

fn endpoint(namespace: &str, path: &str) -> String {
    if path.is_empty() {
        format!("/{namespace}/")
    } else {
        format!("/{namespace}/{path}")
    }
}

fn parse_challenge(headers: &HeaderMap) -> Result<(&str, &str), AuthenticationError> {
    let realm = headers.get(REALM_HEADER);
    let challenge = headers.get(CHALLENGE_HEADER);
    let (realm, challenge) = match (realm, challenge) {
        (None, None) => return Err(AuthenticationError::Unsupported),
        (Some(realm), Some(challenge)) => (realm, challenge),
        _ => {
            return Err(AuthenticationError::MalformedResponse(
                MalformedAuthReason::IncompleteHeaders,
            ));
        }
    };
    let realm = realm.to_str().map_err(|_| {
        AuthenticationError::MalformedResponse(MalformedAuthReason::InvalidHeader(REALM_HEADER))
    })?;
    let challenge = challenge.to_str().map_err(|_| {
        AuthenticationError::MalformedResponse(MalformedAuthReason::InvalidHeader(CHALLENGE_HEADER))
    })?;
    if realm.is_empty() {
        return Err(AuthenticationError::MalformedResponse(
            MalformedAuthReason::EmptyHeader(REALM_HEADER),
        ));
    }
    if challenge.is_empty() {
        return Err(AuthenticationError::MalformedResponse(
            MalformedAuthReason::EmptyHeader(CHALLENGE_HEADER),
        ));
    }
    Ok((realm, challenge))
}

fn collect_rci_errors(value: &Value, entries: &mut Vec<RciStatusEntry>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key == "status"
                    && let Value::Array(statuses) = child
                {
                    entries.extend(statuses.iter().filter_map(|status| {
                        let object = status.as_object()?;
                        let status = object.get("status")?.as_str()?;
                        (status == "error").then(|| RciStatusEntry {
                            code: object.get("code").and_then(Value::as_str).map(Box::from),
                            ident: object.get("ident").and_then(Value::as_str).map(Box::from),
                            message: object.get("message").and_then(Value::as_str).map(Box::from),
                        })
                    }));
                }
                collect_rci_errors(child, entries);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_rci_errors(child, entries);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::StatusCode;

    use super::{KeeneticClient, needs_rci_inspection, parse_challenge};
    use crate::{AuthenticationError, ConfigError, MalformedAuthReason};

    #[test]
    fn validates_and_normalizes_origins_without_io() {
        let client = KeeneticClient::builder()
            .base_url("http://127.0.0.1:9")
            .build()
            .unwrap();
        assert!(format!("{client:?}").contains("http://127.0.0.1:9/"));

        for invalid in [
            "ftp://router",
            "http://user:password@router",
            "http://@router",
            "http://router/rci",
            "http://router/.",
            "http://router/base/..",
            "http://router/?query=value",
            "http://router/#fragment",
        ] {
            assert!(KeeneticClient::builder().base_url(invalid).build().is_err());
        }
    }

    #[test]
    fn rejects_connect_timeout_for_injected_transport() {
        let result = KeeneticClient::builder()
            .base_url("http://router")
            .http_client(reqwest::Client::new())
            .connect_timeout(Duration::from_secs(1))
            .build();
        assert!(matches!(
            result,
            Err(ConfigError::ConnectTimeoutWithHttpClient)
        ));
    }

    #[test]
    fn client_is_send_and_sync() {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KeeneticClient>();
    }

    #[test]
    fn rci_prefilter_detects_literal_and_potentially_escaped_errors() {
        assert!(needs_rci_inspection(br#"{"status":[{"status":"error"}]}"#));
        assert!(needs_rci_inspection(
            br#"{"sta\u0074us":[{"status":"\u0065rror"}]}"#
        ));
        assert!(!needs_rci_inspection(br#"{"result":"error"}"#));
        assert!(!needs_rci_inspection(
            br#"{"message":"no error was reported"}"#
        ));
        assert!(!needs_rci_inspection(
            br#"{"revision":"first\r\nsecond","status":"message"}"#
        ));
        assert!(!needs_rci_inspection(br#"{"status":"errorish"}"#));
    }

    #[test]
    fn classifies_auth_headers() {
        let empty = reqwest::header::HeaderMap::new();
        assert_eq!(
            parse_challenge(&empty),
            Err(AuthenticationError::Unsupported)
        );

        let mut incomplete = reqwest::header::HeaderMap::new();
        incomplete.insert("x-ndm-realm", "router".parse().unwrap());
        assert_eq!(
            parse_challenge(&incomplete),
            Err(AuthenticationError::MalformedResponse(
                MalformedAuthReason::IncompleteHeaders
            ))
        );

        let _ = StatusCode::OK;
    }
}
