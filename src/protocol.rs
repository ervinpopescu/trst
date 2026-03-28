use serde::{Deserialize, Serialize};

// --- JSON-RPC envelope ---

#[derive(Serialize)]
pub struct RpcRequest<'a> {
    pub method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<u64>,
}

#[derive(Deserialize)]
pub struct RpcResponse {
    pub result: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub tag: Option<u64>,
}

// --- Torrent ---

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Torrent {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub total_size: i64,
    #[serde(default)]
    pub percent_done: f64,
    #[serde(default)]
    pub rate_download: i64,
    #[serde(default)]
    pub rate_upload: i64,
    #[serde(default)]
    pub upload_ratio: f64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default)]
    pub peers_connected: i64,
    #[serde(default)]
    pub peers_sending_to_us: i64,
    #[serde(default)]
    pub peers_getting_from_us: i64,
    #[serde(default)]
    pub seeders: i64,
    #[serde(default)]
    pub leechers: i64,
    #[serde(default)]
    pub hash_string: String,
    #[serde(default)]
    pub download_dir: String,
    #[serde(default)]
    pub added_date: i64,
    #[serde(default)]
    pub done_date: i64,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub error: i64,
    #[serde(default)]
    pub error_string: String,
    #[serde(default)]
    pub size_when_done: i64,
    #[serde(default)]
    pub left_until_done: i64,
    #[serde(default)]
    pub downloaded_ever: i64,
    #[serde(default)]
    pub uploaded_ever: i64,
    #[serde(default)]
    pub queue_position: i64,
    #[serde(default)]
    pub is_finished: bool,
    #[serde(default)]
    pub files: Vec<TorrentFile>,
    #[serde(default)]
    pub file_stats: Vec<FileStats>,
    #[serde(default)]
    pub tracker_stats: Vec<TrackerStats>,
    #[serde(default)]
    pub peers: Vec<Peer>,
}

