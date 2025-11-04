# workmux

Giga opinionated zero-friction workflow tool that orchestrates git worktrees and
tmux windows to create isolated development environments, perfect for running
multiple AI agents in parallel without conflict.

## Philosophy

- **One worktree, one tmux window**: Each git worktree gets its own dedicated,
  pre-configured tmux window
- **Parallel AI-powered development**: Safely run multiple AI agents on
  different features simultaneously. Each agent gets a fully isolated
  environment, preventing them from interfering with each other
- **Frictionless**: Multi-step workflows reduced to simple commands
- **Configuration as code**: Define your tmux layout and setup steps in
  `.workmux.yaml`

## Features

- Create git worktrees with matching tmux windows in a single command (`add`)
- Automatically set up your preferred pane layout (editor, shell, watchers,
  etc.)
- Run post-creation hooks (install dependencies, setup database, etc.)
- Copy or symlink configuration files (`.env`, `node_modules`) into new
  worktrees
- Merge branches and clean up everything (worktree, tmux window, branches) in
  one command (`merge`)
- List all worktrees with their tmux and merge status
- Bootstrap projects with an initial configuration file (`init`)
- Dynamic shell completions for branch names

## Installation

```bash
cargo install workmux
```

## Quick start

1. **Initialize configuration (optional)**:

```bash
workmux init
```

This creates a `.workmux.yaml` file to customize your workflow (pane layouts,
setup commands, file operations, etc.). workmux works out of the box with
sensible defaults, so this step is optional.

2. **Create a new worktree and tmux window**:

```bash
workmux add new-feature
```

This will:

- Create a git worktree at
  `<project_root>/../<project_name>__worktrees/new-feature`
- Create a tmux window named `new-feature`
- Automatically switch your tmux client to the new window

3. **When done, merge and clean up**:

```bash
workmux merge new-feature
```

Merges your branch into main and cleans up everything (tmux window, worktree,
and local branch).

## Configuration

workmux works out of the box with sensible defaults. To customize your workflow,
create a `.workmux.yaml` file in your project root:

```yaml
# The primary branch to merge into. If not set, workmux will attempt to auto-detect it.
# main_branch: main

# Custom directory for worktrees. Can be an absolute path or relative to the git repo root.
# Defaults to a sibling directory named '<project_name>__worktrees'.
# worktree_dir: ".worktrees"

# Custom prefix for tmux window names. Defaults to "wm-".
# window_prefix: "wm-"

# Commands to run after the worktree is created but before tmux setup is finalized.
# Useful for installing dependencies or running database migrations.
post_create:
  - pnpm install
  - pnpm db:migrate

# Files to copy from the repo root into the new worktree.
# Supports glob patterns.
files:
  copy:
    - '.env.example'
  # Files/directories to symlink from the repo root into the new worktree.
  # Useful for sharing heavy directories like node_modules or build caches.
  symlink:
    - 'node_modules'
    - '.next'

# Defines the tmux pane layout for new windows.
panes:
  - command: claude # The first pane is created by default
    focus: true # This pane will have focus
  - command: pnpm run dev
    split: horizontal # Splits the window horizontally, creating a new pane to the right
```

### Configuration Options

- **main_branch**: Branch to merge into (optional, auto-detected from remote or
  checks for `main`/`master`)
- **worktree_dir**: Custom directory for worktrees (absolute or relative to repo
  root)
- **window_prefix**: Prefix for tmux window names (default: `wm-`)
- **panes**: Array of pane configurations
  - **command**: Command to run in the pane
  - **focus**: Whether this pane should receive focus (default: false)
  - **split**: How to split from previous pane (`horizontal` or `vertical`)
- **post_create**: Commands to run after worktree creation (in the new worktree
  directory)
- **files**: File operations to perform on worktree creation
  - **copy**: List of glob patterns for files to copy
  - **symlink**: List of glob patterns for files/directories to symlink

**Note**: Worktrees are created in `<project>__worktrees` as a sibling directory
to your project by default.

## Commands

