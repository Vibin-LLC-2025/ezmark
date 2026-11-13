//! Security harness and launch behaviour, driven through the real app.
//!
//! Each test spawns the `test-harness` build of `ezmark` and reads the JSON
//! results it prints on stdout (plan section 11). A loopback TCP listener in
//! this process stands in for any remote host: fixtures reference it, and it
//! must never receive a connection beyond its own positive control. No
//! WebDriver is involved, so this runs the same on Windows and macOS.
//!
//! What this proves is deliberately narrow: each fixture's attempted resource
//! loads and navigations were stopped (by the sanitizer, the enforced CSP, the
//! click handler, or the Rust navigation guard) and reached no listener. It
//! does not prove the absence of all outbound traffic from the process.
//!
//! Run with `cargo test -p ezmark --features test-harness`.
#![cfg(feature = "test-harness")]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_ezmark");
const RESULT_PREFIX: &str = "EZMARK_HARNESS_RESULT ";
const EVENT_PREFIX: &str = "EZMARK_HARNESS_EVENT ";
const TIMEOUT: Duration = Duration::from_secs(90);

/// Start the webview without the network lock, so that a zero-connection
/// result is the CSP's doing and nothing else's. The lock is Windows-only;
/// elsewhere this changes nothing.
fn csp_alone() -> (&'static str, String) {
    ("EZMARK_HARNESS_NETWORK_LOCK", "off".to_owned())
}

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// A loopback HTTP listener that records every request it receives.
struct Loopback {
    origin: String,
    authority: String,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Loopback {
    /// Start listening, then prove the listener accepts connections, so that a
    /// silent listener failure cannot pass as a blocked request.
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let addr = listener.local_addr().unwrap();
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (counter, log) = (Arc::clone(&connections), Arc::clone(&requests));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buffer = [0; 2048];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]);
                let first_line = request.lines().next().unwrap_or("(no request line)");
                log.lock().unwrap().push(first_line.to_owned());
                let _ = stream.write_all(b"HTTP/1.0 204 No Content\r\nConnection: close\r\n\r\n");
            }
        });
        let loopback = Self {
            origin: format!("http://{addr}"),
            authority: addr.to_string(),
            connections,
            requests,
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

    /// Fail, naming each request, if anything but the positive control arrived.
    fn assert_untouched(&self) {
        let beyond_control = self.beyond_control();
        // A copy, so that a failure here does not poison the listener's lock.
        let requests = self.requests.lock().unwrap().clone();
        assert_eq!(
            beyond_control, 0,
            "outbound connections reached the listener: {requests:#?}"
        );
    }

    /// Connections other than the positive control.
    fn beyond_control(&self) -> usize {
        self.connections.load(Ordering::SeqCst) - 1
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }

    /// A fixture with every `{{LOOPBACK}}` token pointed at this listener.
    fn instantiate(&self, fixture: &str, into: &Path) -> PathBuf {
        let text = std::fs::read_to_string(repo_file(fixture)).expect("read fixture");
        let name = Path::new(fixture).file_name().unwrap();
        let path = into.join(name);
        let text = text
            .replace("{{LOOPBACK}}", &self.origin)
            .replace("{{LOOPBACK_AUTHORITY}}", &self.authority);
        std::fs::write(&path, text).unwrap();
        path
    }
}

/// A fresh scratch directory for one test, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ezmark-harness-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One running harness process.
struct Harness {
    id: String,
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
        Self {
            id: id.to_owned(),
            child,
            lines,
        }
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
        let _ = self.child.wait();
        remove_webview_data(&self.id);
    }
}

