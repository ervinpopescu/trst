use super::*;
use std::collections::BTreeSet;

impl App {
    /// Returns the Transmission torrent IDs currently targeted by user selection or cursor position.
    pub fn target_ids(&self) -> Vec<i64> {
        let visible = self.filtered_torrents();
        if self.selected.is_empty() {
            visible
                .get(self.cursor)
                .map(|t| vec![t.id])
                .unwrap_or_default()
        } else {
            self.selected
                .iter()
                .filter_map(|&i| visible.get(i).map(|t| t.id))
                .collect()
        }
    }

    /// Returns the file indices targeted by file selection or file cursor position.
    pub fn file_target_indices(&self) -> Vec<usize> {
        if self.file_selected.is_empty() {
            vec![self.file_cursor]
        } else {
            self.file_selected.iter().copied().collect()
        }
    }

    /// Clamps the main torrent cursor position within valid bounds of the filtered torrent list.
    pub fn clamp_cursor(&mut self) {
        let len = self.filtered_torrents().len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    /// Clamps the file detail cursor position within valid bounds of the active detail torrent's files.
    pub fn clamp_file_cursor(&mut self) {
        let len = self
            .detail_torrent
            .as_ref()
            .map(|t| t.files.len())
            .unwrap_or(0);
        if len == 0 {
            self.file_cursor = 0;
        } else if self.file_cursor >= len {
            self.file_cursor = len - 1;
        }
    }

    /// Moves the cursor down by one entry, updating selection bounds when multi-selecting.
    pub fn move_down(
        cursor: &mut usize,
        selected: &mut BTreeSet<usize>,
        limit: usize,
        selecting: bool,
    ) {
        if limit == 0 {
            return;
        }
        if selecting {
            selected.insert(*cursor);
            if *cursor + 1 < limit {
                *cursor += 1;
                selected.insert(*cursor);
            }
        } else if *cursor + 1 < limit {
            *cursor += 1;
        }
    }

    /// Moves the cursor up by one entry, updating selection bounds when multi-selecting.
    pub fn move_up(
        cursor: &mut usize,
        selected: &mut BTreeSet<usize>,
        limit: usize,
        selecting: bool,
    ) {
        if limit == 0 {
            return;
        }
        if selecting {
            selected.insert(*cursor);
            if *cursor > 0 {
                *cursor -= 1;
                selected.insert(*cursor);
            }
        } else if *cursor > 0 {
            *cursor -= 1;
        }
    }
}

#[cfg(test)]
mod tests;