impl Torrent {
    pub fn status_str(&self) -> &'static str {
        match self.status {
            0 => "Stopped",
            1 => "Queued verify",
            2 => "Verifying",
            3 => "Queued",
            4 => "Downloading",
            5 => "Queued seed",
            6 => "Seeding",
            _ => "Unknown",
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.status == 0
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub length: i64,
    #[serde(default)]
    pub bytes_completed: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileStats {
    #[serde(default)]
    pub wanted: bool,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub bytes_completed: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePriority {
    Unwanted,
    Low,
    Normal,
    High,
}

impl FilePriority {
    pub fn from_stats(stats: &FileStats) -> Self {
        if !stats.wanted {
            return Self::Unwanted;
        }
        match stats.priority {
            -1 => Self::Low,
            1 => Self::High,
            _ => Self::Normal,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Unwanted => Self::Low,
            Self::Low => Self::Normal,
            Self::Normal => Self::High,
            Self::High => Self::Unwanted,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Unwanted => Self::High,
            Self::Low => Self::Unwanted,
            Self::Normal => Self::Low,
            Self::High => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unwanted => "skip",
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrackerStats {
    #[serde(default)]
    pub announce: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub seeder_count: i64,
    #[serde(default)]
    pub leecher_count: i64,
    #[serde(default)]
    pub last_announce_result: String,
    #[serde(default)]
    pub last_announce_time: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub port: i64,
    #[serde(default)]
    pub client_name: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub rate_to_client: i64,
    #[serde(default)]
    pub rate_to_peer: i64,
    #[serde(default)]
    pub flag_str: String,
    #[serde(default)]
    pub is_encrypted: bool,
}

// --- Session ---

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    #[serde(default)]
    pub active_torrent_count: i64,
    #[serde(default)]
    pub paused_torrent_count: i64,
    #[serde(default)]
    pub torrent_count: i64,
    #[serde(default)]
    pub download_speed: i64,
    #[serde(default)]
    pub upload_speed: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct FreeSpace {
    #[serde(default)]
    pub size_bytes: i64,
    #[serde(default)]
    pub total_size: i64,
    #[serde(default)]
    pub path: String,
}

// --- Field names for torrent-get ---

pub const TORRENT_LIST_FIELDS: &[&str] = &[
    "id",
    "name",
    "status",
    "totalSize",
    "percentDone",
    "rateDownload",
    "rateUpload",
    "uploadRatio",
    "eta",
    "peersConnected",
    "peersSendingToUs",
    "peersGettingFromUs",
    "error",
    "errorString",
    "sizeWhenDone",
    "leftUntilDone",
    "queuePosition",
    "isFinished",
    "trackerStats",
];

pub const TORRENT_DETAIL_FIELDS: &[&str] = &[
    "id",
    "name",
    "status",
    "totalSize",
    "percentDone",
    "rateDownload",
    "rateUpload",
    "uploadRatio",
    "eta",
    "peersConnected",
    "peersSendingToUs",
    "peersGettingFromUs",
    "hashString",
    "downloadDir",
    "addedDate",
    "doneDate",
    "comment",
    "error",
    "errorString",
    "sizeWhenDone",
    "leftUntilDone",
    "downloadedEver",
    "uploadedEver",
    "queuePosition",
    "isFinished",
    "files",
    "fileStats",
    "trackerStats",
    "peers",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_str() {
        let mut t = Torrent::default();
        t.status = 0;
        assert_eq!(t.status_str(), "Stopped");
        assert!(t.is_stopped());

        t.status = 1;
        assert_eq!(t.status_str(), "Queued verify");
        assert!(!t.is_stopped());

        t.status = 2;
        assert_eq!(t.status_str(), "Verifying");

        t.status = 3;
        assert_eq!(t.status_str(), "Queued");

        t.status = 4;
        assert_eq!(t.status_str(), "Downloading");

        t.status = 5;
        assert_eq!(t.status_str(), "Queued seed");

        t.status = 6;
        assert_eq!(t.status_str(), "Seeding");

        t.status = 99;
        assert_eq!(t.status_str(), "Unknown");
    }

    #[test]
    fn test_file_priority() {
        // Test from_stats
        let stats_unwanted = FileStats {
            wanted: false,
            priority: 0,
            bytes_completed: 0,
        };
        assert_eq!(
            FilePriority::from_stats(&stats_unwanted),
            FilePriority::Unwanted
        );

        let stats_low = FileStats {
            wanted: true,
            priority: -1,
            bytes_completed: 0,
        };
        assert_eq!(FilePriority::from_stats(&stats_low), FilePriority::Low);

        let stats_normal = FileStats {
            wanted: true,
            priority: 0,
            bytes_completed: 0,
        };
        assert_eq!(
            FilePriority::from_stats(&stats_normal),
            FilePriority::Normal
        );

        let stats_high = FileStats {
            wanted: true,
            priority: 1,
            bytes_completed: 0,
        };
        assert_eq!(FilePriority::from_stats(&stats_high), FilePriority::High);

        // Test next
        assert_eq!(FilePriority::Unwanted.next(), FilePriority::Low);
        assert_eq!(FilePriority::Low.next(), FilePriority::Normal);
        assert_eq!(FilePriority::Normal.next(), FilePriority::High);
        assert_eq!(FilePriority::High.next(), FilePriority::Unwanted);

        // Test prev
        assert_eq!(FilePriority::Unwanted.prev(), FilePriority::High);
        assert_eq!(FilePriority::High.prev(), FilePriority::Normal);
        assert_eq!(FilePriority::Normal.prev(), FilePriority::Low);
        assert_eq!(FilePriority::Low.prev(), FilePriority::Unwanted);

        // Test label
        assert_eq!(FilePriority::Unwanted.label(), "skip");
        assert_eq!(FilePriority::Low.label(), "low");
        assert_eq!(FilePriority::Normal.label(), "normal");
        assert_eq!(FilePriority::High.label(), "high");
    }
}
