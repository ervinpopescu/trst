mod app;
mod client;
mod config;
mod credentials;
mod protocol;
#[cfg(feature = "rsync")]
mod rsync;
mod ui;
mod util;

struct Args {
    url: String,
    username: Option<String>,
    password: Option<String>,
    clear_auth: bool,
}

fn parse_args() -> Args {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I>(iter: I) -> Args
where
    I: Iterator<Item = String>,
{
    let mut args = Args {
        url: String::new(),
        username: None,
        password: None,
        clear_auth: false,
    };
    let mut host = None;
    let mut iter = iter.peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-u" | "--url" => {
                args.url = iter.next().unwrap_or_else(|| {
                    eprintln!("error: --url requires a value");
                    std::process::exit(1);
                });
            }
            "-n" | "--username" => args.username = iter.next(),
            "-p" | "--password" => {
                args.password = iter.next();
                eprintln!(
                    "warning: passing password via -p is visible in process listings and shell history"
                );
            }
            "--clear-auth" => {
                args.clear_auth = true;
            }
            "-h" | "--help" => {
                println!("trst — Transmission remote TUI\n");
                println!("Usage: trst [HOST[:PORT]] [OPTIONS]\n");
                println!("Arguments:");
                println!(
                    "  [HOST[:PORT] | URL]    Transmission host or full URL [default: localhost:9091]"
                );
                println!("\nOptions:");
                println!("  -u, --url <URL>        Full RPC URL (overrides positional)");
                println!("  -n, --username <USER>  Username for authentication");
                println!("  -p, --password <PASS>  Password for authentication");
                println!(
                    "      --clear-auth       Remove saved credentials for the resolved URL and exit"
                );
                println!("  -h, --help             Print help");
                std::process::exit(0);
            }
            s if !s.starts_with('-') => host = Some(s.to_string()),
            other => {
                eprintln!("error: unknown argument: {other:?}");
                eprintln!("try 'trst --help' for usage");
                std::process::exit(1);
            }
        }
    }
    // Only set url from positional host if --url/-u was not already provided.
    if args.url.is_empty()
        && let Some(h) = host.as_deref()
    {
        args.url = if h.starts_with("http://") || h.starts_with("https://") {
            h.to_string()
        } else {
            let h = if h.contains(':') {
                h.to_string()
            } else {
                format!("{h}:9091")
            };
            format!("http://{h}/transmission/rpc")
        };
        // If neither --url nor a positional host was given, leave args.url empty so
        // main() can fall back to config.connection.url before the hardcoded default.
    }
    args
}

/// Selects the keyring backend for this process.
///
/// On Linux, probes the Secret Service via a background thread with a 2-second
/// timeout. Switches the process-wide builder to the kernel persistent keyring
/// (keyutils_persistent) when Secret Service is unavailable OR locked — a locked
/// keyring silently accepts writes but rejects reads, so we treat it the same as
/// absent. "Unavailable" means anything other than Ok or NoEntry from the probe.
/// Safe to call multiple times — only the first call does work. No-op on other platforms.
#[cfg(target_os = "linux")]
fn init_keyring_backend() {
    static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    DONE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // Write a probe credential then read it back. A locked keyring silently
            // accepts writes but rejects reads — NoEntry on a missing entry is not
            // enough to detect that state.
            let is_unavailable = match keyring::Entry::new("trst-probe", "probe") {
                Err(_) => true,
                Ok(entry) => match entry.set_password("ok") {
                    Err(_) => true,
                    Ok(_) => {
                        let readable = matches!(entry.get_password(), Ok(s) if s == "ok");
                        let _ = entry.delete_credential();
                        !readable
                    }
                },
            };
            let _ = tx.send(is_unavailable);
        });
        let use_fallback = matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Ok(true) | Err(_)
        );
        if use_fallback {
            keyring::set_default_credential_builder(
                keyring::keyutils_persistent::default_credential_builder(),
            );
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn init_keyring_backend() {}
fn main() -> std::io::Result<()> {
    init_keyring_backend();
    let mut config = config::Config::load();
    let args = parse_args();

    let url = if !args.url.is_empty() {
        args.url.clone()
    } else if let Some(u) = config.connection.url.clone() {
        u
    } else {
        "http://localhost:9091/transmission/rpc".to_string()
    };

    let pending_url = (!args.url.is_empty()).then(|| url.clone());

    if args.clear_auth {
        match credentials::delete(&url) {
            Ok(true) => println!("Credentials for {url} removed."),
            Ok(false) => println!("No credentials stored for {url}."),
            Err(e) => eprintln!("error: {e}"),
        }
        config.connection.username = None;
        config.connection.password = None;
        config.save();
        return Ok(());
    }

    let cli_username = args
        .username
        .as_deref()
        .or(config.connection.username.as_deref())
        .map(str::to_string);
    let cli_password = args
        .password
        .as_deref()
        .or(config.connection.password.as_deref())
        .map(str::to_string);

    let auth: Option<(String, String)> = match (&cli_username, &cli_password) {
        (Some(u), Some(p)) => {
            config.connection.url = Some(url.clone());
            if credentials::save(&url, u, p).is_err() {
                config.connection.username = Some(u.clone());
                config.connection.password = Some(p.clone());
            } else {
                config.connection.username = None;
                config.connection.password = None;
            }
            config.save();
            Some((u.clone(), p.clone()))
        }
        _ => {
            // First try to load from the keyring
            credentials::load(&url).or_else(|| {
                // If keyring is empty, check if we have them in the config file
                if let (Some(u), Some(p)) =
                    (&config.connection.username, &config.connection.password)
                {
                    Some((u.clone(), p.clone()))
                } else {
                    None
                }
            })
        }
    };

    if auth.is_some()
        && url.starts_with("http://")
        && !url.contains("localhost")
        && !url.contains("127.0.0.1")
        && !url.contains("[::1]")
    {
        eprintln!("warning: credentials are being sent over plain HTTP to a remote host");
        eprintln!("  consider fronting Transmission with an HTTPS reverse proxy");
    }

    let client = client::TransmissionClient::new(
        &url,
        auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str())),
        config.connection.timeout,
    );
    let app = app::App::new(client, config).with_pending_url_save(pending_url);

    let terminal = ratatui::init();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        prev_hook(info);
    }));
    let result = app.run(terminal);
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests;
