use config::{Config, ConfigError, FileFormat};
use directories::ProjectDirs;
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::theme::{Theme, load_theme};

pub mod theme;

const DEFAULT_CONFIG: &str = include_str!("../default.toml");

#[derive(Debug)]
pub enum Error {
    Config(config::ConfigError),
    IO(std::io::Error),
}

impl From<ConfigError> for Error {
    fn from(value: ConfigError) -> Self {
        Error::Config(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IO(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(e) => write!(f, "config error: {e}"),
            Error::IO(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// The Manta runtime configuration includidng Settings and Theme data
#[derive(Debug, Clone, Deserialize)]
pub struct MantaConfig {
    pub settings: Settings,
    pub theme: Theme,
}

impl Default for MantaConfig {
    fn default() -> Self {
        Self {
            settings: Default::default(),
            theme: Default::default(),
        }
    }
}

/// Raw config values loaded from the toml file
#[derive(Default, Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(rename = "theme", default = "default_theme_name")]
    theme_ref: String,

    pub font_size: f32,
}

fn default_theme_name() -> String {
    "default".into()
}

fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("org", "strigl", "manta").map(|dirs| dirs.config_dir().to_path_buf())
}

fn resolve_includes(main: &Path) -> Vec<PathBuf> {
    #[derive(Deserialize)]
    struct Includes {
        #[serde(default)]
        include: Vec<String>,
    }

    std::fs::read_to_string(main)
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|v| v.try_into::<Includes>().ok())
        .map(|i| i.include.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

pub fn load() -> Result<MantaConfig, Error> {
    println!("Load");
    let mut builder =
        Config::builder().add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml));

    let dir = config_dir().unwrap_or_else(|| PathBuf::from("."));

    let main = dir.join("config.toml");

    if main.exists() {
        for include in resolve_includes(&main) {
            builder = builder.add_source(config::File::from(dir.join(include)));
        }
        builder = builder.add_source(config::File::from(main));
    }

    let settings: Settings = builder.build()?.try_deserialize()?;
    let theme = load_theme(&dir, &settings.theme_ref)?;

    Ok(MantaConfig {
        settings: settings,
        theme: theme,
    })
}
