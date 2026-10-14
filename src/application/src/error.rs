use ommatix_domain::DomainError;
use thiserror::Error;

/// Error reported by an adapter through a port.
#[derive(Debug, Error)]
#[error("{adapter}: {message}")]
pub struct PortError {
    pub adapter: &'static str,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl PortError {
    pub fn new(adapter: &'static str, message: impl Into<String>) -> Self {
        Self {
            adapter,
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("frame source failed: {0}")]
    Capture(#[source] PortError),
}
