use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::git;

/// Configuration for file operations during worktree creation
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct FileConfig {
    /// Glob patterns for files to copy from the repo root to the new worktree
    #[serde(default)]
    pub copy: Vec<String>,

    /// Glob patterns for files to symlink from the repo root into the new worktree
    #[serde(default)]
    pub symlink: Vec<String>,
}

/// Configuration for the workmux tool, read from .workmux.yaml
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// The primary branch to merge into (optional, auto-detected if not set)
    #[serde(default)]
    pub main_branch: Option<String>,

    /// Directory where worktrees should be created (optional, defaults to <project>__worktrees pattern)
    /// Can be relative to repo root or absolute path
    #[serde(default)]
    pub worktree_dir: Option<String>,

    /// Prefix for tmux window names (optional, defaults to "wm-")
    #[serde(default)]
    pub window_prefix: Option<String>,

    /// Tmux pane configuration
    #[serde(default)]
    pub panes: Vec<PaneConfig>,

    /// Commands to run after creating the worktree
    #[serde(default)]
    pub post_create: Vec<String>,

    /// File operations to perform after creating the worktree
    #[serde(default)]
    pub files: FileConfig,
}

/// Configuration for a single tmux pane
#[derive(Debug, Deserialize, Serialize)]
pub struct PaneConfig {
    /// Command to run in this pane
    pub command: String,

    /// Whether this pane should receive focus after creation
    #[serde(default)]
    pub focus: bool,

    /// Split direction from the previous pane (horizontal or vertical)
    #[serde(default)]
    pub split: Option<SplitDirection>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

impl Config {
    /// Load configuration from .workmux.yaml or .workmux.yml in the current directory
    pub fn load() -> anyhow::Result<Self> {
        // Look for both .yaml and .yml
        let config_path_yaml = Path::new(".workmux.yaml");
        let config_path_yml = Path::new(".workmux.yml");

        let config_path = if config_path_yaml.exists() {
            Some(config_path_yaml)
        } else if config_path_yml.exists() {
            Some(config_path_yml)
        } else {
            None
        };

        if let Some(path) = config_path {
            let contents = fs::read_to_string(path)?;
            let config: Config = serde_yaml::from_str(&contents)?;
            Ok(config)
        } else {
            // No config file found. Generate default, checking for Claude context.
            if let Ok(repo_root) = git::get_repo_root() {
                if repo_root.join("CLAUDE.md").exists() {
                    return Ok(Config::claude_default());
                }
            }
            // Fallback to standard default
            Ok(Config::default())
        }
    }

    /// Get the default configuration
    pub fn default() -> Self {
        // Use the user's shell, or default to bash if $SHELL is not set
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());

        Config {
            main_branch: None,
            worktree_dir: None,
            window_prefix: None,
            panes: vec![
                PaneConfig {
                    command: shell.clone(),
                    focus: true,
                    split: None,
                },
                PaneConfig {
                    command: "clear".to_string(),
                    focus: false,
                    split: Some(SplitDirection::Horizontal),
                },
            ],
            post_create: vec![],
            files: FileConfig::default(),
        }
    }

    /// Generate a default configuration for a Claude-enabled project.
    fn claude_default() -> Self {
        Config {
            main_branch: None,
            worktree_dir: None,
            window_prefix: None,
            panes: vec![
                PaneConfig {
                    command: "claude".to_string(),
                    focus: true,
                    split: None,
                },
                PaneConfig {
                    command: "clear".to_string(),
                    focus: false,
                    split: Some(SplitDirection::Horizontal),
                },
            ],
            post_create: vec![],
            files: FileConfig::default(),
        }
    }

    /// Get the window prefix to use, defaulting to "wm-" if not configured
    pub fn window_prefix(&self) -> &str {
        self.window_prefix.as_deref().unwrap_or("wm-")
    }

    /// Create an example .workmux.yaml configuration file
    pub fn init() -> anyhow::Result<()> {
        use std::path::PathBuf;

        let config_path = PathBuf::from(".workmux.yaml");

        if config_path.exists() {
            return Err(anyhow::anyhow!(
                ".workmux.yaml already exists. Remove it first if you want to regenerate it."
            ));
        }

        let example_config = r#"# workmux configuration file
# This file defines how workmux creates tmux windows and sets up worktrees

# The primary branch to merge into (optional, auto-detected if not set)
# main_branch: main

# Optional: customize where worktrees are created
# Supports relative paths (to repo root) or absolute paths
# Default: <project_root>/../<project_name>__worktrees
# worktree_dir: .worktrees
# worktree_dir: /path/to/custom/location

# Optional: customize the tmux window name prefix
# Default: wm-
# window_prefix: wm-

# Setup commands to run after worktree and file operations are complete.
# These run as regular processes in your terminal (not in tmux panes).
# You'll see progress output before switching to tmux.
# Use this for: dependency installation, database setup, initial builds.
# For long-running processes (dev servers, watchers), use 'panes' instead.
post_create:
  # - pnpm install
  # - npm run db:setup

# Defines the tmux layout for a new window.
# Panes are created in order. Commands are sent to their respective panes.
panes:
  - command: clear
    focus: true
  - command: clear
    split: horizontal

# Example: Add more panes for your workflow
# panes:
#   - command: nvim .
#     focus: true
#   - command: npm run dev
#     split: vertical

# File operations
# These run after worktree creation but before setup commands
files:
  # Copy files that might be modified per-worktree.
  # Use this for files that should start the same but may diverge.
  # copy:
  #   - some-config.json

  # Symlink shared configuration or documentation.
  # Changes to the source will reflect in all worktrees.
  symlink:
    - .env
    - CLAUDE.md
    - .config/*
"#;

        fs::write(&config_path, example_config)?;

        println!("✓ Created .workmux.yaml");
        println!("\nEdit this file to customize your workmux workflow:");
        println!("  - Add or remove panes");
        println!("  - Change the layout");
        println!("  - Add post-create hooks");

        Ok(())
    }
}
