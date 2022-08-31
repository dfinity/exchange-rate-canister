use std::{
    ffi::OsStr,
    fs, os,
    path::{Path, PathBuf},
    process::Command,
};

use xrc::{Exchange, EXCHANGES};

pub struct Image {
    project_name: String,
}

impl Image {
    pub fn builder() -> ImageBuilder {
        ImageBuilder::new()
    }

    fn start(self) -> Self {
        self
    }

    pub fn command(&self, cmd: &str) {}

    pub fn call_canister(&self, method: &str, payload: &[u8]) {}
}

impl Drop for Image {
    fn drop(&mut self) {
        compose_stop(&self.project_name)
    }
}

impl Default for Image {
    fn default() -> Self {
        Self {
            project_name: Default::default(),
        }
    }
}

pub struct ImageBuilder {
    image: Image,
}

impl ImageBuilder {
    fn new() -> Self {
        Self {
            image: Image::default(),
        }
    }

    pub fn with_project_name(mut self, project_name: String) -> ImageBuilder {
        self.image.project_name = project_name;
        self
    }

    pub fn build(self) -> Image {
        setup_image_project_directory(&self.image.project_name);
        compose_build_and_up(&self.image.project_name);
        self.image
    }
}

fn working_directory() -> String {
    std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()
}

fn image_directory(project_name: &str) -> String {
    format!("{}/gen/{}", working_directory(), project_name)
}

fn setup_image_project_directory(project_name: &str) {
    let mut path = PathBuf::from(image_directory(project_name));
    fs::create_dir_all(path.as_path()).expect("Failed to make base directory");

    // Add nginx directory
    path.push("nginx");
    fs::create_dir_all(path.as_path()).expect("Failed to make nginx directory");
    path.pop();

    // Add logs directory
    path.push("logs");
    fs::create_dir_all(path.as_path()).expect("Failed to make logs directory");
    path.pop();

    dump_exchange_information_into_json();
}

fn dump_exchange_information_into_json() {}

fn compose<I, S>(project_name: &str, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker-compose");
    let output = command
        .env("COMPOSE_PROJECT_NAME", project_name)
        .env("WORKING_DIRECTORY", working_directory())
        .args(["-f", "docker/docker-compose.yml"])
        .args(args)
        .output()
        .expect("failed to up and build");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
}

fn compose_build_and_up(project_name: &str) {
    compose(project_name, ["up", "--build", "-d"])
}

fn compose_exec(project_name: &str, command: &str) {
    let cmd = command.split(" ");
    compose(project_name, cmd);
}

fn compose_stop(project_name: &str) {
    compose(project_name, ["stop"])
}
