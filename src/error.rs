use std::{borrow::Cow, fmt, sync::Arc};

use reqwest::{Method, StatusCode};
use thiserror::Error;

/// Errors produced while constructing a client.
#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// No base URL was configured.
    #[error("a base URL is required")]
    MissingBaseUrl,
    /// The base URL is not a valid URL.
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    /// Only HTTP and HTTPS origins are supported.
    #[error("unsupported base URL scheme `{0}`; only http and https are supported")]
    UnsupportedScheme(String),
    /// The URL does not contain a host.
    #[error("the base URL must contain a host")]
    MissingHost,
    /// User information must not be embedded in the URL.
    #[error("the base URL must not contain embedded credentials")]
    EmbeddedCredentials,
    /// The base URL must describe an origin rather than an endpoint.
    #[error("the base URL must not contain a path, query, or fragment")]
    NotAnOrigin,
    /// The username is empty.
    #[error("the login must not be empty")]
    EmptyLogin,
    /// A connect timeout cannot affect an injected transport.
    #[error("connect_timeout cannot be combined with an injected reqwest client")]
    ConnectTimeoutWithHttpClient,
    /// A timeout must be greater than zero.
    #[error("{0} must be greater than zero")]
    ZeroTimeout(&'static str),
    /// The internal transport could not be built.
    #[error("failed to build the internal HTTP client")]
    HttpClient(#[source] Arc<reqwest::Error>),
}

/// Authentication failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum AuthenticationError {
    /// The endpoint requires authentication but no credentials were provided.
    #[error("authentication is required, but no credentials were configured")]
    Required,
    /// The challenge scheme is not the supported LAN protocol.
    #[error(
        "the server did not provide a supported LAN authentication challenge; KeenDNS authentication is not supported"
    )]
    Unsupported,
    /// The `/auth` challenge response was incomplete or malformed.
    #[error("the authentication challenge response is malformed: {0}")]
    MalformedResponse(MalformedAuthReason),
    /// The server rejected the challenge-response credentials.
    #[error("the server rejected the supplied credentials")]
    RejectedCredentials,
    /// The original request remained unauthorized after a successful login.
    #[error("the endpoint remained unauthorized after successful authentication")]
    UnauthorizedAfterAuthentication,
}

