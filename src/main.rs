//! `tetron-sync-receiver`: a headless, CLI-first home-side rsync receiver
//! for `tetron-mobile-sync` (the GPL-3.0 Android photo-backup addon). This
//! binary is MPL-2.0 and embeds no GPL code -- it only generates
//! `rsyncd.conf` and supervises the system's own stock `rsync --daemon`,
//! the same "no receiver code" spirit tetron-mobile-sync's own SYNC-010
//! docstring commits to, just realized as a real product instead of a
//! bash script.
//!
//! Same "unprivileged client over the existing IPC socket" shape as
//! `tetron-webui`/`tetron-systray`: a per-user service (`systemd --user` /
//! launchd LaunchAgent), no root needed to run, only to place the binary
//! in `/usr/local/bin` alongside its siblings via
//! `contrib/install-tetron-suite.sh` in the tetron core repo.
//!
//! Design record: `DO-NOT-COMMIT/PLAN_tetron-sync-receiver_2026-08-21.md`.
//!
//! CLI-first, not webui-dependent: every subcommand below works with no
//! HTTP server or browser involved. If tetron-webui is installed, it is
//! expected to shell out to this same CLI (with `--json`) for its Add-ons
//! panel rather than reimplementing any of this logic itself.

use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, Subcommand};
use serde_json::json;

mod config;
mod roster;
mod service;

pub(crate) const FULL_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_SHA"), ")");

/// Whether `--json` output mode is active (set once in `main`). Same
/// pattern as tetron core's own `src/main.rs` -- one global flag, not a
/// per-subcommand one, so every subcommand supports it uniformly.
static JSON_FLAG: AtomicBool = AtomicBool::new(false);

fn json_enabled() -> bool {
    JSON_FLAG.load(Ordering::Relaxed)
}

#[derive(Parser)]
#[command(name = "tetron-sync-receiver", version = FULL_VERSION)]
struct Cli {
    /// Emit machine-readable JSON instead of styled text. Supported by
    /// every subcommand that prints data (`receiver status`, `module list`,
    /// `allow list`) -- exactly `tetron` core's own `--json` convention
    /// (`global = true`, so it works at any subcommand position).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: TopCmd,
}

#[derive(Subcommand)]
enum TopCmd {
    /// Install and start the per-user service (systemd --user on Linux, a
    /// launchd LaunchAgent on macOS)
    Install {
        /// Port to bind the rsync daemon on. Must be >1024 -- this process
        /// never runs as root, so it can't bind a privileged port.
        #[arg(short = 'p', long, default_value_t = config::DEFAULT_PORT)]
        port: u16,
    },
    /// Stop and remove the per-user service
    Uninstall,
    /// Print the tetron-sync-receiver version
    #[command(visible_alias = "ver")]
    Version,
    /// Foreground entry point the installed service execs into -- not
    /// meant to be run by hand. Execs directly into the system `rsync`
    /// binary (`--no-detach` so it stays the tracked process; systemd's own
    /// `Restart=on-failure` covers crash-restart, no bespoke supervision
    /// loop needed here).
    #[command(hide = true)]
    Run,
    /// Start/stop/check the already-installed service without touching its
    /// registration
    #[command(subcommand)]
    Receiver(ReceiverCmd),
    /// Manage rsyncd modules (name -> path mappings)
    #[command(subcommand)]
    Module(ModuleCmd),
    /// Manage the mesh-IP allow-list
    #[command(subcommand)]
    Allow(AllowCmd),
}

#[derive(Subcommand)]
enum ReceiverCmd {
    /// Start the already-installed service
    Enable,
    /// Stop the already-installed service
    Disable,
    /// Report whether the service is running, plus a config summary
    Status,
    /// Change the port, restarting the service if it's currently running
    /// (unlike module/allow edits, a port change can't take effect without
    /// a restart -- the port is a command-line argument to the supervised
    /// `rsync --daemon` process, not something inside rsyncd.conf it
    /// re-reads per connection)
    Port { port: u16 },
}

