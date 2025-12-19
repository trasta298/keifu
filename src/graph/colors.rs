//! ブランチ色管理

use ratatui::style::Color;

/// レーンごとの色パレット（8色ローテーション）
pub const LANE_COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Green,
    Color::Magenta,
    Color::Yellow,
    Color::Blue,
    Color::Red,
    Color::LightCyan,
    Color::LightGreen,
];

/// カラーインデックスから色を取得
pub fn get_color_by_index(color_index: usize) -> Color {
    LANE_COLORS[color_index % LANE_COLORS.len()]
}

/// レーン番号から色を取得（後方互換性のため残す）
pub fn get_lane_color(lane: usize) -> Color {
    get_color_by_index(lane)
}

/// 色の距離を計算（同じ色 = 0、隣接インデックス = 1、...）
fn color_distance(a: usize, b: usize) -> usize {
    let len = LANE_COLORS.len();
    let a = a % len;
    let b = b % len;
    let diff = if a > b { a - b } else { b - a };
    diff.min(len - diff)
}

/// レーン再利用時に異なる色を割り当てるための色管理
#[derive(Debug)]
pub struct ColorAssigner {
    /// 各レーンに割り当てられた現在のカラーインデックス
    lane_colors: Vec<Option<usize>>,
    /// 各レーンで最後に使用されたカラーインデックス（再利用時の参照用）
    lane_last_color: Vec<usize>,
    /// 次に試すグローバルカラーインデックス
    next_color_index: usize,
}

impl ColorAssigner {
    pub fn new() -> Self {
        Self {
            lane_colors: Vec::new(),
            lane_last_color: Vec::new(),
            next_color_index: 0,
        }
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

    /// 新しいブランチに色を割り当て（前回の色と隣接色を避ける）
    pub fn assign_color(&mut self, lane: usize) -> usize {
        self.ensure_capacity(lane);

        // 避けるべき色を収集：このレーンの前回の色 + 隣接レーンの現在の色
        let mut avoid_colors: Vec<usize> = Vec::new();

        // このレーンで前回使用した色
        avoid_colors.push(self.lane_last_color[lane]);

        // 左隣レーンの現在の色
        if lane > 0 {
            if let Some(color) = self.lane_colors.get(lane - 1).and_then(|c| *c) {
                avoid_colors.push(color);
            }
        }
        // 右隣レーンの現在の色
        if let Some(color) = self.lane_colors.get(lane + 1).and_then(|c| *c) {
            avoid_colors.push(color);
        }

        // 避けるべき色から最も距離が離れた色を選択
        let mut best_color = self.next_color_index;
        let mut best_min_distance = 0usize;

        for candidate in 0..LANE_COLORS.len() {
            let color_idx = (self.next_color_index + candidate) % LANE_COLORS.len();
            let min_distance = avoid_colors
                .iter()
                .map(|&c| color_distance(color_idx, c))
                .min()
                .unwrap_or(LANE_COLORS.len());

            if min_distance > best_min_distance {
                best_min_distance = min_distance;
                best_color = color_idx;
            }
        }

        // 状態を更新
        self.lane_colors[lane] = Some(best_color);
        self.lane_last_color[lane] = best_color;
        self.next_color_index = (best_color + 1) % LANE_COLORS.len();

        best_color
    }

    /// 既存のレーンを継続使用
    pub fn continue_lane(&mut self, lane: usize) -> usize {
        self.ensure_capacity(lane);
        self.lane_colors[lane].unwrap_or_else(|| self.assign_color(lane))
    }

    /// レーンを解放（ブランチ終了時）
    pub fn release_lane(&mut self, lane: usize) {
        if lane < self.lane_colors.len() {
            self.lane_colors[lane] = None;
        }
    }
}

impl Default for ColorAssigner {
    fn default() -> Self {
        Self::new()
    }
}