- [`add`](#workmux-add-branch-name) - Create a new worktree and tmux window
- [`merge`](#workmux-merge-branch-name) - Merge a branch and clean up everything
- [`remove`](#workmux-remove-branch-name) - Remove a worktree without merging
- [`list`](#workmux-list) - List all worktrees with status
- [`init`](#workmux-init) - Generate configuration file
- [`completions`](#workmux-completions-shell) - Generate shell completions

### `workmux add <branch-name>`

Creates a new git worktree with a matching tmux window and switches you to it
immediately. If the branch doesn't exist, it will be created automatically.

- `<branch-name>`: Name of the branch to create or switch to.

**What happens:**

1. Creates a git worktree at
   `<project_root>/../<project_name>__worktrees/<branch-name>`
2. Creates a new tmux window named after the branch
3. Runs any configured file operations (copy/symlink)
4. Executes `post_create` commands if defined in config
5. Sets up your configured tmux pane layout
6. Automatically switches your tmux client to the new window

**Examples:**

```bash
# Create a new branch and worktree
workmux add user-auth

# Use an existing branch
workmux add existing-work

# Branch names with slashes work too
workmux add feature/new-api
```

---

### `workmux merge [branch-name]`

Merges a branch into the main branch and automatically cleans up all associated
resources (worktree, tmux window, and local branch).

- `[branch-name]`: Optional name of the branch to merge. If omitted,
  automatically detects the current branch from the worktree you're in.

**Common options:**

- `--ignore-uncommitted`: Commit any staged changes before merging without
  opening an editor
- `--delete-remote`, `-r`: Also delete the remote branch after a successful
  merge

**What happens:**

1. Determines which branch to merge (specified branch or current branch if
   omitted)
2. Checks for uncommitted changes (errors if found, unless
   `--ignore-uncommitted` is used)
3. Commits staged changes if present (unless `--ignore-uncommitted` is used)
4. Pulls latest changes to main branch
5. Merges your branch into main
6. Deletes the tmux window (including the one you're currently in if you ran
   this from a worktree)
7. Removes the worktree
8. Deletes the local branch

**Typical workflow:**

When you're done working in a worktree, simply run `workmux merge` from within
that worktree's tmux window. The command will automatically detect which branch
you're on, merge it into main, and close the current window as part of cleanup.

**Examples:**

```bash
# Merge branch from main branch
workmux merge user-auth

# Merge the current worktree you're in
# (run this from within the worktree's tmux window)
workmux merge
```

---

### `workmux remove <branch-name>` (alias: `rm`)

Removes a worktree, tmux window, and branch without merging. Useful for
abandoning work or cleaning up experimental branches.

- `<branch-name>`: Name of the branch to remove.

**Common options:**

- `--force`, `-f`: Skip confirmation prompt and ignore uncommitted changes
- `--delete-remote`, `-r`: Also delete the remote branch

**Examples:**

```bash
# Remove with confirmation if unmerged
workmux remove experiment

# Use the alias
workmux rm old-work

# Force remove without prompts
workmux rm -f experiment

# Force remove and delete remote branch
workmux rm -f -r old-work
```

---

### `workmux list` (alias: `ls`)

Lists all git worktrees with their tmux window status and merge status.

**Examples:**

```bash
# List all worktrees
workmux list
```

**Example output:**

```
BRANCH      TMUX    UNMERGED    PATH
------      ----    --------    ----
main        -       -           ~/project
user-auth   ✓       -           ~/project__worktrees/user-auth
bug-fix     ✓       ●           ~/project__worktrees/bug-fix
```

**Key:**

- `✓` in TMUX column = tmux window exists for this worktree
- `●` in UNMERGED column = branch has commits not merged into main
- `-` = not applicable

---

### `workmux init`

Generates an example `.workmux.yaml` configuration file in the current directory
with sensible defaults and helpful comments.

**Examples:**

```bash
workmux init
```

This creates a `.workmux.yaml` file that you can customize to define your tmux
layout, post-creation hooks, and file operations.

---

### `workmux completions <shell>`

Generates shell completion script for the specified shell. Completions provide
tab-completion for commands and dynamic branch name suggestions.

- `<shell>`: Shell type: `bash`, `zsh`, or `fish`.

**Examples:**

```bash
# Generate completions for zsh
workmux completions zsh
```

See the [Shell Completions](#shell-completions) section for installation
instructions.

## Workflow Example

Here's a complete workflow:

```bash
# Start a new feature
workmux add user-auth

# Work on your feature...
# (tmux automatically sets up your configured panes and environment)

# When ready, merge and clean up
workmux merge user-auth

# Start another feature
workmux add api-endpoint

# List all active worktrees
workmux list
```

## Why workmux?

`workmux` automates over a dozen manual steps into two simple commands, and
unlocks parallel, AI-driven development workflows that are otherwise
impractical.

### Without workmux:

```bash
# 1. Manually create the worktree and environment
git worktree add ../worktrees/user-auth -b user-auth
cd ../worktrees/user-auth
cp ../../project/.env.example .env
ln -s ../../project/node_modules .
npm install
# ... and other setup steps

# 2. Manually create and configure the tmux window
tmux new-window -n user-auth
tmux split-window -h 'npm run dev'
tmux send-keys -t 0 'claude' C-m
# ... repeat for every pane in your desired layout

# 3. When done, manually merge and clean everything up
cd ../../project
git switch main && git pull
git merge --no-ff user-auth
tmux kill-window -t user-auth
git worktree remove ../worktrees/user-auth
git branch -d user-auth
```

### With workmux:

```bash
# Create the environment
workmux add user-auth

# ... work on the feature ...

# Merge and clean up
workmux merge
```

### The Parallel AI Workflow (with workmux):

Delegate multiple complex tasks to AI agents and let them work at the same time.
This workflow is cumbersome to manage manually.

```bash
# Task 1: Refactor the user model (for Agent 1)
workmux add refactor/user-model

# Task 2: Build a new API endpoint (for Agent 2, in parallel)
workmux add feature/new-api

# ... Command agents work simultaneously in their isolated environments ...

# Merge each task as it's completed
workmux merge refactor/user-model
workmux merge feature/new-api
```

## Shell Completions

To enable tab completions for commands and branch names, add the following to
your shell's configuration file.

For **bash**, add to your `.bashrc`:

```bash
eval "$(workmux completions bash)"
```

For **zsh**, add to your `.zshrc`:

```bash
eval "$(workmux completions zsh)"
```

For **fish**, add to your `config.fish`:

```bash
workmux completions fish | source
```

## Requirements

- Rust (for building)
- Git 2.5+ (for worktree support)
- tmux
