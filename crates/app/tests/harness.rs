//! Security harness and launch behaviour, driven through the real app.
//!
//! Each test spawns the `test-harness` build of `ezmark` and reads the JSON
//! results it prints on stdout (plan section 11). A loopback TCP listener in
//! this process stands in for any remote host: fixtures reference it, and it
//! must never receive a connection beyond its own positive control. No
//! WebDriver is involved, so this runs the same on Windows and macOS.
//!
//! What this proves is deliberately narrow: each fixture's attempted resource
//! loads were blocked by an enforced CSP and reached no listener. It does not
//! prove the absence of all outbound traffic from the process.
//!
//! Run with `cargo test -p ezmark --features test-harness`.
#![cfg(feature = "test-harness")]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_ezmark");
const RESULT_PREFIX: &str = "EZMARK_HARNESS_RESULT ";
const EVENT_PREFIX: &str = "EZMARK_HARNESS_EVENT ";
const TIMEOUT: Duration = Duration::from_secs(90);

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// A loopback HTTP listener that counts connections.
struct Loopback {
    origin: String,
    connections: Arc<AtomicUsize>,
}

impl Loopback {
    /// Start listening, then prove the listener accepts connections, so that a
    /// silent listener failure cannot pass as a blocked request.
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let addr = listener.local_addr().unwrap();
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buffer = [0; 2048];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(b"HTTP/1.0 204 No Content\r\nConnection: close\r\n\r\n");
            }
        });
        let loopback = Self {
            origin: format!("http://{addr}"),
            connections,
        };
        loopback.positive_control(addr);
        loopback
    }

    fn positive_control(&self, addr: SocketAddr) {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .expect("positive control: the loopback listener must accept a connection");
        stream
            .write_all(b"GET /positive-control HTTP/1.0\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with("HTTP/1.0 204"),
            "positive control got {response:?}"
        );
        assert_eq!(
            self.connections.load(Ordering::SeqCst),
            1,
            "positive control not counted"
        );
    }

    /// Connections other than the positive control.
    fn outbound(&self) -> usize {
        self.connections.load(Ordering::SeqCst) - 1
    }

    /// A fixture with every `{{LOOPBACK}}` token pointed at this listener.
    fn instantiate(&self, fixture: &str, into: &Path) -> PathBuf {
        let text = std::fs::read_to_string(repo_file(fixture)).expect("read fixture");
        let name = Path::new(fixture).file_name().unwrap();
        let path = into.join(name);
        std::fs::write(&path, text.replace("{{LOOPBACK}}", &self.origin)).unwrap();
        path
    }
}

/// A fresh scratch directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ezmark-harness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One running harness process.
struct Harness {
    child: Child,
    lines: mpsc::Receiver<String>,
}

impl Harness {
    fn spawn(id: &str, args: &[OsString], cwd: &Path, env: &[(&str, String)]) -> Self {
        let mut command = Command::new(BIN);
        command
            .args(args)
            .current_dir(cwd)
            .env("EZMARK_HARNESS_ID", id)
            .env("EZMARK_HARNESS_TIMEOUT_SECS", "75")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("spawn harness build");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    fn next_line(&self, deadline: Instant) -> String {
        let remaining = deadline.saturating_duration_since(Instant::now());
        self.lines
            .recv_timeout(remaining)
            .expect("harness produced no output in time")
    }

    /// Wait for the next `rendered` event.
    fn wait_rendered(&self) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let line = self.next_line(deadline);
            if let Some(json) = line.strip_prefix(EVENT_PREFIX) {
                return serde_json::from_str(json).expect("event JSON");
            }
            assert!(
                !line.starts_with(RESULT_PREFIX),
                "finished before rendering: {line}"
            );
        }
    }

    /// Wait for the final result and for the process to exit.
    fn finish(mut self) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        let result = loop {
            let line = self.next_line(deadline);
            if let Some(json) = line.strip_prefix(RESULT_PREFIX) {
                break serde_json::from_str::<Value>(json).expect("result JSON");
            }
        };
        let status = self.child.wait().expect("wait for harness exit");
        assert_eq!(result["timedOut"], false, "harness timed out: {result:#}");
        assert!(status.success(), "harness exited with {status}: {result:#}");
        result
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn run(id: &str, args: &[OsString], cwd: &Path, env: &[(&str, String)]) -> Value {
    let result = Harness::spawn(id, args, cwd, env).finish();
    assert_common(&result);
    result
}

/// Checks every run makes: the page raised no errors, and the probe command,
/// which is enrolled in the ACL but granted to no capability, was rejected by
/// the real webview IPC path without its body running.
fn assert_common(result: &Value) {
    let page = &result["page"];
    assert_eq!(
        page["errors"],
        Value::Array(vec![]),
        "page errors: {result:#}"
    );
    assert_eq!(
        page["aclProbe"]["rejected"], true,
        "ACL let the probe through: {result:#}"
    );
    let error = page["aclProbe"]["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("not allowed"),
        "unexpected ACL error: {error}"
    );
    assert_eq!(
        result["deniedProbeExecuted"], false,
        "probe body ran: {result:#}"
    );
}

