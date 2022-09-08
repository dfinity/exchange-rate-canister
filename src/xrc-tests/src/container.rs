use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use serde::Serialize;
use thiserror::Error;

use crate::templates;

pub struct ExchangeResponse {
    pub name: String,
    pub url: String,
    pub status_code: u16,
    pub maybe_json: Option<serde_json::Value>,
    pub delay_secs: u64,
}

impl ExchangeResponse {
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

#[derive(Default)]
pub struct ExchangeResponseBuilder {
    response: ExchangeResponse,
}

impl ExchangeResponseBuilder {
    pub fn build(self) -> ExchangeResponse {
        self.response
    }

    pub fn name(mut self, name: String) -> Self {
        self.response.name = name;
        self
    }

    pub fn url(mut self, url: String) -> Self {
        self.response.url = url;
        self
    }

    #[allow(dead_code)]
    pub fn status_code(mut self, status_code: u16) -> Self {
        self.response.status_code = status_code;
        self
    }

    pub fn json(mut self, json: serde_json::Value) -> Self {
        self.response.maybe_json = Some(json);
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

    /// Provides an entrypoint into the container so the canister can be called.
    pub fn call_canister<Tuple, C>(
        &self,
        method_name: &str,
        args: Tuple,
    ) -> Result<CallCanisterOutput<C>, CallCanisterError>
    where
        Tuple: candid::utils::ArgumentEncoder,
        C: candid::CandidType + serde::de::DeserializeOwned,
    {
        let encoded = candid::encode_args(args).map_err(CallCanisterError::Candid)?;
        let payload = hex::encode(encoded);
        let cmd = format!(
            "dfx canister call --type raw --output raw xrc {} {}",
            method_name, payload
        );
        let (stdout, stderr) = compose_exec(self, &cmd).map_err(CallCanisterError::Io)?;
        let output = stdout.trim_end();
        let bytes = hex::decode(output).map_err(CallCanisterError::Hex)?;

        Ok(CallCanisterOutput {
            result: candid::decode_one(&bytes).map_err(CallCanisterError::Candid)?,
        })
    }
}

/// Used to ensure the that there is at least 1 attempt to stop the actual container process.
impl Drop for Container {
    fn drop(&mut self) {
        compose_stop(&self)
    }
}

impl From<ContainerConfig> for Container {
    fn from(config: ContainerConfig) -> Self {
        let mut exchange_responses: HashMap<String, ContainerNginxServerConfig> = HashMap::new();

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

#[derive(Debug, Serialize)]
struct ContainerNginxServerConfig {
    name: String,
    host: String,
    locations: Vec<ContainerNginxServerLocationConfig>,
}

#[derive(Debug, Serialize)]
struct ContainerNginxServerLocationConfig {
    maybe_json: Option<serde_json::Value>,
    status_code: u16,
    path: String,
}

pub struct ContainerBuilder {
    config: ContainerConfig,
}

impl ContainerBuilder {
    fn new() -> Self {
        Self {
            config: ContainerConfig::default(),
        }
    }

    pub fn name<S>(mut self, name: S) -> Self
    where
        S: Into<String>,
    {
        self.config.name = name.into();
        self
    }

    pub fn exchange_responses(mut self, responses: Vec<ExchangeResponse>) -> Self {
        self.config.exchange_responses = responses;
        self
    }

    pub fn build(self) -> Container {
        Container::from(self.config)
    }
}

#[derive(Debug, Error)]
pub enum RunScenarioError {
    #[error("Failed to start container: {0}")]
    FailedToStartContainer(std::io::Error),
    #[error("Attempted to setup the nginx directory: {0}")]
    SetupNginxDirectory(SetupNginxDirectoryError),
    #[error("Failed to verify nginx is running: {0}")]
    VerifyNginxIsRunningFailed(VerifyNginxIsRunningError),
    #[error("Failed to verify replica is running: {0}")]
    VerifyReplicaIsRunningFailed(VerifyReplicaIsRunningError),
    #[error("Failed to install the canister: {0}")]
    FailedToInstallCanister(InstallCanisterError),
    #[error("Failed to run scenario due to i/o error: {0}")]
    Scenario(std::io::Error),
}

pub fn run_scenario<F>(container: Container, scenario: F) -> Result<(), RunScenarioError>
where
    F: FnOnce(&Container) -> std::io::Result<()>,
{
    setup_nginx_directory(&container).map_err(RunScenarioError::SetupNginxDirectory)?;
    setup_scenario_directory(&container);

    compose_build_and_up(&container).map_err(RunScenarioError::FailedToStartContainer)?;

    verify_nginx_is_running(&container).map_err(RunScenarioError::VerifyNginxIsRunningFailed)?;
    verify_replica_is_running(&container)
        .map_err(RunScenarioError::VerifyReplicaIsRunningFailed)?;
    install_canister(&container).map_err(RunScenarioError::FailedToInstallCanister)?;

    scenario(&container).map_err(RunScenarioError::Scenario)?;
    compose_stop(&container);
    Ok(())
}

fn working_directory() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
}

fn generation_directory(container: &Container) -> PathBuf {
    let mut dir = working_directory();
    dir.push("gen");
    dir.push(&container.name);
    dir
}

fn nginx_directory(container: &Container) -> PathBuf {
    let mut dir = generation_directory(container);
    dir.push("nginx");
    dir
}

fn log_directory(container: &Container) -> PathBuf {
    let mut dir = generation_directory(container);
    dir.push("log");
    dir
}

#[derive(Debug, Error)]
pub enum SetupNginxDirectoryError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Tried to generate the entrypoint init.sh script: {0}")]
    GenerateEntrypointInitSh(GenerateEntrypointInitShError),
    #[error("Tried to generate the nginx.conf: {0}")]
    GenerateNginxConf(GenerateNginxConfError),
    #[error("Tried to generate the exchange responses: {0}")]
    GenerateExchangeResponses(GenerateExchangeResponsesError),
}

fn setup_nginx_directory(container: &Container) -> Result<(), SetupNginxDirectoryError> {
    let nginx_dir = nginx_directory(container);
    fs::create_dir_all(nginx_dir).map_err(SetupNginxDirectoryError::Io)?;

    // Adds the init.sh used by the Dockerfile's entrypoint.
    let mut init_sh_path = nginx_directory(container);
    init_sh_path.push("init.sh");
    generate_entrypoint_init_sh_script(container, init_sh_path)
        .map_err(SetupNginxDirectoryError::GenerateEntrypointInitSh)?;

    // Adds the nginx configuration file.
    let mut conf_path = nginx_directory(container);
    conf_path.push("conf");
    fs::create_dir_all(&conf_path).map_err(SetupNginxDirectoryError::Io)?;
    conf_path.push("default.conf");
    generate_nginx_conf(container, conf_path)
        .map_err(SetupNginxDirectoryError::GenerateNginxConf)?;

    // Adds the exchange responses.
    let mut json_path = nginx_directory(container);
    json_path.push("json");
    fs::create_dir_all(&json_path).map_err(SetupNginxDirectoryError::Io)?;
    generate_exchange_responses(container, json_path)
        .map_err(SetupNginxDirectoryError::GenerateExchangeResponses)
}

fn setup_log_directory(container: &Container) {
    let log_dir = log_directory(container);
    fs::create_dir_all(log_dir).expect("Failed to make nginx directory");

    // Add nginx log directory.
    let mut nginx_dir = log_directory(container);
    nginx_dir.push("nginx");
    fs::create_dir_all(nginx_dir).expect("Failed to make nginx directory");

    // Add supervisor log directory.
    let mut supervisor_dir = log_directory(container);
    supervisor_dir.push("supervisor");
    fs::create_dir_all(supervisor_dir).expect("Failed to make nginx directory");
}

fn setup_scenario_directory(container: &Container) {
    setup_log_directory(container);
}

#[derive(Debug, Error)]
pub enum GenerateEntrypointInitShError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Render(tera::Error),
}

fn generate_entrypoint_init_sh_script<P>(
    container: &Container,
    path: P,
) -> Result<(), GenerateEntrypointInitShError>
where
    P: AsRef<Path>,
{
    let contents = render_init_sh(container).map_err(GenerateEntrypointInitShError::Render)?;
    fs::write(path, contents).map_err(GenerateEntrypointInitShError::Io)
}

#[derive(Debug, Error)]
pub enum GenerateNginxConfError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Render(tera::Error),
}

