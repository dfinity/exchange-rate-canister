use std::collections::HashMap;

use serde::Serialize;
use thiserror::Error;

/// A response from the `e2e` container's nginx process that is given back to
/// the `xrc` canister when asking for rates from various exchanges.
pub struct ExchangeResponse {
    /// Name of the exchange.
    pub name: String,
    /// The URL that will be accessed.
    pub url: String,
    /// The HTTP status code of the response.
    pub status_code: u16,
    /// A JSON body that the response may serve.
    pub maybe_json: Option<serde_json::Value>,
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
            maybe_json: Default::default(),
            delay_secs: Default::default(),
        }
    }
}

/// Used to build a [ExchangeResponse] for that will be used to serve a response
/// from the container's nginx process.
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

    /// TODO: allow varying status codes to test for misbehavior.
    #[allow(dead_code)]
    pub fn status_code(mut self, status_code: u16) -> Self {
        self.response.status_code = status_code;
        self
    }

    /// Set the response's JSON body.
    pub fn json(mut self, json: serde_json::Value) -> Self {
        self.response.maybe_json = Some(json);
        self
    }

    /// TODO: implement a nginx delay for caching test
    #[allow(dead_code)]
    pub fn delay_secs(mut self, delay_secs: u64) -> Self {
        self.response.delay_secs = delay_secs;
        self
    }
}

#[derive(Default)]
struct ContainerConfig {
    name: String,
    exchange_responses: Vec<ExchangeResponse>,
}

/// This struct contains the result from the canister and metadata about the call
/// to the canister.
#[derive(Debug)]
pub struct CallCanisterOutput<T: candid::CandidType> {
    /// The actual result from the canister.
    pub result: T,
}

/// Represents the possible errors returned when calling the canister.
#[derive(Debug, Error)]
pub enum CallCanisterError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Candid(candid::Error),
    #[error("Tried to decode payload from canister: {0}")]
    Hex(hex::FromHexError),
}

/// This struct represents a running `e2e` container that includes a replica and nginx.
pub struct Container {
    name: String,
    responses: HashMap<String, ContainerNginxServerConfig>,
}

impl Container {
    /// Starts a builder chain to configure how an `e2e` container should be configured.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }
}

impl From<ContainerConfig> for Container {
    fn from(config: ContainerConfig) -> Self {
        let mut exchange_responses: HashMap<String, ContainerNginxServerConfig> = HashMap::new();

        // Transform the exchange responses into something consumable for template rendering
        // the nginx.conf and init.sh entrypoint.
        for response in config.exchange_responses {
            let url = url::Url::parse(&response.url).expect("failed to parse url");
            let host = url.host().expect("Failed to get host").to_string();
            match exchange_responses.get_mut(&host) {
                Some(c) => c.locations.push(ContainerNginxServerLocationConfig {
                    maybe_json: response.maybe_json,
                    status_code: response.status_code,
                    path: url.path().to_string(),
                }),
                None => {
                    let host_clone = host.clone();
                    exchange_responses.insert(
                        host,
                        ContainerNginxServerConfig {
                            name: response.name,
                            host: host_clone,
                            locations: vec![ContainerNginxServerLocationConfig {
                                maybe_json: response.maybe_json,
                                path: url.path().to_string(),
                                status_code: response.status_code,
                            }],
                        },
                    );
                }
            }
        }

        Self {
            name: config.name,
            responses: exchange_responses,
        }
    }
}

/// Represents a `server` section in the nginx config.
#[derive(Debug, Serialize)]
struct ContainerNginxServerConfig {
    /// Name of the config.
    name: String,
    /// Domain part of the URL.
    host: String,
    /// The paths under the server.
    locations: Vec<ContainerNginxServerLocationConfig>,
}

/// Represents a `location` block in the `server` section of an nginx config.
#[derive(Debug, Serialize)]
struct ContainerNginxServerLocationConfig {
    /// May contain a JSON value. The actual content to be served.
    maybe_json: Option<serde_json::Value>,
    /// The status code nginx should return to a request.
    status_code: u16,
    /// The path portion of the URL (/a/b/c).
    path: String,
}

/// Used to build a [Container] in order to run tests against the `xrc` canister.
pub struct ContainerBuilder {
    config: ContainerConfig,
}

impl ContainerBuilder {
    fn new() -> Self {
        Self {
            config: ContainerConfig::default(),
        }
    }

    /// Used to set the name of the container when ran.
    /// Suggested use is to use the name of the test.
    pub fn name<S>(mut self, name: S) -> Self
    where
        S: Into<String>,
    {
        self.config.name = name.into();
        self
    }

    /// Used to set the responses that will be served from the `nginx` process
    /// in the container.
    pub fn exchange_responses(mut self, responses: Vec<ExchangeResponse>) -> Self {
        self.config.exchange_responses = responses;
        self
    }

    /// Creates a [Container] from the defined config.
    pub fn build(self) -> Container {
        Container::from(self.config)
    }
}
