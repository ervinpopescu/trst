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
    assert!(result.is_ok());
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