/// The harness build keeps its webview profile under the temp directory, one
/// folder per id. The webview's helper processes let go of it a moment after
/// the app exits, so removal is retried briefly and is best effort.
fn remove_webview_data(id: &str) {
    let dir = std::env::temp_dir().join("ezmark-harness").join(id);
    for _ in 0..20 {
        if !dir.exists() || std::fs::remove_dir_all(&dir).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn run(id: &str, args: &[OsString], cwd: &Path, env: &[(&str, String)]) -> Value {
    let result = Harness::spawn(id, args, cwd, env).finish();
    assert_common(&result);
    result
}

/// Checks every run makes: the page raised no errors, it is still the app
/// page, no injected script ran, and the probe command, which is enrolled in
/// the ACL but granted to no capability, was rejected by the real webview IPC
/// path without its body running.
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
    assert_eq!(
        page["payloadRan"],
        Value::Null,
        "a payload's script ran: {result:#}"
    );
    let started = page["location"]["startedAt"].as_str().unwrap_or_default();
    let now = page["location"]["now"].as_str().unwrap_or_default();
    assert!(!started.is_empty(), "no location reported: {result:#}");
    assert_eq!(
        now.split('#').next(),
        started.split('#').next(),
        "the page navigated away: {result:#}"
    );
    for navigation in result["navigations"].as_array().expect("navigations") {
        let url = navigation["url"].as_str().unwrap_or_default();
        assert_eq!(
            navigation["allowed"],
            origin_of(url) == origin_of(started),
            "the guard and the app origin disagree: {result:#}"
        );
    }
}

/// `scheme://host[:port]` of an absolute URL.
fn origin_of(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |at| at + 3);
    let end = url[after_scheme..]
        .find(['/', '#', '?'])
        .map_or(url.len(), |at| after_scheme + at);
    &url[..end]
}

fn last_render(result: &Value) -> &Value {
    result["page"]["renders"]
        .as_array()
        .and_then(|renders| renders.last())
        .expect("at least one render")
}

fn csp_reports(result: &Value) -> &Vec<Value> {
    result["page"]["cspReports"]
        .as_array()
        .expect("csp reports")
}

/// The enforced report whose `blockedURI` starts with `url`.
fn enforced_report<'a>(result: &'a Value, url: &str) -> &'a Value {
    let report = csp_reports(result)
        .iter()
        .find(|report| {
            report["blockedURI"]
                .as_str()
                .is_some_and(|blocked| blocked.starts_with(url))
        })
        .unwrap_or_else(|| panic!("no CSP report naming {url}: {result:#}"));
    assert_eq!(report["disposition"], "enforce", "{report:#}");
    report
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item.as_str().expect("a string"))
        .collect()
}

#[test]
fn sanitized_hostile_fixture_runs_clean() {
    let loopback = Loopback::start();
    let dir = Scratch::new("sanitized");
    let fixture = loopback.instantiate("tests/security/sanitized/hostile.md", &dir);

    let result = run("sanitized", &[fixture.into()], &dir, &[]);

    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    let dom = &last_render(&result)["dom"];
    assert_eq!(dom["scripts"], 0, "{dom:#}");
    assert_eq!(dom["inlineHandlers"], 0, "{dom:#}");
    assert_eq!(dom["scriptUrls"], 0, "{dom:#}");
    assert_eq!(dom["frames"], 0, "{dom:#}");
    // The only attribute left that names a URL is a link's own target.
    assert_eq!(dom["urlAttributes"], Value::Array(vec![]), "{dom:#}");
    let html = dom["html"].as_str().unwrap();
    for kept in [
        format!("href=\"{}\"", loopback.url("/http-link")),
        format!("href=\"{}\"", loopback.url("/kept-http-link")),
        "type=\"checkbox\"".to_owned(),
        "id=\"user-content-content\"".to_owned(),
        "name=\"user-content-status\"".to_owned(),
    ] {
        assert!(html.contains(&kept), "no {kept} in {html}");
    }
    for removed in [
        "protocol-relative-link",
        "backslash-link",
        "padded-link",
        "id=\"content\"",
        "name=\"status\"",
        "document-notice",
        "<link",
        "cite=",
        "typed",
    ] {
        assert!(!html.contains(removed), "{removed} in {html}");
    }
    // Every image the document asked for is now a line of text saying why.
    assert_eq!(dom["images"], Value::Array(vec![]), "{dom:#}");
    let placeholders = dom["placeholders"].as_array().unwrap();
    let with_reason = |reason: &str| {
        placeholders
            .iter()
            .filter(|placeholder| placeholder["reason"] == reason)
            .count()
    };
    assert_eq!(with_reason("remote"), 5, "{placeholders:#?}");
    assert_eq!(with_reason("missing"), 1, "{placeholders:#?}");
    assert_eq!(with_reason("unsupported"), 1, "{placeholders:#?}");
    assert_eq!(placeholders.len(), 7, "{placeholders:#?}");
    assert!(
        placeholders[0]["text"]
            .as_str()
            .unwrap()
            .contains(&loopback.url("/md-image.png")),
        "the placeholder does not show the URL: {placeholders:#?}"
    );

    assert_eq!(csp_reports(&result), &Vec::<Value>::new(), "{result:#}");
    loopback.assert_untouched();
}