fn last_render(result: &Value) -> &Value {
    result["page"]["renders"]
        .as_array()
        .and_then(|renders| renders.last())
        .expect("at least one render")
}

#[test]
fn sanitized_hostile_fixture_runs_clean() {
    let loopback = Loopback::start();
    let dir = scratch("sanitized");
    let fixture = loopback.instantiate("tests/security/sanitized/hostile.md", &dir);

    let result = run("sanitized", &[fixture.into()], &dir, &[]);

    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    let dom = &last_render(&result)["dom"];
    assert_eq!(dom["scripts"], 0, "{dom:#}");
    assert_eq!(dom["inlineHandlers"], 0, "{dom:#}");
    assert_eq!(dom["scriptUrls"], 0, "{dom:#}");
    assert_eq!(dom["frames"], 0, "{dom:#}");
    let html = dom["html"].as_str().unwrap();
    assert!(
        !html.contains(&loopback.origin),
        "a loopback URL survived: {html}"
    );
    assert_eq!(
        result["page"]["cspReports"],
        Value::Array(vec![]),
        "{result:#}"
    );
    assert_eq!(
        loopback.outbound(),
        0,
        "outbound connections reached the listener"
    );
}

#[test]
fn enforcement_fixture_is_blocked_by_the_enforced_csp() {
    let loopback = Loopback::start();
    let dir = scratch("enforcement");
    let fixture = loopback.instantiate("tests/security/enforcement/remote-img.html", &dir);
    let doc = dir.join("host.md");
    std::fs::write(&doc, "# Enforcement host\n").unwrap();

    let result = run(
        "enforcement",
        &[doc.into()],
        &dir,
        &[("EZMARK_HARNESS_INJECT", fixture.display().to_string())],
    );

    let injected = &result["page"]["injected"];
    assert_eq!(
        injected["images"].as_array().map(Vec::len),
        Some(1),
        "{result:#}"
    );
    let blocked_url = format!("{}/enforcement-pixel.png", loopback.origin);
    let reports = result["page"]["cspReports"]
        .as_array()
        .expect("csp reports");
    // Both WebView2 and WKWebView report the full blocked URL (CI, 2026-10-27).
    let report = reports
        .iter()
        .find(|r| r["blockedURI"] == blocked_url.as_str())
        .unwrap_or_else(|| panic!("no CSP report naming {blocked_url}: {result:#}"));
    assert_eq!(report["disposition"], "enforce", "{report:#}");
    assert_eq!(report["effectiveDirective"], "img-src", "{report:#}");
    assert_eq!(
        loopback.outbound(),
        0,
        "the blocked image still reached the listener"
    );
    eprintln!("enforced CSP report: {report}");
}