#[derive(Subcommand)]
enum ModuleCmd {
    /// Point the default module at a backup directory -- the one command a
    /// normal setup runs. Creates the `tetron-sync` module if this receiver
    /// has none, repoints it if it already exists. Any other (advanced,
    /// renamed) module is left untouched.
    SetDir { path: String },
    /// Add (or replace) a module. Without `--name` this manages the default
    /// `tetron-sync` module (same as `set-dir`); `--name` is for advanced
    /// multi-module setups only and must be matched by the phone by hand.
    Add {
        #[arg(long, default_value_t = config::DEFAULT_MODULE.to_owned())]
        name: String,
        path: String,
    },
    /// Remove a module by name (defaults to the `tetron-sync` module)
    Remove {
        #[arg(default_value_t = config::DEFAULT_MODULE.to_owned())]
        name: String,
    },
    /// List configured modules
    List,
}

#[derive(Subcommand)]
enum AllowCmd {
    /// Allow a peer by mesh IP or by hostname. A bare IP is stored directly
    /// (always works, no local tetron daemon needed); anything else is
    /// resolved as a hostname via the local tetron daemon's own peer roster
    /// (IPC, not tetron-webui) -- the receiver already has this itself.
    Add { value: String },
    /// Deprecated: use `allow add <hostname>`. Kept one release as an alias.
    #[command(hide = true)]
    AddPeer { hostname: String },
    /// Remove an allowed IP
    Remove { ip: String },
    /// List allowed IPs
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    JSON_FLAG.store(cli.json, Ordering::Relaxed);

    match cli.command {
        TopCmd::Install { port } => {
            let mut state = config::load()?;
            state.port = port;
            config::save(&state)?;
            service::install(port)?;
        }
        TopCmd::Uninstall => service::uninstall()?,
        TopCmd::Version => println!("tetron-sync-receiver {FULL_VERSION}"),
        TopCmd::Run => {
            let state = config::load()?;
            let conf_path = config::rsyncd_conf_path()?;
            let err = Command::new("rsync")
                .arg("--daemon")
                .arg("--no-detach")
                .arg(format!("--config={}", conf_path.display()))
                .arg(format!("--port={}", state.port))
                .exec();
            anyhow::bail!("failed to exec rsync --daemon: {err}");
        }
        TopCmd::Receiver(cmd) => run_receiver_cmd(cmd)?,
        TopCmd::Module(cmd) => run_module_cmd(cmd)?,
        TopCmd::Allow(cmd) => run_allow_cmd(cmd).await?,
    }
    Ok(())
}

fn run_receiver_cmd(cmd: ReceiverCmd) -> anyhow::Result<()> {
    match cmd {
        ReceiverCmd::Enable => service::start()?,
        ReceiverCmd::Disable => service::stop()?,
        ReceiverCmd::Status => {
            let state = config::load()?;
            let active = service::is_active();
            if json_enabled() {
                println!(
                    "{}",
                    json!({
                        "active": active,
                        "port": state.port,
                        "module_count": state.modules.len(),
                        "allow_count": state.allow.len(),
                    })
                );
            } else {
                println!("service: {}", if active { "running" } else { "stopped" });
                println!("port: {}", state.port);
                println!("modules: {}", state.modules.len());
                println!("allowed IPs: {}", state.allow.len());
            }
        }
        ReceiverCmd::Port { port } => {
            let mut state = config::load()?;
            state.port = port;
            config::save(&state)?;
            service::restart_if_active()?;
        }
    }
    Ok(())
}

/// A module name becomes an `[name]` section header in `rsyncd.conf`, so it
/// must be a single clean token.
fn validate_module_name(name: String) -> anyhow::Result<String> {
    let n = name.trim();
    anyhow::ensure!(!n.is_empty(), "module name cannot be empty");
    anyhow::ensure!(
        !n.contains(['[', ']', '/', '\n', '\r', '\t', ' ']),
        "module name '{n}' must be one word with no spaces, slashes or brackets"
    );
    Ok(n.to_string())
}

