//! Bounded, local snapshots. Atomic replacement keeps the last valid state on a crash.
use crate::domain::{Package, PackageId, Service};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub operation: String,
    pub started: u64,
    pub finished: Option<u64>,
    pub result: String,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub updated: u64,
    pub installed: Vec<Package>,
    pub outdated: Vec<Package>,
    pub services: Vec<Service>,
    #[serde(default)]
    pub catalogue: Vec<Package>,
    pub details: VecDeque<((bool, PackageId), String)>,
    pub history: VecDeque<HistoryEntry>,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    backend: PathBuf,
    snapshot: Snapshot,
}
pub struct Store {
    path: PathBuf,
    backend: PathBuf,
}
impl Store {
    pub fn new(path: PathBuf, backend: PathBuf) -> Self {
        Self { path, backend }
    }
    pub fn load(&self) -> anyhow::Result<Snapshot> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Snapshot::default()),
            Err(e) => return Err(e.into()),
        };
        anyhow::ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "Saved state exceeds size limit"
        );
        let mut data: Envelope = serde_json::from_slice(&bytes)?;
        if data.version != 1 || data.backend != self.backend {
            return Ok(Snapshot::default());
        }
        data.snapshot.details.truncate(64);
        data.snapshot.history.truncate(100);
        for entry in &mut data.snapshot.history {
            if entry.finished.is_none() {
                entry.result = "Interrupted (previous session ended)".into();
                entry.finished = Some(now());
            }
        }
        if now().saturating_sub(data.snapshot.updated) > 3600 {
            data.snapshot.details.clear();
        }
        Ok(data.snapshot)
    }
    pub fn save(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid state path"))?;
        std::fs::create_dir_all(parent)?;
        let temp = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        let data = serde_json::to_vec(&Envelope {
            version: 1,
            backend: self.backend.clone(),
            snapshot,
        })?;
        anyhow::ensure!(
            data.len() <= 16 * 1024 * 1024,
            "Saved state exceeds size limit"
        );
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&data)?;
        file.sync_all()?;
        std::fs::rename(&temp, &self.path)?;
        Ok(())
    }
}
