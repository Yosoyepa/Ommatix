use thiserror::Error;

/// Error reported by an adapter through a port.
///
/// `Display` shows only this level (`adapter: message`); the underlying cause, if any, is
/// available through [`std::error::Error::source`] so reporters can print the chain once.
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

/// Formats an error and all its causes on one line: `outer: cause: root cause`.
pub fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = error.to_string();
    let mut cause = error.source();
    while let Some(e) = cause {
        out.push_str(": ");
        out.push_str(&e.to_string());
        cause = e.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_lists_every_cause_once() {
        let io = std::io::Error::other("device busy");
        let err = PortError::new("screen", "capture failed").with_source(io);
        assert_eq!(error_chain(&err), "screen: capture failed: device busy");
    }
}
