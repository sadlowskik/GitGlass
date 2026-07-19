use std::cell::RefCell;
use std::path::Path;

use git2::DiffOptions;
use serde::Serialize;

use crate::error::AppResult;

use super::ops::open_repo;

/// Upper bound on lines returned for one file. A generated file or a lockfile
/// can diff to hundreds of thousands of lines; every one of them becomes a JSON
/// object over IPC and a DOM node in the viewer. The viewer shows a notice when
/// this trips rather than pretending it rendered the whole thing.
const MAX_DIFF_LINES: usize = 20_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLineDto {
    /// "context" | "add" | "delete"
    // &'static str, not String: this is one of three fixed values, and it was
    // allocating a fresh heap String per diff line.
    pub origin: &'static str,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunkDto {
    pub header: String,
    pub lines: Vec<DiffLineDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffDto {
    pub path: String,
    pub is_binary: bool,
    pub hunks: Vec<DiffHunkDto>,
    /// True when the diff exceeded `MAX_DIFF_LINES` and was cut short.
    pub truncated: bool,
}

/// Produce the diff for a single file: its full change relative to HEAD
/// (staged + unstaged combined), which is what a user expects to "see the
/// changes" for a file. New/untracked files show as all-additions.
pub fn file_diff(repo_path: &Path, file_path: &str) -> AppResult<FileDiffDto> {
    let repo = open_repo(repo_path)?;
    let rel = super::ops::to_repo_relative(&repo, file_path);

    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());

    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .context_lines(3)
        .show_untracked_content(true);
    if let Some(rel) = &rel {
        opts.pathspec(rel);
    }

    let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))?;

    let acc = RefCell::new(FileDiffDto {
        path: rel.clone().unwrap_or_else(|| file_path.to_string()),
        is_binary: false,
        hunks: Vec::new(),
        truncated: false,
    });
    let lines_emitted = RefCell::new(0usize);

    diff.foreach(
        &mut |delta, _| {
            if delta.flags().is_binary() {
                acc.borrow_mut().is_binary = true;
            }
            true
        },
        None,
        Some(&mut |_delta, hunk| {
            let header = String::from_utf8_lossy(hunk.header())
                .trim_end()
                .to_string();
            acc.borrow_mut().hunks.push(DiffHunkDto {
                header,
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            let origin = match line.origin() {
                '+' => "add",
                '-' => "delete",
                ' ' => "context",
                // 'H'/'F' headers and other markers are not content lines.
                _ => return true,
            };
            // Keep returning true so libgit2 finishes cleanly; we just stop
            // accumulating once we've hit the cap.
            let mut n = lines_emitted.borrow_mut();
            if *n >= MAX_DIFF_LINES {
                acc.borrow_mut().truncated = true;
                return true;
            }
            *n += 1;

            let content = String::from_utf8_lossy(line.content())
                .trim_end_matches(['\n', '\r'])
                .to_string();
            let dto = DiffLineDto {
                origin,
                old_lineno: line.old_lineno(),
                new_lineno: line.new_lineno(),
                content,
            };
            let mut a = acc.borrow_mut();
            if let Some(h) = a.hunks.last_mut() {
                h.lines.push(dto);
            }
            true
        }),
    )?;

    Ok(acc.into_inner())
}
