use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

/// A stand-in `docker` that lists the containers in `$FAKE_PS` and, for
/// `docker logs`, echoes the arguments it got plus one stdout and one stderr
/// line, then exits (as real `docker logs -f` does when a container stops).
const FAKE_DOCKER: &str = r#"#!/bin/sh
case "$1" in
  ps)
    [ -n "$FAKE_PS_FAIL" ] && { echo "Cannot connect to the Docker daemon" >&2; exit 1; }
    cat "$FAKE_PS" ;;
  logs)
    shift
    for id; do :; done
    echo "args: $*"
    echo "hello from $id"
    echo "ERROR from $id" >&2 ;;
esac
"#;

const CONTAINERS: &str = "id1\tproj-web-1\tweb\t1\n\
                          id2\tproj-web-2\tweb\t2\n\
                          id3\tproj-kyberlifeorder-1\tkyberlifeorder\t1\n\
                          id4\tstandalone-redis\t\t\n";

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let docker = dir.path().join("docker");
        std::fs::write(&docker, FAKE_DOCKER).unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.path().join("ps.txt"), CONTAINERS).unwrap();
        Self { dir }
    }

    fn ps_file(&self) -> std::path::PathBuf {
        self.dir.path().join("ps.txt")
    }

    fn command(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dtail"));
        cmd.args(args)
            .env("PATH", path)
            .env("FAKE_PS", self.ps_file())
            .env_remove("FAKE_PS_FAIL");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("failed to run dtail")
    }
}

fn stdout_lines(out: &Output) -> Vec<String> {
    let mut lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

#[test]
fn prefixes_stdout_and_stderr_lines_with_service_labels() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "order"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout_lines(&out),
        vec![
            "kyberlifeorder │ ERROR from id3",
            "kyberlifeorder │ args: --follow --tail 10 id3",
            "kyberlifeorder │ hello from id3",
        ]
    );
}

#[test]
fn patterns_match_service_names_not_compose_project() {
    let fx = Fixture::new();
    // "proj" is only in the container names, never in a service name.
    let out = fx.run(&["-1", "proj"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no running containers match"));
}

#[test]
fn full_names_labels_and_matches_by_container_name() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "-f", "-g", "hello", "proj-web"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout_lines(&out),
        vec!["proj-web-1 │ hello from id1", "proj-web-2 │ hello from id2"]
    );
}

#[test]
fn extra_replicas_and_plain_containers_get_their_own_labels() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "-g", "hello", "web", "redis"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout_lines(&out),
        vec![
            "standalone-redis │ hello from id4",
            "web              │ hello from id1",
            "web-2            │ hello from id2",
        ]
    );
}

#[test]
fn exclude_skips_matching_containers() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "-g", "hello", "-x", "web", "-x", "redis"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(stdout_lines(&out), vec!["kyberlifeorder │ hello from id3"]);
}

#[test]
fn grep_filters_lines_case_insensitively() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "-g", "error", "order"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(stdout_lines(&out), vec!["kyberlifeorder │ ERROR from id3"]);
}

#[test]
fn history_flags_are_passed_to_docker_logs() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "-g", "args", "-n", "50", "-t", "order"]);
    assert_eq!(
        stdout_lines(&out),
        vec!["kyberlifeorder │ args: --follow --timestamps --tail 50 id3"]
    );

    let out = fx.run(&["-1", "-g", "args", "-s", "5m", "order"]);
    assert_eq!(
        stdout_lines(&out),
        vec!["kyberlifeorder │ args: --follow --since 5m id3"]
    );
}

#[test]
fn docker_failure_is_reported() {
    let fx = Fixture::new();
    let out = fx
        .command(&["-1"])
        .env("FAKE_PS_FAIL", "1")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cannot connect to the Docker daemon"));
}

#[test]
fn invalid_pattern_fails() {
    let fx = Fixture::new();
    let out = fx.run(&["-1", "("]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid pattern"));
}

/// Runs dtail in watch mode, calls `during` once its initial tails have
/// printed something, lets it run `secs` more seconds, and returns its output.
fn watch(fx: &Fixture, args: &[&str], secs: u64, during: impl FnOnce(&Path)) -> String {
    let out_path = fx.dir.path().join("out.txt");
    let mut child = fx
        .command(args)
        .stdout(Stdio::from(std::fs::File::create(&out_path).unwrap()))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let read = || std::fs::read_to_string(&out_path).unwrap();
    for _ in 0..100 {
        if !read().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    during(&fx.ps_file());
    thread::sleep(Duration::from_secs(secs));
    child.kill().unwrap();
    child.wait().unwrap();
    read()
}

#[test]
fn watch_picks_up_new_containers_from_their_first_line() {
    let fx = Fixture::new();
    let out = watch(
        &fx,
        &["--interval", "1", "-g", "args", "order|newsvc"],
        2,
        |ps| {
            // Replace atomically so dtail never reads a half-written listing.
            let tmp = ps.with_extension("tmp");
            std::fs::write(&tmp, format!("{CONTAINERS}id9\tproj-newsvc-1\tnewsvc\t1\n")).unwrap();
            std::fs::rename(tmp, ps).unwrap();
        },
    );
    assert!(!out.contains("--tail 10 id9"), "{out}");
    assert!(out.contains("│ args: --follow id9\n"), "{out}");
}

#[test]
fn watch_resumes_a_restarted_container_from_where_it_stopped() {
    let fx = Fixture::new();
    // The fake `docker logs` exits immediately while `docker ps` still lists
    // the container, which looks exactly like a stop followed by a restart.
    let out = watch(&fx, &["--interval", "1", "-g", "args", "order"], 2, |_| {});
    assert!(out.contains("args: --follow --tail 10 id3"), "{out}");
    assert!(out.contains("args: --follow --since "), "{out}");
}
