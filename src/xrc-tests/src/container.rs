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

pub struct Container {
    name: String,
    responses: HashMap<String, ContainerNginxServerConfig>,
}

impl Container {
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }

    pub fn call_canister<Tuple>(&self, method_name: &str, args: Tuple) -> ContainerOutput
    where
        Tuple: candid::utils::ArgumentEncoder,
    {
        let encoded = candid::encode_args(args).expect("Failed to encode arguments!");
        let payload = hex::encode(encoded);
        let cmd = format!(
            "dfx canister call --type raw --output pp xrc {} {}",
            method_name, payload
        );
        println!("Calling canister : {}", cmd);
        compose_exec(self, &cmd);
        ContainerOutput {}
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

pub struct ContainerOutput {}

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

pub fn run_scenario<F>(container: Container, scenario: F) -> ContainerOutput
where
    F: FnOnce(&Container) -> ContainerOutput,
{
    setup_scenario_directory(&container);
    compose_build_and_up(&container);
    verify_nginx_is_running(&container);
    verify_replica_is_running(&container);
    install_canister(&container);

    let output = scenario(&container);
    compose_stop(&container);
    output
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

fn setup_nginx_directory(container: &Container) {
    let nginx_dir = nginx_directory(container);
    fs::create_dir_all(nginx_dir).expect("Failed to make nginx directory");

    // Adds the init.sh used by the Dockerfile's entrypoint.
    let mut init_sh_path = nginx_directory(container);
    init_sh_path.push("init.sh");
    generate_entrypoint_init_sh_script(container, init_sh_path);

    // Adds the nginx configuration file.
    let mut conf_path = nginx_directory(container);
    conf_path.push("conf");
    fs::create_dir_all(&conf_path).expect("Failed to make nginx directory");
    conf_path.push("default.conf");
    generate_nginx_conf(container, conf_path);

    // Adds the exchange responses.
    let mut json_path = nginx_directory(container);
    json_path.push("json");
    fs::create_dir_all(&json_path).expect("Failed to make nginx directory");
    generate_exchange_responses(container, json_path);
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
    setup_nginx_directory(container);
    setup_log_directory(container);
}

fn generate_entrypoint_init_sh_script<P>(container: &Container, path: P)
where
    P: AsRef<Path>,
{
    let contents = render_init_sh(container);
    fs::write(path, contents).expect("failed to write contents to `init.sh`");
}

fn generate_nginx_conf<P>(container: &Container, path: P)
where
    P: AsRef<Path>,
{
    let contents = render_nginx_conf(container);
    fs::write(path, contents).expect("failed to write contents to `default.conf`");
}

fn generate_exchange_responses<P>(container: &Container, path: P)
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

            let contents = serde_json::to_string_pretty(value).unwrap();
            fs::write(&path, contents).expect("failed to write contents to json file");
        }
    }
}

fn verify_nginx_is_running(container: &Container) {
    println!("Verifying nginx is running...");
    let (stdout, _) = compose_exec(container, "supervisorctl status nginx");
    for _ in 0..30 {
        if stdout.contains("RUNNING") {
            println!("nginx is running");
            break;
        }
        sleep(Duration::from_secs(1));
    }
}

fn verify_replica_is_running(container: &Container) {
    println!("Verifying replica is running...");
    let (stdout, _) = dfx_ping(container);
    for _ in 0..30 {
        if !stdout.is_empty() {
            println!("Replica is running");
            break;
        }
        sleep(Duration::from_secs(1));
    }
}

fn dfx_ping(container: &Container) -> (String, String) {
    compose_exec(container, "dfx ping")
}

fn install_canister(container: &Container) {
    compose_exec(container, "dfx canister create xrc");
    compose_exec(
        container,
        "dfx canister install xrc --wasm /canister/xrc.wasm",
    );
}

fn compose<I, S>(container: &Container, args: I) -> (String, String)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker-compose");
    let output = command
        .env("COMPOSE_PROJECT_NAME", &container.name)
        .args(["-f", "docker/docker-compose.yml"])
        .args(args)
        .output()
        .expect("failed to up and build");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    println!("stdout\n{}", stdout);
    println!("stderr\n{}", stderr);
    (stdout, stderr)
}

fn compose_build_and_up(container: &Container) {
    compose(
        container,
        ["up", "--build", "--force-recreate", "-d", "e2e"],
    );
}

fn compose_exec(container: &Container, command: &str) -> (String, String) {
    let formatted = format!("exec -T {} {}", "e2e", command);
    let cmd = formatted.split(' ');
    compose(container, cmd)
}

fn compose_stop(container: &Container) {
    compose(container, ["stop"]);
}

fn render_nginx_conf(container: &Container) -> String {
    templates::render(templates::Template::NginxConf, &container.responses)
        .expect("failed to render `default.conf`")
}

fn render_init_sh(container: &Container) -> String {
    templates::render(templates::Template::InitSh, &container.responses)
        .expect("failed to render `init.sh`")
}
