use serde::Deserialize;
use std::path::PathBuf;
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub brew_path: PathBuf,
    pub max_output_lines: usize,
}
impl Default for Config {
    fn default() -> Self {
        let brew_path = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .into_iter()
            .find(|p| std::path::Path::new(p).is_file())
            .unwrap_or("brew")
            .into();
        Self {
            brew_path,
            max_output_lines: 2000,
        }
    }
}
pub fn dirs() -> anyhow::Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("dev", "LazyBrew", "LazyBrew")
        .ok_or_else(|| anyhow::anyhow!("Cannot locate user directories"))
}
impl Config {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let explicit = path.is_some();
        let path = path.unwrap_or(dirs()?.config_dir().join("config.toml"));
        let mut config: Self = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text)?,
            Err(e) if !explicit && e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        config.max_output_lines = config.max_output_lines.clamp(100, 10000);
        Ok(config)
    }
}
