use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{cmd, config, git, tmux};

/// Result of creating a worktree
pub struct CreateResult {
    pub worktree_path: PathBuf,
    pub branch_name: String,
    pub post_create_hooks_run: usize,
}

/// Result of merging a worktree
pub struct MergeResult {
    pub branch_merged: String,
    pub main_branch: String,
    pub had_staged_changes: bool,
}

/// Result of removing a worktree
pub struct RemoveResult {
    pub branch_removed: String,
}

/// Result of cleanup operations
pub struct CleanupResult {
    pub tmux_window_killed: bool,
    pub worktree_removed: bool,
    pub local_branch_deleted: bool,
    pub remote_branch_deleted: bool,
    pub remote_delete_error: Option<String>,
}

/// Create a new worktree with tmux window and panes
pub fn create(branch_name: &str, config: &config::Config) -> Result<CreateResult> {
    // Pre-flight checks
    if !git::is_git_repo()? {
        return Err(anyhow!("Not in a git repository"));
    }

    if !tmux::is_running()? {
        return Err(anyhow!(
            "tmux is not running. Please start a tmux session first."
        ));
    }

    let prefix = config.window_prefix();
    if tmux::window_exists(prefix, branch_name)? {
        return Err(anyhow!(
            "A tmux window named '{}' already exists",
            branch_name
        ));
    }

    if git::worktree_exists(branch_name)? {
        return Err(anyhow!(
            "A worktree for branch '{}' already exists",
            branch_name
        ));
    }

    // Auto-detect: create branch if it doesn't exist
    let branch_exists = git::branch_exists(branch_name)?;
    let create_new = !branch_exists;

    // Determine worktree path: use config.worktree_dir or default to <project>__worktrees pattern
    let repo_root = git::get_repo_root()?;
    let base_dir = if let Some(ref worktree_dir) = config.worktree_dir {
        let path = Path::new(worktree_dir);
        if path.is_absolute() {
            // Use absolute path as-is
            path.to_path_buf()
        } else {
            // Relative path: resolve from repo root
            repo_root.join(path)
        }
    } else {
        // Default behavior: <project_root>/../<project_name>__worktrees
        let project_name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Could not determine project name"))?;
        repo_root
            .parent()
            .ok_or_else(|| anyhow!("Could not determine parent directory"))?
            .join(format!("{}__worktrees", project_name))
    };
    let worktree_path = base_dir.join(branch_name);

    // Create worktree
    git::create_worktree(&worktree_path, branch_name, create_new)
        .context("Failed to create git worktree")?;

    // Create tmux window
    tmux::create_window(prefix, branch_name, &worktree_path)
        .context("Failed to create tmux window")?;

    // Perform file operations (copy and symlink)
    handle_file_operations(&repo_root, &worktree_path, &config.files)
        .context("Failed to perform file operations")?;

    // Run post-create hooks as regular processes in the worktree directory
    // This happens BEFORE switching to tmux, so users see progress in their terminal
    let hooks_run = config.post_create.len();
    if !config.post_create.is_empty() {
        for (idx, command) in config.post_create.iter().enumerate() {
            println!("  [{}/{}] Running: {}", idx + 1, hooks_run, command);
            cmd::shell_command(command, &worktree_path)
                .with_context(|| format!("Failed to run post-create command: '{}'", command))?;
        }
    }

    // Setup panes
    let pane_setup_result = tmux::setup_panes(prefix, branch_name, &config.panes, &worktree_path)
        .context("Failed to setup panes")?;

    // Focus the configured pane
    tmux::select_pane(prefix, branch_name, pane_setup_result.focus_pane_index)?;

    // Switch to the new window
    tmux::select_window(prefix, branch_name)?;

    Ok(CreateResult {
        worktree_path,
        branch_name: branch_name.to_string(),
        post_create_hooks_run: hooks_run,
    })
}