/// Why an authentication response was malformed.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum MalformedAuthReason {
    /// One challenge header was present without the other.
    #[error("the challenge headers are incomplete")]
    IncompleteHeaders,
    /// A challenge header was not valid visible text.
    #[error("header `{0}` is not valid text")]
    InvalidHeader(&'static str),
    /// A challenge header was empty.
    #[error("header `{0}` is empty")]
    EmptyHeader(&'static str),
}

/// Errors returned by client operations.
#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Query serialization failed before any network request.
    #[error(transparent)]
    QuerySerialization(#[from] QuerySerializationError),
    /// JSON body serialization failed before any network request.
    #[error(transparent)]
    JsonSerialization(#[from] JsonSerializationError),
    /// The HTTP transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Authentication could not be completed.
    #[error(transparent)]
    Authentication(#[from] AuthenticationError),
    /// The response used an unsuccessful HTTP status.
    #[error(transparent)]
    Http(#[from] HttpError),
    /// RCI reported errors inside a successful response.
    #[error(transparent)]
    Rci(#[from] RciError),
    /// The response body was not valid JSON.
    #[error(transparent)]
    ResponseJson(#[from] ResponseJsonError),
    /// The JSON response did not match the target type.
    #[error(transparent)]
    ResponseDeserialization(#[from] ResponseDeserializationError),
}
/// The operation and endpoint associated with an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    method: Method,
    endpoint: Cow<'static, str>,
}

impl RequestContext {
    pub(crate) fn new(method: Method, endpoint: impl Into<Cow<'static, str>>) -> Self {
        Self {
            method,
            endpoint: endpoint.into(),
        }
    }

    /// Returns the HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the normalized endpoint path without query values.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl fmt::Display for RequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.method, self.endpoint)
    }
}

/// A query could not be serialized before sending the request.
#[derive(Clone, Error)]
#[error("failed to serialize query for {context}")]
pub struct QuerySerializationError {
    context: RequestContext,
    #[source]
    source: Arc<serde_urlencoded::ser::Error>,
}

impl QuerySerializationError {
    pub(crate) fn new(context: RequestContext, source: serde_urlencoded::ser::Error) -> Self {
        Self {
            context,
            source: Arc::new(source),
        }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl fmt::Debug for QuerySerializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QuerySerializationError")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

/// A JSON request body could not be serialized before sending the request.
#[derive(Clone, Error)]
#[error("failed to serialize JSON body for {context}")]
pub struct JsonSerializationError {
    context: RequestContext,
    #[source]
    source: Arc<serde_json::Error>,
}

impl JsonSerializationError {
    pub(crate) fn new(context: RequestContext, source: serde_json::Error) -> Self {
        Self {
            context,
            source: Arc::new(source),
        }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl fmt::Debug for JsonSerializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JsonSerializationError")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

/// A request failed at the HTTP transport layer.
#[derive(Clone, Error)]
#[error("transport failure while sending {context}")]
pub struct TransportError {
    context: RequestContext,
    #[source]
    source: Arc<reqwest::Error>,
}

impl TransportError {
    pub(crate) fn new(context: RequestContext, source: reqwest::Error) -> Self {
        Self {
            context,
            source: Arc::new(source),
        }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }

    /// Reports whether the transport classified the failure as a timeout.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.source.is_timeout()
    }
}

impl fmt::Debug for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransportError")
            .field("context", &self.context)
            .field("is_timeout", &self.is_timeout())
            .finish_non_exhaustive()
    }
}

/// A response used an unsuccessful HTTP status.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("HTTP {status} from {context}")]
pub struct HttpError {
    context: RequestContext,
    status: StatusCode,
}

impl HttpError {
    pub(crate) const fn new(context: RequestContext, status: StatusCode) -> Self {
        Self { context, status }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }

    /// Returns the response status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }
}

/// An error entry embedded in an otherwise successful RCI HTTP response.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RciStatusEntry {
    /// Router-specific error code.
    pub code: Option<Box<str>>,
    /// Router-specific component identifier.
    pub ident: Option<Box<str>>,
    /// Human-readable router message.
    pub message: Option<Box<str>>,
}

/// One or more RCI errors embedded in a JSON response.
#[derive(Clone, Error, Eq, PartialEq)]
#[error("RCI returned {} error status entr{} for {context}", entries.len(), if entries.len() == 1 { "y" } else { "ies" })]
pub struct RciError {
    context: RequestContext,
    entries: Vec<RciStatusEntry>,
}

impl RciError {
    pub(crate) const fn new(context: RequestContext, entries: Vec<RciStatusEntry>) -> Self {
        Self { context, entries }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }

    /// Returns every discovered error entry in traversal order.
    #[must_use]
    pub fn entries(&self) -> &[RciStatusEntry] {
        &self.entries
    }
}

impl fmt::Debug for RciError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RciError")
            .field("context", &self.context)
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

/// A successful HTTP response was not syntactically valid JSON.
#[derive(Clone, Error)]
#[error("invalid JSON response from {context}")]
pub struct ResponseJsonError {
    context: RequestContext,
    #[source]
    source: Arc<serde_json::Error>,
}

impl ResponseJsonError {
    pub(crate) fn new(context: RequestContext, source: serde_json::Error) -> Self {
        Self {
            context,
            source: Arc::new(source),
        }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl fmt::Debug for ResponseJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseJsonError")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

/// Valid JSON did not match the requested response type.
#[derive(Clone, Error)]
#[error("JSON response from {context} does not match the requested type")]
pub struct ResponseDeserializationError {
    context: RequestContext,
    #[source]
    source: Arc<serde_json::Error>,
}

impl ResponseDeserializationError {
    pub(crate) fn new(context: RequestContext, source: serde_json::Error) -> Self {
        Self {
            context,
            source: Arc::new(source),
        }
    }

    /// Returns the non-sensitive request context.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl fmt::Debug for ResponseDeserializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseDeserializationError")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}
