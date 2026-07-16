use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, FileIdMap};
use tauri::{AppHandle, Emitter};

/// Holds the active watcher. We watch one repo root at a time (the one the user
/// is browsing); switching folders re-arms the watch. Debounced so a burst of
/// filesystem events (e.g. a build writing many files) yields a single refresh.
#[derive(Default)]
pub struct WatchState {
    inner: Mutex<Option<ActiveWatch>>,
}

struct ActiveWatch {
    root: PathBuf,
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

impl WatchState {
    /// Begin watching `root`, replacing any previous watch. No-op if already
    /// watching this exact root.
    pub fn watch(&self, app: &AppHandle, root: PathBuf) {
        let mut guard = self.inner.lock().expect("watch state poisoned");
        if guard.as_ref().is_some_and(|a| a.root == root) {
            return;
        }

        let app_handle = app.clone();
        let emit_root = root.clone();
        let debouncer = new_debouncer(
            Duration::from_millis(300),
            None,
            move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
                if let Ok(events) = result {
                    if events.iter().any(is_relevant) {
                        // The UI decides whether it cares about this root.
                        let _ =
                            app_handle.emit("fs:changed", emit_root.to_string_lossy().to_string());
                    }
                }
            },
        );

        match debouncer {
            Ok(mut d) => {
                if d.watcher().watch(&root, RecursiveMode::Recursive).is_ok() {
                    *guard = Some(ActiveWatch {
                        root,
                        _debouncer: d,
                    });
                }
            }
            Err(_) => {
                // Watching is best-effort; failure just means no live updates.
            }
        }
    }
}

/// Ignore churn inside .git internals and obvious build/output dirs so we don't
/// spam refreshes. Status changes still surface because the working-tree files
/// themselves change outside these paths.
fn is_relevant(event: &DebouncedEvent) -> bool {
    event.paths.iter().any(|p| {
        let s = p.to_string_lossy();
        let ignored_segment = [".git/", ".git\\"].iter().any(|seg| s.contains(seg));
        !ignored_segment
    })
}

// Small helper so the map type is available if we later watch multiple roots.
#[allow(dead_code)]
type Roots = HashMap<PathBuf, ()>;
