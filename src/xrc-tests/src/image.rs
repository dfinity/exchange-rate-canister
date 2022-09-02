use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use ic_cdk::export::candid::encode_one;
use serde::Serialize;
use xrc::{candid, Exchange, EXCHANGES};

use crate::templates::{self, NGINX_SERVER_CONF};

pub struct Scenario {
    name: String,
    request: Option<candid::GetExchangeRateRequest>,
    responses: Vec<ScenarioExchangeConfig>,
}

impl Scenario {
    pub fn builder() -> ScenarioBuilder {
        ScenarioBuilder::new()
    }
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            name: Default::default(),
            request: Default::default(),
            responses: Default::default(),
        }
    }
}

#[derive(Serialize)]
struct ScenarioExchangeConfig {
    name: String,
    maybe_json: Option<serde_json::Value>,
    status_code: u16,
    host: String,
    path: String,
}

pub struct ScenarioOutput {}

pub struct ScenarioBuilder {
    scenario: Scenario,
}

impl ScenarioBuilder {
    fn new() -> Self {
        Self {
            scenario: Scenario::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.scenario.name = name;
        self
    }

    pub fn request(mut self, request: candid::GetExchangeRateRequest) -> Self {
        self.scenario.request = Some(request);
        self
    }

    pub fn responses<F>(mut self, response_fn: F) -> Self
    where
        F: Fn(&Exchange) -> (u16, Option<serde_json::Value>),
    {
        let responses = EXCHANGES
            .iter()
            .map(|e| (e, response_fn(e)))
            .map(|(e, (status_code, maybe_json))| {
                let url = get_url(e);
                ScenarioExchangeConfig {
                    name: e.to_string().to_lowercase(),
                    maybe_json,
                    status_code,
                    host: url.host().unwrap().to_string(),
                    path: url.path().to_string(),
                }
            })
            .collect::<Vec<_>>();
        self.scenario.responses = responses;
        self
    }

    pub fn run(self) -> ScenarioOutput {
        setup_image_project_directory(&self.scenario);
        compose_build_and_up(&self.scenario);
        start_dfx(&self.scenario);
        dfx_ping(&self.scenario);
        install_canister(&self.scenario);
        call_canister(&self.scenario);
        compose_stop(&self.scenario);
        ScenarioOutput {}
    }
}

fn workspace_directory() -> String {
    format!("{}/../../", working_directory())
}

fn working_directory() -> String {
    std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()
}

fn image_directory(scenario: &Scenario) -> String {
    format!("{}/gen/{}", working_directory(), scenario.name)
}

fn setup_image_project_directory(scenario: &Scenario) {
    let mut path = PathBuf::from(image_directory(scenario));
    fs::create_dir_all(path.as_path()).expect("Failed to make base directory");

    // Add nginx directory
    path.push("nginx");
    fs::create_dir_all(path.as_path()).expect("Failed to make nginx directory");

    path.push("generate-certs-and-keys.sh");
    generate_nginx_certs_and_keys_sh_script(scenario, path.as_path());
    path.pop();
    path.push("conf");
    fs::create_dir_all(path.as_path()).expect("Failed to make nginx directory");
    path.push("default.conf");
    generate_nginx_conf(scenario, path.as_path());
    path.pop();
    path.pop();
    path.push("json");
    fs::create_dir_all(path.as_path()).expect("Failed to make nginx directory");
    generate_exchange_responses(scenario, path.as_path());
}

fn generate_nginx_certs_and_keys_sh_script<P>(_: &Scenario, path: P)
where
    P: AsRef<Path>,
{
    let contents = templates::render_certs_and_keys_sh()
        .expect("failed to render `generate_certs_and_keys.sh`");
    fs::write(path, contents).expect("failed to write contents to `generate_certs_and_keys.sh`");
}

fn generate_nginx_conf<P>(scenario: &Scenario, path: P)
where
    P: AsRef<Path>,
{
    let contents = render_nginx_conf(scenario);
    fs::write(path, contents).expect("failed to write contents to `default.conf`");
}

fn generate_exchange_responses<P>(scenario: &Scenario, path: P)
where
    P: AsRef<Path>,
{
    for config in &scenario.responses {
        let default = serde_json::json!({});
        let value = match config.maybe_json {
            Some(ref json) => json,
            None => &default,
        };

        let mut path = PathBuf::from(path.as_ref());
        path.push(format!("{}.json", config.name));

        let contents = serde_json::to_string_pretty(value).unwrap();
        fs::write(&path, contents).expect("failed to write contents to json file");
    }
}

fn start_dfx(scenario: &Scenario) {
    compose_exec(
        scenario,
        "dfx start --clean --background --enable-canister-http",
    )
}

fn dfx_ping(scenario: &Scenario) {
    compose_exec(scenario, "dfx ping");
}

fn install_canister(scenario: &Scenario) {
    compose_exec(scenario, "dfx canister create xrc");
    compose_exec(scenario, "dfx canister install xrc --wasm xrc.wasm");
}

fn call_canister(scenario: &Scenario) {
    let request = candid::GetExchangeRateRequest {
        timestamp: Some(1614596340),
        quote_asset: xrc::candid::Asset {
            symbol: "btc".to_string(),
            class: xrc::candid::AssetClass::Cryptocurrency,
        },
        base_asset: xrc::candid::Asset {
            symbol: "icp".to_string(),
            class: xrc::candid::AssetClass::Cryptocurrency,
        },
    };
    let encoded = encode_one(&request).unwrap();
    let payload = hex::encode(encoded);
    let cmd = format!(
        "dfx canister call --type raw --output pp xrc get_exchange_rates {}",
        payload
    );
    compose_exec(scenario, &cmd);
}

fn compose<I, S>(scenario: &Scenario, args: I)
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
    println!("stdout\n{}", String::from_utf8_lossy(&output.stdout));
    println!("stderr\n{}", String::from_utf8_lossy(&output.stderr));
}

fn compose_build_and_up(scenario: &Scenario) {
    compose(scenario, ["up", "--build", "-d", "e2e"])
}

fn compose_exec(scenario: &Scenario, command: &str) {
    let formatted = format!("exec -T {} {}", "e2e", command);
    let cmd = formatted.split(" ");
    compose(scenario, cmd);
}

fn compose_stop(scenario: &Scenario) {
    compose(scenario, ["stop"])
}

fn get_url(exchange: &Exchange) -> url::Url {
    let url = url::Url::parse(&exchange.get_url("", "", 0)).expect("failed to parse");
    url
}

pub fn render_nginx_conf(scenario: &Scenario) -> String {
    templates::render(NGINX_SERVER_CONF, &scenario.responses)
        .expect("failed to render `default.conf`")
}
