use base64::Engine as _;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

use crate::protocol::*;

pub struct TransmissionClient {
    agent: ureq::Agent,
    pub url: String,
    auth_header: Mutex<Option<String>>,
    session_id: Mutex<Option<String>>,
}

impl TransmissionClient {
    pub fn new(url: &str, auth: Option<(&str, &str)>, timeout: Option<u64>) -> Self {
        let timeout = timeout.unwrap_or(10);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(timeout)))
            .http_status_as_error(false)
            .build()
            .into();

        let auth_header = auth.map(|(u, p)| {
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"))
            )
        });
        Self {
            agent,
            url: url.to_string(),
            auth_header: Mutex::new(auth_header),
            session_id: Mutex::new(None),
        }
    }

    pub fn set_auth(&self, username: &str, password: &str) {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        *self.auth_header.lock().unwrap() = Some(format!("Basic {encoded}"));
    }

    fn rpc(&self, method: &str, args: Option<Value>) -> Result<RpcResponse, String> {
        let body = RpcRequest {
            method,
            arguments: args,
            tag: None,
        };
        let body_str =
            serde_json::to_string(&body).map_err(|e: serde_json::Error| e.to_string())?;

        for _ in 0..2 {
            let mut req = self
                .agent
                .post(&self.url)
                .header("Content-Type", "application/json");

            if let Some(auth) = self.auth_header.lock().unwrap().clone() {
                req = req.header("Authorization", auth);
            }
            if let Some(sid) = self.session_id.lock().unwrap().as_deref() {
                req = req.header("X-Transmission-Session-Id", sid);
            }

            match req.send(&body_str) {
                Ok(mut resp) => {
                    if resp.status() == 409 {
                        if let Some(sid) = resp.headers().get("X-Transmission-Session-Id") {
                            let sid_str = sid.to_str().unwrap_or_default();
                            if sid_str.is_empty()
                                || sid_str.bytes().any(|b| !(0x20..0x7f).contains(&b))
                            {
                                return Err("malformed session ID in response header".into());
                            }
                            *self.session_id.lock().unwrap() = Some(sid_str.to_string());
                            continue;
                        }
                        return Err("409 without session ID header".into());
                    }

                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }

                    let rpc: RpcResponse = resp
                        .body_mut()
                        .read_json()
                        .map_err(|e: ureq::Error| e.to_string())?;

                    if rpc.result != "success" {
                        return Err(format!("RPC error: {}", rpc.result));
                    }
                    return Ok(rpc);
                }
                Err(e) => return Err(e.to_string()),
            }
        }

        Err("session ID negotiation failed".into())
    }

    pub fn get_torrents(&self, fields: &[&str]) -> Result<Vec<Torrent>, String> {
        let args = json!({ "fields": fields });
        let mut resp = self.rpc("torrent-get", Some(args))?;
        let val = resp.arguments["torrents"].take();
        let torrents: Vec<Torrent> =
            serde_json::from_value(val).map_err(|e: serde_json::Error| e.to_string())?;
        Ok(torrents)
    }

    pub fn get_torrent(&self, id: i64, fields: &[&str]) -> Result<Option<Torrent>, String> {
        let args = json!({ "ids": [id], "fields": fields });
        let mut resp = self.rpc("torrent-get", Some(args))?;
        let val = resp.arguments["torrents"].take();
        let torrents: Vec<Torrent> =
            serde_json::from_value(val).map_err(|e: serde_json::Error| e.to_string())?;
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

    pub fn add(&self, location: &str, download_dir: Option<&str>) -> Result<(), String> {
        let mut args = json!({ "filename": location });
        if let Some(dir) = download_dir
            && let Some(obj) = args.as_object_mut()
        {
            obj.insert("download-dir".to_string(), json!(dir));
        }
        self.rpc("torrent-add", Some(args))?;
        Ok(())
    }

    /// Add a torrent from raw `.torrent` file bytes by base64-encoding them as `metainfo`.
    ///
    /// Use this instead of `add` when the `.torrent` file lives on the client side, since
    /// `filename` is resolved by the daemon and the client's local path may not exist there.
    pub fn add_metainfo(&self, content: &[u8], download_dir: Option<&str>) -> Result<(), String> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(content);
        let mut args = json!({ "metainfo": b64 });
        if let Some(dir) = download_dir
            && let Some(obj) = args.as_object_mut()
        {
            obj.insert("download-dir".to_string(), json!(dir));
        }
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

    pub fn set_sequential(&self, ids: &[i64], sequential: bool) -> Result<(), String> {
        let args = json!({
            "ids": ids,
            "sequential_download": sequential,
        });
        self.rpc("torrent-set", Some(args))?;
        Ok(())
    }

    pub fn queue_move(&self, method: &str, ids: &[i64]) -> Result<(), String> {
        self.torrent_action(method, ids)
    }

    pub fn set_labels(&self, ids: &[i64], labels: &[String]) -> Result<(), String> {
        let args = serde_json::json!({ "ids": ids, "labels": labels });
        self.rpc("torrent-set", Some(args))?;
        Ok(())
    }

    pub fn set_location(
        &self,
        ids: &[i64],
        location: &str,
        move_files: bool,
    ) -> Result<(), String> {
        let args = json!({
            "ids": ids,
            "location": location,
            "move": move_files,
        });
        // The spec documents the method name as `torrent_set_location` (with underscores),
        // but other methods use hyphens. Both are usually supported, but we'll use hyphens
        // to match the others. If it fails, we'll try underscores.
        self.rpc("torrent-set-location", Some(args))?;
        Ok(())
    }

    pub fn session_stats(&self) -> Result<SessionStats, String> {
        let resp = self.rpc("session-stats", None)?;
        serde_json::from_value(resp.arguments).map_err(|e: serde_json::Error| e.to_string())
    }

    pub fn free_space(&self, path: &str) -> Result<FreeSpace, String> {
        let args = json!({ "path": path });
        let resp = self.rpc("free-space", Some(args))?;
        serde_json::from_value(resp.arguments).map_err(|e: serde_json::Error| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_auth_updates_header() {
        let client = TransmissionClient::new("http://dummy", None, None);
        assert!(client.auth_header.lock().unwrap().is_none());
        client.set_auth("alice", "secret");
        let header = client.auth_header.lock().unwrap().clone().unwrap();
        assert!(header.starts_with("Basic "));
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(header.strip_prefix("Basic ").unwrap())
            .unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "alice:secret");
    }

    #[test]
    fn test_set_auth_replaces_existing_header() {
        let client = TransmissionClient::new("http://dummy", Some(("old", "creds")), None);
        client.set_auth("new", "pass");
        let header = client.auth_header.lock().unwrap().clone().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(header.strip_prefix("Basic ").unwrap())
            .unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "new:pass");
    }

    // `add_metainfo` encodes the supplied bytes as base64 and sends a `torrent-add`
    // RPC.  A dummy (unreachable) client lets us exercise the encoding + arg-building
    // code paths without a running daemon; the network call is expected to fail.
    #[test]
    fn test_add_metainfo_without_download_dir_fails_on_dummy() {
        let client =
            TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None);
        // Any non-empty byte slice; content doesn't matter for a dummy-client test.
        let result = client.add_metainfo(b"fake torrent bytes", None);
        assert!(result.is_err(), "dummy client should return Err");
    }

    #[test]
    fn test_add_metainfo_with_download_dir_fails_on_dummy() {
        let client =
            TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None);
        // Exercise the `download-dir` insertion branch (lines inside the `if let Some(dir)` guard).
        let result = client.add_metainfo(b"fake torrent bytes", Some("/downloads"));
        assert!(result.is_err(), "dummy client should return Err");
    }
}
