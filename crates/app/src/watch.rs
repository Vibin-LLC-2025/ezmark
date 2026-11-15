//! Watching the open file for changes (plan section 9, "Watcher").
//!
//! The file's parent directory is watched, not the file: editors save by
//! writing a temporary file and renaming it over the original, and a watch on
//! the file itself would follow the old one into oblivion. Events that
//! concern the file's name are debounced, and when things go quiet the shell
//! is told to look at the file again. Whether anything is rendered is decided
//! there, by comparing bytes (`documents.rs`), so a save that changes nothing
//! and an event that meant nothing both cost one read and no render.
//!
//! A file that is missing when the quiet period ends is not reported at
//! once. A save by delete-and-rename leaves exactly that gap, so the report
//! waits out a grace period first.
//!
//! Events are not the whole story. A folder that is renamed or deleted takes
//! its watch with it and says nothing, and a file that an editor still holds
//! open cannot be read when its event arrives. So the watcher also looks for
//! itself: every [`HEARTBEAT`] it checks that the file is where it was, and
//! sets the watch up again when a lost file returns; and the shell can ask
//! for another look shortly after a read that failed ([`Watcher::retry`]).
//!
//! The timing logic is [`Debounce`], which never reads a clock: the caller
//! passes the time in, and the tests pass in whatever time they like.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher as _};

/// How long the file must be left alone before it is read again.
const QUIET: Duration = Duration::from_millis(75);

/// A file written to without pause is still read this often.
const MAX_WAIT: Duration = Duration::from_millis(500);

/// How long a file may be absent before the reader is told it is gone.
const GRACE: Duration = Duration::from_millis(600);

/// How often the watcher checks for itself that the file is still there.
const HEARTBEAT: Duration = Duration::from_secs(2);

/// How long after a failed read the file is looked at again.
const RETRY_AFTER: Duration = Duration::from_millis(250);

/// How many failed reads in a row are followed up. With [`RETRY_AFTER`] this
/// gives a held file five seconds; a file that stays unreadable is not
/// polled for ever, and its next change starts the count again.
const RETRIES: u32 = 20;

/// What the watcher asks of the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// The file may have changed: read it again.
    Changed(PathBuf),
    /// The file is gone and stayed gone.
    Missing(PathBuf),
}

/// What to do when a deadline passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Reload,
    ReportMissing,
}

/// The watcher's timing, as a state machine over times the caller supplies.
#[derive(Debug)]
struct Debounce {
    quiet: Duration,
    max_wait: Duration,
    grace: Duration,
    /// When to look at the file next.
    due: Option<Instant>,
    /// The first event of the burst being waited out.
    burst_started: Option<Instant>,
    /// Since when the file has been found absent.
    absent_since: Option<Instant>,
    reported_missing: bool,
    /// How many more failed reads will be followed up.
    retries_left: u32,
}

impl Debounce {
    fn new(quiet: Duration, max_wait: Duration, grace: Duration) -> Self {
        Self {
            quiet,
            max_wait,
            grace,
            due: None,
            burst_started: None,
            absent_since: None,
            reported_missing: false,
            retries_left: RETRIES,
        }
    }

    /// Forget everything: a different file is being watched now.
    fn reset(&mut self) {
        *self = Self::new(self.quiet, self.max_wait, self.grace);
    }

    /// Something happened to the file at `now`.
    fn event(&mut self, now: Instant) {
        let started = *self.burst_started.get_or_insert(now);
        self.due = Some((now + self.quiet).min(started + self.max_wait));
        self.retries_left = RETRIES;
    }

    /// The file was there at `now` but could not be read. Look again
    /// shortly, unless that has been tried often enough already.
    fn retry(&mut self, now: Instant) {
        if self.retries_left > 0 && self.due.is_none() {
            self.retries_left -= 1;
            self.due = Some(now + RETRY_AFTER);
        }
    }

    /// True while the file is known to be absent.
    fn absent(&self) -> bool {
        self.absent_since.is_some()
    }

