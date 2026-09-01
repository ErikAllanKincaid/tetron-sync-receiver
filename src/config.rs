//! Managed state: the set of modules (name -> path) and allowed mesh IPs
//! this receiver exposes, persisted as JSON, with a generated `rsyncd.conf`
//! kept in lockstep on every mutation.
//!
//! `rsyncd.conf` itself is a derived artifact, never hand-edited -- `State`
//! in `state.json` is the single source of truth `module`/`allow`/`port`
//! subcommands read and write. rsync's daemon re-reads `rsyncd.conf` on
//! every new connection (documented rsync behavior), so a module/allow edit
//! here takes effect immediately with no daemon restart; only a port change
//! needs one -- currently only reachable via `install --port`, which
//! already restarts the service itself (see `service::install`).
//!
//! No `uid`/`gid` directives in the generated file: those are silently
//! ignored by rsync unless the daemon runs as root (it deliberately never
//! does here -- see main.rs), so omitting them avoids implying a privilege
//! this process does not have.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default port: derived from rsync's own standard port 873, shifted into
/// the non-privileged range (>1024) so this never needs root to bind it --
/// the whole point of running as a per-user service. Not 8873 (an earlier
/// choice) -- the 8000-9000 range is heavily squatted by common dev tools
/// (django runserver 8000, various http-server/proxy defaults 8080,
/// Jupyter 8888, etc.), so 28873 sits clear of that range and of the Linux
/// ephemeral port range (`net.ipv4.ip_local_port_range`, typically
/// 32768-60999) while keeping the same "873" mnemonic.
pub const DEFAULT_PORT: u16 = 28873;

/// Default module name. The phone (`tetron-mobile-sync`) uses this exact
/// name unless an advanced user overrides it on both sides, so it must be a
/// fixed, coordinated constant -- like [`DEFAULT_PORT`], a value that has to
/// match on the receiver and every phone or the connection fails. Not
/// `photos`: the module token is not a folder and not photo-specific.
pub const DEFAULT_MODULE: &str = "tetron-sync";

#[derive(Serialize, Deserialize, Clone)]
pub struct Module {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub modules: Vec<Module>,
    /// Mesh IPs allowed to connect, shared across all modules (a global
    /// `hosts allow` line, not per-module -- v1 has one receiver, one
    /// allow-list; see DO-NOT-COMMIT/PLAN for the per-module rationale).
    #[serde(default)]
    pub allow: Vec<String>,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for State {
    fn default() -> Self {
        State {
            port: DEFAULT_PORT,
            modules: Vec::new(),
            allow: Vec::new(),
        }
    }
}

fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not determine data directory")?
        .join("tetron-sync-receiver");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn state_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("state.json"))
}

pub fn rsyncd_conf_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("rsyncd.conf"))
}

pub fn pid_file_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("rsyncd.pid"))
}

pub fn log_file_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("rsyncd.log"))
}

/// Lock file backing `max connections` in the generated config. rsync's
/// default (`/var/run/rsyncd.lock`) is unwritable to this non-root per-user
/// daemon, so `max connections` would fail every connection without an
/// explicit writable path here.
pub fn lock_file_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("rsyncd.lock"))
}

pub fn load() -> Result<State> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(State::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

/// Persists `state.json` and regenerates `rsyncd.conf` together -- every
/// mutating subcommand goes through this so the two can never drift apart.
pub fn save(state: &State) -> Result<()> {
    let path = state_path()?;
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))?;
    write_rsyncd_conf(state)?;
    Ok(())
}

fn write_rsyncd_conf(state: &State) -> Result<()> {
    let path = rsyncd_conf_path()?;
    let body = render_rsyncd_conf(state, &pid_file_path()?, &log_file_path()?, &lock_file_path()?);
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Pure renderer for `rsyncd.conf` -- separated from [`write_rsyncd_conf`] so
/// the exact directives can be unit-tested without touching the real data
/// directory.
///
/// Each module is push-only: `write only = true` (the phone pushes; it must
/// not be able to enumerate or pull anything already on the receiver) and
/// `max connections = 1` (one transfer at a time from a given phone -- stops
/// overlapping runs). `read only = false` is kept alongside `write only`
/// because rsync treats them independently and some builds warn if the
/// upload path is left `read only = true`. `lock file` is set explicitly
/// because `max connections` needs a writable one and the rsync default
/// path is root-only. Per-device separation is the client's job (it writes
/// into `<module>/<device-label>/...`), not a per-module concern here.
fn render_rsyncd_conf(state: &State, pid_file: &Path, log_file: &Path, lock_file: &Path) -> String {
    let mut out = String::new();
    out.push_str("# Generated by tetron-sync-receiver -- do not hand-edit, run\n");
    out.push_str("# `tetron-sync-receiver module`/`allow` subcommands instead.\n");
    out.push_str(&format!("pid file = {}\n", pid_file.display()));
    out.push_str(&format!("log file = {}\n", log_file.display()));
    out.push_str(&format!("lock file = {}\n", lock_file.display()));
    out.push_str("use chroot = false\n");
    if state.allow.is_empty() {
        // No allow-list configured yet: deny everything rather than
        // defaulting open. An operator must explicitly add at least one
        // mesh IP before this receiver accepts any connection.
        out.push_str("hosts deny = *\n");
    } else {
        out.push_str(&format!("hosts allow = {}\n", state.allow.join(", ")));
        out.push_str("hosts deny = *\n");
    }
    out.push('\n');
    for module in &state.modules {
        out.push_str(&format!("[{}]\n", module.name));
        out.push_str(&format!("    path = {}\n", module.path));
        out.push_str("    read only = false\n");
        out.push_str("    write only = true\n");
        out.push_str("    max connections = 1\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> State {
        State {
            port: DEFAULT_PORT,
            modules: vec![Module {
                name: DEFAULT_MODULE.into(),
                path: "/home/you/Pictures/phone-backup".into(),
            }],
            allow: vec!["10.88.0.42".into()],
        }
    }

    fn render(state: &State) -> String {
        render_rsyncd_conf(
            state,
            Path::new("/x/pid"),
            Path::new("/x/log"),
            Path::new("/x/lock"),
        )
    }

    #[test]
    fn module_block_is_push_only_and_single_connection() {
        let conf = render(&sample());
        assert!(conf.contains("[tetron-sync]\n"));
        assert!(conf.contains("    path = /home/you/Pictures/phone-backup\n"));
        assert!(conf.contains("    read only = false\n"));
        assert!(conf.contains("    write only = true\n"));
        assert!(conf.contains("    max connections = 1\n"));
    }

    #[test]
    fn max_connections_has_a_writable_lock_file() {
        let conf = render(&sample());
        assert!(conf.contains("lock file = /x/lock\n"));
    }

    #[test]
    fn allow_list_becomes_hosts_allow_plus_deny_all() {
        let conf = render(&sample());
        assert!(conf.contains("hosts allow = 10.88.0.42\n"));
        assert!(conf.contains("hosts deny = *\n"));
    }

    #[test]
    fn empty_allow_list_denies_everything() {
        let mut state = sample();
        state.allow.clear();
        let conf = render(&state);
        assert!(conf.contains("hosts deny = *\n"));
        assert!(!conf.contains("hosts allow"));
    }
}