#[test]
fn enforcement_fixture_is_blocked_by_the_enforced_csp() {
    let loopback = Loopback::start();
    let dir = Scratch::new("enforcement");
    let fixture = loopback.instantiate("tests/security/enforcement/remote-img.html", &dir);
    let doc = dir.join("host.md");
    std::fs::write(&doc, "# Enforcement host\n").unwrap();
    let blocked_url = loopback.url("/enforcement-pixel.png");

    let result = run(
        "enforcement",
        &[doc.into()],
        &dir,
        &[
            ("EZMARK_HARNESS_INJECT", fixture.display().to_string()),
            ("EZMARK_HARNESS_EXPECT_BLOCKED", blocked_url.clone()),
            csp_alone(),
        ],
    );

    let injected = &result["page"]["injected"];
    assert_eq!(
        injected["images"].as_array().map(Vec::len),
        Some(1),
        "{result:#}"
    );
    // Both WebView2 and WKWebView report the full blocked URL (CI, 2026-10-27).
    let report = enforced_report(&result, &blocked_url);
    assert_eq!(report["blockedURI"], blocked_url.as_str(), "{report:#}");
    assert_eq!(report["effectiveDirective"], "img-src", "{report:#}");
    loopback.assert_untouched();
    eprintln!("enforced CSP report: {report}");
}

/// Every resource-loading element and every script-driven request, past the
/// sanitizer and without the network lock, against the CSP alone.
#[test]
fn enforcement_corpus_reaches_no_listener() {
    let loopback = Loopback::start();
    let dir = Scratch::new("resources");
    let fixture = loopback.instantiate("tests/security/enforcement/resources.html", &dir);
    let doc = dir.join("host.md");
    std::fs::write(&doc, "# Enforcement host\n").unwrap();
    // Loads that both engines report by URL. The rest of the fixture is held
    // to the stricter and engine-independent standard: no connection.
    let expected = [
        "/img.png",
        "/stylesheet.css",
        "/fetch",
        "/script-element.js",
    ]
    .map(|path| loopback.url(path));

    let result = run(
        "resources",
        &[doc.into()],
        &dir,
        &[
            ("EZMARK_HARNESS_INJECT", fixture.display().to_string()),
            ("EZMARK_HARNESS_CONNECT_PROBE", loopback.origin.clone()),
            ("EZMARK_HARNESS_EXPECT_BLOCKED", expected.join(" ")),
            csp_alone(),
        ],
    );

    for (url, directive) in expected.iter().zip([
        "img-src",
        "style-src-elem",
        "connect-src",
        "script-src-elem",
    ]) {
        let report = enforced_report(&result, url);
        assert_eq!(report["effectiveDirective"], directive, "{report:#}");
    }
    // Inline script and inline style are reported as "inline", not by URL.
    for (blocked, directive) in [
        ("inline", "script-src-elem"),
        ("inline", "script-src-attr"),
        ("inline", "style-src-elem"),
        ("inline", "style-src-attr"),
    ] {
        assert!(
            csp_reports(&result).iter().any(|report| {
                report["blockedURI"]
                    .as_str()
                    .is_some_and(|uri| uri.starts_with(blocked))
                    && report["effectiveDirective"] == directive
                    && report["disposition"] == "enforce"
            }),
            "no enforced {directive} report for {blocked}: {result:#}"
        );
    }
    let probes = &result["page"]["connectProbes"];
    for refused in [
        "fetch",
        "xhr",
        "websocket",
        "eventsource",
        "script-element",
        "eval",
    ] {
        let outcome = probes[refused].as_str().unwrap_or_default();
        assert!(outcome.starts_with("refused"), "{refused}: {outcome}");
    }
    loopback.assert_untouched();
    let summary: Vec<String> = csp_reports(&result)
        .iter()
        .map(|report| {
            let blocked = report["blockedURI"].as_str().unwrap_or_default();
            let directive = report["effectiveDirective"].as_str().unwrap_or_default();
            format!("{directive} {}", blocked.replace(&loopback.origin, ""))
        })
        .collect();
    eprintln!("enforcement corpus CSP reports: {summary:?}");
    eprintln!("enforcement corpus connect probes: {probes}");
}

