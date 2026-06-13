//! Mock exchange/forex response data shared by the test harnesses.
//!
//! These types describe a canned HTTP response for a given outcall URL. They are consumed
//! both by the legacy Docker/nginx harness (which writes them to files nginx serves) and by
//! the PocketIC harness (which feeds them back via `mock_canister_http_response`). They live
//! here, independent of either harness, so the `mock_responses` dataset compiles on its own.

use serde::Serialize;

/// The body contents for an exchange response.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub enum ResponseBody {
    /// Signifies that the body is JSON.
    Json(Vec<u8>),
    /// Signifies that the body is XML.
    #[allow(dead_code)]
    Xml(Vec<u8>),
    /// Signifies that the body has not been set.
    Empty,
}

impl core::fmt::Display for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseBody::Json(_) => write!(f, "json"),
            ResponseBody::Xml(_) => write!(f, "xml"),
            ResponseBody::Empty => write!(f, "empty"),
        }
    }
}

impl Default for ResponseBody {
    fn default() -> Self {
        Self::Empty
    }
}

/// A canned response that is served to the `xrc` canister when it asks for rates from
/// various exchanges or forex sources.
pub struct ExchangeResponse {
    /// Name of the exchange.
    pub name: String,
    /// The URL that will be accessed.
    pub url: String,
    /// The HTTP status code of the response.
    pub status_code: u16,
    /// A body that the response may serve.
    pub body: ResponseBody,
    /// A delay to slow down the response from being delivered.
    pub delay_secs: u64,
}

impl ExchangeResponse {
    /// Returns a builder to conveniently set up a response.
    pub fn builder() -> ExchangeResponseBuilder {
        Default::default()
    }
}

impl Default for ExchangeResponse {
    fn default() -> Self {
        Self {
            name: Default::default(),
            url: Default::default(),
            status_code: 200,
            body: Default::default(),
            delay_secs: Default::default(),
        }
    }
}

/// Used to build an [ExchangeResponse].
#[derive(Default)]
pub struct ExchangeResponseBuilder {
    response: ExchangeResponse,
}

impl ExchangeResponseBuilder {
    /// Returns the completed response.
    pub fn build(self) -> ExchangeResponse {
        self.response
    }

    /// Set the name of the exchange.
    pub fn name(mut self, name: String) -> Self {
        self.response.name = name;
        self
    }

    /// Set the endpoint's url that will return the response.
    pub fn url(mut self, url: String) -> Self {
        self.response.url = url;
        self
    }

    #[allow(dead_code)]
    pub fn status_code(mut self, status_code: u16) -> Self {
        self.response.status_code = status_code;
        self
    }

    pub fn body(mut self, body: ResponseBody) -> Self {
        self.response.body = body;
        self
    }

    #[allow(dead_code)]
    pub fn delay_secs(mut self, delay_secs: u64) -> Self {
        self.response.delay_secs = delay_secs;
        self
    }
}