#[test]
fn relative_image_is_served_through_the_asset_protocol() {
    let dir = scratch("asset");
    let docs = dir.join("dir with spaces").join("ünïcödé docs");
    std::fs::create_dir_all(docs.join("img")).unwrap();
    std::fs::copy(
        repo_file("tests/security/assets/pixel-3x2.png"),
        docs.join("img").join("pixel 3x2.png"),
    )
    .unwrap();
    std::fs::write(dir.join("outside.png"), b"not served").unwrap();
    // Raw HTML too, so both image paths through the sanitizer are covered.
    std::fs::write(
        docs.join("images.md"),
        "# Images\n\n![pixel](img/pixel%203x2.png)\n\n\
         <img src=\"img/pixel 3x2.png\" alt=\"raw\">\n\n![outside](../../outside.png)\n",
    )
    .unwrap();

    let result = run(
        "asset",
        &[docs.join("images.md").into()],
        &dir,
        &[("EZMARK_HARNESS_PROBE_ASSET", "../../outside.png".to_owned())],
    );

    let images = last_render(&result)["dom"]["images"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(images.len(), 3, "{result:#}");
    for image in &images[..2] {
        assert_eq!(image["naturalWidth"], 3, "image did not load: {image:#}");
        assert_eq!(image["naturalHeight"], 2, "{image:#}");
        let src = image["src"].as_str().unwrap();
        assert!(src.contains("ezasset"), "not an asset URL: {src}");
    }
    assert_eq!(
        images[2]["src"],
        Value::Null,
        "out-of-root image kept a src: {result:#}"
    );

    let probes = result["page"]["assetProbes"].as_array().unwrap();
    let probe = |kind: &str| probes.iter().find(|p| p["kind"] == kind).cloned().unwrap();
    assert_eq!(probe("current")["loaded"], true, "{probes:#?}");
    assert_eq!(
        probe("stale")["loaded"],
        false,
        "stale generation was served"
    );
    assert_eq!(
        probe("crafted")["loaded"],
        false,
        "handler served a file outside the root"
    );
    assert_eq!(
        result["page"]["cspReports"],
        Value::Array(vec![]),
        "{result:#}"
    );
}

#[test]
fn no_argument_shows_the_empty_state() {
    let dir = scratch("empty");
    let result = run(
        "empty",
        &[],
        &dir,
        &[("EZMARK_HARNESS_RENDERS", "0".to_owned())],
    );
    assert_eq!(result["view"]["kind"], "empty", "{result:#}");
    assert_eq!(last_render(&result)["kind"], "empty");
}

#[test]
fn missing_file_shows_a_clear_error() {
    let dir = scratch("missing");
    let result = run("missing", &["no such file.md".into()], &dir, &[]);
    assert_eq!(result["view"]["kind"], "error", "{result:#}");
    let message = result["view"]["message"].as_str().unwrap();
    assert!(message.contains("File not found"), "{message}");
    assert!(message.contains("no such file.md"), "{message}");
    let html = last_render(&result)["dom"]["html"].as_str().unwrap();
    assert!(html.contains("no such file.md"), "error not shown: {html}");
}

#[test]
fn relative_unicode_path_resolves_against_the_working_directory() {
    let dir = scratch("relative");
    let sub = dir.join("sub dir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("naïve ✓ notes.md"), "# Ünïcödé heading\n").unwrap();

    let relative = Path::new("sub dir").join("naïve ✓ notes.md");
    let result = run("relative", &[relative.into()], &dir, &[]);

    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    assert_eq!(result["view"]["title"], "naïve ✓ notes.md");
    let html = last_render(&result)["dom"]["html"].as_str().unwrap();
    assert!(html.contains("Ünïcödé heading"), "{html}");
    assert_eq!(result["opens"][0]["origin"], "argv");
    assert_eq!(
        result["opens"][0]["buffered"], true,
        "argv opens wait for the frontend"
    );
}

#[test]
fn second_launch_is_forwarded_to_the_running_instance() {
    let dir = scratch("single");
    let first = dir.join("first.md");
    std::fs::write(&first, "# First\n").unwrap();
    let second_dir = dir.join("second dir");
    std::fs::create_dir_all(&second_dir).unwrap();
    std::fs::write(second_dir.join("second.md"), "# Second\n").unwrap();

    let id = "single-instance";
    let renders = [("EZMARK_HARNESS_RENDERS", "2".to_owned())];
    let running = Harness::spawn(id, &[first.into()], &dir, &renders);
    assert_eq!(running.wait_rendered()["data"]["kind"], "document");

    // Same identifier, relative path, different working directory.
    let status = Command::new(BIN)
        .arg("second.md")
        .current_dir(&second_dir)
        .env("EZMARK_HARNESS_ID", id)
        .stdout(Stdio::null())
        .status()
        .expect("second launch");
    assert!(status.success(), "second launch exited with {status}");

    let result = running.finish();
    assert_common(&result);
    assert_eq!(result["view"]["title"], "second.md", "{result:#}");
    let forwarded = &result["opens"][1];
    assert_eq!(forwarded["origin"], "second-instance", "{result:#}");
    let path = forwarded["path"].as_str().unwrap();
    assert!(
        path.contains("second dir"),
        "cwd of the second launch not used: {path}"
    );
}

/// macOS cold open through Launch Services, the path Finder double-clicks and
/// `open` share. The file arrives as `RunEvent::Opened`, before the webview is
/// ready, so it must be buffered and replayed. Needs the bundled harness app:
///
/// ```sh
/// cargo tauri build --debug --bundles app --features test-harness
/// EZMARK_HARNESS_APP=$PWD/target/debug/bundle/macos/ezmark.app \
///   cargo test -p ezmark --features test-harness --test harness -- --ignored cold_open
/// ```
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs the bundled harness app; CI runs it on macOS"]
fn cold_open_through_launch_services_is_buffered_and_replayed() {
    let app = std::env::var("EZMARK_HARNESS_APP").expect("set EZMARK_HARNESS_APP to the .app");
    let dir = scratch("cold-open");
    let doc = dir.join("opened by launch services.md");
    std::fs::write(&doc, "# Opened cold\n").unwrap();
    let out = dir.join("result.json");

    // Cold start: no instance of the app may be running.
    let _ = Command::new("pkill").args(["-x", "ezmark"]).status();
    std::thread::sleep(Duration::from_secs(1));

    let status = Command::new("open")
        .arg("-a")
        .arg(&app)
        .arg("--env")
        .arg(format!("EZMARK_HARNESS_OUT={}", out.display()))
        .arg("--env")
        .arg("EZMARK_HARNESS_ID=cold-open")
        .arg(&doc)
        .status()
        .expect("run open");
    assert!(status.success(), "open exited with {status}");

    let deadline = Instant::now() + TIMEOUT;
    let text = loop {
        if let Ok(text) = std::fs::read_to_string(&out) {
            break text;
        }
        assert!(Instant::now() < deadline, "no result from the bundled app");
        std::thread::sleep(Duration::from_millis(250));
    };
    let result: Value = serde_json::from_str(&text).expect("result JSON");
    eprintln!("cold open result: {result}");
    assert_eq!(result["timedOut"], false, "{result:#}");
    assert_common(&result);
    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    assert_eq!(result["view"]["title"], "opened by launch services.md");
    let open = &result["opens"][0];
    assert_eq!(open["origin"], "opened-event", "{result:#}");
    assert_eq!(
        open["buffered"], true,
        "the open event arrived after readiness: {result:#}"
    );
}
