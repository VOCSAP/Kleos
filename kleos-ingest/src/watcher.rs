use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::ledger::Ledger;
use crate::summarizer;
use crate::tailer;
use crate::writer::KleosWriter;

pub async fn run(config: Config, ledger: Ledger, writer: KleosWriter, dry_run: bool) {
    run_with_completion(config, ledger, writer, dry_run, None).await;
}

async fn run_with_completion(
    config: Config,
    ledger: Ledger,
    writer: KleosWriter,
    dry_run: bool,
    completed: Option<mpsc::UnboundedSender<()>>,
) {
    let config = Arc::new(config);
    let ledger = Arc::new(ledger);
    let writer = Arc::new(Mutex::new(writer));

    let (tx, mut rx) = mpsc::channel::<PathBuf>(256);

    // File watcher
    let watch_dir = config.watch_dir.clone();
    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in event.paths {
                                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                                    let _ = tx_clone.blocking_send(path);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            NotifyConfig::default(),
        )
        .expect("failed to create file watcher");

        if !watch_dir.exists() {
            std::fs::create_dir_all(&watch_dir).expect("failed to create watch directory");
        }
        watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .expect("failed to watch directory");

        tracing::info!(dir = %watch_dir.display(), "file watcher started");

        // Keep the watcher alive
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });

    // Track last activity per file for idle detection
    let mut last_activity: HashMap<PathBuf, Instant> = HashMap::new();

    // One active tail task per path (BINGEST-1). When a new event arrives for a
    // path whose task is still running, we record activity but do not spawn a
    // second concurrent task -- the running task will read up to EOF and record
    // the final offset, so no data is lost. A pending-event flag per path
    // ensures we re-tail once the current task finishes to pick up anything
    // that arrived after the in-flight read.
    /// State of a per-path tail slot.
    struct TailSlot {
        /// The running tail task.
        handle: JoinHandle<()>,
        /// True when at least one event arrived while the task was in-flight.
        pending: bool,
    }
    // One active TailSlot per watched path.
    let mut active_tails: HashMap<PathBuf, TailSlot> = HashMap::new();

    // Idle check ticker (also sends systemd watchdog ping)
    let mut idle_interval = tokio::time::interval(Duration::from_secs(30));
    // Fast ticker that re-spawns pending tail slots and prunes finished ones
    // (NEW-2/BF-2). Decoupled from the 30s idle/summarize cadence so coalesced
    // events are picked up within ~2s instead of waiting up to 30s.
    let mut respawn_interval = tokio::time::interval(Duration::from_secs(2));
    #[cfg(unix)]
    let notify_socket = std::env::var("NOTIFY_SOCKET").ok();

    // Signal handler
    #[cfg(unix)]
    let mut sigterm = {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM");
        Box::pin(async move { signal.recv().await })
    };
    #[cfg(not(unix))]
    let mut sigterm = Box::pin(std::future::pending::<()>());
    let mut ctrl_c = Box::pin(tokio::signal::ctrl_c());

    loop {
        tokio::select! {
            Some(path) = rx.recv() => {
                last_activity.insert(path.clone(), Instant::now());

                // Reap finished tasks and handle pending-event re-spawn.
                let finished = active_tails
                    .get(&path)
                    .map(|s| s.handle.is_finished())
                    .unwrap_or(true);

                if finished {
                    // No in-flight task -- spawn a new one.
                    let config = Arc::clone(&config);
                    let ledger = Arc::clone(&ledger);
                    let writer = Arc::clone(&writer);
                    let completed = completed.clone();
                    let path_clone = path.clone();
                    let handle = tokio::spawn(async move {
                        tailer::tail_file(path_clone, config, ledger, writer, dry_run).await;
                        if let Some(completed) = completed {
                            let _ = completed.send(());
                        }
                    });
                    active_tails.insert(path, TailSlot { handle, pending: false });
                } else {
                    // Task already running for this path -- mark pending so we
                    // re-tail once it completes rather than racing on the ledger.
                    if let Some(slot) = active_tails.get_mut(&path) {
                        slot.pending = true;
                    }
                }
            }
            _ = idle_interval.tick() => {
                #[cfg(unix)]
                if let Some(ref sock_path) = notify_socket {
                    if let Ok(sock) = UnixDatagram::unbound() {
                        let _ = sock.send_to(b"WATCHDOG=1", sock_path);
                    }
                }

                let idle_threshold = Duration::from_secs(config.summary_idle_secs);
                let now = Instant::now();
                let mut to_summarize = Vec::new();
                for (path, last) in &last_activity {
                    if now.duration_since(*last) > idle_threshold {
                        let path_str = path.to_string_lossy().to_string();
                        if !ledger.is_summarized(&path_str) {
                            if let Some((project, session_id)) = tailer::parse_session_path(path) {
                                to_summarize.push((path.clone(), project, session_id));
                            }
                        }
                    }
                }
                for (path, project, session_id) in to_summarize {
                    if !dry_run {
                        let path_str = path.to_string_lossy().to_string();
                        summarizer::summarize_session(
                            &session_id, &project, &path, &ledger, Arc::clone(&writer)
                        ).await;
                        ledger.mark_summarized(&path_str);
                    }
                    last_activity.remove(&path);
                }
            }
            _ = respawn_interval.tick() => {
                // Re-spawn pending tail tasks whose prior task has finished
                // (BINGEST-1): events coalesced while a tail was in-flight were
                // deferred instead of raced, so we pick them up here. NEW-2: this
                // runs every ~2s rather than only on the 30s idle tick, bounding
                // coalesced-event latency.
                let mut to_respawn: Vec<PathBuf> = Vec::new();
                for (path, slot) in active_tails.iter() {
                    if slot.pending && slot.handle.is_finished() {
                        to_respawn.push(path.clone());
                    }
                }
                for path in to_respawn {
                    let config = Arc::clone(&config);
                    let ledger = Arc::clone(&ledger);
                    let writer = Arc::clone(&writer);
                    let completed = completed.clone();
                    let path_clone = path.clone();
                    let handle = tokio::spawn(async move {
                        tailer::tail_file(path_clone, config, ledger, writer, dry_run).await;
                        if let Some(completed) = completed {
                            let _ = completed.send(());
                        }
                    });
                    active_tails.insert(path, TailSlot { handle, pending: false });
                }

                // BF-2: drop finished, non-pending tail slots so active_tails does
                // not grow unboundedly for a long-lived process watching many
                // session files. A later event for a pruned path simply spawns a
                // fresh slot (the absent-entry branch treats "no slot" as "spawn").
                active_tails.retain(|_, slot| !slot.handle.is_finished() || slot.pending);
            }
            _ = &mut ctrl_c => {
                tracing::info!("received SIGINT, shutting down");
                break;
            }
            _ = &mut sigterm => {
                tracing::info!("received SIGTERM, shutting down");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: String) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    async fn serve_one(listener: TcpListener, requests: Arc<AtomicUsize>, response_body: &str) {
        let (mut stream, _) = listener.accept().await.unwrap();
        requests.fetch_add(1, Ordering::SeqCst);
        let mut request = [0; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body,
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    }

    #[tokio::test]
    async fn dry_run_leaves_absent_ledger_and_skips_summary_requests() {
        let temp_dir = std::env::temp_dir().join(format!(
            "kleos-ingest-watcher-dry-run-{}",
            std::process::id()
        ));
        let watch_dir = temp_dir.join("transcripts");
        let ledger_path = temp_dir.join("ledger.db");
        std::fs::create_dir_all(&watch_dir).unwrap();

        let ollama_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ollama_url = format!("http://{}", ollama_listener.local_addr().unwrap());
        let kleos_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let kleos_url = format!("http://{}", kleos_listener.local_addr().unwrap());
        let ollama_requests = Arc::new(AtomicUsize::new(0));
        let kleos_requests = Arc::new(AtomicUsize::new(0));
        let ollama_task = tokio::spawn(serve_one(
            ollama_listener,
            Arc::clone(&ollama_requests),
            r#"{"choices":[{"message":{"content":"summary"}}]}"#,
        ));
        let kleos_task = tokio::spawn(serve_one(kleos_listener, Arc::clone(&kleos_requests), "{}"));

        let config = Config {
            watch_dir: watch_dir.clone(),
            kleos_url,
            api_key: None,
            summary_idle_secs: 0,
            novelty_drop_threshold: 0.92,
            ledger_path: ledger_path.clone(),
            host: "test-host".to_string(),
        };
        let ledger = crate::open_ledger(&config, true);
        let writer = KleosWriter::new(&config);
        let _ollama_url = EnvVarGuard::set("KLEOS_INGEST_OLLAMA_URL", ollama_url);
        let (completed_tx, mut completed_rx) = mpsc::unbounded_channel();
        let watcher_task = tokio::spawn(run_with_completion(
            config,
            ledger,
            writer,
            true,
            Some(completed_tx),
        ));

        tokio::time::sleep(Duration::from_millis(200)).await;
        let session_path = watch_dir
            .join("project")
            .join("sessions")
            .join("session.jsonl");
        std::fs::create_dir_all(session_path.parent().unwrap()).unwrap();
        std::fs::write(
            &session_path,
            "{\"type\":\"user\",\"message\":{\"content\":\"Please summarize this completed development session, including its implementation details, test evidence, and any decisions that were recorded for follow-up.\"}}\n",
        )
        .unwrap();

        tokio::time::timeout(Duration::from_secs(5), completed_rx.recv())
            .await
            .expect("watcher must process the transcript event")
            .expect("watcher completion channel must stay open");
        tokio::time::sleep(Duration::from_secs(31)).await;
        watcher_task.abort();
        let _ = watcher_task.await;
        ollama_task.abort();
        kleos_task.abort();

        let ollama_request_count = ollama_requests.load(Ordering::SeqCst);
        let kleos_request_count = kleos_requests.load(Ordering::SeqCst);
        let ledger_exists = ledger_path.exists();
        std::fs::remove_dir_all(&temp_dir).unwrap();

        assert!(!ledger_exists, "dry-run must not create the disk ledger");
        assert_eq!(ollama_request_count, 0, "dry-run must not request Ollama");
        assert_eq!(kleos_request_count, 0, "dry-run must not request Kleos");
    }
}
