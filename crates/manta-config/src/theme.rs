use std::path::{Path, PathBuf};

use config::{Config, File, FileFormat};
use serde::Deserialize;

const DEFAULT_THEME: &str = include_str!("../default-theme.toml");

// I'm keeping this for now. Later this will be a base16 + faces approach
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Theme {
    pub color_bg: u32,
    pub color_surface: u32,
    pub color_text: u32,
    pub color_border: u32,

    pub color_mode_normal: u32,
    pub color_mode_insert: u32,
}

fn resolve_theme_file(dir: &Path, theme: &str) -> Option<PathBuf> {
    let path = Path::new(theme);

    let is_path = path.is_absolute() // Absolute path
        || path
            .extension()
            .map_or(false, |e| e.eq_ignore_ascii_case("toml")) // Relative path in current directory
        || path.components().count() > 1; // Just a relative path

    if is_path {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            dir.join(path)
        };
        return resolved.is_file().then_some(resolved);
    }

    // it is just a theme name
    let path = dir.join("themes").join(format!("{theme}.toml"));

    path.is_file().then_some(path)
}

pub(crate) fn load_theme(dir: &Path, theme_ref: &str) -> Result<Theme, super::Error> {
    let mut builder = Config::builder().add_source(File::from_str(DEFAULT_THEME, FileFormat::Toml));

    let path = resolve_theme_file(dir, theme_ref);
    if let Some(p) = path {
        builder = builder.add_source(File::from(p));
    }

    let theme = builder.build()?.try_deserialize()?;
    dbg!(&theme);
    Ok(theme)
}
