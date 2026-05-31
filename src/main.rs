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

fn main() -> std::io::Result<()> {
    let mut config = config::Config::load();
    let args = parse_args();

    let url = if !args.url.is_empty() {
        args.url.clone()
    } else if let Some(u) = config.connection.url.clone() {
        u
    } else {
        "http://localhost:9091/transmission/rpc".to_string()
    };

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
            if let Err(e) = credentials::save(&url, u, p) {
                eprintln!(
                    "warning: {e}; credentials not saved — you will need to supply them again next session"
                );
            }
            config.connection.url = Some(url.clone());
            config.connection.username = None;
            config.connection.password = None;
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
    let app = app::App::new(client, config);

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
mod tests {
    use super::*;

    fn args(slice: &[&str]) -> impl Iterator<Item = String> {
        slice
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// No arguments → url is empty (main() will fall back to config / hardcoded default).
    #[test]
    fn test_parse_args_no_args_url_is_empty() {
        let a = parse_args_from(args(&[]));
        assert!(a.url.is_empty(), "url must be empty when no args are given");
        assert!(a.username.is_none());
        assert!(a.password.is_none());
    }

    /// Positional host argument → url is derived from the host.
    #[test]
    fn test_parse_args_positional_host_sets_url() {
        let a = parse_args_from(args(&["myserver"]));
        assert_eq!(a.url, "http://myserver:9091/transmission/rpc");
    }

    /// Positional host with explicit port.
    #[test]
    fn test_parse_args_positional_host_with_port_sets_url() {
        let a = parse_args_from(args(&["myserver:8080"]));
        assert_eq!(a.url, "http://myserver:8080/transmission/rpc");
    }

    /// Positional full HTTP URL is passed through unchanged.
    #[test]
    fn test_parse_args_positional_full_url_passthrough() {
        let a = parse_args_from(args(&["http://myserver/transmission/rpc"]));
        assert_eq!(a.url, "http://myserver/transmission/rpc");
    }

    /// --url flag sets the url.
    #[test]
    fn test_parse_args_url_flag_sets_url() {
        let a = parse_args_from(args(&["--url", "http://remotehost:9091/transmission/rpc"]));
        assert_eq!(a.url, "http://remotehost:9091/transmission/rpc");
    }

    /// -u short flag also sets the url.
    #[test]
    fn test_parse_args_url_short_flag_sets_url() {
        let a = parse_args_from(args(&["-u", "http://remotehost/transmission/rpc"]));
        assert_eq!(a.url, "http://remotehost/transmission/rpc");
    }

    /// --url flag takes precedence over a positional host.
    #[test]
    fn test_parse_args_url_flag_overrides_positional() {
        let a = parse_args_from(args(&[
            "somehost",
            "--url",
            "http://explicit/transmission/rpc",
        ]));
        assert_eq!(a.url, "http://explicit/transmission/rpc");
    }

    #[test]
    fn test_parse_args_clear_auth_flag() {
        let a = parse_args_from(args(&["--clear-auth"]));
        assert!(a.clear_auth);
        assert!(a.url.is_empty());
    }

    #[test]
    fn test_parse_args_clear_auth_with_host() {
        let a = parse_args_from(args(&["myserver:9092", "--clear-auth"]));
        assert!(a.clear_auth);
        assert_eq!(a.url, "http://myserver:9092/transmission/rpc");
    }
}
