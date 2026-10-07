//! Homebrew wire models stay here; the rest of the application uses domain types.
use crate::{backend::Result, domain::*};
use serde::Deserialize;
#[derive(Deserialize)]
struct Info {
    formulae: Vec<Formula>,
    casks: Vec<Cask>,
}
#[derive(Deserialize)]
struct Version {
    stable: Option<String>,
}
#[derive(Deserialize)]
struct Installed {
    version: String,
}
#[derive(Deserialize)]
struct Formula {
    name: String,
    full_name: Option<String>,
    desc: Option<String>,
    homepage: String,
    versions: Version,
    installed: Vec<Installed>,
    pinned: bool,
    outdated: bool,
    dependencies: Vec<String>,
    caveats: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum CaskInstalled {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
struct Cask {
    token: String,
    full_token: Option<String>,
    desc: Option<String>,
    homepage: String,
    version: String,
    installed: Option<CaskInstalled>,
    outdated: bool,
    caveats: Option<String>,
}
pub fn info(bytes: &[u8]) -> Result<Vec<PackageInfo>> {
    let wire: Info = serde_json::from_slice(bytes)?;
    let mut result = Vec::new();
    for f in wire.formulae {
        result.push(PackageInfo {
            package: Package {
                id: PackageId::new(f.full_name.unwrap_or(f.name), PackageKind::Formula)?,
                installed: f.installed.into_iter().map(|v| v.version).collect(),
                version: f.versions.stable.unwrap_or_else(|| "HEAD".into()),
                description: f.desc.unwrap_or_default(),
                pinned: f.pinned,
                outdated: f.outdated,
            },
            homepage: f.homepage,
            dependencies: f.dependencies,
            caveats: f.caveats.unwrap_or_default(),
        });
    }
    for c in wire.casks {
        result.push(PackageInfo {
            package: Package {
                id: PackageId::new(c.full_token.unwrap_or(c.token), PackageKind::Cask)?,
                installed: match c.installed {
                    Some(CaskInstalled::One(v)) => vec![v],
                    Some(CaskInstalled::Many(v)) => v,
                    None => vec![],
                },
                version: c.version,
                description: c.desc.unwrap_or_default(),
                pinned: false,
                outdated: c.outdated,
            },
            homepage: c.homepage,
            dependencies: vec![],
            caveats: c.caveats.unwrap_or_default(),
        });
    }
    Ok(result)
}
#[derive(Deserialize)]
struct Outdated {
    formulae: Vec<OldPackage>,
    casks: Vec<OldPackage>,
}
#[derive(Deserialize)]
struct OldPackage {
    name: String,
    installed_versions: Vec<String>,
    current_version: String,
    #[serde(default)]
    pinned: bool,
}
pub fn outdated(bytes: &[u8]) -> Result<Vec<Package>> {
    let wire: Outdated = serde_json::from_slice(bytes)?;
    wire.formulae
        .into_iter()
        .map(|p| (p, PackageKind::Formula))
        .chain(wire.casks.into_iter().map(|p| (p, PackageKind::Cask)))
        .map(|(p, kind)| {
            Ok(Package {
                id: PackageId::new(p.name, kind)?,
                installed: p.installed_versions,
                version: p.current_version,
                description: String::new(),
                pinned: p.pinned,
                outdated: true,
            })
        })
        .collect()
}
pub fn services(bytes: &[u8]) -> Result<Vec<Service>> {
    Ok(serde_json::from_slice(bytes)?)
}
/// Search has no JSON CLI contract. Run each kind separately with colors disabled.
pub fn search(text: &str, kind: PackageKind) -> Vec<Package> {
    text.lines()
        .filter(|l| !l.starts_with("==>"))
        .flat_map(str::split_whitespace)
        .filter_map(|name| PackageId::new(name.trim_end_matches(['✔', '✓']), kind).ok())
        .map(|id| Package {
            id,
            installed: vec![],
            version: String::new(),
            description: String::new(),
            pinned: false,
            outdated: false,
        })
        .collect()
}
