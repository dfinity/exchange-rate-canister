mod utils;

use std::collections::HashMap;

use serde::Serialize;
use thiserror::Error;

use self::utils::{
    compose_build_and_up, compose_exec, compose_stop, install_canister, setup_log_directory,
    setup_nginx_directory, verify_nginx_is_running, verify_replica_is_running,
    InstallCanisterError, SetupNginxDirectoryError, VerifyNginxIsRunningError,
    VerifyReplicaIsRunningError,
};

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

/// A response from the `e2e` container's nginx process that is given back to
/// the `xrc` canister when asking for rates from various exchanges.
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

#[derive(Default)]
struct ContainerConfig {
    name: String,
    exchange_responses: Vec<ExchangeResponse>,
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
    #[error("Failed while calling the canister: {0}")]
    Canister(String),
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

    /// Provides the ability to call endpoints on the `xrc` canister.
    pub fn call_canister<Input, Output>(
        &self,
        method_name: &str,
        arg: &Input,
    ) -> Result<Output, CallCanisterError>
    where
        Input: candid::CandidType,
        Output: candid::CandidType + serde::de::DeserializeOwned,
    {
        let (stdout, stderr) =
            compose_exec(self, "dfx identity get-wallet").map_err(CallCanisterError::Io)?;
        if !stderr.is_empty() {
            return Err(CallCanisterError::Canister(format!(
                "Error while getting wallet ID: {}",
                stderr
            )));
        }
        let wallet_id = stdout.trim();

        let encoded = candid::encode_one(arg).map_err(CallCanisterError::Candid)?;
        let payload = hex::encode(encoded);
        let cmd = format!(
            "dfx canister call --wallet {} --with-cycles {} --type raw --output raw xrc {} {}",
            wallet_id,
            xrc::XRC_REQUEST_CYCLES_COST,
            method_name,
            payload
        );
        let (stdout, stderr) = compose_exec(self, &cmd).map_err(CallCanisterError::Io)?;
        if !stderr.is_empty() {
            return Err(CallCanisterError::Canister(stderr));
        }

        let output = stdout.trim_end();
        let bytes = hex::decode(output).map_err(CallCanisterError::Hex)?;

        candid::decode_one(&bytes).map_err(CallCanisterError::Candid)
    }
}

/// Used to ensure the that there is at least 1 attempt to stop the actual container process.
impl Drop for Container {
    fn drop(&mut self) {
        compose_stop(self)
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
            let mut query_params_fragments = url
                .query_pairs()
                .map(|(key, value)| format!("{}_{}", key, value))
                .collect::<Vec<_>>();
            query_params_fragments.sort();
            let query_params = query_params_fragments.join("_");
            let path = url.path().to_string();
            match exchange_responses.get_mut(&host) {
                Some(c) => c.locations.push(ContainerNginxServerLocationConfig {
                    body: response.body,
                    status_code: response.status_code,
                    path,
                    query_params,
                }),
                None => {
                    let host_clone = host.clone();
                    exchange_responses.insert(
                        host,
                        ContainerNginxServerConfig {
                            name: response.name,
                            host: host_clone,
                            locations: vec![ContainerNginxServerLocationConfig {
                                body: response.body,
                                path,
                                status_code: response.status_code,
                                query_params,
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
    /// Maybe contain a response body to be served to the canister.
    body: ResponseBody,
    /// The status code nginx should return to a request.
    status_code: u16,
    /// The path portion of the URL (/a/b/c).
    path: String,
    /// The query parameters separated by `_` (start_1234_end_1234) for use when
    /// generating the path to the JSON response.
    query_params: String,
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

/// Errors for when the [run_scenario] function fails.
#[derive(Debug, Error)]
pub enum RunScenarioError {
    /// Used when the container could not be started.
    #[error("Failed to start container: {0}")]
    FailedToStartContainer(std::io::Error),
    /// Used when the nginx directory could not be set up correctly.
    #[error("Attempted to setup the nginx directory: {0}")]
    SetupNginxDirectory(SetupNginxDirectoryError),
    /// Used when the log directory could not be set up correctly.
    #[error("Failed to setup log directories: {0}")]
    SetupLogDirectory(std::io::Error),
    /// Used when nginx is not up and running in a given amount of time.
    #[error("Failed to verify nginx is running: {0}")]
    VerifyNginxIsRunningFailed(VerifyNginxIsRunningError),
    /// Used when the replica is not up and running in a given amount of time.
    #[error("Failed to verify replica is running: {0}")]
    VerifyReplicaIsRunningFailed(VerifyReplicaIsRunningError),
    /// Used when the canister fails to install.
    #[error("Failed to install the canister: {0}")]
    FailedToInstallCanister(InstallCanisterError),
    /// An error occurred while sending a command to the container.
    #[error("Failed to run scenario due to i/o error: {0}")]
    Scenario(std::io::Error),
}

/// Given a container instance and a scenario function, this function will create
/// the actual container, start it, verify that the replica and nginx are running,
/// and install the `xrc` canister. It then executes the scenario function allowing
/// the tester to interact with the container to perform the needed test.
/// It finally exits. As the container is moved into the function, it will be dropped.
/// When the drop occurs, a command will be issued to stop the running container process.
pub fn run_scenario<F, R>(container: Container, scenario: F) -> Result<R, RunScenarioError>
where
    F: FnOnce(&Container) -> std::io::Result<R>,
{
    setup_nginx_directory(&container).map_err(RunScenarioError::SetupNginxDirectory)?;
    setup_log_directory(&container).map_err(RunScenarioError::SetupLogDirectory)?;

    compose_build_and_up(&container).map_err(RunScenarioError::FailedToStartContainer)?;

    verify_nginx_is_running(&container).map_err(RunScenarioError::VerifyNginxIsRunningFailed)?;
    verify_replica_is_running(&container)
        .map_err(RunScenarioError::VerifyReplicaIsRunningFailed)?;
    install_canister(&container).map_err(RunScenarioError::FailedToInstallCanister)?;

    std::thread::sleep(std::time::Duration::from_secs(10));
    let output = scenario(&container).map_err(RunScenarioError::Scenario)?;
    Ok(output)
}
