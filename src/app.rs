//! Application state management

use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;
use ratatui::widgets::ListState;

use git2::Oid;

use crate::{
    action::Action,
    git::{
        build_graph,
        graph::GraphLayout,
        operations::{
            checkout_branch, checkout_commit, checkout_remote_branch, create_branch, delete_branch,
            fetch_origin, merge_branch, rebase_branch,
        },
        BranchInfo, CommitDiffInfo, CommitInfo, GitRepository,
    },
};

/// Application modes
#[derive(Debug, Clone)]
pub enum AppMode {
    Normal,
    Help,
    Input {
        title: String,
        input: String,
        action: InputAction,
    },
    Confirm {
        message: String,
        action: ConfirmAction,
    },
    Error {
        message: String,
    },
}

/// Input action kinds
#[derive(Debug, Clone)]
pub enum InputAction {
    CreateBranch,
    Search,
}

/// Confirmation action kinds
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteBranch(String),
    Merge(String),
    Rebase(String),
}

/// Result of async diff computation
struct DiffResult {
    oid: Oid,
    diff: Option<CommitDiffInfo>,
}

/// Application state
pub struct App {
    pub mode: AppMode,
    pub repo: GitRepository,
    pub repo_path: String,
    pub head_name: Option<String>,

    // Data
    pub commits: Vec<CommitInfo>,
    pub branches: Vec<BranchInfo>,
    pub graph_layout: GraphLayout,

    // UI state
    pub graph_list_state: ListState,

    // Branch selection state
    /// List of (node_index, branch_name) for all branches
    pub branch_positions: Vec<(usize, String)>,
    /// Currently selected branch position index
    pub selected_branch_position: Option<usize>,

    // Diff cache (async load)
    diff_cache: Option<CommitDiffInfo>,
    diff_cache_oid: Option<Oid>,
    diff_loading_oid: Option<Oid>,
    diff_receiver: Option<Receiver<DiffResult>>,

    // Uncommitted diff cache
    uncommitted_diff_cache: Option<CommitDiffInfo>,
    uncommitted_diff_loading: bool,
    uncommitted_diff_receiver: Option<Receiver<Option<CommitDiffInfo>>>,

    // Flags
    pub should_quit: bool,

    // Status message with auto-clear
    message: Option<String>,
    message_time: Option<std::time::Instant>,

    // Async fetch
    fetch_receiver: Option<Receiver<Result<(), String>>>,
}

impl App {
    /// Create a new application
    pub fn new() -> Result<Self> {
        let repo = GitRepository::discover()?;
        let repo_path = repo.path.clone();
        let head_name = repo.head_name();

        let commits = repo.get_commits(500)?;
        let branches = repo.get_branches()?;
        let uncommitted_count = repo
            .get_working_tree_status()
            .ok()
            .flatten()
            .map(|s| s.file_count);
        let head_commit_oid = repo.head_oid();
        let graph_layout = build_graph(&commits, &branches, uncommitted_count, head_commit_oid);

        let mut graph_list_state = ListState::default();
        graph_list_state.select(Some(0));

        // Build branch positions and select the first branch if exists
        let branch_positions = Self::build_branch_positions(&graph_layout);
        let selected_branch_position = if branch_positions.is_empty() {
            None
        } else {
            Some(0)
        };

        Ok(Self {
            mode: AppMode::Normal,
            repo,
            repo_path,
            head_name,
            commits,
            branches,
            graph_layout,
            graph_list_state,
            branch_positions,
            selected_branch_position,
            diff_cache: None,
            diff_cache_oid: None,
            diff_loading_oid: None,
            diff_receiver: None,
            uncommitted_diff_cache: None,
            uncommitted_diff_loading: false,
            uncommitted_diff_receiver: None,
            should_quit: false,
            message: None,
            message_time: None,
            fetch_receiver: None,
        })
    }