/// Connection hints and a frame, past the sanitizer. The webview connects for
/// these on its own account, whatever the CSP says (see the fixture), so this
/// is the network lock's test.
#[test]
fn speculative_connections_reach_no_listener() {
    let loopback = Loopback::start();
    let dir = Scratch::new("speculative");
    let fixture = loopback.instantiate("tests/security/enforcement/speculative.html", &dir);
    let doc = dir.join("host.md");
    std::fs::write(&doc, "# Speculative host\n").unwrap();

    let result = run(
        "speculative",
        &[doc.into()],
        &dir,
        &[
            ("EZMARK_HARNESS_INJECT", fixture.display().to_string()),
            // A blocked frame is reported by origin only.
            ("EZMARK_HARNESS_EXPECT_BLOCKED", loopback.origin.clone()),
        ],
    );

    let injected = result["page"]["injected"]["html"].as_str().unwrap();
    assert_eq!(injected.matches("<link").count(), 4, "{injected}");
    let frame = enforced_report(&result, &loopback.origin);
    assert_eq!(frame["effectiveDirective"], "frame-src", "{frame:#}");
    // The lock is WebView2's. WKWebView has none, and opens two connections
    // to this listener on the macos-15 runner, so there the count is
    // reported, in the log and in docs/measurements.md, and not required.
    if cfg!(windows) {
        loopback.assert_untouched();
    } else {
        eprintln!(
            "speculative connections that reached the listener: {}",
            loopback.beyond_control()
        );
    }
}

