use std::path::{Component, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, DebouncedEvent, Debouncer, NoCache};
use tauri::{AppHandle, Emitter};

use crate::secret_scan::SCAN_SKIP_DIRS;

/// Holds the active watcher. We watch one repo root at a time (the one the user
/// is browsing); switching folders re-arms the watch. Debounced so a burst of
/// filesystem events (e.g. a build writing many files) yields a single refresh.
#[derive(Default)]
pub struct WatchState {
    inner: Mutex<Option<ActiveWatch>>,
}

struct ActiveWatch {
    root: PathBuf,
    _debouncer: Debouncer<notify::RecommendedWatcher, NoCache>,
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
        // NoCache, not the default FileIdMap. The map exists to correlate rename
        // events and to resync after the backend drops some; we use neither —
        // the handler only asks "did anything relevant change under this root?"
        // and emits a signal. Carrying it was a slow leak: `add_path` inserts on
        // every Create and only prunes on Remove, so a build writing thousands
        // of content-hashed artifacts grew the map for the life of the watch.
        let debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, NoCache>(
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
            NoCache,
            notify::Config::default(),
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

    /// Stop watching entirely. Called when the user navigates somewhere that
    /// isn't a repo — without this the previous repo stays watched (OS handle,
    /// debouncer thread and cache included) with nobody subscribed.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().expect("watch state poisoned");
        // Dropping ActiveWatch stops the debouncer thread.
        *guard = None;
    }
}

/// Ignore churn inside .git internals and dependency/build output so we don't
/// spam refreshes. Status changes still surface because the working-tree files
/// themselves change outside these paths.
///
/// This filter is load-bearing for performance, not just tidiness: every event
/// that survives it triggers a full libgit2 status walk plus a full file-list
/// re-render in the UI. A build writing into `target/` (24k files in this repo)
/// would otherwise drive that loop continuously for the build's whole duration.
fn is_relevant(event: &DebouncedEvent) -> bool {
    event.paths.iter().any(|p| !is_noise(p))
}

/// True when any path component is a dependency/build directory. Matched
/// component-wise rather than by substring so a folder named `environment`
/// isn't skipped for containing `env`.
fn is_noise(path: &std::path::Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(name) => name
            .to_str()
            .is_some_and(|n| n == ".git" || SCAN_SKIP_DIRS.contains(&n)),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::is_noise;
    use std::path::{Path, PathBuf};

    /// Build a path from components using the platform separator. Backslash
    /// paths must NOT be written as string literals here: on Unix `\` is an
    /// ordinary filename character, so `Path::new(r"C:\repo\.git\index")` is a
    /// SINGLE component and every component-wise assertion below would flip.
    fn p(parts: &[&str]) -> PathBuf {
        parts.iter().copied().collect()
    }

    #[test]
    fn skips_git_internals_and_build_output() {
        for parts in [
            ["repo", ".git", "index"].as_slice(),
            ["repo", "node_modules", "react"].as_slice(),
            ["repo", "src-tauri", "target", "debug"].as_slice(),
            ["repo", "dist", "app.js"].as_slice(),
            ["repo", ".venv", "lib", "site.py"].as_slice(),
        ] {
            let path = p(parts);
            assert!(is_noise(&path), "should be filtered: {}", path.display());
        }
    }

    #[test]
    fn keeps_the_users_own_files() {
        for parts in [
            ["repo", "src", "main.rs"].as_slice(),
            ["repo", "README.md"].as_slice(),
            ["repo", "src", "app.tsx"].as_slice(),
        ] {
            let path = p(parts);
            assert!(
                !is_noise(&path),
                "should NOT be filtered: {}",
                path.display()
            );
        }
    }

    /// Substring matching would swallow these; component matching must not.
    #[test]
    fn does_not_match_partial_component_names() {
        for parts in [
            ["repo", "environment", "config.rs"].as_slice(),
            ["repo", "src", "targeting.rs"].as_slice(),
            ["repo", "distribution", "notes.md"].as_slice(),
            ["repo", "src", "building", "mod.rs"].as_slice(),
        ] {
            let path = p(parts);
            assert!(!is_noise(&path), "false positive on: {}", path.display());
        }
    }

    /// Absolute paths in each platform's own form still resolve component-wise.
    #[test]
    fn works_on_absolute_platform_paths() {
        #[cfg(windows)]
        let (noisy, clean) = (r"C:\repo\node_modules\x.js", r"C:\repo\src\main.rs");
        #[cfg(not(windows))]
        let (noisy, clean) = ("/home/u/repo/node_modules/x.js", "/home/u/repo/src/main.rs");

        assert!(is_noise(Path::new(noisy)));
        assert!(!is_noise(Path::new(clean)));
    }
}