fn generate_nginx_conf<P>(container: &Container, path: P) -> Result<(), GenerateNginxConfError>
where
    P: AsRef<Path>,
{
    let contents = render_nginx_conf(container).map_err(GenerateNginxConfError::Render)?;
    fs::write(path, contents).map_err(GenerateNginxConfError::Io)
}

#[derive(Debug, Error)]
pub enum GenerateExchangeResponsesError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Serialize(serde_json::Error),
}

fn generate_exchange_responses<P>(
    container: &Container,
    path: P,
) -> Result<(), GenerateExchangeResponsesError>
where
    P: AsRef<Path>,
{
    for config in container.responses.values() {
        for location in &config.locations {
            let default = serde_json::json!({});

            let value = match location.maybe_json {
                Some(ref json) => json,
                None => &default,
            };

            let mut path = PathBuf::from(path.as_ref());
            path.push(format!("{}.json", config.name));

            let contents = serde_json::to_string_pretty(value)
                .map_err(GenerateExchangeResponsesError::Serialize)?;
            fs::write(&path, contents).map_err(GenerateExchangeResponsesError::Io)?;
        }
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum VerifyNginxIsRunningError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Failed checking the status of nginx")]
    FailedStatusCheck,
}

/// Uses the container's supervisorctl command to verify that the nginx process is running.
/// If not, it attempts again after waiting 1 second.
fn verify_nginx_is_running(container: &Container) -> Result<(), VerifyNginxIsRunningError> {
    println!("Verifying nginx is running...");

    for _ in 0..30 {
        let (stdout, _) = compose_exec(container, "supervisorctl status nginx")
            .map_err(VerifyNginxIsRunningError::Io)?;
        if stdout.contains("RUNNING") {
            println!("nginx is running");
            return Ok(());
        }
        sleep(Duration::from_secs(1));
    }
    Err(VerifyNginxIsRunningError::FailedStatusCheck)
}

#[derive(Debug, Error)]
pub enum VerifyReplicaIsRunningError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Failed checking the status of the replica")]
    FailedStatusCheck,
}

