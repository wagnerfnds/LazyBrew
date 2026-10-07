use crate::backend::{BrewError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PackageKind {
    Formula,
    Cask,
}
impl PackageKind {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Formula => "--formula",
            Self::Cask => "--cask",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PackageId {
    name: String,
    pub kind: PackageKind,
}
impl PackageId {
    pub fn new(name: impl Into<String>, kind: PackageKind) -> Result<Self> {
        let name = name.into();
        let parts: Vec<_> = name.split('/').collect();
        if !matches!(parts.len(), 1 | 3)
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p.as_bytes()[0].is_ascii_alphanumeric()
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"@+._-".contains(&b))
                    || *p == "."
                    || *p == ".."
            })
        {
            return Err(BrewError::InvalidName(name));
        }
        Ok(Self { name, kind })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Package {
    pub id: PackageId,
    pub installed: Vec<String>,
    pub version: String,
    pub description: String,
    pub pinned: bool,
    pub outdated: bool,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageInfo {
    pub package: Package,
    pub homepage: String,
    pub dependencies: Vec<String>,
    pub caveats: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Service {
    pub name: String,
    pub status: String,
    pub user: Option<String>,
    pub file: Option<String>,
    pub exit_code: Option<i32>,
    pub running: Option<bool>,
    pub loaded: Option<bool>,
    pub schedulable: Option<bool>,
    pub pid: Option<u32>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Operation {
    Sequence {
        label: String,
        steps: Vec<Operation>,
    },
    Link(PackageId),
    Unlink(PackageId),
    Install(PackageId),
    Uninstall(PackageId),
    Upgrade(PackageId),
    Pin(PackageId),
    Unpin(PackageId),
    Start(PackageId),
    Stop(PackageId),
    Restart(PackageId),
    Update,
    Cleanup,
    Doctor,
}
impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Self::Sequence { label, steps } = self {
            writeln!(f, "{label}")?;
            for (index, step) in steps.iter().enumerate() {
                writeln!(f, "{}. {step}", index + 1)?;
            }
            return Ok(());
        }
        let (action, package) = match self {
            Self::Sequence { .. } => unreachable!(),
            Self::Link(p) => (
                "Activate version (link --force; existing files are preserved)",
                Some(p),
            ),
            Self::Unlink(p) => ("Deactivate version (unlink)", Some(p)),
            Self::Install(p) => ("Install", Some(p)),
            Self::Uninstall(p) => ("Uninstall", Some(p)),
            Self::Upgrade(p) => ("Upgrade", Some(p)),
            Self::Pin(p) => ("Pin", Some(p)),
            Self::Unpin(p) => ("Unpin", Some(p)),
            Self::Start(p) => ("Start service", Some(p)),
            Self::Stop(p) => ("Stop service", Some(p)),
            Self::Restart(p) => ("Restart service", Some(p)),
            Self::Update => ("Update Homebrew metadata", None),
            Self::Cleanup => ("Clean up Homebrew downloads and old versions", None),
            Self::Doctor => ("Run Homebrew diagnostics", None),
        };
        write!(f, "{action}")?;
        if let Some(p) = package {
            write!(
                f,
                " {} ({})",
                p.name(),
                match p.kind {
                    PackageKind::Formula => "formula",
                    PackageKind::Cask => "cask",
                }
            )?;
        }
        Ok(())
    }
}

impl<'de> serde::Deserialize<'de> for PackageId {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Wire {
            name: String,
            kind: PackageKind,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.name, wire.kind).map_err(serde::de::Error::custom)
    }
}
