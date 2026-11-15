//! figma-font-agent — expose the fonts installed on this Linux machine to Figma
//! running in a browser.
//!
//! This is an independent reimplementation of the local agent that Figma ships
//! for macOS and Windows; see `docs/protocol.md` and
//! `docs/reverse-engineering.md`.

#[macro_use]
mod logging;

mod config;
mod fonts;
mod preview;
mod proto;
mod server;

use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use tokio::sync::RwLock;

use crate::{logging::Level, server::AppState};

const HELP: &str = "\
figma-font-agent — serve Linux system fonts to Figma in the browser

USAGE:
    figma-font-agent [OPTIONS]

OPTIONS:
    -c, --config <PATH>   Config file to read (default: $XDG_CONFIG_HOME/figma-font-agent/config.json)
        --init-config     Write an example config file and exit
    -p, --port <PORT>     Listen on <PORT>; repeat to listen on several ports
                          (default: 44950, with 44960 as a fallback)
        --no-preview      Disable the SVG font-preview endpoint
        --print-index     Print the /figma/font-files payload and exit
    -v, --verbose         Verbose (debug) logging
    -h, --help            Print this help
    -V, --version         Print the version

The agent listens on 127.0.0.1 only. In Figma, open
Preferences → Permissions and helpers → Apps on device, then click Connect.
";

#[derive(Debug, Default)]
struct Args {
    config: Option<PathBuf>,
    ports: Option<Vec<u16>>,
    init_config: bool,
    print_index: bool,
    no_preview: bool,
    verbose: bool,
    help: bool,
    version: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = Args::default();
        let mut arguments = std::env::args().skip(1);

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => args.help = true,
                "-V" | "--version" => args.version = true,
                "-v" | "--verbose" => args.verbose = true,
                "--init-config" => args.init_config = true,
                "--print-index" => args.print_index = true,
                "--no-preview" => args.no_preview = true,
                "-c" | "--config" => {
                    let path = arguments
                        .next()
                        .context("--config requires a path argument")?;
                    args.config = Some(PathBuf::from(path));
                }
                "-p" | "--port" => {
                    let port = arguments
                        .next()
                        .context("--port requires a number argument")?;
                    args.ports
                        .get_or_insert_with(Vec::new)
                        .push(parse_port(&port)?);
                }
                other if other.starts_with("--port=") => {
                    let port = &other["--port=".len()..];
                    args.ports
                        .get_or_insert_with(Vec::new)
                        .push(parse_port(port)?);
                }
                other if other.starts_with("--config=") => {
                    args.config = Some(PathBuf::from(&other["--config=".len()..]));
                }
                other => bail!("unknown argument `{other}` (try --help)"),
            }
        }

        Ok(args)
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        error!("{error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse()?;

    if args.help {
        print!("{HELP}");
        return Ok(());
    }
    if args.version {
        println!("figma-font-agent {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    logging::set_level(if args.verbose {
        Level::Debug
    } else {
        Level::Info
    });

    if args.init_config {
        let path = config::default_config_path().context("could not determine a config path")?;
        config::write_example(&path)?;
        println!("{}", path.display());
        return Ok(());
    }

    let mut settings = config::load(args.config.as_deref())?;
    if let Some(ports) = args.ports {
        if ports.is_empty() {
            bail!("at least one --port is required");
        }
        settings.ports = ports;
    }
    if args.no_preview {
        settings.enable_preview = false;
    }

    info!(
        "figma-font-agent {} (reporting package {}, version {})",
        env!("CARGO_PKG_VERSION"),
        settings.package,
        settings.version
    );
    if let Some(path) = &settings.config_path {
        info!("config: {}", path.display());
    }

    let scan_settings = settings.clone();
    let index = tokio::task::spawn_blocking(move || fonts::scan(&scan_settings, None)).await??;

    if args.print_index {
        let payload = proto::font_files_response(&index, &settings);
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let state = AppState {
        index: Arc::new(RwLock::new(index)),
        settings: Arc::new(settings),
    };

    let activated = socket_activated();
    let listeners = if activated.is_empty() {
        server::bind(&state.settings.ports).await
    } else {
        info!(
            "using {} socket-activated listener(s) from systemd",
            activated.len()
        );
        activated
    };

    if listeners.is_empty() {
        bail!(
            "could not bind any port ({:?}); is another Figma agent already running?",
            state.settings.ports
        );
    }

    server::spawn_rescan_timer(state.clone());
    server::serve(state, listeners).await
}

fn parse_port(value: &str) -> Result<u16> {
    let port: u16 = value
        .parse()
        .with_context(|| format!("`{value}` is not a valid port number"))?;

    if port == 0 {
        bail!("port 0 is not a usable listening port");
    }

    Ok(port)
}

/// systemd socket activation (`LISTEN_FDS`/`LISTEN_PID`). Using it means the
/// agent can start on the first request instead of burning a process at login.
fn socket_activated() -> Vec<tokio::net::TcpListener> {
    use std::os::fd::FromRawFd;

    let Ok(count) = std::env::var("LISTEN_FDS") else {
        return Vec::new();
    };
    let Ok(count) = count.parse::<i32>() else {
        return Vec::new();
    };

    let pid_matches = std::env::var("LISTEN_PID")
        .ok()
        .and_then(|pid| pid.parse::<u32>().ok())
        == Some(std::process::id());

    if count <= 0 || !pid_matches {
        return Vec::new();
    }

    // Do not leak the activation environment into `fc-list` and friends.
    std::env::remove_var("LISTEN_FDS");
    std::env::remove_var("LISTEN_PID");

    let mut listeners = Vec::new();

    for index in 0..count {
        // SAFETY: systemd passes fds starting at 3 and hands over ownership.
        let listener = unsafe { std::net::TcpListener::from_raw_fd(3 + index) };
        let _ = listener.set_nonblocking(true);

        match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listeners.push(listener),
            Err(error) => warn!("socket activation: fd {} is unusable: {error}", 3 + index),
        }
    }

    listeners
}
