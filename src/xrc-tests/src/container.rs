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

use crate::templates::{self, INIT_SH, NGINX_SERVER_CONF};

pub struct ExchangeResponse {
    pub name: String,
    pub url: String,
    pub status_code: u16,
    pub maybe_json: Option<serde_json::Value>,
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

fn generation_directory(scenario: &Container) -> PathBuf {
    let mut dir = working_directory();
    dir.push("gen");
    dir.push(&scenario.name);
    dir
}

fn nginx_directory(scenario: &Container) -> PathBuf {
    let mut dir = generation_directory(scenario);
    dir.push("nginx");
    dir
}

fn log_directory(scenario: &Container) -> PathBuf {
    let mut dir = generation_directory(scenario);
    dir.push("log");
    dir
}

fn setup_nginx_directory(scenario: &Container) {
    let nginx_dir = nginx_directory(scenario);
    fs::create_dir_all(nginx_dir).expect("Failed to make nginx directory");

    // Adds the init.sh used by the Dockerfile's entrypoint.
    let mut init_sh_path = nginx_directory(scenario);
    init_sh_path.push("init.sh");
    generate_entrypoint_init_sh_script(scenario, init_sh_path);

    // Adds the nginx configuration file.
    let mut conf_path = nginx_directory(scenario);
    conf_path.push("conf");
    fs::create_dir_all(&conf_path).expect("Failed to make nginx directory");
    conf_path.push("default.conf");
    generate_nginx_conf(scenario, conf_path);

    // Adds the exchange responses.
    let mut json_path = nginx_directory(scenario);
    json_path.push("json");
    fs::create_dir_all(&json_path).expect("Failed to make nginx directory");
    generate_exchange_responses(scenario, json_path);
}

fn setup_log_directory(scenario: &Container) {
    let log_dir = log_directory(scenario);
    fs::create_dir_all(log_dir).expect("Failed to make nginx directory");

    // Add nginx log directory.
    let mut nginx_dir = log_directory(scenario);
    nginx_dir.push("nginx");
    fs::create_dir_all(nginx_dir).expect("Failed to make nginx directory");

    // Add supervisor log directory.
    let mut supervisor_dir = log_directory(scenario);
    supervisor_dir.push("supervisor");
    fs::create_dir_all(supervisor_dir).expect("Failed to make nginx directory");
}

fn setup_scenario_directory(scenario: &Container) {
    setup_nginx_directory(scenario);
    setup_log_directory(scenario);
}

fn generate_entrypoint_init_sh_script<P>(container: &Container, path: P)
where
    P: AsRef<Path>,
{
    let contents = render_init_sh(container);
    fs::write(path, contents).expect("failed to write contents to `init.sh`");
}

fn generate_nginx_conf<P>(scenario: &Container, path: P)
where
    P: AsRef<Path>,
{
    let contents = render_nginx_conf(scenario);
    fs::write(path, contents).expect("failed to write contents to `default.conf`");
}

fn generate_exchange_responses<P>(scenario: &Container, path: P)
where
    P: AsRef<Path>,
{
    for (_, config) in &scenario.responses {
        for location in config.locations.iter() {
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

fn verify_nginx_is_running(scenario: &Container) {
    println!("Verifying nginx is running...");
    let (stdout, _) = compose_exec(scenario, "supervisorctl status nginx");
    for _ in 0..30 {
        if stdout.contains("RUNNING") {
            println!("nginx is running");
            break;
        }
        sleep(Duration::from_secs(1));
    }
}

fn verify_replica_is_running(scenario: &Container) {
    println!("Verifying replica is running...");
    let (stdout, _) = dfx_ping(scenario);
    for _ in 0..30 {
        if !stdout.is_empty() {
            println!("Replica is running");
            break;
        }
        sleep(Duration::from_secs(1));
    }
}

fn dfx_ping(scenario: &Container) -> (String, String) {
    compose_exec(scenario, "dfx ping")
}

fn install_canister(scenario: &Container) {
    compose_exec(scenario, "dfx canister create xrc");
    compose_exec(
        scenario,
        "dfx canister install xrc --wasm /canister/xrc.wasm",
    );
}

fn compose<I, S>(scenario: &Container, args: I) -> (String, String)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker-compose");
    let output = command
        .env("COMPOSE_PROJECT_NAME", &scenario.name)
        .env("WORKING_DIRECTORY", working_directory())
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

fn compose_build_and_up(scenario: &Container) {
    compose(scenario, ["up", "--build", "-d", "e2e"]);
}

fn compose_exec(scenario: &Container, command: &str) -> (String, String) {
    let formatted = format!("exec -T {} {}", "e2e", command);
    let cmd = formatted.split(' ');
    compose(scenario, cmd)
}

fn compose_stop(scenario: &Container) {
    compose(scenario, ["stop"]);
}

pub fn render_nginx_conf(scenario: &Container) -> String {
    templates::render(NGINX_SERVER_CONF, &scenario.responses)
        .expect("failed to render `default.conf`")
}

pub fn render_init_sh(scenario: &Container) -> String {
    templates::render(INIT_SH, &scenario.responses).expect("failed to render `init.sh`")
}
