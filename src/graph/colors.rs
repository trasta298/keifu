//! ブランチ色管理

use ratatui::style::Color;
use std::collections::{HashSet, VecDeque};

/// レーンごとの色パレット（11色ローテーション）
pub const LANE_COLORS: [Color; 11] = [
    Color::Cyan,
    Color::Green,
    Color::Magenta,
    Color::Yellow,
    Color::Red,
    Color::LightCyan,
    Color::LightGreen,
    Color::LightMagenta,
    Color::LightYellow,
    Color::LightBlue,  // メインブランチ用
    Color::LightRed,
];

/// カラーインデックスから色を取得
pub fn get_color_by_index(color_index: usize) -> Color {
    LANE_COLORS[color_index % LANE_COLORS.len()]
}

/// レーン番号から色を取得（後方互換性のため残す）
pub fn get_lane_color(lane: usize) -> Color {
    get_color_by_index(lane)
}

/// メインブランチの色（ライトブルー）
pub const MAIN_BRANCH_COLOR: usize = 9; // Color::LightBlue

/// レーン再利用時に異なる色を割り当てるための色管理
#[derive(Debug)]
pub struct ColorAssigner {
    /// 各レーンに割り当てられた現在のカラーインデックス
    lane_colors: Vec<Option<usize>>,
    /// 各レーンで最後に使用されたカラーインデックス（再利用時の参照用）
    lane_last_color: Vec<usize>,
    /// 次に試すグローバルカラーインデックス
    next_color_index: usize,
    /// 予約された色（メインブランチ専用、他のブランチで使用不可）
    reserved_colors: HashSet<usize>,
    /// 最近の色割り当て履歴（行番号, レーン番号, カラーインデックス）
    recent_assignments: VecDeque<(usize, usize, usize)>,
    /// 履歴を保持する最大行数
    history_window: usize,
    /// 現在の行番号
    current_row: usize,
    /// 現在の行でフォーク兄弟として割り当てられた色
    current_fork_colors: HashSet<usize>,
    /// 色の使用回数カウンタ（均等分配のため）
    color_usage_count: [usize; 11],
    /// メインブランチのレーン（色を固定）
    main_lane: Option<usize>,
}

impl ColorAssigner {
    pub fn new() -> Self {
        Self {
            lane_colors: Vec::new(),
            lane_last_color: Vec::new(),
            next_color_index: 0,
            reserved_colors: HashSet::new(),
            recent_assignments: VecDeque::new(),
            history_window: 6,
            current_row: 0,
            current_fork_colors: HashSet::new(),
            color_usage_count: [0; 11],
            main_lane: None,
        }
    }

    /// 指定レーンがメインブランチかどうか
    pub fn is_main_lane(&self, lane: usize) -> bool {
        self.main_lane == Some(lane)
    }

    /// メインブランチの色を取得
    pub fn get_main_color(&self) -> usize {
        MAIN_BRANCH_COLOR
    }

    /// 色を予約（メインブランチ専用にする）
    pub fn reserve_color(&mut self, color_index: usize) {
        self.reserved_colors.insert(color_index);
    }

    /// 指定レーンの容量を確保
    fn ensure_capacity(&mut self, lane: usize) {
        while self.lane_colors.len() <= lane {
            self.lane_colors.push(None);
            self.lane_last_color.push(0);
        }
    }

    /// レーンのカラーインデックスを取得（アクティブな場合）
    pub fn get_lane_color_index(&self, lane: usize) -> Option<usize> {
        self.lane_colors.get(lane).and_then(|c| *c)
    }

    /// 新しい行の処理を開始（フォーク兄弟追跡をリセット）
    pub fn advance_row(&mut self) {
        self.current_row += 1;
        self.current_fork_colors.clear();
    }

    /// フォーク処理を開始（同じコミットから複数ブランチが分岐）
    pub fn begin_fork(&mut self) {
        self.current_fork_colors.clear();
    }

