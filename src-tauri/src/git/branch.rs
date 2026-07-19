use std::path::Path;

use git2::{build::CheckoutBuilder, BranchType, Repository};
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};

use super::ops::open_repo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub name: String,
    pub is_current: bool,
    pub upstream: Option<String>,
}

/// List local branches, marking the current one and its upstream (if any).
pub fn list_branches(repo_path: &Path) -> AppResult<Vec<BranchInfoDto>> {
    let repo = open_repo(repo_path)?;
    let mut out = Vec::new();
    for item in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = item?;
        let name = match branch.name()? {
            Some(n) => n.to_string(),
            None => continue,
        };
        let upstream = branch
            .upstream()
            .ok()
            .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));
        out.push(BranchInfoDto {
            name,
            is_current: branch.is_head(),
            upstream,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Create a new branch from the current HEAD; optionally switch to it.
pub fn create_branch(repo_path: &Path, name: &str, checkout: bool) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::new(ErrorKind::Git, "Please enter a branch name."));
    }
    let repo = open_repo(repo_path)?;
    let head = repo.head().map_err(|_| {
        AppError::new(
            ErrorKind::Git,
            "Make your first save (commit) before creating a branch.",
        )
    })?;
    let commit = head.peel_to_commit()?;
    repo.branch(name, &commit, false).map_err(|e| {
        // The most common failure is a duplicate name.
        AppError::new(
            ErrorKind::Git,
            "Couldn’t create that branch — the name may already exist.",
        )
        .with_detail(e.to_string())
    })?;

    if checkout {
        switch_branch(repo_path, name)?;
    }
    Ok(())
}

/// Switch to an existing local branch. Uses a *safe* checkout: uncommitted
/// changes that don't conflict are carried over; if they would be overwritten,
/// a friendly error is returned (nothing is discarded).
pub fn switch_branch(repo_path: &Path, name: &str) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let refname = branch_refname(&repo, name)?;
    let obj = repo.revparse_single(&refname)?;

    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&obj, Some(&mut checkout)).map_err(|e| {
        AppError::new(
            ErrorKind::Git,
            "Couldn’t switch — you have changes that would be overwritten. Save (commit) them first.",
        )
        .with_detail(e.to_string())
    })?;
    repo.set_head(&refname)?;
    Ok(())
}

/// Delete a local branch. Refuses to delete the branch you're currently on, and
/// — unless `force` — refuses a branch with commits that aren't merged into the
/// current branch or pushed to its upstream. That mirrors `git branch -d` (which
/// protects unmerged work) vs `-D` (which forces); GitGlass previously always
/// force-deleted, silently orphaning commits.
pub fn delete_branch(repo_path: &Path, name: &str, force: bool) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    if branch.is_head() {
        return Err(AppError::new(
            ErrorKind::Git,
            "You can’t delete the branch you’re currently on. Switch to another branch first.",
        ));
    }
    if !force && !branch_is_merged(&repo, &branch) {
        return Err(AppError::new(
            ErrorKind::UnmergedBranch,
            format!(
                "“{name}” has commits that aren’t on your current branch or pushed anywhere. Deleting it discards them for good."
            ),
        ));
    }
    branch.delete()?;
    Ok(())
}

/// Whether every commit on `branch` is already reachable from the current HEAD
/// (merged in) or from the branch's own upstream (pushed) — i.e. deleting it
/// loses nothing. Conservative: any uncertainty returns `false` (treat as
/// unmerged) so we err toward protecting the user's commits.
fn branch_is_merged(repo: &Repository, branch: &git2::Branch) -> bool {
    let Some(tip) = branch.get().target() else {
        // A branch with no target has no commits to lose.
        return true;
    };
    let reachable_from =
        |oid: git2::Oid| oid == tip || repo.graph_descendant_of(oid, tip).unwrap_or(false);
    // Merged into the branch we're on?
    if let Ok(head) = repo.head() {
        if let Some(head_oid) = head.target() {
            if reachable_from(head_oid) {
                return true;
            }
        }
    }
    // Or already pushed (reachable from its upstream)?
    if let Ok(upstream) = branch.upstream() {
        if let Some(up_oid) = upstream.get().target() {
            if reachable_from(up_oid) {
                return true;
            }
        }
    }
    false
}

fn branch_refname(repo: &Repository, name: &str) -> AppResult<String> {
    let branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|_| AppError::new(ErrorKind::NotFound, "That branch doesn’t exist anymore."))?;
    branch
        .get()
        .name()
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::new(ErrorKind::Git, "That branch has an invalid name."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn repo_with_commit() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@e.com").unwrap();
        drop(cfg);
        let root = dir.path().to_path_buf();
        fs::write(root.join("a.txt"), "hi").unwrap();
        crate::git::ops::stage_paths(&root, &[root.join("a.txt").to_string_lossy().to_string()])
            .unwrap();
        crate::git::ops::commit(&root, "init").unwrap();
        (dir, root)
    }

    #[test]
    fn create_switch_delete_roundtrip() {
        let (_d, root) = repo_with_commit();

        create_branch(&root, "feature", true).unwrap();
        let branches = list_branches(&root).unwrap();
        let feature = branches.iter().find(|b| b.name == "feature").unwrap();
        assert!(feature.is_current, "should have switched to the new branch");

        // Can't delete the branch we're on.
        assert!(delete_branch(&root, "feature", false).is_err());

        // Switch back to the default, then delete feature. It has no commits of
        // its own (created at the same tip), so it's "merged" — no force needed.
        let default = branches
            .iter()
            .find(|b| !b.is_current)
            .unwrap()
            .name
            .clone();
        switch_branch(&root, &default).unwrap();
        delete_branch(&root, "feature", false).unwrap();
        assert!(list_branches(&root)
            .unwrap()
            .iter()
            .all(|b| b.name != "feature"));
    }

    #[test]
    fn duplicate_branch_is_friendly_error() {
        let (_d, root) = repo_with_commit();
        create_branch(&root, "dup", false).unwrap();
        let err = create_branch(&root, "dup", false).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Git));
    }

    #[test]
    fn deleting_an_unmerged_branch_is_refused_without_force() {
        let (_d, root) = repo_with_commit();
        create_branch(&root, "feature", true).unwrap();
        // Put a commit on `feature` that exists nowhere else.
        fs::write(root.join("new.txt"), "unmerged work").unwrap();
        crate::git::ops::stage_paths(&root, &[root.join("new.txt").to_string_lossy().to_string()])
            .unwrap();
        crate::git::ops::commit(&root, "unmerged commit").unwrap();

        // Switch away, then try to delete without force → refused.
        let default = list_branches(&root)
            .unwrap()
            .into_iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name;
        switch_branch(&root, &default).unwrap();

        let err = delete_branch(&root, "feature", false).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::UnmergedBranch),
            "kind: {:?}",
            err.kind
        );
        assert!(list_branches(&root)
            .unwrap()
            .iter()
            .any(|b| b.name == "feature"));

        // Forcing it through succeeds.
        delete_branch(&root, "feature", true).unwrap();
        assert!(list_branches(&root)
            .unwrap()
            .iter()
            .all(|b| b.name != "feature"));
    }

    #[test]
    fn deleting_a_merged_branch_needs_no_force() {
        let (_d, root) = repo_with_commit();
        // Branch at the current tip, add no commits → fully merged.
        create_branch(&root, "topic", false).unwrap();
        delete_branch(&root, "topic", false).unwrap();
        assert!(list_branches(&root)
            .unwrap()
            .iter()
            .all(|b| b.name != "topic"));
    }
}
