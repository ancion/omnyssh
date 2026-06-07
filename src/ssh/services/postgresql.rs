//! PostgreSQL service provider.
//!
//! Detects PostgreSQL from probe output.

use async_trait::async_trait;

use super::ServiceProvider;
use crate::event::ServiceKind;
use crate::ssh::probe::ProbeOutput;

/// PostgreSQL service provider.
pub struct PostgreSQLProvider;

#[async_trait]
impl ServiceProvider for PostgreSQLProvider {
    fn kind(&self) -> ServiceKind {
        ServiceKind::PostgreSQL
    }

    fn detect(&self, probe_output: &ProbeOutput) -> bool {
        // Check for postgresql service in systemd OR port 5432 in listening ports
        if let Some(services) = probe_output.get_section("SERVICES") {
            if services.contains("postgresql") {
                return true;
            }
        }
        if let Some(listen) = probe_output.get_section("LISTEN") {
            if listen.contains(":5432") || listen.contains("5432") {
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
    fn test_pg_detect_from_systemd() {
        let probe = "===OMNYSSH:SERVICES===\npostgresql.service\nsshd.service\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(PostgreSQLProvider.detect(&parsed));
    }

    #[test]
    fn test_pg_detect_from_port() {
        let probe = "===OMNYSSH:LISTEN===\n0.0.0.0:5432\tLISTEN\n";
        let parsed = ProbeOutput::parse(probe).expect("should parse");
        assert!(PostgreSQLProvider.detect(&parsed));
    }
}
