use super::{BrewBackend, BrewError, CommandHandle, Result, parser, runner};
use crate::domain::*;
use std::path::PathBuf;
pub struct CliBackend {
    path: PathBuf,
}
impl CliBackend {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    async fn query(&self, args: &[&str]) -> Result<Vec<u8>> {
        runner::query(
            &self.path,
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
        )
        .await
    }
    async fn search_kind(&self, args: &[&str]) -> Result<Vec<u8>> {
        match self.query(args).await {
            // Search is the sole text-only endpoint. Homebrew exits 1 on no matches.
            Err(BrewError::Command(message))
                if message.starts_with("Error: No formulae or casks found for ") =>
            {
                Ok(Vec::new())
            }
            other => other,
        }
    }
}
pub fn operation_args(op: &Operation) -> Result<Vec<String>> {
    let args: Vec<&str> = match op {
        Operation::Sequence { .. } => {
            return Err(BrewError::Unsupported(
                "Sequence requires a command plan".into(),
            ));
        }
        Operation::Link(p) | Operation::Unlink(p) => {
            if p.kind != PackageKind::Formula {
                return Err(BrewError::Unsupported(
                    "Version links require a formula".into(),
                ));
            }
            if matches!(op, Operation::Link(_)) {
                vec!["link", "--force", "--formula", p.name()]
            } else {
                vec!["unlink", "--formula", p.name()]
            }
        }
        Operation::Install(p) => vec!["install", p.kind.flag(), p.name()],
        Operation::Uninstall(p) => vec!["uninstall", p.kind.flag(), p.name()],
        Operation::Upgrade(p) => vec!["upgrade", p.kind.flag(), p.name()],
        Operation::Pin(p) | Operation::Unpin(p) => {
            if p.kind != PackageKind::Formula {
                return Err(BrewError::Unsupported("Only formulae can be pinned".into()));
            }
            vec![
                if matches!(op, Operation::Pin(_)) {
                    "pin"
                } else {
                    "unpin"
                },
                p.name(),
            ]
        }
        Operation::Start(p) | Operation::Stop(p) | Operation::Restart(p) => {
            if p.kind != PackageKind::Formula {
                return Err(BrewError::Unsupported("Services require a formula".into()));
            }
            vec![
                "services",
                match op {
                    Operation::Start(_) => "start",
                    Operation::Stop(_) => "stop",
                    _ => "restart",
                },
                p.name(),
            ]
        }
        Operation::Update => vec!["update"],
        Operation::Cleanup => vec!["cleanup"],
        Operation::Doctor => vec!["doctor"],
    };
    Ok(args.into_iter().map(String::from).collect())
}
impl BrewBackend for CliBackend {
    async fn installed_packages(&self) -> Result<Vec<Package>> {
        Ok(
            parser::info(&self.query(&["info", "--json=v2", "--installed"]).await?)?
                .into_iter()
                .map(|i| i.package)
                .collect(),
        )
    }
    async fn outdated_packages(&self) -> Result<Vec<Package>> {
        parser::outdated(&self.query(&["outdated", "--json=v2"]).await?)
    }
    async fn services(&self) -> Result<Vec<Service>> {
        parser::services(&self.query(&["services", "list", "--json"]).await?)
    }
    async fn service_info(&self, name: &str) -> Result<Vec<Service>> {
        let id = PackageId::new(name, PackageKind::Formula)?;
        parser::services(
            &self
                .query(&["services", "info", id.name(), "--json"])
                .await?,
        )
    }
    async fn package_info(&self, id: &PackageId) -> Result<PackageInfo> {
        parser::info(
            &self
                .query(&["info", "--json=v2", id.kind.flag(), id.name()])
                .await?,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| BrewError::Command("Package not found".into()))
    }
    async fn catalogue(&self) -> Result<Vec<Package>> {
        let (formulae, casks) =
            tokio::try_join!(self.query(&["formulae"]), self.query(&["casks"]))?;
        let mut packages =
            parser::search(&String::from_utf8_lossy(&formulae), PackageKind::Formula);
        packages.extend(parser::search(
            &String::from_utf8_lossy(&casks),
            PackageKind::Cask,
        ));
        Ok(packages)
    }
    async fn search(&self, query: &str) -> Result<Vec<Package>> {
        // Restrict search to literal package-like substrings, never flags or regexes.
        PackageId::new(query, PackageKind::Formula)?;
        let formula_args = ["search", "--formula", query];
        let cask_args = ["search", "--cask", query];
        let (formula, cask) = tokio::try_join!(
            self.search_kind(&formula_args),
            self.search_kind(&cask_args)
        )?;
        let mut results = parser::search(&String::from_utf8_lossy(&formula), PackageKind::Formula);
        results.extend(parser::search(
            &String::from_utf8_lossy(&cask),
            PackageKind::Cask,
        ));
        Ok(results)
    }
    async fn execute(&self, operation: Operation) -> Result<CommandHandle> {
        let plan = operation_plan(&operation)?;
        runner::spawn_plan(&self.path, plan)
    }
}

pub fn operation_plan(operation: &Operation) -> Result<Vec<Vec<String>>> {
    match operation {
        Operation::Sequence { steps, .. } => {
            if steps.is_empty() || steps.len() > 100 {
                return Err(BrewError::Unsupported(
                    "Invalid operation plan length".into(),
                ));
            }
            steps.iter().map(operation_args).collect()
        }
        operation => Ok(vec![operation_args(operation)?]),
    }
}
