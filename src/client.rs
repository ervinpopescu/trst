use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protocol::*;

pub struct TransmissionClient {
    http: Client,
    url: String,
    auth: Option<(String, String)>,
    session_id: Arc<Mutex<Option<String>>>,
}

impl TransmissionClient {
    pub fn new(url: &str, auth: Option<(&str, &str)>) -> Self {
        Self {
            http: Client::new(),
            url: url.to_string(),
            auth: auth.map(|(u, p)| (u.to_string(), p.to_string())),
            session_id: Arc::new(Mutex::new(None)),
        }
    }

    async fn rpc(&self, method: &str, args: Option<Value>) -> Result<RpcResponse, String> {
        let body = RpcRequest {
            method,
            arguments: args,
            tag: None,
        };

        for _ in 0..2 {
            let mut req = self.http.post(&self.url).json(&body);
            if let Some((user, pass)) = &self.auth {
                req = req.basic_auth(user, Some(pass));
            }
            if let Some(sid) = self.session_id.lock().await.as_deref() {
                req = req.header("X-Transmission-Session-Id", sid);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;

            if resp.status() == StatusCode::CONFLICT {
                if let Some(sid) = resp.headers().get("X-Transmission-Session-Id") {
                    *self.session_id.lock().await =
                        Some(sid.to_str().unwrap_or_default().to_string());
                    continue;
                }
                return Err("409 without session ID header".into());
            }

            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }

            let rpc: RpcResponse = resp.json().await.map_err(|e| e.to_string())?;
            if rpc.result != "success" {
                return Err(format!("RPC error: {}", rpc.result));
            }
            return Ok(rpc);
        }

        Err("session ID negotiation failed".into())
    }

    pub async fn get_torrents(&self, fields: &[&str]) -> Result<Vec<Torrent>, String> {
        let args = json!({ "fields": fields });
        let resp = self.rpc("torrent-get", Some(args)).await?;
        let torrents: Vec<Torrent> =
            serde_json::from_value(resp.arguments["torrents"].clone())
                .map_err(|e| e.to_string())?;
        Ok(torrents)
    }

    pub async fn get_torrent(
        &self,
        id: i64,
        fields: &[&str],
    ) -> Result<Option<Torrent>, String> {
        let args = json!({ "ids": [id], "fields": fields });
        let resp = self.rpc("torrent-get", Some(args)).await?;
        let torrents: Vec<Torrent> =
            serde_json::from_value(resp.arguments["torrents"].clone())
                .map_err(|e| e.to_string())?;
        Ok(torrents.into_iter().next())
    }

    async fn torrent_action(&self, method: &str, ids: &[i64]) -> Result<(), String> {
        let args = json!({ "ids": ids });
        self.rpc(method, Some(args)).await?;
        Ok(())
    }

    pub async fn start(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-start", ids).await
    }

    pub async fn stop(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-stop", ids).await
    }

    pub async fn verify(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-verify", ids).await
    }

    pub async fn reannounce(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-reannounce", ids).await
    }

    pub async fn remove(&self, ids: &[i64], delete_local: bool) -> Result<(), String> {
        let args = json!({
            "ids": ids,
            "delete-local-data": delete_local,
        });
        self.rpc("torrent-remove", Some(args)).await?;
        Ok(())
    }

    pub async fn add(&self, location: &str) -> Result<(), String> {
        let args = json!({ "filename": location });
        self.rpc("torrent-add", Some(args)).await?;
        Ok(())
    }

    pub async fn set_file_priorities(
        &self,
        torrent_id: i64,
        priorities: &[(usize, FilePriority)],
    ) -> Result<(), String> {
        let mut high = vec![];
        let mut normal = vec![];
        let mut low = vec![];
        let mut wanted = vec![];
        let mut unwanted = vec![];

        for &(idx, prio) in priorities {
            match prio {
                FilePriority::High => {
                    high.push(idx);
                    wanted.push(idx);
                }
                FilePriority::Normal => {
                    normal.push(idx);
                    wanted.push(idx);
                }
                FilePriority::Low => {
                    low.push(idx);
                    wanted.push(idx);
                }
                FilePriority::Unwanted => {
                    unwanted.push(idx);
                }
            }
        }

        let mut args = json!({ "ids": [torrent_id] });
        let obj = args.as_object_mut().unwrap();
        if !high.is_empty() {
            obj.insert("priority-high".into(), json!(high));
        }
        if !normal.is_empty() {
            obj.insert("priority-normal".into(), json!(normal));
        }
        if !low.is_empty() {
            obj.insert("priority-low".into(), json!(low));
        }
        if !wanted.is_empty() {
            obj.insert("files-wanted".into(), json!(wanted));
        }
        if !unwanted.is_empty() {
            obj.insert("files-unwanted".into(), json!(unwanted));
        }

        self.rpc("torrent-set", Some(args)).await?;
        Ok(())
    }

    pub async fn queue_move(&self, method: &str, ids: &[i64]) -> Result<(), String> {
        self.torrent_action(method, ids).await
    }

    pub async fn session_stats(&self) -> Result<SessionStats, String> {
        let resp = self.rpc("session-stats", None).await?;
        serde_json::from_value(resp.arguments).map_err(|e| e.to_string())
    }

    pub async fn free_space(&self, path: &str) -> Result<FreeSpace, String> {
        let args = json!({ "path": path });
        let resp = self.rpc("free-space", Some(args)).await?;
        serde_json::from_value(resp.arguments).map_err(|e| e.to_string())
    }
}
