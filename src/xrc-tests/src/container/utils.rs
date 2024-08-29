use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use thiserror::Error;

use crate::templates;

use super::{Container, ResponseBody};

/// Get the working directory which is based off of the `CARGO_MANIFEST_DIR`
/// environment variable.
fn working_directory() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
}

/// Get the directory where all generated files and log results.
fn generation_directory(container: &Container) -> PathBuf {
    let mut dir = working_directory();
    dir.push("gen");
    dir.push(&container.name);
    dir
}

/// Get the directory where all of nginx's generated files (responses, nginx.conf, init.sh).
fn nginx_directory(container: &Container) -> PathBuf {
    let mut dir = generation_directory(container);
    dir.push("nginx");
    dir
}

/// Get the directory where all of nginx and supervisord's log results reside.
fn log_directory(container: &Container) -> PathBuf {
    let mut dir = generation_directory(container);
    dir.push("log");
    dir
}

/// Errors for when the [setup_nginx_directory] function fails.
#[derive(Debug, Error)]
pub enum SetupNginxDirectoryError {
    /// Used when the function fails to create directories.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when there is a failure with generating the entrypoint `init.sh`.
    #[error("Tried to generate the entrypoint init.sh script: {0}")]
    GenerateEntrypointInitSh(GenerateEntrypointInitShError),
    /// Used when there is a failure with generating the `nginx.conf`.
    #[error("Tried to generate the nginx.conf: {0}")]
    GenerateNginxConf(GenerateNginxConfError),
    /// Used when there is a failure with generated the JSON responses for nginx.
    #[error("Tried to generate the exchange responses: {0}")]
    GenerateExchangeResponses(GenerateExchangeResponsesError),
}