    /// 新しいブランチに色を割り当て（ペナルティベースのアルゴリズム）
    /// is_fork_sibling: trueの場合はフォーク兄弟として扱い、同一フォーク内での色重複を避ける
    /// use_reserved: trueの場合は予約色も使用可能（メインブランチ用）
    fn assign_color_advanced(
        &mut self,
        lane: usize,
        is_fork_sibling: bool,
        use_reserved: bool,
    ) -> usize {
        self.ensure_capacity(lane);

        // 各色のペナルティを計算
        let mut color_penalties: [f64; 11] = [0.0; 11];

        // 1. このレーンの前回の色（高ペナルティ）
        let last_color = self.lane_last_color[lane];
        color_penalties[last_color] += 10.0;

        // 2. 全アクティブレーンの色（距離ベースの重み付き）
        for (other_lane, color_opt) in self.lane_colors.iter().enumerate() {
            if let Some(color) = color_opt {
                let lane_distance = (lane as isize - other_lane as isize).unsigned_abs() as f64;
                // 近いレーンほど高いペナルティ
                let weight = 8.0 / (lane_distance + 1.0);
                color_penalties[*color] += weight;
            }
        }

        // 3. 最近の色割り当て履歴（垂直方向の重複回避）
        for &(row, hist_lane, color) in &self.recent_assignments {
            let row_distance = self.current_row.saturating_sub(row) as f64;
            let lane_distance = (lane as isize - hist_lane as isize).unsigned_abs() as f64;

            // 近い行・近いレーンほど高いペナルティ
            let row_weight = 4.0 / (row_distance + 1.0);
            let lane_weight = 2.0 / (lane_distance + 1.0);
            color_penalties[color] += row_weight * lane_weight;
        }

        // 4. フォーク兄弟の色（最高ペナルティ - 同じフォークで同じ色は避ける）
        if is_fork_sibling {
            for &color in &self.current_fork_colors {
                color_penalties[color] += 100.0;
            }
        }

        // 5. 色の使用頻度（均等分配のため）
        let max_usage = *self.color_usage_count.iter().max().unwrap_or(&0) as f64;
        if max_usage > 0.0 {
            for (color, &count) in self.color_usage_count.iter().enumerate() {
                color_penalties[color] += (count as f64 / max_usage) * 2.0;
            }
        }

        // 最適な色を選択（ペナルティが最小の色）
        let mut best_color = self.next_color_index;
        let mut best_penalty = f64::MAX;

        for candidate in 0..LANE_COLORS.len() {
            let color_idx = (self.next_color_index + candidate) % LANE_COLORS.len();

            // 予約色をスキップ（use_reserved=falseの場合）
            if !use_reserved && self.reserved_colors.contains(&color_idx) {
                continue;
            }

            let penalty = color_penalties[color_idx];
            if penalty < best_penalty {
                best_penalty = penalty;
                best_color = color_idx;
            }
        }

        // 状態を更新
        self.lane_colors[lane] = Some(best_color);
        self.lane_last_color[lane] = best_color;
        self.next_color_index = (best_color + 1) % LANE_COLORS.len();

        // 履歴に追加
        self.recent_assignments
            .push_back((self.current_row, lane, best_color));
        while self.recent_assignments.len() > self.history_window {
            self.recent_assignments.pop_front();
        }

        // 使用回数をインクリメント
        self.color_usage_count[best_color] += 1;

        // フォーク兄弟として追跡
        if is_fork_sibling {
            self.current_fork_colors.insert(best_color);
        }

        best_color
    }

    /// 新しいブランチに色を割り当て（予約色は使用しない）
    pub fn assign_color(&mut self, lane: usize) -> usize {
        self.assign_color_advanced(lane, false, false)
    }

    /// フォーク兄弟として色を割り当て（同じフォーク内での色重複を避ける）
    pub fn assign_fork_sibling_color(&mut self, lane: usize) -> usize {
        self.assign_color_advanced(lane, true, false)
    }

    /// メインブランチに色を割り当て（青を固定で使用し、予約する）
    pub fn assign_main_color(&mut self, lane: usize) -> usize {
        self.ensure_capacity(lane);
        let color = MAIN_BRANCH_COLOR;
        self.lane_colors[lane] = Some(color);
        self.lane_last_color[lane] = color;
        self.reserve_color(color);
        self.main_lane = Some(lane);
        self.color_usage_count[color] += 1;
        color
    }

    /// 既存のレーンを継続使用
    /// メインレーンの場合は常に青を返す
    pub fn continue_lane(&mut self, lane: usize) -> usize {
        if self.main_lane == Some(lane) {
            return MAIN_BRANCH_COLOR;
        }
        self.ensure_capacity(lane);
        self.lane_colors[lane].unwrap_or_else(|| self.assign_color(lane))
    }

    /// レーンを解放（ブランチ終了時）
    /// メインレーンの色は解放しない
    pub fn release_lane(&mut self, lane: usize) {
        if lane < self.lane_colors.len() && self.main_lane != Some(lane) {
            self.lane_colors[lane] = None;
        }
    }
}

impl Default for ColorAssigner {
    fn default() -> Self {
        Self::new()
    }
}
