//! Horizontal graph view widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, StatefulWidget, Widget},
};

use crate::{
    git::graph::{
        HorizontalChunk, HorizontalGraphLayout, HorizontalSelection, HorizontalCellType,
        TagDisplay, TagPosition,
    },
    graph::colors::get_color_by_index,
};

pub struct HorizontalGraphViewWidget<'a> {
    pub layout: &'a HorizontalGraphLayout,
    pub terminal_width: usize,
    pub terminal_height: usize,
    pub show_tags: bool,
}

impl<'a> HorizontalGraphViewWidget<'a> {
    pub fn new(layout: &'a HorizontalGraphLayout, width: usize, height: usize, show_tags: bool) -> Self {
        Self {
            layout,
            terminal_width: width,
            terminal_height: height,
            show_tags,
        }
    }
}

pub struct HorizontalGraphState {
    pub current_chunk: usize,
    pub selection: HorizontalSelection,
}

impl<'a> StatefulWidget for HorizontalGraphViewWidget<'a> {
    type State = HorizontalGraphState;


    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Clear the area
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);
        
        // Calculate if we have tags to show (per-chunk basis)
        let has_any_top_tags = self.show_tags && self.layout.chunks.iter()
            .any(|c| c.tags.iter().any(|t| t.position == TagPosition::Top));
        let has_any_bottom_tags = self.show_tags && self.layout.chunks.iter()
            .any(|c| c.tags.iter().any(|t| t.position == TagPosition::Bottom));
        
        // Calculate height per chunk:
        // - 1 line: chunk header
        // - 1 line: top whitespace padding
        // - 1 line: top tag labels (if any)
        // - 1 line: top connectors (if any top tags) 
        // - N lines: graph lanes (with connectors through them for multi-lane tags)
        // - 1 line: bottom connectors (if any bottom tags)
        // - 1 line: bottom tag labels (if any)
        // - 1 line: bottom whitespace padding
        let header_line = 1;
        let top_padding = 1;
        let bottom_padding = 1;
        let top_tag_lines = if has_any_top_tags { 2 } else { 0 }; // labels + connectors
        let bottom_tag_lines = if has_any_bottom_tags { 2 } else { 0 }; // connectors + labels
        let lanes_count = self.layout.lanes.len();
        let chunk_height = header_line + top_padding + top_tag_lines + lanes_count + bottom_tag_lines + bottom_padding;
        
        if chunk_height == 0 { return; }
        
        let available_height = inner.height as usize;
        let chunks_fit = available_height / chunk_height.max(1);
        let visible_chunks = chunks_fit.max(1);
        
        // Simple scroll logic: ensure selected is visible
        let start_chunk_idx = if state.current_chunk >= visible_chunks {
            state.current_chunk.saturating_sub(visible_chunks / 2)
        } else {
            0
        };
        
        let end_chunk_idx = (start_chunk_idx + visible_chunks).min(self.layout.chunks.len());
        
        let mut current_y = inner.y;
        
        for chunk_idx in start_chunk_idx..end_chunk_idx {
            if let Some(chunk) = self.layout.chunks.get(chunk_idx) {
                // === CHUNK HEADER ===
                if current_y >= inner.y + inner.height { break; }
                let header_text = format!("Chunk {} [Commits {}-{}]", 
                    chunk_idx + 1, 
                    chunk.start_column, 
                    chunk.end_column
                );
                buf.set_string(inner.x, current_y, header_text, Style::default().fg(Color::DarkGray));
                current_y += 1;
                
                // === TOP WHITESPACE PADDING ===
                if current_y >= inner.y + inner.height { break; }
                current_y += 1; // Empty line
                
                // Get tags for this chunk
                let top_tags: Vec<&TagDisplay> = if self.show_tags {
                    chunk.tags.iter().filter(|t| t.position == TagPosition::Top).collect()
                } else {
                    vec![]
                };
                let bottom_tags: Vec<&TagDisplay> = if self.show_tags {
                    chunk.tags.iter().filter(|t| t.position == TagPosition::Bottom).collect()
                } else {
                    vec![]
                };
                
                // === TOP TAG LABELS ===
                if self.show_tags && has_any_top_tags {
                    if current_y >= inner.y + inner.height { break; }
                    self.render_tag_label_line(buf, inner.x, current_y, chunk, TagPosition::Top, inner.width as usize);
                    current_y += 1;
                    
                    // === TOP TAG CONNECTORS === (dedicated line between labels and lane 0)
                    if current_y >= inner.y + inner.height { break; }
                    self.render_connector_line(buf, inner.x, current_y, chunk, &top_tags, inner.width as usize);
                    current_y += 1;
                }
                
                // === RENDER GRAPH LANES ===
                for lane in 0..chunk.lane_count {
                    if current_y >= inner.y + inner.height { break; }
                    
                    // Render the lane content
                    self.render_lane(buf, inner.x, current_y, chunk, lane, &state.selection);
                    
                    // Draw vertical connectors for TOP tags that need to pass through this lane
                    // (for tags on lanes BELOW this one, going up to the connector line)
                    for tag in &top_tags {
                        if lane < tag.lane {
                            // tag.column is already a cell array index
                            let x_pos = inner.x + (tag.column * Self::COLUMN_WIDTH) as u16;
                            if x_pos < inner.x + inner.width {
                                let color = get_color_by_index(tag.color_index);
                                buf[(x_pos, current_y)]
                                    .set_char('│')
                                    .set_style(Style::default().fg(color));
                            }
                        }
                    }
                    
                    // Draw vertical connectors for BOTTOM tags that need to pass through this lane
                    // (for tags on lanes ABOVE this one, going down to the connector line)
                    for tag in &bottom_tags {
                        if lane > tag.lane {
                            // tag.column is already a cell array index
                            let x_pos = inner.x + (tag.column * Self::COLUMN_WIDTH) as u16;
                            if x_pos < inner.x + inner.width {
                                let color = get_color_by_index(tag.color_index);
                                buf[(x_pos, current_y)]
                                    .set_char('│')
                                    .set_style(Style::default().fg(color));
                            }
                        }
                    }
                    
                    // Render branch name on the right
                    if let Some(lane_info) = self.layout.lanes.iter().find(|l| l.lane == lane) {
                         let show_label = lane_info.last_chunk_index == Some(chunk_idx);
                         
                         if show_label && !lane_info.branch_names.is_empty() {
                             let name = lane_info.branch_names[0].clone();
                             let name_width = name.len();
                             let x = inner.x + inner.width.saturating_sub(name_width as u16);
                             
                             let graph_width = (chunk.end_column - chunk.start_column) * 3;
                             if x > inner.x + graph_width as u16 + 1 {
                                buf.set_string(x, current_y, name, Style::default().fg(get_color_by_index(lane_info.color_index)));
                             }
                         }
                    }
                    
                    current_y += 1;
                }
                
                // === BOTTOM TAG CONNECTORS === (dedicated line between last lane and labels)
                if self.show_tags && has_any_bottom_tags {
                    if current_y >= inner.y + inner.height { break; }
                    self.render_connector_line(buf, inner.x, current_y, chunk, &bottom_tags, inner.width as usize);
                    current_y += 1;
                    
                    // === BOTTOM TAG LABELS ===
                    if current_y >= inner.y + inner.height { break; }
                    self.render_tag_label_line(buf, inner.x, current_y, chunk, TagPosition::Bottom, inner.width as usize);
                    current_y += 1;
                }
                
                // === BOTTOM WHITESPACE PADDING ===
                if current_y >= inner.y + inner.height { break; }
                current_y += 1; // Empty line for visual separation
            }
        }

        // Render status bar at bottom (overlay)
        if let Some(chunk) = self.layout.chunks.get(state.current_chunk) {
             self.render_status_bar(buf, inner, chunk);
        }
    }
}

