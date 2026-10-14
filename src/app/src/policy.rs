//! The local policy as the operator configured it (`[safety]`).

use ommatix_application::{LocalPolicy, PolicyStore};

/// A [`PolicyStore`] fixed at startup from the configuration file. Nothing in the process
/// writes it, so the agent can never change it (thesis chapter 08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPolicy(LocalPolicy);

impl ConfigPolicy {
    pub fn new(policy: LocalPolicy) -> Self {
        Self(policy)
    }
}

impl PolicyStore for ConfigPolicy {
    fn current(&self) -> LocalPolicy {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_what_it_was_built_with() {
        let policy = LocalPolicy {
            include_values: true,
            ..LocalPolicy::default()
        };
        assert_eq!(ConfigPolicy::new(policy.clone()).current(), policy);
    }
}