    /// Refresh repository data
    pub fn refresh(&mut self) -> Result<()> {
        // Save the currently selected branch name for restoration
        let prev_branch_name = self
            .selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))
            .map(|(_, name)| name.clone());

        self.commits = self.repo.get_commits(500)?;
        self.branches = self.repo.get_branches()?;
        let uncommitted_count = self
            .repo
            .get_working_tree_status()
            .ok()
            .flatten()
            .map(|s| s.file_count);
        let head_commit_oid = self.repo.head_oid();
        self.graph_layout =
            build_graph(&self.commits, &self.branches, uncommitted_count, head_commit_oid);
        self.head_name = self.repo.head_name();

        // Rebuild branch positions
        self.branch_positions = Self::build_branch_positions(&self.graph_layout);

        // Restore branch selection if the branch still exists
        self.selected_branch_position = prev_branch_name.and_then(|name| {
            self.branch_positions
                .iter()
                .position(|(_, n)| n == &name)
        });

        // Sync node selection with branch selection
        if let Some(pos) = self.selected_branch_position {
            if let Some((node_idx, _)) = self.branch_positions.get(pos) {
                self.graph_list_state.select(Some(*node_idx));
            }
        }

        // Clear cache
        self.diff_cache = None;
        self.diff_cache_oid = None;
        self.diff_loading_oid = None;
        self.diff_receiver = None;
        self.uncommitted_diff_cache = None;
        self.uncommitted_diff_loading = false;
        self.uncommitted_diff_receiver = None;

        // Clamp the selection
        let max_commit = self.graph_layout.nodes.len().saturating_sub(1);
        if let Some(selected) = self.graph_list_state.selected() {
            if selected > max_commit {
                self.graph_list_state.select(Some(max_commit));
            }
        }

        Ok(())
    }

    /// Check if async fetch has completed and process the result
    pub fn update_fetch_status(&mut self) {
        let result = match &self.fetch_receiver {
            Some(rx) => rx.try_recv().ok(),
            None => None,
        };

        if let Some(fetch_result) = result {
            self.fetch_receiver = None;

            match fetch_result {
                Ok(()) => {
                    if let Err(e) = self.refresh() {
                        self.show_error(format!("Refresh failed: {}", e));
                    } else {
                        self.set_message("Fetched from origin");
                    }
                }
                Err(e) => {
                    self.show_error(e);
                }
            }
        }
    }

    /// Check if fetch is currently in progress
    pub fn is_fetching(&self) -> bool {
        self.fetch_receiver.is_some()
    }

    /// Set a status message (will auto-clear after a few seconds)
    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_time = Some(std::time::Instant::now());
    }

    /// Get current message if not expired (5 seconds timeout)
    pub fn get_message(&self) -> Option<&str> {
        const MESSAGE_TIMEOUT_SECS: u64 = 5;

        // Don't timeout while fetching
        if self.is_fetching() {
            return self.message.as_deref();
        }

        match (&self.message, &self.message_time) {
            (Some(msg), Some(time)) => {
                if time.elapsed().as_secs() < MESSAGE_TIMEOUT_SECS {
                    Some(msg.as_str())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Update diff info for the selected commit (async)
    pub fn update_diff_cache(&mut self) {
        // Pull in completed results for commit diff
        if let Some(ref receiver) = self.diff_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.diff_cache = result.diff;
                self.diff_cache_oid = Some(result.oid);
                self.diff_loading_oid = None;
                self.diff_receiver = None;
            }
        }

        // Pull in completed results for uncommitted diff
        if let Some(ref receiver) = self.uncommitted_diff_receiver {
            if let Ok(diff) = receiver.try_recv() {
                self.uncommitted_diff_cache = diff;
                self.uncommitted_diff_loading = false;
                self.uncommitted_diff_receiver = None;
            }
        }

        // Check if uncommitted node is selected
        let selected_node = self.graph_list_state.selected().and_then(|idx| {
            self.graph_layout.nodes.get(idx)
        });

        let Some(node) = selected_node else {
            return;
        };

        // Handle uncommitted node
        if node.is_uncommitted {
            // Do nothing if cache exists or already loading
            if self.uncommitted_diff_cache.is_some() || self.uncommitted_diff_loading {
                return;
            }

            // Compute uncommitted diff in the background
            let (tx, rx) = mpsc::channel();
            let repo_path = self.repo_path.clone();

            self.uncommitted_diff_loading = true;
            self.uncommitted_diff_receiver = Some(rx);

            thread::spawn(move || {
                let diff = git2::Repository::open(&repo_path)
                    .ok()
                    .and_then(|repo| CommitDiffInfo::from_working_tree(&repo).ok());

                let _ = tx.send(diff);
            });
            return;
        }

        // Handle regular commit node
        let Some(commit) = &node.commit else {
            return;
        };

        let oid = commit.oid;

        // Do nothing if the cache is valid
        if self.diff_cache_oid == Some(oid) {
            return;
        }

        // Do nothing if already loading
        if self.diff_loading_oid == Some(oid) {
            return;
        }

        // Compute diff in the background
        let (tx, rx) = mpsc::channel();
        let repo_path = self.repo_path.clone();

        self.diff_loading_oid = Some(oid);
        self.diff_receiver = Some(rx);

        thread::spawn(move || {
            let diff = git2::Repository::open(&repo_path)
                .ok()
                .and_then(|repo| CommitDiffInfo::from_commit(&repo, oid).ok());

            let _ = tx.send(DiffResult { oid, diff });
        });
    }

    /// Get cached diff info for the currently selected node
    pub fn cached_diff(&self) -> Option<&CommitDiffInfo> {
        let selected_node = self.graph_list_state.selected().and_then(|idx| {
            self.graph_layout.nodes.get(idx)
        });

        if let Some(node) = selected_node {
            if node.is_uncommitted {
                return self.uncommitted_diff_cache.as_ref();
            }
        }

        self.diff_cache.as_ref()
    }

    /// Whether diff is currently loading for the selected node
    pub fn is_diff_loading(&self) -> bool {
        let selected_node = self.graph_list_state.selected().and_then(|idx| {
            self.graph_layout.nodes.get(idx)
        });

        if let Some(node) = selected_node {
            if node.is_uncommitted {
                return self.uncommitted_diff_loading;
            }
        }

        self.diff_loading_oid.is_some()
    }

    /// Handle an action
    pub fn handle_action(&mut self, action: Action) -> Result<()> {
        match &self.mode {
            AppMode::Normal => self.handle_normal_action(action)?,
            AppMode::Help => self.handle_help_action(action),
            AppMode::Input { .. } => self.handle_input_action(action)?,
            AppMode::Confirm { .. } => self.handle_confirm_action(action)?,
            AppMode::Error { .. } => self.handle_error_action(action),
        }
        Ok(())
    }

    /// Show an error
    pub fn show_error(&mut self, message: String) {
        self.mode = AppMode::Error { message };
    }

    fn handle_normal_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::MoveUp => {
                self.move_selection(-1);
            }
            Action::MoveDown => {
                self.move_selection(1);
            }
            Action::PageUp => {
                self.move_selection(-10);
            }
            Action::PageDown => {
                self.move_selection(10);
            }
            Action::GoToTop => {
                self.select_first();
            }
            Action::GoToBottom => {
                self.select_last();
            }
            Action::NextBranch => {
                self.move_to_next_branch();
            }
            Action::PrevBranch => {
                self.move_to_prev_branch();
            }
            Action::BranchLeft => {
                self.move_branch_left();
            }
            Action::BranchRight => {
                self.move_branch_right();
            }
            Action::ToggleHelp => {
                self.mode = AppMode::Help;
            }
            Action::Refresh => {
                self.refresh()?;
            }
            Action::Fetch => {
                // Don't start another fetch if one is already in progress
                if self.fetch_receiver.is_some() {
                    return Ok(());
                }

                let (tx, rx) = mpsc::channel();
                let repo_path = self.repo_path.clone();

                thread::spawn(move || {
                    let result = fetch_origin(&repo_path).map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });

                self.fetch_receiver = Some(rx);
                self.set_message("Fetching from origin...");
            }
            Action::Checkout => {
                self.do_checkout()?;
            }
            Action::CreateBranch => {
                self.mode = AppMode::Input {
                    title: "New Branch Name".to_string(),
                    input: String::new(),
                    action: InputAction::CreateBranch,
                };
            }
            Action::DeleteBranch => {
                if let Some(branch) = self.selected_branch() {
                    if !branch.is_head && !branch.is_remote {
                        self.mode = AppMode::Confirm {
                            message: format!("Delete branch '{}'?", branch.name),
                            action: ConfirmAction::DeleteBranch(branch.name.clone()),
                        };
                    }
                }
            }
            Action::Merge => {
                if let Some(branch) = self.selected_branch() {
                    if !branch.is_head {
                        self.mode = AppMode::Confirm {
                            message: format!("Merge '{}' into current branch?", branch.name),
                            action: ConfirmAction::Merge(branch.name.clone()),
                        };
                    }
                }
            }
            Action::Rebase => {
                if let Some(branch) = self.selected_branch() {
                    if !branch.is_head {
                        self.mode = AppMode::Confirm {
                            message: format!("Rebase current branch onto '{}'?", branch.name),
                            action: ConfirmAction::Rebase(branch.name.clone()),
                        };
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_help_action(&mut self, action: Action) {
        if matches!(action, Action::ToggleHelp | Action::Quit | Action::Cancel) {
            self.mode = AppMode::Normal;
        }
    }

    fn handle_error_action(&mut self, action: Action) {
        // Close the error on any key
        if matches!(action, Action::Quit | Action::Cancel | Action::Confirm) {
            self.mode = AppMode::Normal;
        }
    }

    fn handle_input_action(&mut self, action: Action) -> Result<()> {
        let (title, input, input_action) = match &self.mode {
            AppMode::Input {
                title,
                input,
                action,
            } => (title.clone(), input.clone(), action.clone()),
            _ => return Ok(()),
        };

        match action {
            Action::Confirm => {
                match input_action {
                    InputAction::CreateBranch => {
                        if !input.is_empty() {
                            if let Some(node) = self.selected_commit_node() {
                                if let Some(commit) = &node.commit {
                                    create_branch(&self.repo.repo, &input, commit.oid)?;
                                    self.refresh()?;
                                }
                            }
                        }
                    }
                    InputAction::Search => {
                        // TODO: Search feature
                    }
                }
                self.mode = AppMode::Normal;
            }
            Action::Cancel => {
                self.mode = AppMode::Normal;
            }
            Action::InputChar(c) => {
                self.mode = AppMode::Input {
                    title,
                    input: format!("{}{}", input, c),
                    action: input_action,
                };
            }
            Action::InputBackspace => {
                let mut new_input = input;
                new_input.pop();
                self.mode = AppMode::Input {
                    title,
                    input: new_input,
                    action: input_action,
                };
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_action(&mut self, action: Action) -> Result<()> {
        let confirm_action = match &self.mode {
            AppMode::Confirm { action, .. } => action.clone(),
            _ => return Ok(()),
        };

        match action {
            Action::Confirm => {
                match confirm_action {
                    ConfirmAction::DeleteBranch(name) => {
                        delete_branch(&self.repo.repo, &name)?;
                    }
                    ConfirmAction::Merge(name) => {
                        merge_branch(&self.repo.repo, &name)?;
                    }
                    ConfirmAction::Rebase(name) => {
                        rebase_branch(&self.repo.repo, &name)?;
                    }
                }
                self.refresh()?;
                self.mode = AppMode::Normal;
            }
            Action::Cancel => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn move_selection(&mut self, delta: i32) {
        let max = self.graph_layout.nodes.len().saturating_sub(1);
        let current = self.graph_list_state.selected().unwrap_or(0);
        let new = (current as i32 + delta).clamp(0, max as i32) as usize;
        self.graph_list_state.select(Some(new));
        self.sync_branch_selection_to_node(new);
    }

    fn select_first(&mut self) {
        self.graph_list_state.select(Some(0));
        self.sync_branch_selection_to_node(0);
    }

    fn select_last(&mut self) {
        let max = self.graph_layout.nodes.len().saturating_sub(1);
        self.graph_list_state.select(Some(max));
        self.sync_branch_selection_to_node(max);
    }

    /// Sync branch selection to the first branch of the given node
    fn sync_branch_selection_to_node(&mut self, node_idx: usize) {
        self.selected_branch_position = self
            .branch_positions
            .iter()
            .position(|(idx, _)| *idx == node_idx);
    }

    /// Move to the next branch (across all commits)
    fn move_to_next_branch(&mut self) {
        if self.branch_positions.is_empty() {
            return;
        }

        let next = match self.selected_branch_position {
            Some(pos) => {
                if pos + 1 < self.branch_positions.len() {
                    pos + 1
                } else {
                    return; // Already at the last branch
                }
            }
            None => 0, // No branch selected, select the first one
        };

        self.selected_branch_position = Some(next);
        if let Some((node_idx, _)) = self.branch_positions.get(next) {
            self.graph_list_state.select(Some(*node_idx));
        }
    }

    /// Move to the previous branch (across all commits)
    fn move_to_prev_branch(&mut self) {
        if self.branch_positions.is_empty() {
            return;
        }

        let prev = match self.selected_branch_position {
            Some(pos) => {
                if pos > 0 {
                    pos - 1
                } else {
                    return; // Already at the first branch
                }
            }
            None => self.branch_positions.len() - 1, // No branch selected, select the last one
        };

        self.selected_branch_position = Some(prev);
        if let Some((node_idx, _)) = self.branch_positions.get(prev) {
            self.graph_list_state.select(Some(*node_idx));
        }
    }

    /// Move to the left branch within the same commit
    fn move_branch_left(&mut self) {
        let Some(pos) = self.selected_branch_position else {
            return;
        };
        if pos == 0 {
            return;
        }

        let Some((current_node, _)) = self.branch_positions.get(pos) else {
            return;
        };
        let Some((prev_node, _)) = self.branch_positions.get(pos - 1) else {
            return;
        };

        // Only move within the same commit
        if current_node == prev_node {
            self.selected_branch_position = Some(pos - 1);
        }
    }

    /// Move to the right branch within the same commit
    fn move_branch_right(&mut self) {
        let Some(pos) = self.selected_branch_position else {
            return;
        };
        if pos + 1 >= self.branch_positions.len() {
            return;
        }

        let Some((current_node, _)) = self.branch_positions.get(pos) else {
            return;
        };
        let Some((next_node, _)) = self.branch_positions.get(pos + 1) else {
            return;
        };

        // Only move within the same commit
        if current_node == next_node {
            self.selected_branch_position = Some(pos + 1);
        }
    }

    /// Get the currently selected branch
    fn selected_branch(&self) -> Option<&BranchInfo> {
        let (_, branch_name) = self
            .selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))?;
        self.branches.iter().find(|b| &b.name == branch_name)
    }

    /// Get the name of the currently selected branch
    pub fn selected_branch_name(&self) -> Option<&str> {
        self.selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))
            .map(|(_, name)| name.as_str())
    }

    fn selected_commit_node(&self) -> Option<&crate::git::graph::GraphNode> {
        self.graph_list_state
            .selected()
            .and_then(|i| self.graph_layout.nodes.get(i))
    }

    fn do_checkout(&mut self) -> Result<()> {
        if let Some(branch) = self.selected_branch() {
            let branch_name = branch.name.clone();
            if branch_name.starts_with("origin/") {
                // For remote branches, create a local branch and check it out
                checkout_remote_branch(&self.repo.repo, &branch_name)?;
            } else {
                checkout_branch(&self.repo.repo, &branch_name)?;
            }
            self.refresh()?;
        } else if let Some(node) = self.selected_commit_node() {
            if let Some(commit) = &node.commit {
                checkout_commit(&self.repo.repo, commit.oid)?;
                self.refresh()?;
            }
        }
        Ok(())
    }

    /// Build a flat list of (node_index, branch_name) for all branches
    /// Excludes remote branches that have a matching local branch (e.g., origin/main when main exists)
    fn build_branch_positions(graph_layout: &GraphLayout) -> Vec<(usize, String)> {
        use std::collections::HashSet;

        graph_layout
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(node_idx, node)| {
                // Collect local branch names in this node
                let local_branches: HashSet<&str> = node
                    .branch_names
                    .iter()
                    .filter(|n| !n.starts_with("origin/"))
                    .map(|s| s.as_str())
                    .collect();

                // Filter out remote branches that have a matching local branch
                node.branch_names
                    .iter()
                    .filter(move |name| {
                        if let Some(local_name) = name.strip_prefix("origin/") {
                            // Exclude if there's a matching local branch
                            !local_branches.contains(local_name)
                        } else {
                            true // Keep all local branches
                        }
                    })
                    .map(move |name| (node_idx, name.clone()))
            })
            .collect()
    }
}
