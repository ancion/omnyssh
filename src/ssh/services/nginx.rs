//! Nginx service provider.
//!
//! Detects the Nginx web server from probe output.

use async_trait::async_trait;

use super::ServiceProvider;
use crate::event::ServiceKind;
use crate::ssh::probe::ProbeOutput;

/// Nginx service provider.
pub struct NginxProvider;

#[async_trait]
impl ServiceProvider for NginxProvider {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Nginx
    }

    fn detect(&self, probe_output: &ProbeOutput) -> bool {
        // Check for nginx in systemd services OR nginx process
        if let Some(services) = probe_output.get_section("SERVICES") {
            if services.contains("nginx") {
                return true;
            }
        }
        if let Some(processes) = probe_output.get_section("PROCESS") {
            if processes.contains("nginx") {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nginx_detect_from_process() {
        let probe = "===OMNYSSH:PROCESS===\nroot 1234 nginx: master process\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(NginxProvider.detect(&parsed));
    }
}
