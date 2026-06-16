//! Docker service provider.
//!
//! Detects Docker and Docker Compose and extracts basic container metrics.

use async_trait::async_trait;

use super::{metric_int, ServiceProvider};
use crate::event::ServiceKind;
use crate::ssh::probe::ProbeOutput;

/// Docker service provider.
pub struct DockerProvider;

#[async_trait]
impl ServiceProvider for DockerProvider {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Docker
    }

    fn detect(&self, probe_output: &ProbeOutput) -> bool {
        // Check if Docker section has content
        probe_output.has_section("DOCKER")
    }

    /// Extract basic metrics from Quick Scan docker ps output.
    /// This allows us to show container count immediately.
    fn quick_metrics(&self, probe_output: &ProbeOutput) -> Vec<super::ServiceMetric> {
        let mut metrics = Vec::new();

        if let Some(docker_output) = probe_output.get_section("DOCKER") {
            // Parse docker ps output: ID\tNames\tStatus\tImage
            let lines: Vec<&str> = docker_output.lines().collect();
            let total = lines.len() as i64;

            // Count running containers (Status contains "Up")
            let running = lines
                .iter()
                .filter(|line| {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        parts[2].contains("Up")
                    } else {
                        false
                    }
                })
                .count() as i64;

            metrics.push(metric_int("containers_total", total));
            metrics.push(metric_int("containers_running", running));
        }

        metrics
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_detect_from_probe() {
        let probe_output = "===OMNYSSH:DOCKER===\nabc123\tnginx\tUp 2 hours\tnginx:latest\n";
        let parsed = ProbeOutput::parse(probe_output).expect("should parse");
        let provider = DockerProvider;
        assert!(provider.detect(&parsed));
    }

    #[test]
    fn test_docker_not_detected_when_absent() {
        let probe_output = "===OMNYSSH:OS===\nUbuntu\n";
        let parsed = ProbeOutput::parse(probe_output).expect("should parse");
        let provider = DockerProvider;
        assert!(!provider.detect(&parsed));
    }
}
