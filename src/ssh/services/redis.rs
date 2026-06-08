//! Redis service provider.
//!
//! Detects Redis servers from probe output.

use async_trait::async_trait;

use super::ServiceProvider;
use crate::event::ServiceKind;
use crate::ssh::probe::ProbeOutput;

/// Redis service provider.
pub struct RedisProvider;

#[async_trait]
impl ServiceProvider for RedisProvider {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Redis
    }

    fn detect(&self, probe_output: &ProbeOutput) -> bool {
        // Check for Redis on port 6379 in listening ports OR redis process
        if let Some(listen) = probe_output.get_section("LISTEN") {
            if listen.contains(":6379") || listen.contains("6379") {
                return true;
            }
        }
        if let Some(processes) = probe_output.get_section("PROCESS") {
            if processes.contains("redis-server") || processes.contains("redis") {
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
    fn test_redis_detect_from_port() {
        let probe = "===OMNYSSH:LISTEN===\n0.0.0.0:6379\tLISTEN\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(RedisProvider.detect(&parsed));
    }

    #[test]
    fn test_redis_detect_from_process() {
        let probe = "===OMNYSSH:PROCESS===\nredis 1234 redis-server\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(RedisProvider.detect(&parsed));
    }

    #[test]
    fn test_redis_not_detected() {
        let probe = "===OMNYSSH:SERVICES===\nsshd.service\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(!RedisProvider.detect(&parsed));
    }
}
