use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;

use crate::protocol::*;

pub struct TransmissionClient {
    agent: ureq::Agent,
    url: String,
    auth_header: Option<String>,
    session_id: Mutex<Option<String>>,
}

impl TransmissionClient {
    pub fn new(url: &str, auth: Option<(&str, &str)>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(10))
            .build();
        let auth_header = auth.map(|(u, p)| {
            format!("Basic {}", base64_encode(&format!("{u}:{p}")))
        });
        Self {
            agent,
            url: url.to_string(),
            auth_header,
            session_id: Mutex::new(None),
        }
    }

    fn rpc(&self, method: &str, args: Option<Value>) -> Result<RpcResponse, String> {
        let body = RpcRequest {
            method,
            arguments: args,
            tag: None,
        };
        let body_str = serde_json::to_string(&body).map_err(|e| e.to_string())?;

        for _ in 0..2 {
            let mut req = self
                .agent
                .post(&self.url)
                .set("Content-Type", "application/json");

            if let Some(auth) = &self.auth_header {
                req = req.set("Authorization", auth);
            }
            if let Some(sid) = self.session_id.lock().unwrap().as_deref() {
                req = req.set("X-Transmission-Session-Id", sid);
            }

            match req.send_string(&body_str) {
                Ok(resp) => {
                    let text = resp.into_string().map_err(|e| e.to_string())?;
                    let rpc: RpcResponse =
                        serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if rpc.result != "success" {
                        return Err(format!("RPC error: {}", rpc.result));
                    }
                    return Ok(rpc);
                }
                Err(ureq::Error::Status(409, resp)) => {
                    if let Some(sid) = resp.header("X-Transmission-Session-Id") {
                        *self.session_id.lock().unwrap() = Some(sid.to_string());
                        continue;
                    }
                    return Err("409 without session ID header".into());
                }
                Err(ureq::Error::Status(code, _)) => {
                    return Err(format!("HTTP {code}"));
                }
                Err(e) => return Err(e.to_string()),
            }
        }

        Err("session ID negotiation failed".into())
    }

    pub fn get_torrents(&self, fields: &[&str]) -> Result<Vec<Torrent>, String> {
        let args = json!({ "fields": fields });
        let resp = self.rpc("torrent-get", Some(args))?;
        let torrents: Vec<Torrent> =
            serde_json::from_value(resp.arguments["torrents"].clone())
                .map_err(|e| e.to_string())?;
        Ok(torrents)
    }

    pub fn get_torrent(&self, id: i64, fields: &[&str]) -> Result<Option<Torrent>, String> {
        let args = json!({ "ids": [id], "fields": fields });
        let resp = self.rpc("torrent-get", Some(args))?;
        let torrents: Vec<Torrent> =
            serde_json::from_value(resp.arguments["torrents"].clone())
                .map_err(|e| e.to_string())?;
        Ok(torrents.into_iter().next())
    }

    fn torrent_action(&self, method: &str, ids: &[i64]) -> Result<(), String> {
        let args = json!({ "ids": ids });
        self.rpc(method, Some(args))?;
        Ok(())
    }

    pub fn start(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-start", ids)
    }

    pub fn stop(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-stop", ids)
    }

    pub fn verify(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-verify", ids)
    }

    pub fn reannounce(&self, ids: &[i64]) -> Result<(), String> {
        self.torrent_action("torrent-reannounce", ids)
    }

    pub fn remove(&self, ids: &[i64], delete_local: bool) -> Result<(), String> {
        let args = json!({
            "ids": ids,
            "delete-local-data": delete_local,
        });
        self.rpc("torrent-remove", Some(args))?;
        Ok(())
    }

    pub fn add(&self, location: &str) -> Result<(), String> {
        let args = json!({ "filename": location });
        self.rpc("torrent-add", Some(args))?;
        Ok(())
    }

    pub fn set_file_priorities(
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

        self.rpc("torrent-set", Some(args))?;
        Ok(())
    }

    pub fn queue_move(&self, method: &str, ids: &[i64]) -> Result<(), String> {
        self.torrent_action(method, ids)
    }

    pub fn session_stats(&self) -> Result<SessionStats, String> {
        let resp = self.rpc("session-stats", None)?;
        serde_json::from_value(resp.arguments).map_err(|e| e.to_string())
    }

    pub fn free_space(&self, path: &str) -> Result<FreeSpace, String> {
        let args = json!({ "path": path });
        let resp = self.rpc("free-space", Some(args))?;
        serde_json::from_value(resp.arguments).map_err(|e| e.to_string())
    }
}

fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}
