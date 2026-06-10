//! Node.js service provider.
//!
//! Detects Node.js processes and PM2-managed applications from probe output.

use async_trait::async_trait;

use super::ServiceProvider;
use crate::event::ServiceKind;
use crate::ssh::probe::ProbeOutput;

/// Node.js service provider.
pub struct NodeJSProvider;

#[async_trait]
impl ServiceProvider for NodeJSProvider {
    fn kind(&self) -> ServiceKind {
        ServiceKind::NodeJS
    }

    fn detect(&self, probe_output: &ProbeOutput) -> bool {
        // Check for node processes in PROCESS section
        if let Some(processes) = probe_output.get_section("PROCESS") {
            if processes.contains("node ") || processes.contains("/node") {
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nodejs_detect_from_process() {
        let probe = "===OMNYSSH:PROCESS===\nuser 1234 /usr/bin/node server.js\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(NodeJSProvider.detect(&parsed));
    }

    #[test]
    fn test_nodejs_not_detected() {
        let probe = "===OMNYSSH:SERVICES===\nsshd.service\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(!NodeJSProvider.detect(&parsed));
    }
}
