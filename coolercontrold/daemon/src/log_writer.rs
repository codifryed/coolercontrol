// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The thread that writes log lines to the journal or stderr.
//!
//! Either sink can stop reading for a while: journald flushing, restarting or backed up, or a
//! stderr pipe nobody drains. A write then blocks whichever thread logged, the main thread
//! included. So lines go through a bounded queue to one `cc-log` thread, the only thread that
//! writes to the sink. A full queue drops the line and counts it, and the thread logs the count
//! once it has written what was queued.

use log::{info, Level, Log, Record};
use std::borrow::Cow;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const THREAD_NAME: &str = "cc-log";

/// Lines held while the sink is not reading. Running normally the queue is close to empty: the
/// thread writes each line as it comes, and journald's socket holds hundreds more on top.
const QUEUE_DEPTH: usize = 512;

/// The longest `drain` waits, so an exit or a panic never hangs on a sink that stopped reading.
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// How long `drain` sleeps while the queue has no room for its marker.
const DRAIN_RETRY: Duration = Duration::from_millis(1);

enum Message<E> {
    Entry(E),
    /// Answered once every entry queued before it is written.
    Drained(SyncSender<()>),
}

/// The queue to the `cc-log` thread. Clones share the queue and its drop count.
pub struct LogWriter<E> {
    tx: SyncSender<Message<E>>,
    dropped: Arc<AtomicUsize>,
}

impl<E: Send + 'static> LogWriter<E> {
    /// Starts the `cc-log` thread, which hands each entry to `write`. `sink` names the sink in the
    /// line that reports dropped entries.
    pub fn start(sink: &'static str, write: impl FnMut(E) + Send + 'static) -> io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(QUEUE_DEPTH);
        let dropped = Arc::new(AtomicUsize::new(0));
        let thread_dropped = Arc::clone(&dropped);
        std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || run(&rx, write, &thread_dropped, sink))?;
        Ok(Self { tx, dropped })
    }

    /// Queues `entry` without waiting. A full queue drops it and counts it.
    pub fn push(&self, entry: E) {
        match self.tx.try_send(Message::Entry(entry)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            // The thread is gone, so nothing is left to write this or report it.
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Waits until every entry queued so far is written, for at most `DRAIN_TIMEOUT`.
    pub fn drain(&self) {
        self.drain_within(DRAIN_TIMEOUT);
    }

    fn drain_within(&self, timeout: Duration) {
        // The writer cannot wait for itself: a panic in a sink lands here on its own thread.
        if std::thread::current().name() == Some(THREAD_NAME) {
            return;
        }
        let deadline = Instant::now() + timeout;
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let mut marker = Message::Drained(done_tx);
        loop {
            match self.tx.try_send(marker) {
                Ok(()) => break,
                Err(TrySendError::Full(unsent)) => marker = unsent,
                Err(TrySendError::Disconnected(_)) => return,
            }
            if Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(DRAIN_RETRY);
        }
        let _ = done_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }

    #[cfg(test)]
    fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl<E> Clone for LogWriter<E> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            dropped: Arc::clone(&self.dropped),
        }
    }
}

/// The target for env_logger's pipe. env_logger writes each formatted record with one
/// `write_all`, and this takes it all at once, so every write is one line.
impl io::Write for LogWriter<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.push(buf.to_vec());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn run<E>(rx: &Receiver<Message<E>>, mut write: impl FnMut(E), dropped: &AtomicUsize, sink: &str) {
    while let Ok(first) = rx.recv() {
        write_message(first, &mut write);
        // Bounded, so a sink that only just keeps up with a flood still reports what it dropped.
        for _ in 1..QUEUE_DEPTH {
            let Ok(message) = rx.try_recv() else {
                break;
            };
            write_message(message, &mut write);
        }
        report_dropped(dropped, sink);
    }
}

fn write_message<E>(message: Message<E>, write: &mut impl FnMut(E)) {
    match message {
        Message::Entry(entry) => write(entry),
        Message::Drained(done) => {
            let _ = done.try_send(());
        }
    }
}