/// Performs copy and symlink operations from the repo root to the worktree
fn handle_file_operations(
    repo_root: &Path,
    worktree_path: &Path,
    file_config: &config::FileConfig,
) -> Result<()> {
    // Handle copies
    for pattern in &file_config.copy {
        let full_pattern = repo_root.join(pattern).to_string_lossy().to_string();
        for entry in glob::glob(&full_pattern)? {
            let source_path = entry?;
            if source_path.is_dir() {
                return Err(anyhow!(
                    "Cannot copy directory '{}'. Only files are supported for copy operations. \
                    Consider using symlink instead, or specify individual files.",
                    source_path.display()
                ));
            }
            let relative_path = source_path.strip_prefix(repo_root)?;
            let dest_path = worktree_path.join(relative_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory for {:?}", dest_path)
                })?;
            }
            fs::copy(&source_path, &dest_path)
                .with_context(|| format!("Failed to copy {:?} to {:?}", source_path, dest_path))?;
        }
    }

    // Handle symlinks
    for pattern in &file_config.symlink {
        let full_pattern = repo_root.join(pattern).to_string_lossy().to_string();
        for entry in glob::glob(&full_pattern)? {
            let source_path = entry?;
            let relative_path = source_path.strip_prefix(repo_root)?;
            let dest_path = worktree_path.join(relative_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory for {:?}", dest_path)
                })?;
            }

            // Critical: create a relative path for the symlink
            let dest_parent = dest_path.parent().ok_or_else(|| {
                anyhow!(
                    "Could not determine parent directory for destination path: {:?}",
                    dest_path
                )
            })?;

            let relative_source = pathdiff::diff_paths(&source_path, dest_parent)
                .ok_or_else(|| anyhow!("Could not create relative path for symlink"))?;

            // Remove existing file/symlink at destination to avoid errors
            // IMPORTANT: Use symlink_metadata to avoid following symlinks
            if let Ok(metadata) = dest_path.symlink_metadata() {
                if metadata.is_dir() {
                    fs::remove_dir_all(&dest_path).with_context(|| {
                        format!("Failed to remove existing directory at {:?}", &dest_path)
                    })?;
                } else {
                    // Handles both files and symlinks
                    fs::remove_file(&dest_path).with_context(|| {
                        format!("Failed to remove existing file/symlink at {:?}", &dest_path)
                    })?;
                }
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&relative_source, &dest_path).with_context(|| {
                format!(
                    "Failed to create symlink from {:?} to {:?}",
                    relative_source, dest_path
                )
            })?;

            #[cfg(windows)]
            {
                if source_path.is_dir() {
                    std::os::windows::fs::symlink_dir(&relative_source, &dest_path)
                } else {
                    std::os::windows::fs::symlink_file(&relative_source, &dest_path)
                }
                .with_context(|| {
                    format!(
                        "Failed to create symlink from {:?} to {:?}",
                        relative_source, dest_path
                    )
                })?;
            }
        }
    }

    Ok(())
}

/// Merge a branch into the main branch and clean up
pub fn merge(
    branch_name: Option<&str>,
    ignore_uncommitted: bool,
    delete_remote: bool,
    config: &config::Config,
) -> Result<MergeResult> {
    // Pre-flight checks
    if !git::is_git_repo()? {
        return Err(anyhow!("Not in a git repository"));
    }

    // Determine the branch to merge
    let branch_to_merge = if let Some(name) = branch_name {
        name.to_string()
    } else {
        // Running from within a worktree - get current branch
        git::get_current_branch().context("Failed to get current branch")?
    };

    // Get worktree path and check for uncommitted changes
    let worktree_path = git::get_worktree_path(&branch_to_merge)
        .with_context(|| format!("No worktree found for branch '{}'", branch_to_merge))?;

    // Check for unstaged changes - error unless ignore_uncommitted flag is used
    if git::has_unstaged_changes(&worktree_path)? && !ignore_uncommitted {
        return Err(anyhow!(
            "Worktree has unstaged changes. Please stage them with 'git add' or stash them first, or use --ignore-uncommitted to ignore."
        ));
    }

    // Check for staged changes - will need to commit (only if not using ignore_uncommitted)
    let had_staged_changes = git::has_staged_changes(&worktree_path)?;
    if had_staged_changes && !ignore_uncommitted {
        // Commit using git's editor (respects $EDITOR or git config)
        git::commit_with_editor(&worktree_path).context("Failed to commit staged changes")?;
    }

    // Get the main branch (from config or auto-detect)
    let main_branch = config
        .main_branch
        .as_ref()
        .map(|s| Ok(s.clone()))
        .unwrap_or_else(git::get_default_branch)
        .context("Failed to determine the main branch. You can specify it in .workmux.toml")?;

    // Get the main worktree path - need to operate there instead of switching branches
    let main_worktree_path =
        git::get_worktree_path(&main_branch).context("Failed to find main branch worktree")?;

    // Pull latest changes in the main worktree
    let has_remote = git::has_remote_tracking_in_worktree(&main_worktree_path)?;
    if has_remote {
        git::pull_in_worktree(&main_worktree_path).context("Failed to pull latest changes")?;
    }

    // Merge the branch into main (in the main worktree)
    git::merge_in_worktree(&main_worktree_path, &branch_to_merge)
        .context("Failed to merge branch")?;

    // Always force cleanup after a successful merge
    let prefix = config.window_prefix();
    cleanup(prefix, &branch_to_merge, true, delete_remote)?;

    Ok(MergeResult {
        branch_merged: branch_to_merge,
        main_branch,
        had_staged_changes,
    })
}