impl<'a> HorizontalGraphViewWidget<'a> {
    // Compact: each column takes 2 character positions:
    // [symbol][connector] - e.g., "●─" or "●┬"
    const COLUMN_WIDTH: usize = 2;

    fn render_lane(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        chunk: &HorizontalChunk,
        lane: usize,
        selection: &HorizontalSelection,
    ) {
        let cells = match chunk.cells.get(lane) {
            Some(c) => c,
            None => return,
        };

        for (col_idx, cell) in cells.iter().enumerate() {
            let is_selected = selection.chunk_index == chunk.index
                && selection.lane == lane
                && selection.column == col_idx;

            let x_pos = x + (col_idx * Self::COLUMN_WIDTH) as u16;
            
            // Determine right connector
            // Check next cell (col_idx + 1)
            let has_right_neighbor = if col_idx + 1 < cells.len() {
                 let next = &cells[col_idx + 1];
                 self.cell_connects_left(next)
            } else {
                 false 
            };
            
            // Also check if current cell connects right
            let connects_right = self.cell_connects_right(cell);
            
            let connector_char = if connects_right && has_right_neighbor {
                '─'
            } else {
                ' '
            };

            self.render_cell_with_connector(buf, x_pos, y, cell, connector_char, is_selected);
        }
    }

