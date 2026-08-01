use serde::{Deserialize, Serialize};

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
    #[allow(dead_code)]
    pub tag: Option<u64>,
}

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
    #[allow(dead_code)]
    pub seeders: i64,
    #[serde(default)]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub size_when_done: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub left_until_done: i64,
    #[serde(default)]
    pub downloaded_ever: i64,
    #[serde(default)]
    pub uploaded_ever: i64,
    #[serde(default)]
    pub queue_position: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub is_finished: bool,
    #[serde(default, rename = "sequential_download")]
    pub sequential_download: bool,
    #[serde(default)]
    pub files: Vec<TorrentFile>,
    #[serde(default)]
    pub file_stats: Vec<FileStats>,
    #[serde(default)]
    pub tracker_stats: Vec<TrackerStats>,
    #[serde(default)]
    #[allow(dead_code)]
    pub peers: Vec<Peer>,
    #[serde(default)]
    pub labels: Vec<String>,
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub last_announce_result: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub last_announce_time: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    #[serde(default)]
    #[allow(dead_code)]
    pub address: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub port: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub client_name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub progress: f64,
    #[serde(default)]
    #[allow(dead_code)]
    pub rate_to_client: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub rate_to_peer: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub flag_str: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub is_encrypted: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    #[serde(default)]
    #[allow(dead_code)]
    pub active_torrent_count: i64,
    #[serde(default)]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub total_size: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub path: String,
}

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
    "sequential_download",
    "trackerStats",
    "labels",
    "downloadDir",
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
    "sequential_download",
    "files",
    "fileStats",
    "trackerStats",
    "peers",
    "labels",
];

#[cfg(test)]
mod tests;
