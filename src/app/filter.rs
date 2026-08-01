use super::*;
use crate::protocol::Torrent;

impl App {
    pub fn filtered_torrents(&self) -> Vec<&Torrent> {
        self.filtered_indices
            .iter()
            .filter_map(|&i| self.torrents.get(i))
            .collect()
    }

    pub fn rebuild_filter(&mut self) {
        let raw = self.filter_input.trim().to_lowercase();
        self.filtered_indices = if raw.is_empty() {
            (0..self.torrents.len()).collect()
        } else if let Some(status) = raw.strip_prefix("status:") {
            let status = status.trim();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.status_str().to_lowercase() == status)
                .map(|(i, _)| i)
                .collect()
        } else if let Some(tracker) = raw.strip_prefix("tracker:") {
            let tracker = tracker.trim().to_string();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    t.tracker_stats.iter().any(|ts| {
                        ts.host.to_lowercase().contains(&tracker)
                            || ts.announce.to_lowercase().contains(&tracker)
                    })
                })
                .map(|(i, _)| i)
                .collect()
        } else if let Some(lbl) = raw.strip_prefix("label:") {
            let lbl = lbl.trim().to_lowercase();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.labels.iter().any(|l| l.to_lowercase().contains(&lbl)))
                .map(|(i, _)| i)
                .collect()
        } else {
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.name.to_lowercase().contains(&raw))
                .map(|(i, _)| i)
                .collect()
        };
    }

    pub(crate) fn sort_torrents(&self, list: &mut [Torrent]) {
        let asc = self.sort_ascending;
        if self.sort_column == SortColumn::Name {
            list.sort_by_cached_key(|t| t.name.to_lowercase());
            if !asc {
                list.reverse();
            }
            return;
        }
        list.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::Name => unreachable!(),
                SortColumn::Size => a.total_size.cmp(&b.total_size),
                SortColumn::Progress => a
                    .percent_done
                    .partial_cmp(&b.percent_done)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Down => a.rate_download.cmp(&b.rate_download),
                SortColumn::Up => a.rate_upload.cmp(&b.rate_upload),
                SortColumn::Eta => a.eta.cmp(&b.eta),
                SortColumn::Ratio => a
                    .upload_ratio
                    .partial_cmp(&b.upload_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Status => a.status.cmp(&b.status),
                SortColumn::Queue => a.queue_position.cmp(&b.queue_position),
            };
            if asc { ord } else { ord.reverse() }
        });
    }
}

#[cfg(test)]
mod tests;
