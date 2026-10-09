//! The parts of the docker CLI that dtail drives.

use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PS_FORMAT: &str = "{{.ID}}\t{{.Names}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.container-number\"}}";

#[derive(Debug, PartialEq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub label: String,
}

/// Parses one line of `docker ps --format PS_FORMAT`. Compose containers are
/// labelled by service name, with the replica number appended past the first.
fn parse_ps_line(line: &str, full_names: bool) -> Option<Container> {
    let mut fields = line.split('\t');
    let id = fields.next().filter(|s| !s.is_empty())?.to_string();
    let name = fields.next().filter(|s| !s.is_empty())?.to_string();
    let service = fields.next().unwrap_or("");
    let number = fields.next().unwrap_or("");

    let label = if full_names || service.is_empty() {
        name.clone()
    } else if number.is_empty() || number == "1" {
        service.to_string()
    } else {
        format!("{service}-{number}")
    };
    Some(Container { id, name, label })
}

/// Lists running containers via the docker CLI, so the active Docker context
/// (Docker Desktop, colima, ...) is respected.
pub fn list_containers(full_names: bool) -> Result<Vec<Container>, String> {
    let out = Command::new("docker")
        .args(["ps", "--format", PS_FORMAT])
        .output()
        .map_err(|e| format!("failed to run docker: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| parse_ps_line(l, full_names))
        .collect())
}

/// How much history to request when starting to tail a container.
pub enum History {
    Tail(u64),
    Since(String),
    All,
}

impl History {
    /// Everything logged from `t` onwards.
    pub fn since_time(t: SystemTime) -> Self {
        let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
        History::Since(format!("{}.{:09}", d.as_secs(), d.subsec_nanos()))
    }

    fn args(&self) -> Vec<String> {
        match self {
            History::Tail(n) => vec!["--tail".into(), n.to_string()],
            History::Since(s) => vec!["--since".into(), s.clone()],
            History::All => vec![],
        }
    }
}

/// Starts a `docker logs --follow` for a container, with its output piped.
pub fn follow_logs(id: &str, history: &History, timestamps: bool) -> Result<Child, String> {
    let mut cmd = Command::new("docker");
    cmd.args(["logs", "--follow"]);
    if timestamps {
        cmd.arg("--timestamps");
    }
    cmd.args(history.args())
        .arg(id)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run docker logs: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_container_is_labelled_by_service() {
        let c = parse_ps_line("abc\tproj-kyberlifeorder-1\tkyberlifeorder\t1", false).unwrap();
        assert_eq!(c.label, "kyberlifeorder");
        assert_eq!(c.name, "proj-kyberlifeorder-1");
        assert_eq!(c.id, "abc");
    }

    #[test]
    fn extra_compose_replicas_get_their_number() {
        let c = parse_ps_line("abc\tproj-web-2\tweb\t2", false).unwrap();
        assert_eq!(c.label, "web-2");
    }

    #[test]
    fn plain_container_is_labelled_by_name() {
        let c = parse_ps_line("abc\tmy-postgres\t\t", false).unwrap();
        assert_eq!(c.label, "my-postgres");
    }

    #[test]
    fn full_names_uses_container_name() {
        let c = parse_ps_line("abc\tproj-web-1\tweb\t1", true).unwrap();
        assert_eq!(c.label, "proj-web-1");
    }

    #[test]
    fn blank_ps_line_is_skipped() {
        assert_eq!(parse_ps_line("", false), None);
    }

    #[test]
    fn since_time_is_a_fractional_unix_timestamp() {
        let t = UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 5);
        assert_eq!(
            History::since_time(t).args(),
            ["--since", "1700000000.000000005"]
        );
    }
}
