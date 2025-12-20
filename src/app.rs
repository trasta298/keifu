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
        operations::{checkout_branch, checkout_commit, checkout_remote_branch, create_branch, delete_branch, merge_branch, rebase_branch},
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

    // Diff cache (async load)
    diff_cache: Option<CommitDiffInfo>,
    diff_cache_oid: Option<Oid>,
    diff_loading_oid: Option<Oid>,
    diff_receiver: Option<Receiver<DiffResult>>,

    // Flags
    pub should_quit: bool,
    pub message: Option<String>,
}

impl App {
    /// Create a new application
    pub fn new() -> Result<Self> {
        let repo = GitRepository::discover()?;
        let repo_path = repo.path.clone();
        let head_name = repo.head_name();

        let commits = repo.get_commits(500)?;
        let branches = repo.get_branches()?;
        let graph_layout = build_graph(&commits, &branches);

        let mut graph_list_state = ListState::default();
        graph_list_state.select(Some(0));

        Ok(Self {
            mode: AppMode::Normal,
            repo,
            repo_path,
            head_name,
            commits,
            branches,
            graph_layout,
            graph_list_state,
            diff_cache: None,
            diff_cache_oid: None,
            diff_loading_oid: None,
            diff_receiver: None,
            should_quit: false,
            message: None,
        })
    }

    /// Refresh repository data
    pub fn refresh(&mut self) -> Result<()> {
        self.commits = self.repo.get_commits(500)?;
        self.branches = self.repo.get_branches()?;
        self.graph_layout = build_graph(&self.commits, &self.branches);
        self.head_name = self.repo.head_name();

        // Clear cache
        self.diff_cache = None;
        self.diff_cache_oid = None;
        self.diff_loading_oid = None;
        self.diff_receiver = None;

        // Clamp the selection
        let max_commit = self.graph_layout.nodes.len().saturating_sub(1);
        if let Some(selected) = self.graph_list_state.selected() {
            if selected > max_commit {
                self.graph_list_state.select(Some(max_commit));
            }
        }

        Ok(())
    }

    /// Update diff info for the selected commit (async)
    pub fn update_diff_cache(&mut self) {
        // Pull in completed results, if any
        if let Some(ref receiver) = self.diff_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.diff_cache = result.diff;
                self.diff_cache_oid = Some(result.oid);
                self.diff_loading_oid = None;
                self.diff_receiver = None;
            }
        }

        let selected_oid = self.graph_list_state.selected().and_then(|idx| {
            self.graph_layout
                .nodes
                .get(idx)
                .and_then(|node| node.commit.as_ref().map(|c| c.oid))
        });

        let Some(oid) = selected_oid else {
            return;
        };

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

    /// Get cached diff info
    pub fn cached_diff(&self) -> Option<&CommitDiffInfo> {
        self.diff_cache.as_ref()
    }

    /// Whether diff is currently loading
    pub fn is_diff_loading(&self) -> bool {
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
                self.jump_to_next_branch();
            }
            Action::PrevBranch => {
                self.jump_to_prev_branch();
            }
            Action::ToggleHelp => {
                self.mode = AppMode::Help;
            }
            Action::Refresh => {
                self.refresh()?;
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
            AppMode::Input { title, input, action } => {
                (title.clone(), input.clone(), action.clone())
            }
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
    }

    fn select_first(&mut self) {
        self.graph_list_state.select(Some(0));
    }

    fn select_last(&mut self) {
        let max = self.graph_layout.nodes.len().saturating_sub(1);
        self.graph_list_state.select(Some(max));
    }

    /// Jump to the next commit that has a branch
    fn jump_to_next_branch(&mut self) {
        let current = self.graph_list_state.selected().unwrap_or(0);
        let nodes = &self.graph_layout.nodes;

        // Find the next node after the current position that has a branch name
        if let Some((i, _)) = nodes
            .iter()
            .enumerate()
            .skip(current + 1)
            .find(|(_, node)| !node.branch_names.is_empty())
        {
            self.graph_list_state.select(Some(i));
        }
    }

    /// Jump to the previous commit that has a branch
    fn jump_to_prev_branch(&mut self) {
        let current = self.graph_list_state.selected().unwrap_or(0);
        let nodes = &self.graph_layout.nodes;

        // Search backward for a node before the current position that has a branch name
        if let Some((i, _)) = nodes
            .iter()
            .enumerate()
            .take(current)
            .rev()
            .find(|(_, node)| !node.branch_names.is_empty())
        {
            self.graph_list_state.select(Some(i));
        }
    }

    /// Get the branch associated with the selected commit
    fn selected_branch(&self) -> Option<&BranchInfo> {
        let node = self.selected_commit_node()?;
        let branch_name = node.branch_names.first()?;
        self.branches.iter().find(|b| &b.name == branch_name)
    }

    fn selected_commit_node(&self) -> Option<&crate::git::graph::GraphNode> {
        self.graph_list_state
            .selected()
            .and_then(|i| self.graph_layout.nodes.get(i))
    }

    fn do_checkout(&mut self) -> Result<()> {
        if let Some(node) = self.selected_commit_node() {
            // Checkout a branch if present, otherwise checkout the commit
            if let Some(branch_name) = node.branch_names.first() {
                if branch_name.starts_with("origin/") {
                    // For remote branches, create a local branch and check it out
                    checkout_remote_branch(&self.repo.repo, branch_name)?;
                } else {
                    checkout_branch(&self.repo.repo, branch_name)?;
                }
                self.refresh()?;
            } else if let Some(commit) = &node.commit {
                checkout_commit(&self.repo.repo, commit.oid)?;
                self.refresh()?;
            }
        }
        Ok(())
    }
}
