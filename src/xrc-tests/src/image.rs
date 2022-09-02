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

type ResponseFn = dyn Fn(&Exchange) -> (u16, Option<serde_json::Value>);

struct ScenarioConfig {
    name: String,
    request: Option<candid::GetExchangeRateRequest>,
    response_fn: Option<Box<ResponseFn>>,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            name: Default::default(),
            request: Default::default(),
            response_fn: Default::default(),
        }
    }
}

pub struct Scenario {
    name: String,
    request: candid::GetExchangeRateRequest,
    responses: Vec<ScenarioExchangeConfig>,
}

impl Scenario {
    pub fn builder() -> ScenarioBuilder {
        ScenarioBuilder::new()
    }
}

impl From<ScenarioConfig> for Scenario {
    fn from(config: ScenarioConfig) -> Self {
        let request = config
            .request
            .expect("A request must be defined to run a scenario.");
        let response_fn = config
            .response_fn
            .expect("Responses must be defined to run a scenario.");

        let responses = EXCHANGES
            .iter()
            .map(|e| {
                let (status_code, maybe_json) = response_fn(e);
                let url = get_url(e, &request);
                ScenarioExchangeConfig {
                    name: e.to_string().to_lowercase(),
                    maybe_json,
                    status_code,
                    host: url.host().unwrap().to_string(),
                    path: url.path().to_string(),
                }
            })
            .collect::<Vec<_>>();

        Self {
            name: config.name,
            request,
            responses,
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
    config: ScenarioConfig,
}

impl ScenarioBuilder {
    fn new() -> Self {
        Self {
            config: ScenarioConfig::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.config.name = name;
        self
    }

    pub fn request(mut self, request: candid::GetExchangeRateRequest) -> Self {
        self.config.request = Some(request);
        self
    }

    pub fn responses(mut self, response_fn: Box<ResponseFn>) -> Self {
        self.config.response_fn = Some(response_fn);
        self
    }

    pub fn run(self) -> ScenarioOutput {
        let scenario = Scenario::from(self.config);

        setup_image_project_directory(&scenario);
        compose_build_and_up(&scenario);
        verify_nginx_is_running(&scenario);
        start_dfx(&scenario);
        dfx_ping(&scenario);
        install_canister(&scenario);
        call_canister(&scenario);
        compose_stop(&scenario);
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

    path.push("init.sh");
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

fn verify_nginx_is_running(scenario: &Scenario) {
    compose_exec(
        scenario,
        "curl https://api.pro.coinbase.com/products/ICP-BTC/candles",
    )
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
    compose_exec(
        scenario,
        "dfx canister install xrc --wasm /canister/xrc.wasm",
    );
}

fn call_canister(scenario: &Scenario) {
    let encoded = encode_one(&scenario.request).unwrap();
    let payload = hex::encode(encoded);
    let cmd = format!(
        "dfx canister call --type raw --output pp xrc get_exchange_rates {}",
        payload
    );
    println!("Calling canister : {}", cmd);
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

fn get_url(exchange: &Exchange, request: &candid::GetExchangeRateRequest) -> url::Url {
    let url = url::Url::parse(&exchange.get_url(
        &request.base_asset.symbol,
        &request.quote_asset.symbol,
        0,
    ))
    .expect("failed to parse");
    url
}

pub fn render_nginx_conf(scenario: &Scenario) -> String {
    templates::render(NGINX_SERVER_CONF, &scenario.responses)
        .expect("failed to render `default.conf`")
}