/// Every way of leaving the app page, past the sanitizer: the click handler,
/// the CSP's `form-action`, the webview's refusal to open windows, and the
/// Rust navigation guard each have to hold. The guard keeps the page where it
/// is, but WebView2 has sent the request by the time it asks, so reaching no
/// listener is again down to the network lock.
#[test]
fn navigation_attempts_never_leave_the_app_page() {
    let loopback = Loopback::start();
    let dir = Scratch::new("navigation");
    let fixture = loopback.instantiate("tests/security/enforcement/navigation.html", &dir);
    let doc = dir.join("host.md");
    std::fs::write(&doc, "# Navigation host\n").unwrap();

    let result = run(
        "navigation",
        &[doc.into()],
        &dir,
        &[
            ("EZMARK_HARNESS_INJECT", fixture.display().to_string()),
            ("EZMARK_HARNESS_NAVIGATION_PROBE", loopback.origin.clone()),
            (
                "EZMARK_HARNESS_EXPECT_DENIED_NAVIGATIONS",
                loopback.url("/location"),
            ),
            ("EZMARK_HARNESS_EXPECT_BLOCKED", loopback.url("/form")),
        ],
    );

    let probes = &result["page"]["navigationProbes"];
    for cancelled in [
        "linkClick",
        "linkMiddleClick",
        "linkContextMenu",
        "newWindowClick",
        "imageLinkClick",
    ] {
        assert_eq!(
            probes[cancelled], true,
            "{cancelled} went through: {probes:#}"
        );
    }
    // In-page links and the menu on plain text are left to the webview.
    assert_eq!(probes["fragmentClick"], false, "{probes:#}");
    assert_eq!(probes["textContextMenu"], false, "{probes:#}");
    assert_eq!(probes["formSubmit"], "clicked", "{probes:#}");
    let form = enforced_report(&result, &loopback.url("/form"));
    assert_eq!(form["effectiveDirective"], "form-action", "{form:#}");

    // The script-driven navigation reached the Rust guard and was denied.
    // `assert_common` has already checked that nothing else was allowed.
    let denied: Vec<&str> = result["navigations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|navigation| navigation["allowed"] == false)
        .map(|navigation| navigation["url"].as_str().unwrap())
        .collect();
    assert!(
        denied.contains(&loopback.url("/location").as_str()),
        "the guard never saw the location change: {result:#}"
    );
    loopback.assert_untouched();
    eprintln!(
        "navigation probes: {probes}, denied by the guard: {denied:?}, window.open: {}",
        probes["windowOpen"]
    );
}

#[test]
fn relative_image_is_served_through_the_asset_protocol() {
    let dir = Scratch::new("asset");
    let docs = dir.join("dir with spaces").join("ünïcödé docs");
    std::fs::create_dir_all(docs.join("img")).unwrap();
    for name in ["pixel 3x2.png", "100%.png"] {
        std::fs::copy(
            repo_file("tests/security/assets/pixel-3x2.png"),
            docs.join("img").join(name),
        )
        .unwrap();
    }
    // A valid PNG, identical to the one that loads inside the root, so a
    // refused probe cannot be a decode failure of bytes that were served.
    std::fs::copy(
        repo_file("tests/security/assets/pixel-3x2.png"),
        dir.join("outside.png"),
    )
    .unwrap();
    // Raw HTML too, so both image paths through the sanitizer are covered.
    std::fs::write(
        docs.join("images.md"),
        "# Images\n\n![pixel](img/pixel%203x2.png)\n\n\
         <img src=\"img/pixel 3x2.png\" alt=\"raw\">\n\n<img src=\"img/100%.png\" alt=\"percent\">\n\n\
         ![outside](../../outside.png)\n\n![gone](img/gone.png)\n",
    )
    .unwrap();

    let result = run(
        "asset",
        &[docs.join("images.md").into()],
        &dir,
        &[("EZMARK_HARNESS_PROBE_ASSET", "../../outside.png".to_owned())],
    );

    let dom = &last_render(&result)["dom"];
    let images = dom["images"].as_array().unwrap();
    assert_eq!(images.len(), 3, "{result:#}");
    for image in images {
        assert_eq!(image["naturalWidth"], 3, "image did not load: {image:#}");
        assert_eq!(image["naturalHeight"], 2, "{image:#}");
        let src = image["src"].as_str().unwrap();
        assert!(src.contains("ezasset"), "not an asset URL: {src}");
    }
    // The two that may not load say why, and name the reference.
    let placeholders = dom["placeholders"].as_array().unwrap();
    assert_eq!(placeholders.len(), 2, "{placeholders:#?}");
    assert_eq!(placeholders[0]["reason"], "outside", "{placeholders:#?}");
    assert!(
        placeholders[0]["text"]
            .as_str()
            .unwrap()
            .contains("../../outside.png")
    );
    assert_eq!(placeholders[1]["reason"], "missing", "{placeholders:#?}");

    let probes = result["page"]["assetProbes"].as_array().unwrap();
    let probe = |kind: &str| {
        let probe = probes.iter().find(|p| p["kind"] == kind).cloned().unwrap();
        // A probe that timed out was neither served nor refused; it must not
        // pass as a refusal.
        assert_eq!(probe["timedOut"], Value::Null, "{kind} probe timed out");
        probe
    };
    assert_eq!(probe("current")["loaded"], true, "{probes:#?}");
    assert_eq!(
        probe("stale")["loaded"],
        false,
        "a previous asset scope was served"
    );
    assert_eq!(
        probe("crafted")["loaded"],
        false,
        "handler served a file outside the root"
    );
    assert_eq!(csp_reports(&result), &Vec::<Value>::new(), "{result:#}");
}

#[test]
fn no_argument_shows_the_empty_state() {
    let dir = Scratch::new("empty");
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
    let dir = Scratch::new("missing");
    let result = run("missing", &["no such file.md".into()], &dir, &[]);
    assert_eq!(result["view"]["kind"], "error", "{result:#}");
    let message = result["view"]["message"].as_str().unwrap();
    assert!(message.contains("File not found"), "{message}");
    assert!(message.contains("no such file.md"), "{message}");
    let html = last_render(&result)["dom"]["html"].as_str().unwrap();
    assert!(html.contains("no such file.md"), "error not shown: {html}");
    let renders = result["page"]["renders"].as_array().unwrap();
    assert_eq!(
        renders.len(),
        1,
        "the drop-a-file screen flashed before the result: {result:#}"
    );
}

#[test]
fn relative_unicode_path_resolves_against_the_working_directory() {
    let dir = Scratch::new("relative");
    let sub = dir.join("sub dir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("naïve ✓ notes.md"), "# Ünïcödé heading\n").unwrap();

    let relative = Path::new("sub dir").join("naïve ✓ notes.md");
    let result = run("relative", &[relative.into()], &dir, &[]);

    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    assert_eq!(result["view"]["title"], "naïve ✓ notes.md");
    assert_eq!(result["windowTitle"], "naïve ✓ notes.md - ezmark");
    let html = last_render(&result)["dom"]["html"].as_str().unwrap();
    assert!(html.contains("Ünïcödé heading"), "{html}");
    assert_eq!(result["opens"][0]["origin"], "argv");
    assert_eq!(
        result["opens"][0]["buffered"], true,
        "argv opens wait for the frontend"
    );
}

/// The files that used to open as garbage or as nothing: UTF-16 as Windows
/// PowerShell writes it, bytes that are no text at all, and nesting deep
/// enough to stall the sanitizer.
#[test]
fn awkward_files_open_and_say_what_was_done() {
    let dir = Scratch::new("awkward");
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(
        "# Wïde heading\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    std::fs::write(dir.join("utf16.md"), utf16).unwrap();
    std::fs::write(dir.join("latin1.md"), b"# Caf\xE9\n").unwrap();
    std::fs::write(
        dir.join("deep.md"),
        format!("{}<b>deep</b>\n", ">".repeat(600)),
    )
    .unwrap();

    let opened = |name: &str| {
        let result = run(&format!("awkward-{name}"), &[name.into()], &dir, &[]);
        assert_eq!(result["view"]["kind"], "document", "{result:#}");
        last_render(&result)["dom"].clone()
    };

    let utf16 = opened("utf16.md");
    assert!(utf16["html"].as_str().unwrap().contains("Wïde heading"));
    assert_eq!(utf16["notices"], Value::Array(vec![]), "{utf16:#}");

    let latin1 = opened("latin1.md");
    assert!(latin1["html"].as_str().unwrap().contains("Caf\u{FFFD}"));
    let notices = strings(&latin1["notices"]);
    assert_eq!(notices.len(), 1, "{latin1:#}");
    assert!(notices[0].contains("not valid UTF-8"), "{notices:?}");

    let deep = opened("deep.md");
    let notices = strings(&deep["notices"]);
    assert_eq!(notices.len(), 1, "{deep:#}");
    assert!(notices[0].contains("plain text"), "{notices:?}");
    let html = deep["html"].as_str().unwrap();
    assert!(
        html.contains("&lt;b&gt;deep&lt;/b&gt;"),
        "markup not escaped"
    );
}

#[test]
fn second_launch_is_forwarded_to_the_running_instance() {
    let dir = Scratch::new("single");
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
    assert_eq!(result["windowTitle"], "second.md - ezmark", "{result:#}");
    let forwarded = &result["opens"][1];
    assert_eq!(forwarded["origin"], "second-instance", "{result:#}");
    let path = forwarded["path"].as_str().unwrap();
    assert!(
        path.contains("second dir"),
        "cwd of the second launch not used: {path}"
    );
}

/// `--help`, `--version`, and arguments that make no sense are answered on
/// the console and never open a window.
#[test]
fn command_line_options_answer_without_opening_a_window() {
    let run = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .env("EZMARK_HARNESS_ID", "cli")
            .env("EZMARK_HARNESS_TIMEOUT_SECS", "20")
            .stdin(Stdio::null())
            .output()
            .expect("run ezmark")
    };
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

    let version = run(&["--version"]);
    assert!(version.status.success());
    assert_eq!(
        text(&version.stdout).trim(),
        format!("ezmark {}", env!("CARGO_PKG_VERSION"))
    );

    let help = run(&["-h"]);
    assert!(help.status.success());
    assert!(text(&help.stdout).contains("Usage: ezmark [FILE]"));

    for (args, problem) in [
        (&["--no-such-option"][..], "unknown option --no-such-option"),
        (&["a.md", "b.md"][..], "one file at a time"),
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains(problem), "{args:?}: {stderr}");
        assert!(
            stderr.contains("Usage: ezmark [FILE]"),
            "{args:?}: {stderr}"
        );
        assert!(output.stdout.is_empty(), "{args:?} printed a result");
    }
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
    let dir = Scratch::new("cold-open");
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
    remove_webview_data("cold-open");
}

/// The same cold open, asked of Finder itself: `tell application "Finder" to
/// open <file> using <app>` is what a double-click on a file associated with
/// ezmark does, without the mouse. Finder passes no environment, so the app
/// runs with harness defaults and its result is read from the fallback file.
/// The first run asks for permission to control Finder (System Settings,
/// Privacy and Security, Automation).
///
/// ```sh
/// cargo tauri build --debug --bundles app --features test-harness
/// EZMARK_HARNESS_APP=$PWD/target/debug/bundle/macos/ezmark.app \
///   cargo test -p ezmark --features test-harness --test harness -- --ignored finder_open
/// ```
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs the bundled harness app and permission to control Finder"]
fn finder_open_is_buffered_and_replayed() {
    let app = std::env::var("EZMARK_HARNESS_APP").expect("set EZMARK_HARNESS_APP to the .app");
    let dir = Scratch::new("finder-open");
    let doc = dir.join("opened by finder.md");
    std::fs::write(&doc, "# Opened by Finder\n").unwrap();
    let out = std::env::temp_dir().join("ezmark-harness-last-result.json");
    let _ = std::fs::remove_file(&out);

    // Cold start: no instance of the app may be running.
    let _ = Command::new("pkill").args(["-x", "ezmark"]).status();
    std::thread::sleep(Duration::from_secs(1));

    let script = format!(
        "tell application \"Finder\" to open (POSIX file {doc:?} as alias) \
         using (POSIX file {app:?} as alias)",
        doc = doc.display().to_string(),
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .expect("run osascript");
    assert!(
        output.status.success(),
        "Finder did not open the file: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let deadline = Instant::now() + TIMEOUT;
    let text = loop {
        if let Ok(text) = std::fs::read_to_string(&out) {
            break text;
        }
        assert!(Instant::now() < deadline, "no result from the bundled app");
        std::thread::sleep(Duration::from_millis(250));
    };
    let result: Value = serde_json::from_str(&text).expect("result JSON");
    eprintln!("finder open result: {result}");
    assert_eq!(result["timedOut"], false, "{result:#}");
    assert_common(&result);
    assert_eq!(result["view"]["kind"], "document", "{result:#}");
    assert_eq!(result["view"]["title"], "opened by finder.md");
    let open = &result["opens"][0];
    assert_eq!(open["origin"], "opened-event", "{result:#}");
    assert_eq!(
        open["buffered"], true,
        "the open event arrived after readiness: {result:#}"
    );
    let _ = std::fs::remove_file(&out);
    remove_webview_data("default");
}