    /// When [`Debounce::poll`] next has something to do, if ever.
    fn deadline(&self) -> Option<Instant> {
        self.due
    }

    /// Call when the deadline may have passed. `exists` is asked at most once,
    /// and only if it has.
    fn poll(&mut self, now: Instant, exists: impl FnOnce() -> bool) -> Option<Action> {
        if self.due.is_none_or(|due| now < due) {
            return None;
        }
        self.due = None;
        self.burst_started = None;
        if exists() {
            self.absent_since = None;
            self.reported_missing = false;
            return Some(Action::Reload);
        }
        let since = *self.absent_since.get_or_insert(now);
        if self.reported_missing {
            None
        } else if now.duration_since(since) >= self.grace {
            self.reported_missing = true;
            Some(Action::ReportMissing)
        } else {
            // Probably the middle of a save. Look again when the grace is up.
            self.due = Some(since + self.grace);
            None
        }
    }
}

enum Message {
    /// Watch this file (canonical path), or nothing.
    Watch(Option<PathBuf>),
    /// The file could not be read just now; look again shortly.
    Retry,
    Fs(notify::Result<notify::Event>),
}

/// A handle to the watcher thread. Dropping it stops the thread.
#[derive(Debug)]
pub struct Watcher {
    // `Sender` is not `Sync` on every toolchain this builds with; the mutex
    // makes the handle usable as Tauri state either way.
    sender: Mutex<mpsc::Sender<Message>>,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Watch(path) => f.debug_tuple("Watch").field(path).finish(),
            Message::Retry => f.write_str("Retry"),
            Message::Fs(_) => f.write_str("Fs"),
        }
    }
}

impl Watcher {
    /// Start the watcher thread. `signal` is called on that thread.
    pub fn start(signal: impl Fn(Signal) + Send + 'static) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let events = sender.clone();
        std::thread::Builder::new()
            .name("ezmark-watch".to_owned())
            .spawn(move || run(&receiver, events, &signal))?;
        Ok(Self {
            sender: Mutex::new(sender),
        })
    }

    /// Watch `file` (a canonical path) from now on, or nothing.
    pub fn watch(&self, file: Option<PathBuf>) {
        self.send(Message::Watch(file));
    }

    /// The watched file was there but could not be read (an editor holds it
    /// open, say). Signal a change again shortly; nothing else may.
    pub fn retry(&self) {
        self.send(Message::Retry);
    }

    fn send(&self, message: Message) {
        let sender = self.sender.lock().unwrap_or_else(|p| p.into_inner());
        let _ = sender.send(message);
    }
}

