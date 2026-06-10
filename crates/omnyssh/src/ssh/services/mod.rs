//! Service provider registry and trait definitions.
//!
//! Each service (Docker, PostgreSQL, Nginx, etc.) implements the
//! [`ServiceProvider`] trait. The registry discovers which providers
//! apply to a given server based on the probe output.

use async_trait::async_trait;

use crate::event::{ServiceKind, ServiceMetric};
use crate::ssh::probe::ProbeOutput;

// Service provider modules
pub mod docker;
pub mod nginx;
pub mod nodejs;
pub mod postgresql;
pub mod redis;

/// Trait for service-specific detection from probe output.
///
/// Each provider implements:
/// 1. Quick detection from probe output
/// 2. Basic metric extraction from Quick Scan output
#[async_trait]
pub trait ServiceProvider: Send + Sync {
    /// Returns the service type this provider handles.
    fn kind(&self) -> ServiceKind;

    /// Quick check: is this service present on the server?
    ///
    /// Called during Quick Scan with the parsed probe output.
    /// Should be fast — only check for presence, not detailed metrics.
    fn detect(&self, probe_output: &ProbeOutput) -> bool;

    /// Extract basic metrics from Quick Scan probe output.
    ///
    /// This is called immediately during Quick Scan to provide basic
    /// service information. Default implementation returns empty metrics.
    fn quick_metrics(&self, _probe_output: &ProbeOutput) -> Vec<ServiceMetric> {
        Vec::new()
    }
}

/// Service registry that manages all available providers.
pub struct ServiceRegistry {
    providers: Vec<Box<dyn ServiceProvider>>,
}

impl ServiceRegistry {
    /// Create a new registry with all built-in providers.
    /// Only 5 core services are supported: Docker, Nginx, PostgreSQL, Redis, Node.js.
    pub fn new() -> Self {
        let providers: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(docker::DockerProvider),
            Box::new(nginx::NginxProvider),
            Box::new(postgresql::PostgreSQLProvider),
            Box::new(redis::RedisProvider),
            Box::new(nodejs::NodeJSProvider),
        ];

        Self { providers }
    }

    /// Detect which services are present based on probe output.
    ///
    /// Returns a list of service kinds that were detected.
    pub fn detect_services(&self, probe_output: &ProbeOutput) -> Vec<ServiceKind> {
        self.providers
            .iter()
            .filter(|p| p.detect(probe_output))
            .map(|p| p.kind())
            .collect()
    }

    /// Get a provider by service kind.
    pub fn get_provider(&self, kind: &ServiceKind) -> Option<&dyn ServiceProvider> {
        self.providers
            .iter()
            .find(|p| &p.kind() == kind)
            .map(|boxed| &**boxed)
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a simple service metric.
pub fn metric_int(name: impl Into<String>, value: i64) -> ServiceMetric {
    ServiceMetric {
        name: name.into(),
        value: crate::event::MetricValue::Integer(value),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ServiceRegistry::new();
        assert_eq!(registry.providers.len(), 5); // Docker, Nginx, PostgreSQL, Redis, Node.js
    }

    #[test]
    fn test_registry_get_provider() {
        let registry = ServiceRegistry::new();
        assert!(registry.get_provider(&ServiceKind::Docker).is_some());
        assert!(registry.get_provider(&ServiceKind::Nginx).is_some());
        assert!(registry.get_provider(&ServiceKind::PostgreSQL).is_some());
        assert!(registry.get_provider(&ServiceKind::Redis).is_some());
        assert!(registry.get_provider(&ServiceKind::NodeJS).is_some());
    }
}
