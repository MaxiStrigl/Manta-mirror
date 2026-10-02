use std::{path::PathBuf, sync::LazyLock};
use config::{Config, FileFormat};
use directories::ProjectDirs;
use serde::Deserialize;

const DEFAULT_CONFIG: &str = include_str!("../default.toml");

#[derive(Debug, Deserialize)]
pub struct MantaConfig {
    pub theme: Theme,
    pub editor_settings: EditorSettings,
}

#[derive(Debug, Deserialize)]
pub struct Theme {
    pub color_bg: u32,
    pub color_surface: u32,
    pub color_text: u32,
    pub color_border: u32,

    pub color_mode_normal: u32,
    pub color_mode_insert: u32
}

#[derive(Debug, Deserialize)]
pub struct EditorSettings {
    pub font_size: f32,
}

fn user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("org", "strigl", "manta")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

pub static CONFIG: LazyLock<MantaConfig> = LazyLock::new(|| {
    let mut builder = Config::builder()
        .add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml));

    if let Some(path) = user_config_path() {
        builder = builder.add_source(config::File::from(path).required(false));
    }

    builder
        .build()
        .expect("Failed to load config")
        .try_deserialize()
        .expect("Invalid config")
});