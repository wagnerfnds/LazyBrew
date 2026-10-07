use serde::Deserialize;
use std::path::PathBuf;
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub brew_path: PathBuf,
    pub theme: crate::theme::ThemeChoice,
    pub max_output_lines: usize,
    pub refresh_interval_secs: u64,
    pub stacks: Vec<crate::workflows::Stack>,
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
            theme: crate::theme::ThemeChoice::Auto,
            max_output_lines: 2000,
            refresh_interval_secs: 300,
            stacks: Vec::new(),
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
        if config.refresh_interval_secs > 0 {
            config.refresh_interval_secs = config.refresh_interval_secs.clamp(30, 86400);
        }
        anyhow::ensure!(config.stacks.len() <= 20, "At most 20 stacks are supported");
        let mut names = std::collections::HashSet::new();
        for stack in &config.stacks {
            stack.validate()?;
            anyhow::ensure!(names.insert(&stack.name), "Stack names must be unique");
        }
        Ok(config)
    }
}