fn report_dropped(dropped: &AtomicUsize, sink: &str) {
    let count = dropped.swap(0, Ordering::Relaxed);
    if count == 0 {
        return;
    }
    // Queued like any other line, so it follows the lines that did get through.
    info!("{count} log lines did not reach {sink}, which stopped taking them for a while");
}

/// A record that can cross to the `cc-log` thread: `Record` borrows from the logging call.
pub struct QueuedRecord {
    level: Level,
    target: String,
    module_path: Option<Cow<'static, str>>,
    file: Option<Cow<'static, str>>,
    line: Option<u32>,
    message: String,
}

impl QueuedRecord {
    /// Key-values are not carried: nothing in the daemon logs them.
    pub fn of(record: &Record) -> Self {
        Self {
            level: record.level(),
            target: record.target().to_owned(),
            module_path: record
                .module_path_static()
                .map(Cow::Borrowed)
                .or_else(|| record.module_path().map(|path| Cow::Owned(path.to_owned()))),
            file: record
                .file_static()
                .map(Cow::Borrowed)
                .or_else(|| record.file().map(|file| Cow::Owned(file.to_owned()))),
            line: record.line(),
            message: record.args().to_string(),
        }
    }

    pub fn log_to(&self, sink: &dyn Log) {
        sink.log(
            &Record::builder()
                .level(self.level)
                .target(&self.target)
                .module_path(self.module_path.as_deref())
                .file(self.file.as_deref())
                .line(self.line)
                .args(format_args!("{}", self.message))
                .build(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Metadata;
    use std::io::Write;
    use std::sync::Mutex;

    /// A writer whose sink records each entry and, once `hold` is set, blocks on the first entry
    /// until the returned gate sender is dropped. `started` fires when the sink takes an entry.
    struct Held {
        writer: LogWriter<usize>,
        written: Arc<Mutex<Vec<usize>>>,
        started: Receiver<()>,
        gate: Option<SyncSender<()>>,
    }

    fn held_writer() -> Held {
        let written = Arc::new(Mutex::new(Vec::new()));
        let sink_written = Arc::clone(&written);
        let (started_tx, started) = mpsc::sync_channel(QUEUE_DEPTH * 2);
        let (gate, gate_rx) = mpsc::sync_channel::<()>(0);
        let writer = LogWriter::start("the test sink", move |entry| {
            let _ = started_tx.try_send(());
            // Blocks until the gate sender is dropped, then returns at once from then on.
            let _ = gate_rx.recv();
            sink_written.lock().unwrap().push(entry);
        })
        .unwrap();
        Held {
            writer,
            written,
            started,
            gate: Some(gate),
        }
    }

    // Goal: entries reach the sink in the order they were queued, and `drain` returns only once
    // they all have. Method: an open sink records entries; queue ten, drain, read the record.
    #[test]
    fn entries_are_written_in_order_before_drain_returns() {
        let mut held = held_writer();
        held.gate = None;
        for entry in 0..10 {
            held.writer.push(entry);
        }

        held.writer.drain();

        assert_eq!(*held.written.lock().unwrap(), (0..10).collect::<Vec<_>>());
        assert_eq!(held.writer.dropped(), 0);
    }

    // Goal: a sink that stops reading never blocks the logging thread; lines past the queue are
    // dropped and counted, and the ones queued are still written once the sink reads again.
    // Method: wedge the sink on the first entry, fill the queue, push three more, then reopen.
    #[test]
    fn a_full_queue_drops_and_counts_without_blocking() {
        let mut held = held_writer();
        held.writer.push(0);
        held.started.recv().unwrap();
        for entry in 1..=QUEUE_DEPTH {
            held.writer.push(entry);
        }
        for entry in 0..3 {
            held.writer.push(QUEUE_DEPTH + 1 + entry);
        }

        assert_eq!(held.writer.dropped(), 3);
        held.gate = None;
        held.writer.drain();
        assert_eq!(
            *held.written.lock().unwrap(),
            (0..=QUEUE_DEPTH).collect::<Vec<_>>()
        );
    }

    // Goal: reporting the dropped lines starts the count again, so each loss is reported once.
    // Method: call it on a count of five and read the count back.
    #[test]
    fn reporting_dropped_lines_resets_the_count() {
        let dropped = AtomicUsize::new(5);

        report_dropped(&dropped, "the test sink");

        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    // Goal: env_logger's pipe writes become one entry per write, bytes intact. Method: two
    // writes through `io::Write`, then drain and compare.
    #[test]
    fn each_pipe_write_is_one_line() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let sink_written = Arc::clone(&written);
        let mut writer = LogWriter::start("the test sink", move |line: Vec<u8>| {
            sink_written.lock().unwrap().push(line);
        })
        .unwrap();

        writer.write_all(b"first line\n").unwrap();
        writer.write_all(b"second line\n").unwrap();
        writer.drain();

        assert_eq!(
            *written.lock().unwrap(),
            vec![b"first line\n".to_vec(), b"second line\n".to_vec()]
        );
    }

    /// Level, target, module path, file, line and message.
    type Logged = (
        Level,
        String,
        Option<String>,
        Option<String>,
        Option<u32>,
        String,
    );

    /// Records the fields of each record it is given.
    struct Fields(Mutex<Vec<Logged>>);

    impl Log for Fields {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            self.0.lock().unwrap().push((
                record.level(),
                record.target().to_owned(),
                record.module_path().map(str::to_owned),
                record.file().map(str::to_owned),
                record.line(),
                record.args().to_string(),
            ));
        }

        fn flush(&self) {}
    }

    // Goal: a record rebuilt on the writer thread carries every field the journal writes, static
    // or not. Method: queue one record with static fields and one with borrowed ones, replay both
    // into a logger that records their fields.
    #[test]
    fn a_queued_record_keeps_its_fields() {
        let target = String::from("dynamic::target");
        let module = String::from("dynamic::module");
        let file = String::from("src/dynamic.rs");
        let queued = [
            QueuedRecord::of(
                &Record::builder()
                    .level(Level::Warn)
                    .target("coolercontrold::static")
                    .module_path_static(Some("coolercontrold::static"))
                    .file_static(Some("src/static.rs"))
                    .line(Some(7))
                    .args(format_args!("static {}", 1))
                    .build(),
            ),
            QueuedRecord::of(
                &Record::builder()
                    .level(Level::Debug)
                    .target(&target)
                    .module_path(Some(&module))
                    .file(Some(&file))
                    .line(None)
                    .args(format_args!("borrowed"))
                    .build(),
            ),
        ];
        drop((target, module, file));
        let fields = Fields(Mutex::new(Vec::new()));

        for record in &queued {
            record.log_to(&fields);
        }

        let logged = fields.0.into_inner().unwrap();
        assert_eq!(
            logged[0],
            (
                Level::Warn,
                "coolercontrold::static".to_owned(),
                Some("coolercontrold::static".to_owned()),
                Some("src/static.rs".to_owned()),
                Some(7),
                "static 1".to_owned()
            )
        );
        assert_eq!(
            logged[1],
            (
                Level::Debug,
                "dynamic::target".to_owned(),
                Some("dynamic::module".to_owned()),
                Some("src/dynamic.rs".to_owned()),
                None,
                "borrowed".to_owned()
            )
        );
    }

    // Goal: `drain` gives up on a sink that never reads again, so an exit cannot hang on it.
    // Method: wedge the sink, fill the queue so the marker cannot even be queued, then drain
    // with a short timeout and time it.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn drain_gives_up_on_a_wedged_sink() {
        let held = held_writer();
        held.writer.push(0);
        held.started.recv().unwrap();
        for entry in 1..=QUEUE_DEPTH {
            held.writer.push(entry);
        }
        let start = Instant::now();

        held.writer.drain_within(Duration::from_millis(50));

        assert!(start.elapsed() >= Duration::from_millis(50));
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    // Goal: a drain on the writer thread itself (a sink that panicked reaches the panic hook
    // there) returns at once instead of waiting on itself. Method: drain a wedged writer with a
    // long timeout from a thread carrying the writer's name, and time it.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn drain_on_the_writer_thread_returns_at_once() {
        let held = held_writer();
        held.writer.push(0);
        held.started.recv().unwrap();
        let writer = held.writer.clone();
        let elapsed = std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let start = Instant::now();
                writer.drain_within(Duration::from_secs(30));
                start.elapsed()
            })
            .unwrap()
            .join()
            .unwrap();

        assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    }
}