/// The module path becomes a `path = ...` line and is the root every push
/// lands under. Require an absolute path (the daemon's working directory is
/// not something the operator should have to reason about), reject a newline
/// (it would corrupt the generated file), and create the directory if it is
/// missing so the first transfer does not fail with a confusing daemon-side
/// error. The client creates the per-device subdirectories itself.
fn validate_module_path(path: String) -> anyhow::Result<String> {
    anyhow::ensure!(
        !path.contains(['\n', '\r']),
        "module path cannot contain a newline"
    );
    let p = std::path::Path::new(&path);
    anyhow::ensure!(p.is_absolute(), "module path must be absolute, got '{path}'");
    std::fs::create_dir_all(p)
        .map_err(|e| anyhow::anyhow!("failed to create module directory {path}: {e}"))?;
    Ok(path)
}

fn run_module_cmd(cmd: ModuleCmd) -> anyhow::Result<()> {
    let mut state = config::load()?;
    match cmd {
        ModuleCmd::SetDir { path } => {
            let name = config::DEFAULT_MODULE.to_string();
            let path = validate_module_path(path)?;
            state.modules.retain(|m| m.name != name);
            state.modules.push(config::Module { name, path });
            config::save(&state)?;
        }
        ModuleCmd::Add { name, path } => {
            let name = validate_module_name(name)?;
            let path = validate_module_path(path)?;
            state.modules.retain(|m| m.name != name);
            state.modules.push(config::Module { name, path });
            config::save(&state)?;
        }
        ModuleCmd::Remove { name } => {
            let before = state.modules.len();
            state.modules.retain(|m| m.name != name);
            anyhow::ensure!(state.modules.len() != before, "no module named '{name}'");
            config::save(&state)?;
        }
        ModuleCmd::List => {
            if json_enabled() {
                let list: Vec<_> = state
                    .modules
                    .iter()
                    .map(|m| json!({"name": m.name, "path": m.path}))
                    .collect();
                println!("{}", json!(list));
            } else {
                for m in &state.modules {
                    println!("{}\t{}", m.name, m.path);
                }
            }
        }
    }
    Ok(())
}

/// `allow add` takes either form: a bare IP is stored as-is (no daemon
/// needed), anything else is looked up as a mesh hostname. Keeps the webui
/// and CLI down to one "address" field instead of two.
async fn resolve_allow_value(value: &str) -> anyhow::Result<String> {
    let v = value.trim();
    if v.parse::<std::net::IpAddr>().is_ok() {
        Ok(v.to_string())
    } else {
        roster::resolve_hostname(v).await
    }
}

async fn run_allow_cmd(cmd: AllowCmd) -> anyhow::Result<()> {
    let mut state = config::load()?;
    match cmd {
        AllowCmd::Add { value } => {
            let ip = resolve_allow_value(&value).await?;
            if !state.allow.contains(&ip) {
                state.allow.push(ip);
            }
            config::save(&state)?;
        }
        AllowCmd::AddPeer { hostname } => {
            let ip = roster::resolve_hostname(&hostname).await?;
            if !state.allow.contains(&ip) {
                state.allow.push(ip);
            }
            config::save(&state)?;
        }
        AllowCmd::Remove { ip } => {
            let before = state.allow.len();
            state.allow.retain(|a| a != &ip);
            anyhow::ensure!(state.allow.len() != before, "'{ip}' is not in the allow-list");
            config::save(&state)?;
        }
        AllowCmd::List => {
            if json_enabled() {
                println!("{}", json!(state.allow));
            } else {
                for ip in &state.allow {
                    println!("{ip}");
                }
            }
        }
    }
    Ok(())
}