/// Remove a worktree without merging
pub fn remove(
    branch_name: &str,
    force: bool,
    delete_remote: bool,
    config: &config::Config,
) -> Result<RemoveResult> {
    if !git::is_git_repo()? {
        return Err(anyhow!("Not in a git repository"));
    }

    // Get worktree path - this also validates that the worktree exists
    let worktree_path = git::get_worktree_path(branch_name)
        .with_context(|| format!("No worktree found for branch '{}'", branch_name))?;

    // Safety Check: Prevent deleting the main branch
    let main_branch = git::get_default_branch()
        .context("Failed to determine the main branch. You can specify it in .workmux.toml")?;
    if branch_name == main_branch {
        return Err(anyhow!("Cannot delete the main branch ('{}')", main_branch));
    }

    if git::has_uncommitted_changes(&worktree_path)? && !force {
        return Err(anyhow!(
            "Worktree has uncommitted changes. Use --force to delete anyway."
        ));
    }

    // Note: Unmerged branch check removed - git branch -d/D handles this natively
    // The CLI provides a user-friendly confirmation prompt before calling this function
    let prefix = config.window_prefix();
    cleanup(prefix, branch_name, force, delete_remote)?;

    Ok(RemoveResult {
        branch_removed: branch_name.to_string(),
    })
}

/// Centralized function to clean up tmux and git resources
pub fn cleanup(
    prefix: &str,
    branch_name: &str,
    force: bool,
    delete_remote: bool,
) -> Result<CleanupResult> {
    let mut result = CleanupResult {
        tmux_window_killed: false,
        worktree_removed: false,
        local_branch_deleted: false,
        remote_branch_deleted: false,
        remote_delete_error: None,
    };

    // Kill tmux window if it exists
    match (tmux::is_running(), tmux::window_exists(prefix, branch_name)) {
        (Ok(true), Ok(true)) => {
            tmux::kill_window(prefix, branch_name).context("Failed to kill tmux window")?;
            result.tmux_window_killed = true;
        }
        (Err(_), _) | (_, Err(_)) => {
            // Error checking tmux status, continue with cleanup
        }
        _ => {
            // Tmux not running or window doesn't exist
        }
    }

    // Remove worktree
    git::remove_worktree(branch_name, force).context("Failed to remove worktree")?;
    result.worktree_removed = true;

    // Prune worktrees to ensure git's state is clean
    git::prune_worktrees().context("Failed to prune worktrees")?;

    // Delete local branch
    git::delete_branch(branch_name, force).context("Failed to delete local branch")?;
    result.local_branch_deleted = true;

    // Delete remote branch if requested
    if delete_remote {
        match git::delete_remote_branch(branch_name) {
            Ok(_) => result.remote_branch_deleted = true,
            Err(e) => result.remote_delete_error = Some(e.to_string()),
        }
    }

    Ok(result)
}

/// List all worktrees with their status
pub struct WorktreeInfo {
    pub branch: String,
    pub path: PathBuf,
    pub has_tmux: bool,
    pub has_unmerged: bool,
}

pub fn list(config: &config::Config) -> Result<Vec<WorktreeInfo>> {
    if !git::is_git_repo()? {
        return Err(anyhow!("Not in a git repository"));
    }

    let worktrees_data = git::list_worktrees()?;

    if worktrees_data.is_empty() {
        return Ok(Vec::new());
    }

    // Check tmux status and get all windows once to avoid repeated process calls
    let tmux_windows: std::collections::HashSet<String> = if tmux::is_running().unwrap_or(false) {
        tmux::get_all_window_names().unwrap_or_default()
    } else {
        std::collections::HashSet::new()
    };

    // Get the main branch for unmerged checks
    let main_branch = git::get_default_branch().ok();

    // Get all unmerged branches in one go for efficiency
    // Prefer checking against remote tracking branch for more accurate results
    let unmerged_branches = main_branch
        .as_deref()
        .and_then(|main| git::get_merge_base(main).ok())
        .and_then(|base| git::get_unmerged_branches(&base).ok())
        .unwrap_or_default(); // Use an empty set on failure

    let prefix = config.window_prefix();
    let worktrees: Vec<WorktreeInfo> = worktrees_data
        .into_iter()
        .map(|(path, branch)| {
            let prefixed_branch_name = tmux::prefixed(prefix, &branch);
            let has_tmux = tmux_windows.contains(&prefixed_branch_name);

            // Check for unmerged commits, but only if this isn't the main branch
            let has_unmerged = if let Some(ref main) = main_branch {
                if branch == *main || branch == "(detached)" {
                    false
                } else {
                    unmerged_branches.contains(&branch)
                }
            } else {
                false
            };

            WorktreeInfo {
                branch,
                path,
                has_tmux,
                has_unmerged,
            }
        })
        .collect();

    Ok(worktrees)
}
