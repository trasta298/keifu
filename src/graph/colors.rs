//! Branch color management

use ratatui::style::Color;
use std::collections::{HashSet, VecDeque};

/// Per-lane color palette (11-color rotation)
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
    Color::LightBlue, // Main branch
    Color::LightRed,
];

/// Color index for uncommitted changes (gray)
pub const UNCOMMITTED_COLOR_INDEX: usize = usize::MAX;

/// Get a color from a color index
pub fn get_color_by_index(color_index: usize) -> Color {
    if color_index == UNCOMMITTED_COLOR_INDEX {
        return Color::DarkGray;
    }
    LANE_COLORS[color_index % LANE_COLORS.len()]
}

/// Get a color from a lane number (kept for backward compatibility)
pub fn get_lane_color(lane: usize) -> Color {
    get_color_by_index(lane)
}

/// Main branch color (light blue)
pub const MAIN_BRANCH_COLOR: usize = 9; // Color::LightBlue

/// Color assignment to vary colors when lanes are reused
#[derive(Debug)]
pub struct ColorAssigner {
    /// Current color index assigned to each lane
    lane_colors: Vec<Option<usize>>,
    /// Last color index used per lane (for reuse)
    lane_last_color: Vec<usize>,
    /// Next global color index to try
    next_color_index: usize,
    /// Reserved colors (main branch only, unavailable to others)
    reserved_colors: HashSet<usize>,
    /// Recent color assignment history (row, lane, color index)
    recent_assignments: VecDeque<(usize, usize, usize)>,
    /// Max rows to keep in history
    history_window: usize,
    /// Current row number
    current_row: usize,
    /// Colors assigned to fork siblings on the current row
    current_fork_colors: HashSet<usize>,
    /// Color usage counters (for balancing)
    color_usage_count: [usize; 11],
    /// Lane of the main branch (fixed color)
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

    /// Whether the lane is the main branch
    pub fn is_main_lane(&self, lane: usize) -> bool {
        self.main_lane == Some(lane)
    }

    /// Get the main branch color
    pub fn get_main_color(&self) -> usize {
        MAIN_BRANCH_COLOR
    }

    /// Reserve a color (main branch only)
    pub fn reserve_color(&mut self, color_index: usize) {
        self.reserved_colors.insert(color_index);
    }

    /// Ensure capacity for the lane
    fn ensure_capacity(&mut self, lane: usize) {
        while self.lane_colors.len() <= lane {
            self.lane_colors.push(None);
            self.lane_last_color.push(0);
        }
    }

    /// Get the lane color index (if active)
    pub fn get_lane_color_index(&self, lane: usize) -> Option<usize> {
        self.lane_colors.get(lane).and_then(|c| *c)
    }

    /// Start a new row (reset fork sibling tracking)
    pub fn advance_row(&mut self) {
        self.current_row += 1;
        self.current_fork_colors.clear();
    }

    /// Begin a fork (multiple branches from the same commit)
    pub fn begin_fork(&mut self) {
        self.current_fork_colors.clear();
    }

    /// Assign a color to a new branch (penalty-based algorithm)
    /// is_fork_sibling: true treats it as a fork sibling to avoid duplicate colors within a fork
    /// use_reserved: true allows reserved colors (for the main branch)
    fn assign_color_advanced(
        &mut self,
        lane: usize,
        is_fork_sibling: bool,
        use_reserved: bool,
    ) -> usize {
        self.ensure_capacity(lane);

        // Compute penalties for each color
        let mut color_penalties: [f64; 11] = [0.0; 11];

        // 1. Last color on this lane (high penalty)
        let last_color = self.lane_last_color[lane];
        color_penalties[last_color] += 10.0;

        // 2. Colors on all active lanes (distance-weighted)
        for (other_lane, color_opt) in self.lane_colors.iter().enumerate() {
            if let Some(color) = color_opt {
                let lane_distance = (lane as isize - other_lane as isize).unsigned_abs() as f64;
                // Closer lanes get higher penalty
                let weight = 8.0 / (lane_distance + 1.0);
                color_penalties[*color] += weight;
            }
        }

        // 3. Recent assignment history (avoid vertical repeats)
        for &(row, hist_lane, color) in &self.recent_assignments {
            let row_distance = self.current_row.saturating_sub(row) as f64;
            let lane_distance = (lane as isize - hist_lane as isize).unsigned_abs() as f64;

            // Closer rows and lanes get higher penalty
            let row_weight = 4.0 / (row_distance + 1.0);
            let lane_weight = 2.0 / (lane_distance + 1.0);
            color_penalties[color] += row_weight * lane_weight;
        }

        // 4. Fork sibling colors (highest penalty to avoid duplicates in a fork)
        if is_fork_sibling {
            for &color in &self.current_fork_colors {
                color_penalties[color] += 100.0;
            }
        }

        // 5. Color usage frequency (balance distribution)
        let max_usage = *self.color_usage_count.iter().max().unwrap_or(&0) as f64;
        if max_usage > 0.0 {
            for (color, &count) in self.color_usage_count.iter().enumerate() {
                color_penalties[color] += (count as f64 / max_usage) * 2.0;
            }
        }

        // Choose the best color (lowest penalty)
        let mut best_color = self.next_color_index;
        let mut best_penalty = f64::MAX;

        for candidate in 0..LANE_COLORS.len() {
            let color_idx = (self.next_color_index + candidate) % LANE_COLORS.len();

            // Skip reserved colors when use_reserved is false
            if !use_reserved && self.reserved_colors.contains(&color_idx) {
                continue;
            }

            let penalty = color_penalties[color_idx];
            if penalty < best_penalty {
                best_penalty = penalty;
                best_color = color_idx;
            }
        }

        // Update state
        self.lane_colors[lane] = Some(best_color);
        self.lane_last_color[lane] = best_color;
        self.next_color_index = (best_color + 1) % LANE_COLORS.len();

        // Add to history
        self.recent_assignments
            .push_back((self.current_row, lane, best_color));
        while self.recent_assignments.len() > self.history_window {
            self.recent_assignments.pop_front();
        }

        // Increment usage count
        self.color_usage_count[best_color] += 1;

        // Track as fork sibling
        if is_fork_sibling {
            self.current_fork_colors.insert(best_color);
        }

        best_color
    }

    /// Assign a color to a new branch (do not use reserved colors)
    pub fn assign_color(&mut self, lane: usize) -> usize {
        self.assign_color_advanced(lane, false, false)
    }

    /// Assign a color as a fork sibling (avoid duplicates within a fork)
    pub fn assign_fork_sibling_color(&mut self, lane: usize) -> usize {
        self.assign_color_advanced(lane, true, false)
    }

    /// Assign a color to the main branch (fixed blue, reserve it)
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

    /// Continue using an existing lane
    /// Always return blue for the main lane
    pub fn continue_lane(&mut self, lane: usize) -> usize {
        if self.main_lane == Some(lane) {
            return MAIN_BRANCH_COLOR;
        }
        self.ensure_capacity(lane);
        self.lane_colors[lane].unwrap_or_else(|| self.assign_color(lane))
    }

    /// Release a lane (when a branch ends)
    /// Do not release the main lane color
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