/// Pings the replica until it returns a valid response to ensure it is running.
/// If not, it attempts again after waiting 1 second.
fn verify_replica_is_running(container: &Container) -> Result<(), VerifyReplicaIsRunningError> {
    println!("Verifying replica is running...");

    for _ in 0..30 {
        let result = ping_replica(container);
        match result {
            Ok(_) => {
                println!("Replica is running");
                return Ok(());
            }
            Err(PingReplicaError::Io(err)) => return Err(VerifyReplicaIsRunningError::Io(err)),
            Err(PingReplicaError::FailedStatusCheck) => {
                sleep(Duration::from_secs(1));
            }
        };
    }

    Err(VerifyReplicaIsRunningError::FailedStatusCheck)
}

#[derive(Debug)]
enum PingReplicaError {
    FailedStatusCheck,
    Io(std::io::Error),
}

/// Pings the replica inside of the container to check the replica's status.
fn ping_replica(container: &Container) -> Result<(), PingReplicaError> {
    let (stdout, _) = compose_exec(container, "dfx ping").map_err(PingReplicaError::Io)?;
    if !stdout.contains("ic_api_version") {
        return Err(PingReplicaError::FailedStatusCheck);
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum InstallCanisterError {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("Failed to create canister: {0}")]
    FailedToCreateCanister(String),
    #[error("Failed to install canister: {0}")]
    FailedToInstallCanister(String),
}

fn install_canister(container: &Container) -> Result<(), InstallCanisterError> {
    let (_, stderr) =
        compose_exec(container, "dfx canister create xrc").map_err(InstallCanisterError::Io)?;

    if !stderr.contains("xrc canister created") {
        return Err(InstallCanisterError::FailedToCreateCanister(
            stderr.replace('\n', " "),
        ));
    }

    let (_, stderr) = compose_exec(
        container,
        "dfx canister install xrc --wasm /canister/xrc.wasm",
    )
    .map_err(InstallCanisterError::Io)?;

    if !stderr.contains("Installing code for canister xrc") {
        return Err(InstallCanisterError::FailedToInstallCanister(
            stderr.replace('\n', " "),
        ));
    }

    println!("xrc canister successfully installed!");
    Ok(())
}

/// A generate method for accessing docker-compose.
fn compose<I, S>(container: &Container, args: I) -> std::io::Result<(String, String)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker-compose");
    let output = command
        .env("COMPOSE_PROJECT_NAME", &container.name)
        .args(["-f", "docker/docker-compose.yml"])
        .args(args)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr))
}

/// Starts up the container with a fresh build and recreates it.
fn compose_build_and_up(container: &Container) -> std::io::Result<()> {
    let (_, stderr) = compose(
        container,
        ["up", "--build", "--force-recreate", "-d", "e2e"],
    )?;

    println!("{}", stderr);

    Ok(())
}

/// Executes a command on the container.
fn compose_exec(container: &Container, command: &str) -> std::io::Result<(String, String)> {
    let formatted = format!("exec -T {} {}", "e2e", command);
    let cmd = formatted.split(' ');
    compose(container, cmd)
}

/// Stops the `e2e` container.
fn compose_stop(container: &Container) {
    if let Err(err) = compose(container, ["stop"]) {
        eprintln!("Failed to stop container {}: {}", container.name, err);
    }
}

/// Renders the nginx.conf with the container config.
fn render_nginx_conf(container: &Container) -> Result<String, tera::Error> {
    templates::render(templates::Template::NginxConf, &container.responses)
}

/// Renders the entrypoint init.sh with the container config.
fn render_init_sh(container: &Container) -> Result<String, tera::Error> {
    templates::render(templates::Template::InitSh, &container.responses)
}
