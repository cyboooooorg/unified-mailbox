//! Runs the server binary to check what an operator sees when startup
//! fails: the reason on stderr and exit status 1, whatever `RUST_LOG`.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run(dir: &std::path::Path, vars: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_unified-mailbox"))
        .current_dir(dir)
        .env_clear()
        .envs(vars.iter().copied())
        .output()
        .expect("run the server")
}

#[test]
fn invalid_variables_listed() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &[("DRY_RUN", "maybe"), ("SPAM_THRESHOLD", "101")],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("DRY_RUN"), "{stderr}");
    assert!(stderr.contains("SPAM_THRESHOLD"), "{stderr}");
}

#[test]
fn fatal_error_shown_when_logs_filter_it_out() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("afile"), "").unwrap();
    let out = run(
        dir.path(),
        &[("DATA_DIR", "./afile"), ("RUST_LOG", "sqlite=debug")],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("DATA_DIR"), "{stderr}");
}

const BIN: &str = env!("CARGO_BIN_EXE_unified-mailbox");

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Raw HTTP/1.1 over a TCP stream: `(status, body)`.
fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(head.as_bytes())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::other(format!("bad response: {raw:?}")))?;
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    Ok((status, body.to_owned()))
}

/// A running server, killed when dropped so a failing test leaves none.
struct Server {
    child: Child,
    addr: SocketAddr,
}

impl Server {
    fn start(dir: &Path) -> Server {
        let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
        let child = Command::new(BIN)
            .current_dir(dir)
            .env_clear()
            .env("DATA_DIR", "./d")
            .env("LISTEN_ADDR", addr.to_string())
            .env("RUST_LOG", "off")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the server");
        let mut server = Server { child, addr };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok((200, _)) = request(addr, "GET", "/api/v1/health", &[], "") {
                return server;
            }
            if let Some(status) = server.child.try_wait().unwrap() {
                panic!("the server exited early: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "the server did not answer in 10 s"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// SIGTERM, then waits up to 10 s for the exit status.
    fn terminate(&mut self) -> std::process::ExitStatus {
        self.stop("TERM")
    }

    /// Sends `signal` (`TERM`, `INT`), then waits up to 10 s for the exit
    /// status.
    fn stop(&mut self, signal: &str) -> std::process::ExitStatus {
        let status = Command::new("kill")
            .args([&format!("-{signal}"), &self.child.id().to_string()])
            .status()
            .expect("run kill");
        assert!(status.success());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "no exit within 10 s of SIG{signal}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pair_code(dir: &Path) -> Output {
    Command::new(BIN)
        .arg("pair-code")
        .current_dir(dir)
        .env_clear()
        .env("DATA_DIR", "./d")
        .env("RUST_LOG", "off")
        .output()
        .expect("run pair-code")
}

#[test]
fn serves_until_sigterm() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::start(dir.path());
    assert!(dir.path().join("d/unified-mailbox.db").is_file());
    assert!(dir.path().join("d/secret.key").is_file());
    assert!(server.terminate().success());
}

#[test]
fn serves_until_sigint() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::start(dir.path());
    assert!(server.stop("INT").success());
}

#[test]
fn logs_go_to_stderr_not_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .arg("pair-code")
        .current_dir(dir.path())
        .env_clear()
        .env("DATA_DIR", "./d")
        .env("RUST_LOG", "info")
        .output()
        .expect("run pair-code");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("Pairing code: "), "{stdout}");
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
    assert!(
        !String::from_utf8_lossy(&out.stderr).is_empty(),
        "expected logs on stderr at RUST_LOG=info"
    );
}

#[test]
fn pair_code_while_serving() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::start(dir.path());
    let out = pair_code(dir.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = stdout.lines().next().unwrap();
    let code = line.strip_prefix("Pairing code: ").expect(line);
    let bytes = code.as_bytes();
    assert_eq!(bytes.len(), 9, "{code}");
    assert_eq!(bytes[4], b'-', "{code}");
    assert!(
        code.bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_digit() || b.is_ascii_uppercase()),
        "{code}"
    );
    assert!(stdout.contains("Valid until "), "{stdout}");

    let body = format!(r#"{{"code":"{code}","name":"test phone"}}"#);
    let (status, response) = request(
        server.addr,
        "POST",
        "/api/v1/pairing",
        &[("Content-Type", "application/json")],
        &body,
    )
    .unwrap();
    assert_eq!(status, 201, "{response}");
    assert!(server.terminate().success());
}

#[test]
fn pair_code_on_fresh_install() {
    let dir = tempfile::tempdir().unwrap();
    let out = pair_code(dir.path());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.path().join("d/unified-mailbox.db").is_file());
    assert!(dir.path().join("d/secret.key").is_file());
}

#[test]
fn port_in_use() {
    let dir = tempfile::tempdir().unwrap();
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap();
    let out = run(
        dir.path(),
        &[
            ("DATA_DIR", "./d"),
            ("LISTEN_ADDR", &addr.to_string()),
            ("RUST_LOG", "off"),
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("LISTEN_ADDR"), "{stderr}");
}

#[test]
fn unknown_argument() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .arg("serve")
        .current_dir(dir.path())
        .env_clear()
        .env("DATA_DIR", "./d")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr.contains("usage:"), "{stderr}");
    assert!(!dir.path().join("d").exists());
}
