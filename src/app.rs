//! Application state management

use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use anyhow::Result;
use ratatui::widgets::ListState;

use git2::Oid;

use crate::{
    action::Action,
    config::Config,
    git::{
        build_graph,
        build_horizontal_graph,
        graph::{
            GraphLayout,
            GraphOrientation,
            HorizontalGraphLayout,
            CompressionMode,
        },
        operations::{
            checkout_branch, checkout_commit, checkout_remote_branch, create_branch, delete_branch,
            fetch_origin, merge_branch, rebase_branch,
        },
        BranchInfo, CommitDiffInfo, CommitInfo, GitRepository, TagInfo, WorkingTreeStatus,
    },
    search::{fuzzy_search_branches, FuzzySearchResult},
};

/// Filter branch names to exclude remote branches that have matching local branches
/// Returns branches in order: local branches first, then remote-only branches
fn filter_remote_duplicates(branch_names: &[String]) -> Vec<&str> {
    use std::collections::HashSet;

    let local_branches: HashSet<&str> = branch_names
        .iter()
        .filter(|n| !n.starts_with("origin/"))
        .map(|s| s.as_str())
        .collect();

    branch_names
        .iter()
        .filter(|name| {
            if let Some(local_name) = name.strip_prefix("origin/") {
                !local_branches.contains(local_name)
            } else {
                true
            }
        })
        .map(|s| s.as_str())
        .collect()
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Search state for branch search feature
#[derive(Debug, Clone, Default)]
struct SearchState {
    /// Fuzzy search results (sorted by score)
    fuzzy_matches: Vec<FuzzySearchResult>,
    /// Selected index in the dropdown (None if no results)
    dropdown_selection: Option<usize>,
    /// Position before search started (for cancel restoration)
    original_position: Option<usize>,
    /// Original node selection before search started
    original_node: Option<usize>,
}

/// Represents the currently selected node in the graph (abstraction over Vertical/Horizontal)
#[derive(Debug)]
pub enum SelectedNode<'a> {
    Commit(&'a CommitInfo),
    Uncommitted { count: usize },
    None,
}

impl SearchState {
    /// Move selection up in the dropdown (with wrap-around)
    fn select_up(&mut self) {
        if self.fuzzy_matches.is_empty() {
            return;
        }
        self.dropdown_selection = Some(match self.dropdown_selection {
            Some(0) | None => self.fuzzy_matches.len() - 1,
            Some(idx) => idx - 1,
        });
    }

    /// Move selection down in the dropdown (with wrap-around)
    fn select_down(&mut self) {
        if self.fuzzy_matches.is_empty() {
            return;
        }
        let last_idx = self.fuzzy_matches.len() - 1;
        self.dropdown_selection = Some(match self.dropdown_selection {
            Some(idx) if idx < last_idx => idx + 1,
            _ => 0,
        });
    }

    /// Get the currently selected result
    fn selected_result(&self) -> Option<&FuzzySearchResult> {
        self.dropdown_selection
            .and_then(|idx| self.fuzzy_matches.get(idx))
    }

    /// Clamp dropdown selection to valid range after results update
    fn clamp_selection(&mut self) {
        if self.fuzzy_matches.is_empty() {
            self.dropdown_selection = None;
        } else if let Some(idx) = self.dropdown_selection {
            if idx >= self.fuzzy_matches.len() {
                self.dropdown_selection = Some(self.fuzzy_matches.len() - 1);
            }
        } else {
            // Auto-select first result if we have results
            self.dropdown_selection = Some(0);
        }
    }
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
    pub tags: Vec<TagInfo>,
    pub graph_layout: GraphLayout,

    // Horizontal layout state
    pub horizontal_layout: Option<HorizontalGraphLayout>,
    pub current_orientation: GraphOrientation,
    /// Cached terminal width used when building horizontal layout (for resize detection)
    pub horizontal_graph_width: usize,
    /// Whether to show tags in horizontal view
    pub show_tags: bool,
    /// Whether to show the sidebar (legend) in horizontal view
    pub show_sidebar: bool,
    /// Horizontal graph compression mode
    pub compression_mode: CompressionMode,

    // UI state
    pub graph_list_state: ListState,

    // Branch selection state
    /// List of (node_index, branch_name) for all branches
    pub branch_positions: Vec<(usize, String)>,
    /// Currently selected branch position index
    pub selected_branch_position: Option<usize>,

    // Search state
    search_state: SearchState,

    // Diff cache (async load)
    diff_cache: Option<CommitDiffInfo>,
    diff_cache_oid: Option<Oid>,
    diff_loading_oid: Option<Oid>,
    diff_receiver: Option<Receiver<DiffResult>>,

    // Uncommitted diff cache
    uncommitted_diff_cache: Option<CommitDiffInfo>,
    uncommitted_diff_loading: bool,
    uncommitted_diff_receiver: Option<Receiver<Option<CommitDiffInfo>>>,
    /// Cache key: working tree status at the time of caching (for invalidation)
    uncommitted_cache_key: Option<WorkingTreeStatus>,

    // Flags
    pub should_quit: bool,

    // Status message with auto-clear
    message: Option<String>,
    message_time: Option<std::time::Instant>,

    // Async fetch
    fetch_receiver: Option<Receiver<Result<(), String>>>,
    /// Whether to suppress error dialogs for fetch failures (for auto-fetch)
    fetch_silent: bool,

    // Auto-refresh state
    config: Config,
    last_refresh_time: Instant,
    last_fetch_time: Instant,
}

impl App {
    /// Create a new application
    /// Uses the default orientation from config
    pub fn new() -> Result<Self> {
        Self::new_with_orientation(None)
    }

    /// Create a new application with optional orientation
    /// Uses CLI override if provided, otherwise falls back to config
    pub fn new_with_orientation(cli_orientation: Option<GraphOrientation>) -> Result<Self> {
        let config = Config::load();
        let now = Instant::now();

        let repo = GitRepository::discover()?;
        let repo_path = repo.path.clone();
        let head_name = repo.head_name();

        // Determine orientation: CLI override -> config -> default
        let orientation = cli_orientation.unwrap_or(config.graph.orientation);
        let uncommitted_count = repo
            .get_working_tree_status()
            .ok()
            .flatten()
            .map(|s| s.file_count);
        let head_commit_oid = repo.head_oid();

        let commits = repo.get_commits(500)?;
        let branches = repo.get_branches()?;
        let tags = repo.get_tags().unwrap_or_default();

        // Build graph layout based on orientation
        let graph_layout = build_graph(&commits, &branches, uncommitted_count, head_commit_oid);

        let mut graph_list_state = ListState::default();
        graph_list_state.select(Some(0));

        // Build branch positions
        let branch_positions = Self::build_branch_positions(&graph_layout);

        // Determine initial branch selection
        // If uncommitted node exists (at index 0), don't select any branch
        // Otherwise, select the first branch if exists
        let has_uncommitted_node = graph_layout
            .nodes
            .first()
            .is_some_and(|node| node.is_uncommitted);
        let selected_branch_position = if has_uncommitted_node || branch_positions.is_empty() {
            None
        } else {
            Some(0)
        };

        // Create horizontal layout if orientation is Horizontal
        let horizontal_layout = if orientation == GraphOrientation::Horizontal {
            Some(build_horizontal_graph(
                &commits,
                &branches,
                &tags,
                uncommitted_count,
                head_commit_oid,
                80, // Default terminal width
                CompressionMode::default(),
            ))
        } else {
            None
        };

        Ok(Self {
            mode: AppMode::Normal,
            repo,
            repo_path,
            head_name,
            commits,
            branches,
            tags,
            graph_layout,
            horizontal_layout,
            current_orientation: orientation,
            horizontal_graph_width: 80, // Initial default, updated on first render
            show_tags: true, // Tags are shown by default
            show_sidebar: true, // Sidebar is shown by default
            compression_mode: CompressionMode::default(),
            graph_list_state,
            branch_positions,
            selected_branch_position,
            search_state: SearchState::default(),
            diff_cache: None,
            diff_cache_oid: None,
            diff_loading_oid: None,
            diff_receiver: None,
            uncommitted_diff_cache: None,
            uncommitted_diff_loading: false,
            uncommitted_diff_receiver: None,
            uncommitted_cache_key: None,
            should_quit: false,
            message: None,
            message_time: None,
            fetch_receiver: None,
            fetch_silent: false,
            config,
            last_refresh_time: now,
            last_fetch_time: now,
        })
    }

    /// Clear all diff caches
    fn clear_all_diff_caches(&mut self) {
        self.diff_cache = None;
        self.diff_cache_oid = None;
        self.diff_loading_oid = None;
        self.diff_receiver = None;
        self.clear_uncommitted_diff_cache();
    }

    /// Clear uncommitted diff cache only
    fn clear_uncommitted_diff_cache(&mut self) {
        self.uncommitted_diff_cache = None;
        self.uncommitted_diff_loading = false;
        self.uncommitted_diff_receiver = None;
        self.uncommitted_cache_key = None;
    }

    /// Update horizontal layout if terminal width changed
    pub fn update_horizontal_layout_width(&mut self, new_width: usize) {
        if self.current_orientation == GraphOrientation::Horizontal 
            && new_width != self.horizontal_graph_width 
        {
            self.horizontal_graph_width = new_width;
            
            // Rebuild horizontal layout with new width
            let uncommitted_count = self
                .repo
                .get_working_tree_status()
                .ok()
                .flatten()
                .map(|s| s.file_count);
            let head_commit_oid = self.repo.head_oid();

            self.horizontal_layout = Some(build_horizontal_graph(
                &self.commits,
                &self.branches,
                &self.tags,
                uncommitted_count,
                head_commit_oid,
                new_width,
                self.compression_mode,
            ));
        }
    }

    /// Refresh repository data
    /// If `force` is true, always clears diff cache (for manual refresh)
    /// If `force` is false, keeps cache when the same content is selected (for auto-refresh)
    pub fn refresh(&mut self, force: bool) -> Result<()> {
        // Save the current selection state for restoration
        let was_uncommitted_selected = self
            .graph_list_state
            .selected()
            .and_then(|idx| self.graph_layout.nodes.get(idx))
            .is_some_and(|node| node.is_uncommitted);

        let prev_branch_name = self
            .selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))
            .map(|(_, name)| name.clone());

        // Get working tree status once and reuse
        let working_tree_status = self.repo.get_working_tree_status().ok().flatten();
        let uncommitted_count = working_tree_status.as_ref().map(|s| s.file_count);

        self.commits = self.repo.get_commits(500)?;
        self.branches = self.repo.get_branches()?;
        self.tags = self.repo.get_tags().unwrap_or_default();
        let head_commit_oid = self.repo.head_oid();
        self.graph_layout = build_graph(
            &self.commits,
            &self.branches,
            uncommitted_count,
            head_commit_oid,
        );
        self.head_name = self.repo.head_name();

        // Rebuild branch positions
        self.branch_positions = Self::build_branch_positions(&self.graph_layout);

        // Restore selection state
        // Check if uncommitted node still exists in the new graph
        let has_uncommitted_node = self
            .graph_layout
            .nodes
            .first()
            .is_some_and(|node| node.is_uncommitted);

        if was_uncommitted_selected && has_uncommitted_node {
            // Restore uncommitted node selection
            self.graph_list_state.select(Some(0));
            self.selected_branch_position = None;
        } else {
            // Restore branch selection if the branch still exists
            self.selected_branch_position = prev_branch_name
                .and_then(|name| self.branch_positions.iter().position(|(_, n)| n == &name));

            // Sync node selection with branch selection
            if let Some(pos) = self.selected_branch_position {
                if let Some((node_idx, _)) = self.branch_positions.get(pos) {
                    self.graph_list_state.select(Some(*node_idx));
                }
            }
        }

        // Handle diff cache based on force flag
        if force {
            self.clear_all_diff_caches();
        } else {
            // Auto-refresh: smart cache - only clear if selection changed
            let selected_oid = self
                .graph_list_state
                .selected()
                .and_then(|idx| self.graph_layout.nodes.get(idx))
                .and_then(|n| n.commit.as_ref())
                .map(|c| c.oid);

            // Keep commit diff cache if the same commit is still selected
            if self.diff_cache_oid != selected_oid {
                self.diff_cache = None;
                self.diff_cache_oid = None;
                self.diff_loading_oid = None;
                self.diff_receiver = None;
            }

            // Keep uncommitted diff cache only if:
            // 1. Uncommitted node is still selected (was_uncommitted_selected && has_uncommitted_node)
            // 2. The working tree status hasn't changed (same files and mtimes)
            let uncommitted_still_selected = was_uncommitted_selected && has_uncommitted_node;
            if !uncommitted_still_selected || self.uncommitted_cache_key != working_tree_status {
                self.clear_uncommitted_diff_cache();
            }
        }

        // Clear search state on refresh to avoid stale indices
        // Skip if in search mode to prevent clearing active search results
        if !self.is_in_search_mode() {
            self.search_state = SearchState::default();
        }

        // Clamp the selection
        let max_commit = self.graph_layout.nodes.len().saturating_sub(1);
        if let Some(selected) = self.graph_list_state.selected() {
            if selected > max_commit {
                self.graph_list_state.select(Some(max_commit));
            }
        }

        Ok(())
    }

    /// Update fuzzy search results for the given query
    fn update_fuzzy_search(&mut self, query: &str) {
        self.search_state.fuzzy_matches = fuzzy_search_branches(query, &self.branch_positions);
        self.search_state.clamp_selection();
    }

    /// Jump to the currently selected search result
    fn jump_to_search_result(&mut self) {
        let Some(result) = self.search_state.selected_result() else {
            return;
        };
        let branch_idx = result.branch_idx;
        let Some((node_idx, _)) = self.branch_positions.get(branch_idx) else {
            return;
        };

        self.selected_branch_position = Some(branch_idx);
        self.graph_list_state.select(Some(*node_idx));
    }

    /// Save current position before starting search
    fn save_search_position(&mut self) {
        self.search_state.original_position = self.selected_branch_position;
        self.search_state.original_node = self.graph_list_state.selected();
    }

    /// Restore position saved before search (for cancel)
    fn restore_search_position(&mut self) {
        self.selected_branch_position = self.search_state.original_position;
        if let Some(node) = self.search_state.original_node {
            self.graph_list_state.select(Some(node));
        }
    }

    /// Get current search results for UI rendering
    pub fn search_results(&self) -> &[FuzzySearchResult] {
        &self.search_state.fuzzy_matches
    }

    /// Get current dropdown selection index
    pub fn search_selection(&self) -> Option<usize> {
        self.search_state.dropdown_selection
    }

    /// Check if currently in search input mode
    pub fn is_in_search_mode(&self) -> bool {
        matches!(
            &self.mode,
            AppMode::Input {
                action: InputAction::Search,
                ..
            }
        )
    }

    /// Jump to the currently checked out branch (HEAD)
    fn jump_to_head(&mut self) {
        // Find the HEAD branch name
        let Some(head_name) = &self.head_name else {
            return;
        };

        // Find the branch position index that matches HEAD
        let Some((branch_pos_idx, (node_idx, _))) = self
            .branch_positions
            .iter()
            .enumerate()
            .find(|(_, (_, name))| name == head_name)
        else {
            return;
        };

        self.selected_branch_position = Some(branch_pos_idx);
        self.graph_list_state.select(Some(*node_idx));
    }

    /// Check if async fetch has completed and process the result
    pub fn update_fetch_status(&mut self) {
        let Some(rx) = &self.fetch_receiver else {
            return;
        };
        let Ok(fetch_result) = rx.try_recv() else {
            return;
        };

        let silent = self.fetch_silent;
        self.fetch_receiver = None;
        self.fetch_silent = false;

        match fetch_result {
            Ok(()) => {
                self.reset_timers();
                match self.refresh(true) {
                    Ok(()) => self.set_message("Fetched from origin"),
                    Err(e) => self.show_error(format!("Refresh failed: {e}")),
                }
            }
            Err(e) if !silent => self.show_error(e),
            Err(_) => {} // Silent mode: suppress error dialog for auto-fetch
        }
    }

    /// Check if fetch is currently in progress
    pub fn is_fetching(&self) -> bool {
        self.fetch_receiver.is_some()
    }

    /// Check and perform auto-refresh if interval has elapsed
    pub fn check_auto_refresh(&mut self) {
        if self.is_fetching() {
            return;
        }

        let now = Instant::now();
        let refresh_config = &self.config.refresh;

        // Auto-fetch (check first as it includes refresh)
        if refresh_config.auto_fetch
            && now.duration_since(self.last_fetch_time).as_secs() >= refresh_config.fetch_interval
        {
            self.start_fetch(false, true); // silent=true for auto-fetch
            return;
        }

        // Auto-refresh
        if refresh_config.auto_refresh
            && now.duration_since(self.last_refresh_time).as_secs()
                >= refresh_config.refresh_interval
        {
            let _ = self.refresh(false);
            self.last_refresh_time = now;
        }
    }

    /// Start fetch in background
    /// If `show_message` is true, displays "Fetching from origin..."
    /// If `silent` is true, errors will not show a dialog (for auto-fetch)
    fn start_fetch(&mut self, show_message: bool, silent: bool) {
        let (tx, rx) = mpsc::channel();
        let repo_path = self.repo_path.clone();

        thread::spawn(move || {
            let result = fetch_origin(&repo_path).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        self.fetch_receiver = Some(rx);
        self.fetch_silent = silent;
        if show_message {
            self.set_message("Fetching from origin...");
        }
    }

    /// Reset both timers (call after manual refresh/fetch)
    fn reset_timers(&mut self) {
        let now = Instant::now();
        self.last_refresh_time = now;
        self.last_fetch_time = now;
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

        let msg = self.message.as_deref()?;
        let time = self.message_time.as_ref()?;

        if time.elapsed().as_secs() < MESSAGE_TIMEOUT_SECS {
            Some(msg)
        } else {
            None
        }
    }

    /// Get search match count
    pub fn search_match_count(&self) -> usize {
        self.search_state.fuzzy_matches.len()
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

        // Check selection
        let selected = self.get_selected_node();

        // Handle selected node type
        match selected {
            SelectedNode::Uncommitted { .. } => {
                // Do nothing if cache exists or already loading
                if self.uncommitted_diff_cache.is_some() || self.uncommitted_diff_loading {
                    return;
                }

                // Compute uncommitted diff in the background
                let (tx, rx) = mpsc::channel();
                let repo_path = self.repo_path.clone();

                // Save current working tree status as cache key before starting computation
                self.uncommitted_cache_key = self.repo.get_working_tree_status().ok().flatten();

                self.uncommitted_diff_loading = true;
                self.uncommitted_diff_receiver = Some(rx);

                thread::spawn(move || {
                    let diff = git2::Repository::open(&repo_path)
                        .ok()
                        .and_then(|repo| CommitDiffInfo::from_working_tree(&repo).ok());

                    let _ = tx.send(diff);
                });
            }
            SelectedNode::Commit(commit) => {
                let oid = commit.oid;

                // Do nothing if the cache is valid
                if self.diff_cache_oid == Some(oid) {
                    return;
                }

                // Do nothing if already loading this OID
                if self.diff_loading_oid == Some(oid) {
                    return;
                }

                // Start loading diff
                self.diff_cache_oid = None;
                self.diff_cache = None;
                self.diff_loading_oid = Some(oid);

                let (tx, rx) = mpsc::channel();
                let repo_path = self.repo_path.clone(); // Use configured path
                let oid_copy = oid;

                thread::spawn(move || {
                    let result = match git2::Repository::open(&repo_path) {
                        Ok(repo) => {
                            let diff = CommitDiffInfo::from_commit(&repo, oid_copy).ok();
                            DiffResult {
                                oid: oid_copy,
                                diff,
                            }
                        }
                        Err(_) => DiffResult {
                            oid: oid_copy,
                            diff: None,
                        },
                    };
                    let _ = tx.send(result);
                });

                self.diff_receiver = Some(rx);
            }
            SelectedNode::None => {}
        }
    }

    /// Get cached diff info for the currently selected node
    pub fn cached_diff(&self) -> Option<&CommitDiffInfo> {
        match self.get_selected_node() {
            SelectedNode::Uncommitted { .. } => self.uncommitted_diff_cache.as_ref(),
            SelectedNode::Commit(_) => self.diff_cache.as_ref(),
            SelectedNode::None => None,
        }
    }

    /// Whether diff is currently loading for the selected node
    pub fn is_diff_loading(&self) -> bool {
        match self.get_selected_node() {
            SelectedNode::Uncommitted { .. } => self.uncommitted_diff_loading,
            SelectedNode::Commit(_) => self.diff_loading_oid.is_some(),
            SelectedNode::None => false,
        }
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
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_selection(-1),
                    GraphOrientation::Horizontal => self.move_selection_up(),
                }
            }
            Action::MoveDown => {
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_selection(1),
                    GraphOrientation::Horizontal => self.move_selection_down(),
                }
            }
            Action::PageUp => {
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_selection(-10),
                    GraphOrientation::Horizontal => self.prev_chunk(),
                }
            }
            Action::PageDown => {
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_selection(10),
                    GraphOrientation::Horizontal => self.next_chunk(),
                }
            }
            Action::GoToTop => {
                self.select_first();
            }
            Action::GoToBottom => {
                self.select_last();
            }
            Action::JumpToHead => {
                self.jump_to_head();
            }
            Action::NextBranch => {
                self.move_to_next_branch();
            }
            Action::PrevBranch => {
                self.move_to_prev_branch();
            }
            Action::MoveLeft => {
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_branch_left(),
                    GraphOrientation::Horizontal => self.move_selection_left(),
                }
            }
            Action::MoveRight => {
                match self.current_orientation {
                    GraphOrientation::Vertical => self.move_branch_right(),
                    GraphOrientation::Horizontal => self.move_selection_right(),
                }
            }

            // Horizontal navigation (context-sensitive based on orientation)
            Action::MoveHorizontalLeft => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.move_selection_left();
                } else {
                    self.move_selection(-1);
                }
            }
            Action::MoveHorizontalRight => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.move_selection_right();
                } else {
                    self.move_selection(1);
                }
            }
            Action::MoveHorizontalUp => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.move_selection_up();
                } else {
                    self.move_selection(-1);
                }
            }
            Action::MoveHorizontalDown => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.move_selection_down();
                } else {
                    self.move_selection(1);
                }
            }
            Action::HorizontalPrevChunk => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.prev_chunk();
                } else {
                    self.move_selection(-10);
                }
            }
            Action::HorizontalNextChunk => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.next_chunk();
                } else {
                    self.move_selection(10);
                }
            }
            Action::ToggleHelp => {
                self.mode = AppMode::Help;
            }
            Action::ToggleOrientation => {
                self.toggle_orientation();
            }
            Action::ToggleTags => {
                self.show_tags = !self.show_tags;
                if self.show_tags {
                    self.set_message("Tags: ON");
                } else {
                    self.set_message("Tags: OFF");
                }
            }
            Action::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                if self.show_sidebar {
                    self.set_message("Sidebar: ON");
                } else {
                    self.set_message("Sidebar: OFF");
                }
            }
            Action::ToggleCompression => {
                if self.current_orientation == GraphOrientation::Horizontal {
                    self.compression_mode = self.compression_mode.next();
                    self.set_message(format!("Compression: {:?}", self.compression_mode));
                    
                    // Rebuild layout
                    let uncommitted_count = self
                        .repo
                        .get_working_tree_status()
                        .ok()
                        .flatten()
                        .map(|s| s.file_count);
                    let head_commit_oid = self.repo.head_oid();

                    self.horizontal_layout = Some(build_horizontal_graph(
                        &self.commits,
                        &self.branches,
                        &self.tags,
                        uncommitted_count,
                        head_commit_oid,
                        self.horizontal_graph_width,
                        self.compression_mode,
                    ));
                    
                    // We might need to adjust selection if it was on a compressed node, 
                    // but since compressed nodes aren't selectable (they are just dots), 
                    // we should be fine or snap to nearest.
                    self.snap_to_nearest_commit();
                } else {
                    self.set_message("Compression only available in horizontal mode");
                }
            }
            Action::Refresh => {
                self.refresh(true)?;
                self.reset_timers();
            }
            Action::Fetch => {
                if !self.is_fetching() {
                    self.start_fetch(true, false); // silent=false for manual fetch
                }
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
            Action::Search => {
                // Save position for cancel restoration
                self.save_search_position();
                self.mode = AppMode::Input {
                    title: "Search branches".to_string(),
                    input: String::new(),
                    action: InputAction::Search,
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
        let AppMode::Input {
            title,
            input,
            action: input_action,
        } = &self.mode
        else {
            return Ok(());
        };
        let (title, mut input, input_action) = (title.clone(), input.clone(), input_action.clone());

        match action {
            Action::Confirm => {
                match input_action {
                    InputAction::CreateBranch => {
                        if !input.is_empty() {
                            if let Some(node) = self.selected_commit_node() {
                                if let Some(commit) = &node.commit {
                                    create_branch(&self.repo.repo, &input, commit.oid)?;
                                    self.refresh(true)?;
                                }
                            }
                        }
                    }
                    InputAction::Search => {
                        // Jump to selected result and exit search mode
                        self.jump_to_search_result();
                    }
                }
                // Clear search state after confirming
                self.search_state = SearchState::default();
                self.mode = AppMode::Normal;
            }
            Action::Cancel => {
                // Restore position when canceling search
                if matches!(input_action, InputAction::Search) {
                    self.restore_search_position();
                }
                self.search_state = SearchState::default();
                self.mode = AppMode::Normal;
            }
            Action::InputChar(c) => {
                input.push(c);

                // Incremental fuzzy search with live preview
                if matches!(input_action, InputAction::Search) {
                    self.update_fuzzy_search(&input);
                    self.jump_to_search_result();
                }

                self.mode = AppMode::Input {
                    title,
                    input,
                    action: input_action,
                };
            }
            Action::InputBackspace => {
                // Empty input + backspace = cancel (like Esc)
                if input.is_empty() {
                    if matches!(input_action, InputAction::Search) {
                        self.restore_search_position();
                    }
                    self.search_state = SearchState::default();
                    self.mode = AppMode::Normal;
                    return Ok(());
                }

                input.pop();

                // Update fuzzy search on backspace with live preview
                if matches!(input_action, InputAction::Search) {
                    self.update_fuzzy_search(&input);
                    self.jump_to_search_result();
                }

                self.mode = AppMode::Input {
                    title,
                    input,
                    action: input_action,
                };
            }
            Action::SearchSelectUp => {
                self.search_state.select_up();
                self.jump_to_search_result();
            }
            Action::SearchSelectDown => {
                self.search_state.select_down();
                self.jump_to_search_result();
            }
            Action::SearchSelectUpQuiet => {
                self.search_state.select_up();
                // No graph jump - just move in dropdown
            }
            Action::SearchSelectDownQuiet => {
                self.search_state.select_down();
                // No graph jump - just move in dropdown
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_action(&mut self, action: Action) -> Result<()> {
        let AppMode::Confirm {
            action: confirm_action,
            ..
        } = &self.mode
        else {
            return Ok(());
        };
        let confirm_action = confirm_action.clone();

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
                self.refresh(true)?;
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

    /// Move to an adjacent branch within the same commit
    fn move_branch_within_node(&mut self, delta: isize) {
        let Some(pos) = self.selected_branch_position else {
            return;
        };

        let new_pos = (pos as isize + delta) as usize;
        if new_pos >= self.branch_positions.len() {
            return;
        }

        let Some((current_node, _)) = self.branch_positions.get(pos) else {
            return;
        };
        let Some((target_node, _)) = self.branch_positions.get(new_pos) else {
            return;
        };

        // Only move within the same commit
        if current_node == target_node {
            self.selected_branch_position = Some(new_pos);
        }
    }

    /// Move to the left branch within the same commit
    fn move_branch_left(&mut self) {
        self.move_branch_within_node(-1);
    }

    /// Move to the right branch within the same commit
    fn move_branch_right(&mut self) {
        self.move_branch_within_node(1);
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

    /// Returns all branch names for the currently selected node
    pub fn selected_node_branches(&self) -> Vec<&str> {
        let Some(node_idx) = self.graph_list_state.selected() else {
            return vec![];
        };
        self.branch_positions
            .iter()
            .filter(|(idx, _)| *idx == node_idx)
            .map(|(_, name)| name.as_str())
            .collect()
    }

    fn selected_commit_node(&self) -> Option<&crate::git::graph::GraphNode> {
        self.graph_list_state
            .selected()
            .and_then(|i| self.graph_layout.nodes.get(i))
    }

    /// Get the currently selected node (works for both Vertical and Horizontal layouts)
    pub fn get_selected_node(&self) -> SelectedNode<'_> {
        match self.current_orientation {
            GraphOrientation::Vertical => {
                if let Some(node) = self.selected_commit_node() {
                    if node.is_uncommitted {
                        SelectedNode::Uncommitted {
                            count: node.uncommitted_count,
                        }
                    } else if let Some(commit) = &node.commit {
                        SelectedNode::Commit(commit)
                    } else {
                        SelectedNode::None
                    }
                } else {
                    SelectedNode::None
                }
            }
            GraphOrientation::Horizontal => {
                if let Some(layout) = &self.horizontal_layout {
                    let sel = layout.selection;
                    if let Some(chunk) = layout.chunks.get(sel.chunk_index) {
                        // Check bounds to be safe
                        if sel.lane < chunk.commits.len() {
                            let lane_commits = &chunk.commits[sel.lane];
                            if sel.column < lane_commits.len() {
                                if let Some(commit) = &lane_commits[sel.column] {
                                    return SelectedNode::Commit(commit);
                                }
                            }
                        }
                    }
                }
                SelectedNode::None
            }
        }
    }

    /// Get names of branches associated with the selected node's lane
    /// For Horizontal: Returns the branches assigned to the current lane
    /// For Vertical: Returns branches pointing to the current commit (tips only)
    pub fn selected_node_lane_branches(&self) -> Vec<String> {
        match self.current_orientation {
            GraphOrientation::Vertical => {
                if let Some(node) = self.selected_commit_node() {
                    node.branch_names.clone()
                } else {
                    Vec::new()
                }
            }
            GraphOrientation::Horizontal => {
                if let Some(layout) = &self.horizontal_layout {
                    let lane_idx = layout.selection.lane;
                    if let Some(lane_info) = layout.lanes.iter().find(|l| l.lane == lane_idx) {
                        return lane_info.branch_names.clone();
                    }
                }
                Vec::new()
            }
        }
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
            self.refresh(true)?;
        } else if let Some(node) = self.selected_commit_node() {
            if let Some(commit) = &node.commit {
                checkout_commit(&self.repo.repo, commit.oid)?;
                self.refresh(true)?;
            }
        }
        Ok(())
    }

    /// Build a flat list of (node_index, branch_name) for all branches
    /// Excludes remote branches that have a matching local branch (e.g., origin/main when main exists)
    /// Order matches optimize_branch_display: local branches first, then remote-only branches
    fn build_branch_positions(graph_layout: &GraphLayout) -> Vec<(usize, String)> {
        graph_layout
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(node_idx, node)| {
                filter_remote_duplicates(&node.branch_names)
                    .into_iter()
                    .map(move |name| (node_idx, name.to_string()))
            })
            .collect()
    }

    // Horizontal layout navigation methods

    /// Helper to check if a commit exists at specific coordinates
    fn is_commit_at(&self, chunk_idx: usize, lane: usize, col: usize) -> bool {
        if let Some(layout) = &self.horizontal_layout {
            if let Some(chunk) = layout.chunks.get(chunk_idx) {
                if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(col)) {
                     return matches!(cell, crate::git::graph::HorizontalCellType::Commit(_));
                }
            }
        }
        false
    }
    
    /// Helper: Snap selection to the nearest commit on the current lane
    fn snap_to_nearest_commit(&mut self) {
        let Some(layout) = &self.horizontal_layout else { return; };
        let lane = layout.selection.lane;
        let chunk_idx = layout.selection.chunk_index;
        let col = layout.selection.column;

        // check current
        if self.is_commit_at(chunk_idx, lane, col) { return; }
        
        // search radius
        let radius = 20; 
        for i in 1..=radius {
             // Check Left
            if col >= i {
                if self.is_commit_at(chunk_idx, lane, col - i) {
                    if let Some(l) = self.horizontal_layout.as_mut() {
                        l.selection.column = col - i;
                    }
                    return;
                }
            }
            // Check Right
            if self.is_commit_at(chunk_idx, lane, col + i) {
                if let Some(l) = self.horizontal_layout.as_mut() {
                    l.selection.column = col + i;
                }
                return;
            }
        }
    }

    /// Move to previous (older) commit (left in horizontal mode)
    /// With reversed chunks: chunk 0 = newest, chunk N = oldest
    /// LEFT arrow goes to older commits = higher chunk index
    pub fn move_selection_left(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            let mut curr_chunk_idx = layout.selection.chunk_index;
            let mut curr_col = layout.selection.column;
            let lane = layout.selection.lane;
            let max_chunk_idx = layout.chunks.len().saturating_sub(1);

            // Search backwards (older) for the next commit cell
            loop {
                if curr_col > 0 {
                    // Move left within current chunk
                    curr_col -= 1;
                } else if curr_chunk_idx < max_chunk_idx {
                    // At left edge of chunk, go to next chunk (older = higher index)
                    curr_chunk_idx += 1;
                    if let Some(chunk) = layout.chunks.get(curr_chunk_idx) {
                        // Start from right side of older chunk
                        curr_col = chunk.cells.get(lane).map(|r| r.len().saturating_sub(1)).unwrap_or(0);
                    } else {
                        break; 
                    }
                } else {
                    break; // Start of history (oldest commits)
                }

                // Check if found commit
                if let Some(chunk) = layout.chunks.get(curr_chunk_idx) {
                    if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(curr_col)) {
                        if matches!(cell, crate::git::graph::HorizontalCellType::Commit(_)) {
                            layout.selection.chunk_index = curr_chunk_idx;
                            layout.selection.column = curr_col;
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Move to next (newer) commit (right in horizontal mode)
    /// With reversed chunks: chunk 0 = newest, chunk N = oldest
    /// RIGHT arrow goes to newer commits = lower chunk index
    pub fn move_selection_right(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            let mut curr_chunk_idx = layout.selection.chunk_index;
            let mut curr_col = layout.selection.column;
            let lane = layout.selection.lane;

            // Search forwards (newer) for the next commit cell
            loop {
                let chunk = match layout.chunks.get(curr_chunk_idx) {
                    Some(c) => c,
                    None => break,
                };
                let chunk_width = chunk.cells.get(lane).map(|r| r.len()).unwrap_or(0);
                
                if curr_col < chunk_width.saturating_sub(1) {
                    // Move right within current chunk
                    curr_col += 1;
                } else if curr_chunk_idx > 0 {
                    // At right edge of chunk, go to previous chunk (newer = lower index)
                    curr_chunk_idx -= 1;
                    curr_col = 0; // Start from left side of newer chunk
                } else {
                    break; // End of history (newest commits)
                }
                
                // Check if found commit
                if let Some(next_chunk) = layout.chunks.get(curr_chunk_idx) {
                     if let Some(cell) = next_chunk.cells.get(lane).and_then(|row| row.get(curr_col)) {
                        if matches!(cell, crate::git::graph::HorizontalCellType::Commit(_)) {
                            layout.selection.chunk_index = curr_chunk_idx;
                            layout.selection.column = curr_col;
                            break;
                        }
                     }
                }
            }
        }
    }

    /// Move to previous lane (up)
    pub fn move_selection_up(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            if layout.selection.lane > 0 {
                layout.selection.lane -= 1;
            }
        }
        self.snap_to_nearest_commit();
    }

    /// Move to next lane (down)
    pub fn move_selection_down(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            if let Some(chunk) = layout.chunks.get(layout.selection.chunk_index) {
                if layout.selection.lane < chunk.lane_count - 1 {
                    layout.selection.lane += 1;
                }
            }
        }
        self.snap_to_nearest_commit();
    }

    /// Page up - previous chunk
    pub fn prev_chunk(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            if layout.selection.chunk_index > 0 {
                layout.selection.chunk_index -= 1;
                layout.selection.column = 0;
            }
        }
    }

    /// Page down - next chunk
    pub fn next_chunk(&mut self) {
        if let Some(ref mut layout) = self.horizontal_layout {
            if layout.selection.chunk_index < layout.chunks.len() - 1 {
                layout.selection.chunk_index += 1;
                layout.selection.column = 0;
            }
        }
    }

    /// Toggle between vertical and horizontal orientation
    pub fn toggle_orientation(&mut self) {
        // Save current selection state
        let selected_oid = self
            .graph_list_state
            .selected()
            .and_then(|idx| self.graph_layout.nodes.get(idx))
            .and_then(|node| node.commit.as_ref())
            .map(|c| c.oid);

        let selected_branch_name = self.selected_branch_name().map(|s| s.to_string());

        // Toggle orientation
        self.current_orientation = match self.current_orientation {
            GraphOrientation::Vertical => GraphOrientation::Horizontal,
            GraphOrientation::Horizontal => GraphOrientation::Vertical,
        };

        // Rebuild layout based on new orientation
        match self.current_orientation {
            GraphOrientation::Vertical => {
                // Clear horizontal layout
                self.horizontal_layout = None;
            }
            GraphOrientation::Horizontal => {
                // Reset cached width to 0 to force rebuild with actual panel width on next render
                // The update_horizontal_layout_width() call in draw_horizontal_layout() 
                // will rebuild with the correct width
                self.horizontal_graph_width = 0;
                
                // Build initial horizontal layout with placeholder width
                // This will be rebuilt immediately on render with correct width
                let uncommitted_count = self
                    .repo
                    .get_working_tree_status()
                    .ok()
                    .flatten()
                    .map(|s| s.file_count);
                let head_commit_oid = self.repo.head_oid();

                self.horizontal_layout = Some(build_horizontal_graph(
                    &self.commits,
                    &self.branches,
                    &self.tags,
                    uncommitted_count,
                    head_commit_oid,
                    80, // Placeholder, will be rebuilt with actual width on first render
                    self.compression_mode,
                ));
            }
        }

        // Restore selection: try to find the same commit, fall back to branch name
        if let Some(oid) = selected_oid {
            // Find the node with this commit OID
            if let Some((node_idx, _)) = self
                .graph_layout
                .nodes
                .iter()
                .enumerate()
                .find(|(_, node)| node.commit.as_ref().map(|c| c.oid) == Some(oid))
            {
                self.graph_list_state.select(Some(node_idx));
                self.sync_branch_selection_to_node(node_idx);
                let orientation_name = match self.current_orientation {
                    GraphOrientation::Vertical => "vertical",
                    GraphOrientation::Horizontal => "horizontal",
                };
                self.set_message(format!("Switched to {} layout", orientation_name));
                return;
            }
        }

        // Fallback: restore by branch name
        if let Some(branch_name) = selected_branch_name {
            if let Some((pos, (node_idx, _))) = self
                .branch_positions
                .iter()
                .enumerate()
                .find(|(_, (_, name))| name == &branch_name)
            {
                self.selected_branch_position = Some(pos);
                self.graph_list_state.select(Some(*node_idx));
            }
        }

        let orientation_name = match self.current_orientation {
            GraphOrientation::Vertical => "vertical",
            GraphOrientation::Horizontal => "horizontal",
        };
        self.set_message(format!("Switched to {} layout", orientation_name));
    }
}
