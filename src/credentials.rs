use std::sync::mpsc;
use std::time::Duration;

/// Save `username` and `password` to the OS keyring for `url`.
///
/// Runs the blocking keyring call in a background thread with a 2-second
/// timeout so callers never hang. Returns `Ok(())` on success or `Err(msg)`
/// with the specific keyring error on failure.
pub fn save(url: &str, username: &str, password: &str) -> Result<(), String> {
    let url = url.to_string();
    let creds = format!("{username}\n{password}");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match keyring::Entry::new("trst", &url) {
            Err(e) => Err(e.to_string()),
            Ok(entry) => entry.set_password(&creds).map_err(|e| e.to_string()),
        };
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "keyring timed out".to_string())
        .and_then(|r| r)
}

/// Load `(username, password)` from the OS keyring for `url`, or `None` if
/// no credentials are stored or the keyring is unavailable.
pub fn load(url: &str) -> Option<(String, String)> {
    let url = url.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let entry = keyring::Entry::new("trst", &url).ok()?;
            let secret = entry.get_password().ok()?;
            let mut parts = secret.splitn(2, '\n');
            let user = parts.next()?.to_string();
            let pass = parts.next()?.to_string();
            Some((user, pass))
        })();
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(2)).ok().flatten()
}

/// Delete credentials from the OS keyring for `url`.
///
/// Returns `Ok(true)` if credentials were removed, `Ok(false)` if nothing was
/// stored (or the keyring service is unavailable), or `Err(msg)` on an
/// unexpected failure.
pub fn delete(url: &str) -> Result<bool, String> {
    let url = url.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match keyring::Entry::new("trst", &url) {
            Err(_) => Ok(false),
            Ok(entry) => match entry.delete_credential() {
                Ok(()) => Ok(true),
                Err(
                    keyring::Error::NoEntry
                    | keyring::Error::PlatformFailure(_)
                    | keyring::Error::NoStorageAccess(_),
                ) => Ok(false),
                Err(e) => Err(e.to_string()),
            },
        };
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "keyring timed out".to_string())
        .and_then(|r| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(suffix: &str) -> String {
        format!("http://trst-creds-test-{suffix}.local:19091/rpc")
    }

    #[test]
    fn save_completes_without_hanging() {
        let _ = save(&url("save"), "user", "pass");
    }

    #[test]
    fn load_returns_none_for_unknown_url() {
        assert!(load(&url("load-miss")).is_none());
    }

    #[test]
    fn delete_returns_ok_for_unknown_url() {
        let result = delete(&url("delete-miss"));
        assert!(matches!(result, Ok(_)));
    }

    #[test]
    fn roundtrip_when_keyring_available() {
        let u = url("roundtrip");
        let _ = delete(&u);
        match save(&u, "alice", "hunter2") {
            Ok(()) => {
                assert_eq!(load(&u), Some(("alice".to_string(), "hunter2".to_string())));
                assert_eq!(delete(&u), Ok(true));
                assert!(load(&u).is_none());
            }
            Err(_) => {
                assert!(load(&u).is_none());
            }
        }
    }
}