    fn render_cell_with_connector(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        cell: &HorizontalCellType,
        connector_char: char,
        is_selected: bool,
    ) {
        let (main_char, color) = self.cell_to_char(cell);
        let is_commit = matches!(cell, HorizontalCellType::Commit(_));

        // Create style with background highlight for selected cells
        let style = if is_selected {
            Style::default()
                .fg(color)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD)
        };

        // For selected commits, render leading whitespace with highlight
        // This creates a 3-char wide selection: [ ●─] where [ ] has background
        if is_selected && is_commit && x > 0 {
            buf[(x - 1, y)]
                .set_char(' ')
                .set_style(Style::default().bg(Color::DarkGray));
        }

        buf[(x, y)]
            .set_char(main_char)
            .set_style(style);

        // Middle position (Connector)
        let middle_char = if is_commit {
            connector_char
        } else if main_char == '─' || main_char == '┼' || main_char == '┬' || main_char == '┴' || main_char == '├' || main_char == '╭' || main_char == '╰' {
             '─'
        } else {
             connector_char
        };
        
        // Connector style (match selection background if selected)
        let connector_style = if is_selected {
            Style::default()
                .fg(color)
                .bg(Color::DarkGray)
        } else {
            Style::default()
                .fg(color)
        };

        buf[(x + 1, y)]
            .set_char(middle_char)
            .set_style(connector_style);
    }

    fn cell_connects_right(&self, cell: &HorizontalCellType) -> bool {
        match cell {
            HorizontalCellType::HLine(_) |
            HorizontalCellType::JumpDown(_) | // ╭
            HorizontalCellType::JumpUp(_) |   // ╰
            HorizontalCellType::TeeDown(_) |  // ┬
            HorizontalCellType::TeeUp(_) |    // ┴
            HorizontalCellType::TeeRight(_) | // ├
            HorizontalCellType::Cross(_, _) | // ┼
            HorizontalCellType::Commit(_) => true, // Commits can connect right
            _ => false,
        }
    }

    fn cell_connects_left(&self, cell: &HorizontalCellType) -> bool {
        match cell {
            HorizontalCellType::HLine(_) |
            HorizontalCellType::HookDown(_) | // ╮
            HorizontalCellType::HookUp(_) |   // ╯
            HorizontalCellType::TeeDown(_) |  // ┬
            HorizontalCellType::TeeUp(_) |    // ┴
            HorizontalCellType::TeeLeft(_) |  // ┤
            HorizontalCellType::Cross(_, _) |
            HorizontalCellType::Commit(_) => true,
            _ => false,
        }
    }

    fn cell_to_char(&self, cell: &HorizontalCellType) -> (char, Color) {
        let (ch, color_idx) = match cell {
            HorizontalCellType::Empty => (' ', 0),
            HorizontalCellType::Commit(c) => ('●', *c),
            HorizontalCellType::Pipe(c) => ('│', *c),
            HorizontalCellType::HLine(c) => ('─', *c),
            HorizontalCellType::JumpUp(c) => ('╰', *c),
            HorizontalCellType::JumpDown(c) => ('╭', *c),
            HorizontalCellType::HookUp(c) => ('╯', *c),
            HorizontalCellType::HookDown(c) => ('╮', *c),
            HorizontalCellType::TeeDown(c) => ('┬', *c),
            HorizontalCellType::TeeUp(c) => ('┴', *c),
            HorizontalCellType::TeeLeft(c) => ('┤', *c),
            HorizontalCellType::TeeRight(c) => ('├', *c),
            HorizontalCellType::Cross(v, _) => ('┼', *v),
        };
        (ch, get_color_by_index(color_idx))
    }

    fn render_status_bar(&self, buf: &mut Buffer, area: Rect, chunk: &HorizontalChunk) {
        let y = area.y + area.height - 1;
        let mut text = format!("Chunk {}/{} | Total commits: {}",
            chunk.index + 1,
            self.layout.chunks.len(),
            self.layout.total_columns);

        if chunk.index < self.layout.chunks.len() - 1 {
            text.push_str(" | → More");
        }

        buf.set_string(area.x, y, text, Style::default().fg(Color::DarkGray));
    }

    /// Render tag labels on a dedicated line (top or bottom)
    /// This only draws the tag names, not vertical connectors
    fn render_tag_label_line(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        chunk: &HorizontalChunk,
        position: TagPosition,
        max_width: usize,
    ) {
        // Filter tags for this position
        let tags: Vec<&TagDisplay> = chunk.tags.iter()
            .filter(|t| t.position == position)
            .collect();
        
        if tags.is_empty() {
            return;
        }

        // Sort tags by column for consistent rendering (left to right)
        let mut sorted_tags: Vec<&TagDisplay> = tags.clone();
        sorted_tags.sort_by_key(|t| t.column);
        
        // Track the rightmost x position used so far to avoid overlaps
        let mut rightmost_used: u16 = 0;
        
        for tag in &sorted_tags {
            // tag.column is already a cell array index (relative to chunk start)
            let connector_x = x + (tag.column * Self::COLUMN_WIDTH) as u16;
            
            // Make sure we don't write outside bounds
            if connector_x >= x + max_width as u16 {
                continue;
            }
            
            // Format label with bookmark emoji
            let label = format!("🔖{}", &tag.name);
            let label_char_count = label.chars().count();
            
            // Center the label above/below the connector
            let half_width = (label_char_count / 2) as u16;
            let label_x = if connector_x >= x + half_width {
                connector_x - half_width
            } else {
                x // Clamp to left edge if centering would go off-screen
            };
            
            // Skip if this label would overlap with a previously drawn one
            // Add 1 char padding between labels
            if label_x < rightmost_used + 1 && rightmost_used > 0 {
                continue;
            }
            
            // Only draw label if there's room
            let available_space = (x + max_width as u16).saturating_sub(label_x) as usize;
            if available_space > 0 {
                let display_label = truncate_string(&label, available_space);
                let drawn_width = display_label.chars().count() as u16;
                buf.set_string(
                    label_x,
                    y,
                    display_label,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                );
                // Update rightmost position
                rightmost_used = label_x + drawn_width;
            }
        }
    }
    
    /// Render vertical connectors (│) on a dedicated line between tag labels and graph lanes
    fn render_connector_line(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        _chunk: &HorizontalChunk,
        tags: &[&TagDisplay],
        max_width: usize,
    ) {
        for tag in tags {
            // tag.column is already a cell array index (relative to chunk start)
            let x_pos = x + (tag.column * Self::COLUMN_WIDTH) as u16;
            
            // Make sure we don't write outside bounds
            if x_pos >= x + max_width as u16 {
                continue;
            }

            let color = get_color_by_index(tag.color_index);
            buf[(x_pos, y)]
                .set_char('│')
                .set_style(Style::default().fg(color));
        }
    }
}

/// Truncate a string to fit within max_len characters
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len > 2 {
        let truncated: String = s.chars().take(max_len - 2).collect();
        format!("{}..", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}