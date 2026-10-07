pub mod cli;
pub mod parser;
pub mod runner;
use crate::domain::*;
pub use runner::{CommandEvent, CommandHandle};
use std::future::Future;
pub type Result<T> = std::result::Result<T, BrewError>;
#[derive(Debug, thiserror::Error)]
pub enum BrewError {
    #[error("Invalid package name: {0}")]
    InvalidName(String),
    #[error("Homebrew I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid Homebrew JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Homebrew command failed: {0}")]
    Command(String),
    #[error("Homebrew query timed out after 60 seconds")]
    Timeout,
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}
pub trait BrewBackend: Send + Sync + 'static {
    fn installed_packages(&self) -> impl Future<Output = Result<Vec<Package>>> + Send;
    fn outdated_packages(&self) -> impl Future<Output = Result<Vec<Package>>> + Send;
    fn services(&self) -> impl Future<Output = Result<Vec<Service>>> + Send;
    fn service_info(&self, name: &str) -> impl Future<Output = Result<Vec<Service>>> + Send;
    fn package_info(&self, id: &PackageId) -> impl Future<Output = Result<PackageInfo>> + Send;
    fn catalogue(&self) -> impl Future<Output = Result<Vec<Package>>> + Send {
        async { Err(BrewError::Unsupported("Catalogue unavailable".into())) }
    }
    fn search(&self, query: &str) -> impl Future<Output = Result<Vec<Package>>> + Send;
    fn execute(&self, operation: Operation) -> impl Future<Output = Result<CommandHandle>> + Send;
}
