//! Reviewed Homebrew plans for installed runtime versions and configured stacks.
use crate::{
    backend::{BrewError, Result},
    domain::*,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub name: String,
    pub formulae: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
}
impl Stack {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 64
            || self.name.chars().any(char::is_control)
            || self.formulae.is_empty()
            || self.formulae.len() > 25
        {
            return Err(BrewError::Unsupported(
                "Stacks need a name and 1–25 formulae".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for name in &self.formulae {
            PackageId::new(name, PackageKind::Formula)?;
            if !seen.insert(name) {
                return Err(BrewError::Unsupported("Duplicate stack formula".into()));
            }
        }
        seen.clear();
        for service in &self.services {
            if !self.formulae.contains(service) || !seen.insert(service) {
                return Err(BrewError::Unsupported(
                    "Stack services must be unique members of its formulae".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn plan(&self, action: char, installed: &[Package]) -> Result<Operation> {
        self.validate()?;
        let mut steps = Vec::new();
        if action == 'i' {
            for name in &self.formulae {
                let id = PackageId::new(name, PackageKind::Formula)?;
                if !installed
                    .iter()
                    .any(|package| package.id == id && !package.installed.is_empty())
                {
                    steps.push(Operation::Install(id));
                }
            }
        } else if action != 's' && action != 't' {
            return Err(BrewError::Unsupported("Unknown stack action".into()));
        }
        for name in &self.services {
            let id = PackageId::new(name, PackageKind::Formula)?;
            if action == 't' {
                steps.push(Operation::Stop(id));
            } else {
                steps.push(Operation::Start(id));
            }
        }
        if steps.is_empty() {
            return Err(BrewError::Unsupported(
                "Stack already installed; no services to change".into(),
            ));
        }
        Ok(Operation::Sequence {
            label: format!(
                "{} stack {}",
                if action == 't' {
                    "Stop"
                } else if action == 'i' {
                    "Install and start"
                } else {
                    "Start"
                },
                self.name
            ),
            steps,
        })
    }
}
fn family(id: &PackageId) -> Option<&str> {
    if id.kind != PackageKind::Formula || id.name().contains('/') {
        return None;
    }
    let base = id.name().split('@').next()?;
    matches!(base, "php" | "node" | "postgresql").then_some(base)
}
pub fn switch_version(
    target: &PackageId,
    installed: &[Package],
    services: &[Service],
) -> Result<Operation> {
    let family = family(target).ok_or_else(|| {
        BrewError::Unsupported("Select an installed PHP, Node or PostgreSQL formula".into())
    })?;
    if !installed
        .iter()
        .any(|p| p.id == *target && !p.installed.is_empty())
    {
        return Err(BrewError::Unsupported(
            "Install the target version first".into(),
        ));
    }
    let mut steps = Vec::new();
    let alternatives: Vec<_> = installed
        .iter()
        .filter(|p| {
            self::family(&p.id) == Some(family) && p.id != *target && !p.installed.is_empty()
        })
        .collect();
    for p in &alternatives {
        if services
            .iter()
            .any(|s| s.name == p.id.name() && (s.running == Some(true) || s.status == "started"))
        {
            steps.push(Operation::Stop(p.id.clone()));
        }
    }
    for p in alternatives {
        steps.push(Operation::Unlink(p.id.clone()));
    }
    steps.push(Operation::Link(target.clone()));
    // Only transfer a service if a sibling was actually running, and the target has
    // an existing service definition. PostgreSQL data directories are never migrated.
    if steps.iter().any(|step| matches!(step, Operation::Stop(_)))
        && services.iter().any(|s| s.name == target.name())
    {
        steps.push(Operation::Start(target.clone()));
    }
    Ok(Operation::Sequence {
        label: format!(
            "Switch active {family} to {} (no database migration)",
            target.name()
        ),
        steps,
    })
}