/// This function is used to setup the generated nginx directories and files that are used to
/// determine how nginx will respond to calls from the `xrc` canister.
pub fn setup_nginx_directory(container: &Container) -> Result<(), SetupNginxDirectoryError> {
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

/// This function is used to setup the logging directories where the container will place their
/// log files. The `nginx` directory is used for nginx access and error logs. `supervisor` is used
/// for `dfx`'s replica and the messages from supervisor itself.
pub fn setup_log_directory(container: &Container) -> std::io::Result<()> {
    let log_dir = log_directory(container);
    fs::create_dir_all(log_dir)?;

    // Add nginx log directory.
    let mut nginx_dir = log_directory(container);
    nginx_dir.push("nginx");
    fs::create_dir_all(nginx_dir)?;

    // Add supervisor log directory.
    let mut supervisor_dir = log_directory(container);
    supervisor_dir.push("supervisor");
    fs::create_dir_all(supervisor_dir)
}

/// Errors for when the [generate_nginx_conf] function fails.
#[derive(Debug, Error)]
pub enum GenerateEntrypointInitShError {
    /// Used when failing to write a rendered product to the generated content directory.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when there is an error with rendering the template. Usually an error
    /// with the syntax of the template.
    #[error("{0}")]
    Render(tera::Error),
}

/// Renders and writes a Docker entrypoint shell script to a provided path.
pub fn generate_entrypoint_init_sh_script<P>(
    container: &Container,
    path: P,
) -> Result<(), GenerateEntrypointInitShError>
where
    P: AsRef<Path>,
{
    let contents = templates::render(templates::Template::InitSh, &container.responses)
        .map_err(GenerateEntrypointInitShError::Render)?;
    fs::write(path, contents).map_err(GenerateEntrypointInitShError::Io)
}

/// Errors for when the [generate_nginx_conf] function fails.
#[derive(Debug, Error)]
pub enum GenerateNginxConfError {
    /// Used when failing to write a rendered product to the generated content directory.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when there is an error with rendering the template. Usually an error
    /// with the syntax of the template.
    #[error("{0}")]
    Render(tera::Error),
}

/// Renders and writes an nginx configuration to a provided path.
fn generate_nginx_conf<P>(container: &Container, path: P) -> Result<(), GenerateNginxConfError>
where
    P: AsRef<Path>,
{
    let contents = templates::render(templates::Template::NginxConf, &container.responses)
        .map_err(GenerateNginxConfError::Render)?;
    fs::write(path, contents).map_err(GenerateNginxConfError::Io)
}

/// Errors for when the [generate_exchange_responses] function fails to produce
/// the responses.
#[derive(Debug, Error)]
pub enum GenerateExchangeResponsesError {
    /// Used when failing to write a response to the generated content directory.
    #[error("{0}")]
    Io(std::io::Error),
}

/// This function takes the container's configured responses and dumps the JSON or XML
/// into files so nginx can serve the responses to the `xrc` canister.
pub fn generate_exchange_responses<P>(
    container: &Container,
    path: P,
) -> Result<(), GenerateExchangeResponsesError>
where
    P: AsRef<Path>,
{
    for config in container.responses.values() {
        for location in &config.locations {
            let contents = match &location.body {
                ResponseBody::Json(body) | ResponseBody::Xml(body) => body,
                ResponseBody::Empty => continue,
            };

            let mut buf = PathBuf::from(path.as_ref());
            buf.push(&config.name);
            buf.push(location.path.trim_start_matches('/'));
            fs::create_dir_all(&buf).map_err(GenerateExchangeResponsesError::Io)?;

            buf.push(format!("{}.{}", location.query_params, location.body));

            fs::write(&buf, contents).map_err(GenerateExchangeResponsesError::Io)?;
        }
    }
    Ok(())
}

/// Errors for when the [verify_nginx_is_running] function fails.
#[derive(Debug, Error)]
pub enum VerifyNginxIsRunningError {
    /// Used when an error occurred while attempting to send the supervisorctl status check command to the container.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when the status check continuously fails.
    #[error("Failed checking the status of nginx")]
    FailedStatusCheck,
}

/// Uses the container's supervisorctl command to verify that the nginx process is running.
/// If not, it attempts again after waiting 1 second.
pub fn verify_nginx_is_running(container: &Container) -> Result<(), VerifyNginxIsRunningError> {
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

/// Errors for when the [verify_replica_is_running] function fails.
#[derive(Debug, Error)]
pub enum VerifyReplicaIsRunningError {
    /// Used when an error occurred while attempting to send the ping command to the container.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when the ping replica continuously fails.
    #[error("Failed checking the status of the replica")]
    FailedStatusCheck,
}

/// Pings the replica until it returns a valid response to ensure it is running.
/// If not, it attempts again after waiting 1 second.
pub fn verify_replica_is_running(container: &Container) -> Result<(), VerifyReplicaIsRunningError> {
    println!("Verifying replica is running...");

    for _ in 0..30 {
        let result = ping_replica(container);
        match result {
            Ok(_) => {
                println!("Replica is running");
                // It's possible that the replica responds to http call with
                // "Replica is unhealthy: WaitingForCertifiedState"
                // Wait another one second so the replica will be more likely to be ready
                sleep(Duration::from_secs(1));
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

/// Errors for when the [ping_replica] function fails.
#[derive(Debug)]
enum PingReplicaError {
    /// Used when an error occurred while attempting to send the command to the container.
    Io(std::io::Error),
    /// Used when the ping response does not contain the `ic_api_version` field.
    FailedStatusCheck,
}

/// Pings the replica inside of the container to check the replica's status.
fn ping_replica(container: &Container) -> Result<(), PingReplicaError> {
    let (stdout, _) = compose_exec(container, "dfx ping").map_err(PingReplicaError::Io)?;
    if !stdout.contains("ic_api_version") {
        return Err(PingReplicaError::FailedStatusCheck);
    }

    Ok(())
}

/// Errors for when the [install_canister] function fails.
#[derive(Debug, Error)]
pub enum InstallCanisterError {
    /// Used when an error occurred while attempting to send the command to the container.
    #[error("{0}")]
    Io(std::io::Error),
    /// Used when the canister cannot be created on the container's replica.
    #[error("Failed to create canister: {0}")]
    FailedToCreateCanister(String),
    /// Used when the canister failed to install on the container's replica.
    #[error("Failed to install canister: {0}")]
    FailedToInstallCanister(String),
}

/// Creates and installs the canister on the container's replica.
pub fn install_canister(container: &Container) -> Result<(), InstallCanisterError> {
    let (_, stderr) =
        compose_exec(container, "dfx canister create xrc").map_err(InstallCanisterError::Io)?;

    if !stderr.contains("xrc canister created") {
        return Err(InstallCanisterError::FailedToCreateCanister(
            stderr.replace('\n', " "),
        ));
    }

    let (_, stderr) = compose_exec(
        container,
        "dfx canister install xrc --wasm /canister/xrc.wasm.gz",
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

/// A generate method for accessing docker-compose to interact with the `e2e` container.
fn compose<I, S>(container: &Container, args: I) -> std::io::Result<(String, String)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker");
    let output = command
        .env("COMPOSE_PROJECT_NAME", &container.name)
        .args(["compose", "-f", "docker/docker-compose.yml"])
        .args(args)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr))
}

/// Starts up the container with a fresh build and recreates it.
pub fn compose_build_and_up(container: &Container) -> std::io::Result<()> {
    let (_, stderr) = compose(
        container,
        ["up", "--build", "--force-recreate", "-d", "e2e"],
    )?;

    println!("{}", stderr);

    Ok(())
}

/// Executes a command on the container.
pub fn compose_exec(container: &Container, command: &str) -> std::io::Result<(String, String)> {
    let formatted = format!("exec -T {} {}", "e2e", command);
    let cmd = formatted.split(' ');
    compose(container, cmd)
}

/// Stops the `e2e` container.
pub fn compose_stop(container: &Container) {
    if let Err(err) = compose(container, ["stop"]) {
        eprintln!("Failed to stop container {}: {}", container.name, err);
    }
}
