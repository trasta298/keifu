//! アプリケーション状態管理

use anyhow::Result;
use ratatui::widgets::ListState;

use crate::{
    action::Action,
    git::{
        build_graph,
        graph::GraphLayout,
        operations::{checkout_branch, checkout_commit, create_branch, delete_branch, merge_branch, rebase_branch},
        BranchInfo, CommitInfo, GitRepository,
    },
};

/// アプリケーションモード
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
}

/// 入力アクションの種類
#[derive(Debug, Clone)]
pub enum InputAction {
    CreateBranch,
    Search,
}

/// 確認アクションの種類
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteBranch(String),
    Merge(String),
    Rebase(String),
}

/// アプリケーション状態
pub struct App {
    pub mode: AppMode,
    pub repo: GitRepository,
    pub repo_path: String,
    pub head_name: Option<String>,

    // データ
    pub commits: Vec<CommitInfo>,
    pub branches: Vec<BranchInfo>,
    pub graph_layout: GraphLayout,

    // UI状態
    pub graph_list_state: ListState,

    // フラグ
    pub should_quit: bool,
    pub message: Option<String>,
}

impl App {
    /// 新しいアプリケーションを作成
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
            should_quit: false,
            message: None,
        })
    }

    /// リポジトリ情報を更新
    pub fn refresh(&mut self) -> Result<()> {
        self.commits = self.repo.get_commits(500)?;
        self.branches = self.repo.get_branches()?;
        self.graph_layout = build_graph(&self.commits, &self.branches);
        self.head_name = self.repo.head_name();

        // 選択位置を調整
        let max_commit = self.graph_layout.nodes.len().saturating_sub(1);
        if let Some(selected) = self.graph_list_state.selected() {
            if selected > max_commit {
                self.graph_list_state.select(Some(max_commit));
            }
        }

        Ok(())
    }

    /// アクションを処理
    pub fn handle_action(&mut self, action: Action) -> Result<()> {
        match &self.mode {
            AppMode::Normal => self.handle_normal_action(action)?,
            AppMode::Help => self.handle_help_action(action),
            AppMode::Input { .. } => self.handle_input_action(action)?,
            AppMode::Confirm { .. } => self.handle_confirm_action(action)?,
        }
        Ok(())
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
                        // TODO: 検索機能
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

    /// 次のブランチがあるコミットへジャンプ
    fn jump_to_next_branch(&mut self) {
        let current = self.graph_list_state.selected().unwrap_or(0);
        let nodes = &self.graph_layout.nodes;

        // 現在位置より後で、ブランチ名を持つノードを探す
        if let Some((i, _)) = nodes
            .iter()
            .enumerate()
            .skip(current + 1)
            .find(|(_, node)| !node.branch_names.is_empty())
        {
            self.graph_list_state.select(Some(i));
        }
    }

    /// 前のブランチがあるコミットへジャンプ
    fn jump_to_prev_branch(&mut self) {
        let current = self.graph_list_state.selected().unwrap_or(0);
        let nodes = &self.graph_layout.nodes;

        // 現在位置より前で、ブランチ名を持つノードを探す（逆順）
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

    /// 現在選択中のコミットにあるブランチを取得
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
            // ブランチがあればブランチをチェックアウト、なければコミットをチェックアウト
            if let Some(branch_name) = node.branch_names.first() {
                if !branch_name.starts_with("origin/") {
                    checkout_branch(&self.repo.repo, branch_name)?;
                    self.refresh()?;
                }
            } else if let Some(commit) = &node.commit {
                checkout_commit(&self.repo.repo, commit.oid)?;
                self.refresh()?;
            }
        }
        Ok(())
    }
}
