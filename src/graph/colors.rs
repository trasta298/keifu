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

/// レーン番号から色を取得
pub fn get_lane_color(lane: usize) -> Color {
    LANE_COLORS[lane % LANE_COLORS.len()]
}