fn run(receiver: &mpsc::Receiver<Message>, events: mpsc::Sender<Message>, signal: &dyn Fn(Signal)) {
    let watcher = notify::recommended_watcher(move |event| {
        let _ = events.send(Message::Fs(event));
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            eprintln!("ezmark: live reload is off, the file watcher did not start: {error}");
            return;
        }
    };
    let mut target: Option<PathBuf> = None;
    let mut debounce = Debounce::new(QUIET, MAX_WAIT, GRACE);
    let mut heartbeat = Instant::now() + HEARTBEAT;

    loop {
        // With nothing watched there is nothing to check, and no reason to wake.
        let wake = match (&target, debounce.deadline()) {
            (None, deadline) => deadline,
            (Some(_), Some(deadline)) => Some(deadline.min(heartbeat)),
            (Some(_), None) => Some(heartbeat),
        };
        let message = match wake {
            Some(wake) => {
                match receiver.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                    Ok(message) => Some(message),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            None => match receiver.recv() {
                Ok(message) => Some(message),
                Err(_) => return,
            },
        };
        match message {
            Some(Message::Watch(file)) => {
                if file != target {
                    retarget(&mut watcher, target.as_deref(), file.as_deref());
                    target = file;
                    debounce.reset();
                    // The file was read before this watch existed. Look once
                    // more, so a save in between is not missed until the next.
                    debounce.event(Instant::now());
                }
            }
            Some(Message::Fs(Ok(event))) => {
                if target.as_deref().is_some_and(|file| concerns(&event, file)) {
                    debounce.event(Instant::now());
                }
            }
            // The backend lost track (an overflow, say): look for ourselves.
            Some(Message::Fs(Err(_))) => debounce.event(Instant::now()),
            Some(Message::Retry) => debounce.retry(Instant::now()),
            None => {}
        }
        let Some(file) = &target else { continue };
        let now = Instant::now();
        if now >= heartbeat {
            heartbeat = now + HEARTBEAT;
            // The folder's watch does not survive the folder being renamed
            // or deleted, and nothing announces that. If the file's presence
            // is not what the last look found, something happened unseen.
            if file.is_file() == debounce.absent() {
                if debounce.absent() {
                    // Back, possibly in a new folder of the old name.
                    rewatch(&mut watcher, file);
                }
                debounce.event(now);
            }
        }
        match debounce.poll(Instant::now(), || file.is_file()) {
            Some(Action::Reload) => signal(Signal::Changed(file.clone())),
            Some(Action::ReportMissing) => signal(Signal::Missing(file.clone())),
            None => {}
        }
    }
}

/// Watch `file`'s folder afresh: the folder there now may not be the one
/// that was watched before.
fn rewatch(watcher: &mut notify::RecommendedWatcher, file: &Path) {
    let Some(folder) = file.parent().map(dunce::simplified) else {
        return;
    };
    let _ = watcher.unwatch(folder);
    if let Err(error) = watcher.watch(folder, RecursiveMode::NonRecursive) {
        eprintln!(
            "ezmark: live reload is off for this file, {} cannot be watched: {error}",
            folder.display()
        );
    }
}

/// Move the directory watch from `old`'s folder to `new`'s.
fn retarget(watcher: &mut notify::RecommendedWatcher, old: Option<&Path>, new: Option<&Path>) {
    let folder = |file: &Path| {
        file.parent()
            .map(|dir| dunce::simplified(dir).to_path_buf())
    };
    let (old, new) = (old.and_then(folder), new.and_then(folder));
    if old == new {
        return;
    }
    if let Some(old) = old {
        let _ = watcher.unwatch(&old);
    }
    if let Some(new) = new
        && let Err(error) = watcher.watch(&new, RecursiveMode::NonRecursive)
    {
        eprintln!(
            "ezmark: live reload is off for this file, {} cannot be watched: {error}",
            new.display()
        );
    }
}

/// True if `event` is about the file named like `file` in the watched folder.
///
/// Only the name is compared: the folder is the one watched, and the backend
/// may spell its path differently (a symlinked temp directory, another case).
fn concerns(event: &notify::Event, file: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false; // reading the file, as this app does, changes nothing
    }
    if event.need_rescan() || event.paths.is_empty() {
        return true;
    }
    let Some(name) = file.file_name() else {
        return false;
    };
    event.paths.iter().any(|path| {
        path.file_name()
            .is_some_and(|other| other.eq_ignore_ascii_case(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn debounce() -> Debounce {
        Debounce::new(75 * MS, 500 * MS, 600 * MS)
    }

    #[test]
    fn nothing_happens_until_the_file_has_been_quiet() {
        let t0 = Instant::now();
        let mut d = debounce();
        assert_eq!(d.deadline(), None);
        assert_eq!(d.poll(t0, || panic!("not asked")), None);

        d.event(t0);
        assert_eq!(d.deadline(), Some(t0 + 75 * MS));
        assert_eq!(d.poll(t0 + 74 * MS, || panic!("too early to ask")), None);
        assert_eq!(d.poll(t0 + 75 * MS, || true), Some(Action::Reload));
        assert_eq!(d.deadline(), None, "one reload per burst");
        assert_eq!(d.poll(t0 + 500 * MS, || panic!("not asked")), None);
    }

    #[test]
    fn ten_saves_in_a_hundred_milliseconds_are_one_reload() {
        let t0 = Instant::now();
        let mut d = debounce();
        let mut reloads = 0;
        for save in 0..10 {
            let now = t0 + 10 * save * MS;
            reloads += usize::from(d.poll(now, || true).is_some());
            d.event(now);
        }
        assert_eq!(reloads, 0, "not while the saves keep coming");
        assert_eq!(d.deadline(), Some(t0 + 165 * MS), "75 ms after the last");
        assert_eq!(d.poll(t0 + 165 * MS, || true), Some(Action::Reload));
    }

    #[test]
    fn a_file_written_without_pause_is_still_read_now_and_then() {
        let t0 = Instant::now();
        let mut d = debounce();
        let mut reloads = Vec::new();
        for tick in 0..=120 {
            let now = t0 + 10 * tick * MS;
            if d.poll(now, || true).is_some() {
                reloads.push(tick * 10);
            }
            d.event(now);
        }
        assert_eq!(reloads, [500, 1000], "every MAX_WAIT, not never");
    }

    #[test]
    fn a_gap_in_a_save_is_not_reported_as_a_missing_file() {
        let t0 = Instant::now();
        let mut d = debounce();
        // Delete, then rename the new file into place 200 ms later.
        d.event(t0);
        assert_eq!(d.poll(t0 + 75 * MS, || false), None, "absent, but wait");
        assert_eq!(
            d.deadline(),
            Some(t0 + 675 * MS),
            "look again after the grace"
        );
        d.event(t0 + 200 * MS);
        assert_eq!(d.poll(t0 + 275 * MS, || true), Some(Action::Reload));
        assert_eq!(d.deadline(), None);
    }

    #[test]
    fn a_file_that_stays_gone_is_reported_once_and_its_return_is_a_reload() {
        let t0 = Instant::now();
        let mut d = debounce();
        d.event(t0);
        assert_eq!(d.poll(t0 + 75 * MS, || false), None);
        assert_eq!(
            d.poll(t0 + 675 * MS, || false),
            Some(Action::ReportMissing),
            "gone for the whole grace period"
        );
        assert_eq!(d.deadline(), None);

        // More events while it is gone: nothing more to say.
        d.event(t0 + 1000 * MS);
        assert_eq!(d.poll(t0 + 1075 * MS, || false), None);
        assert_eq!(d.deadline(), None);

        // It comes back, then goes again: both are news.
        d.event(t0 + 2000 * MS);
        assert_eq!(d.poll(t0 + 2075 * MS, || true), Some(Action::Reload));
        d.event(t0 + 3000 * MS);
        assert_eq!(d.poll(t0 + 3075 * MS, || false), None);
        assert_eq!(
            d.poll(t0 + 3675 * MS, || false),
            Some(Action::ReportMissing)
        );
    }

    #[test]
    fn a_failed_read_is_followed_up_a_bounded_number_of_times() {
        let t0 = Instant::now();
        let mut d = debounce();
        d.event(t0);
        assert_eq!(d.poll(t0 + 75 * MS, || true), Some(Action::Reload));

        // The read fails; each retry leads to one more look, 250 ms later.
        let mut now = t0 + 80 * MS;
        let mut looks = 0;
        loop {
            d.retry(now);
            let Some(due) = d.deadline() else { break };
            assert_eq!(due, now + RETRY_AFTER);
            assert_eq!(d.poll(due, || true), Some(Action::Reload));
            looks += 1;
            now = due + 5 * MS;
        }
        assert_eq!(looks, RETRIES, "then it gives up until the file changes");

        // A real change starts the count again.
        d.event(now);
        assert_eq!(d.poll(now + 75 * MS, || true), Some(Action::Reload));
        d.retry(now + 80 * MS);
        assert!(d.deadline().is_some());
    }

    #[test]
    fn a_retry_never_postpones_a_look_that_is_already_due() {
        let t0 = Instant::now();
        let mut d = debounce();
        d.event(t0);
        d.retry(t0 + 10 * MS);
        assert_eq!(d.deadline(), Some(t0 + 75 * MS));
    }

    #[test]
    fn absence_is_remembered_until_the_file_is_seen_again() {
        let t0 = Instant::now();
        let mut d = debounce();
        assert!(!d.absent());
        d.event(t0);
        d.poll(t0 + 75 * MS, || false);
        assert!(d.absent(), "from the first look that missed it");
        d.event(t0 + 100 * MS);
        d.poll(t0 + 175 * MS, || true);
        assert!(!d.absent());
    }

    #[test]
    fn only_events_about_the_watched_name_count() {
        let file = Path::new("/docs/README.md");
        let event = |kind, paths: &[&str]| notify::Event {
            kind,
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: Default::default(),
        };
        let modify = EventKind::Modify(notify::event::ModifyKind::Any);
        assert!(concerns(&event(modify, &["/docs/README.md"]), file));
        assert!(
            concerns(&event(modify, &["/private/docs/readme.MD"]), file),
            "the folder's spelling and the name's case may differ"
        );
        assert!(
            concerns(
                &event(modify, &["/docs/README.md.tmp", "/docs/README.md"]),
                file
            ),
            "a rename onto the name"
        );
        assert!(concerns(&event(modify, &[]), file), "no paths: assume so");
        assert!(!concerns(&event(modify, &["/docs/other.md"]), file));
        assert!(!concerns(&event(modify, &["/docs/README.md.tmp"]), file));
        let access = EventKind::Access(notify::event::AccessKind::Any);
        assert!(!concerns(&event(access, &["/docs/README.md"]), file));
    }

    /// The real watcher against real saves, in each pattern editors use. The
    /// oracle is the plan's: a signal arrives within a bounded time after the
    /// last write. What is rendered, and whether, is `documents.rs`'s test.
    mod real_files {
        use super::*;

        const PATIENCE: Duration = Duration::from_secs(20);

        struct Watched {
            dir: PathBuf,
            file: PathBuf,
            signals: mpsc::Receiver<Signal>,
            _watcher: Watcher,
        }

        impl Watched {
            fn new(name: &str) -> Self {
                let dir = std::env::temp_dir()
                    .join(format!("ezmark-watch-{name}-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                let dir = std::fs::canonicalize(dir).unwrap();
                let file = dir.join("doc.md");
                std::fs::write(&file, "first\n").unwrap();
                let (sender, signals) = mpsc::channel();
                let watcher = Watcher::start(move |signal| {
                    let _ = sender.send(signal);
                })
                .unwrap();
                watcher.watch(Some(file.clone()));
                let watched = Self {
                    dir,
                    file,
                    signals,
                    _watcher: watcher,
                };
                watched.wait_until_watching();
                watched
            }

            /// The watch is set up on the watcher's thread; saves made before
            /// it is in place would go unseen. Touch the file until one is.
            fn wait_until_watching(&self) {
                let deadline = Instant::now() + PATIENCE;
                while Instant::now() < deadline {
                    std::fs::write(&self.file, "first\n").unwrap();
                    if self
                        .signals
                        .recv_timeout(Duration::from_millis(400))
                        .is_ok()
                    {
                        self.drain();
                        return;
                    }
                }
                panic!("the watcher never noticed a write");
            }

            /// Let a burst finish, and discard what it signalled.
            fn drain(&self) {
                while self
                    .signals
                    .recv_timeout(Duration::from_millis(300))
                    .is_ok()
                {}
            }

            fn expect(&self, expected: &Signal) {
                let deadline = Instant::now() + PATIENCE;
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    match self.signals.recv_timeout(left) {
                        Ok(signal) if signal == *expected => return,
                        Ok(_) => {}
                        Err(_) => panic!("no {expected:?} within {PATIENCE:?}"),
                    }
                }
            }

            fn changed(&self) -> Signal {
                Signal::Changed(self.file.clone())
            }
        }

        impl Drop for Watched {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }

        #[test]
        fn a_write_in_place_is_noticed() {
            let watched = Watched::new("in-place");
            std::fs::write(&watched.file, "second\n").unwrap();
            watched.expect(&watched.changed());
        }

        #[test]
        fn truncate_then_write_is_noticed() {
            use std::io::Write;
            let watched = Watched::new("truncate");
            let mut file = std::fs::File::create(&watched.file).unwrap();
            file.flush().unwrap();
            std::thread::sleep(Duration::from_millis(20));
            file.write_all(b"second\n").unwrap();
            drop(file);
            watched.expect(&watched.changed());
        }

        #[test]
        fn a_temporary_file_renamed_over_the_original_is_noticed() {
            let watched = Watched::new("rename");
            let temporary = watched.dir.join(".doc.md.tmp");
            std::fs::write(&temporary, "second\n").unwrap();
            std::fs::rename(&temporary, &watched.file).unwrap();
            watched.expect(&watched.changed());
            watched.drain();
            // And again: the watch is on the folder, so it survives the swap.
            std::fs::write(&temporary, "third\n").unwrap();
            std::fs::rename(&temporary, &watched.file).unwrap();
            watched.expect(&watched.changed());
        }

        #[test]
        fn rapid_saves_end_in_a_signal_after_the_last_one() {
            let watched = Watched::new("rapid");
            for save in 0..10 {
                std::fs::write(&watched.file, format!("save {save}\n")).unwrap();
                std::thread::sleep(Duration::from_millis(10));
            }
            watched.expect(&watched.changed());
            let mut signals = 1;
            while watched
                .signals
                .recv_timeout(Duration::from_millis(400))
                .is_ok()
            {
                signals += 1;
            }
            assert!(signals <= 10, "{signals} signals for 10 saves");
            assert_eq!(std::fs::read(&watched.file).unwrap(), b"save 9\n");
        }

        #[test]
        fn other_files_in_the_folder_are_ignored() {
            let watched = Watched::new("others");
            std::fs::write(watched.dir.join("other.md"), "x\n").unwrap();
            std::fs::write(watched.dir.join("doc.md.bak"), "x\n").unwrap();
            // Bounded by a write to the watched file: it is the next signal,
            // so nothing was signalled for the two before it.
            std::thread::sleep(Duration::from_millis(300));
            assert!(
                watched.signals.try_recv().is_err(),
                "a signal for another file"
            );
            std::fs::write(&watched.file, "second\n").unwrap();
            watched.expect(&watched.changed());
        }

        /// A folder that is renamed takes its watch along, silently. The
        /// heartbeat is what notices, both that the file is gone and that a
        /// file of that name is back in a new folder of the old name.
        #[test]
        fn a_renamed_folder_is_noticed_and_so_is_its_replacement() {
            let watched = Watched::new("folder");
            let moved = watched.dir.with_extension("moved");
            let _ = std::fs::remove_dir_all(&moved);
            std::fs::rename(&watched.dir, &moved).unwrap();
            watched.expect(&Signal::Missing(watched.file.clone()));

            std::fs::create_dir_all(&watched.dir).unwrap();
            std::fs::write(&watched.file, "back\n").unwrap();
            watched.expect(&watched.changed());
            watched.drain();
            // The watch is on the new folder now: an ordinary save is seen.
            std::fs::write(&watched.file, "and saved again\n").unwrap();
            watched.expect(&watched.changed());
            let _ = std::fs::remove_dir_all(&moved);
        }

        #[test]
        fn a_retry_request_leads_to_another_signal() {
            let watched = Watched::new("retry");
            watched._watcher.retry();
            watched.expect(&watched.changed());
        }

        #[test]
        fn a_deleted_file_is_reported_missing_and_noticed_when_it_returns() {
            let watched = Watched::new("deleted");
            std::fs::remove_file(&watched.file).unwrap();
            watched.expect(&Signal::Missing(watched.file.clone()));
            std::fs::write(&watched.file, "back\n").unwrap();
            watched.expect(&watched.changed());
        }
    }
}
