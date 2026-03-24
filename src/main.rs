mod app;
mod client;
mod config;
#[allow(dead_code)]
mod protocol;
mod ui;
mod util;

use clap::Parser;

#[derive(Parser)]
#[command(name = "trst", about = "Transmission remote TUI")]
struct Args {
    /// Transmission RPC URL
    #[arg(short, long, default_value = "http://localhost:9091/transmission/rpc")]
    url: String,

    /// Username for authentication
    #[arg(short = 'n', long)]
    username: Option<String>,

    /// Password for authentication
    #[arg(short = 'p', long)]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let config = config::Config::load();

    let auth = match (&args.username, &args.password) {
        (Some(u), Some(p)) => Some((u.as_str(), p.as_str())),
        _ => None,
    };

    let client = client::TransmissionClient::new(&args.url, auth);
    let app = app::App::new(client, config);

    let terminal = ratatui::init();
    let result = app.run(terminal).await;
    ratatui::restore();
    result
}
