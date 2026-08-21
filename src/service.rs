//! Per-user service install/uninstall: a `systemd --user` unit on Linux, a
//! launchd LaunchAgent on macOS. Same shape as `tetron-webui`'s own
//! `src/service.rs` (per-user, not system-wide -- no root needed to *run*
//! it, only to *place* the binary in root-owned `/usr/local/bin` alongside
//! its siblings, matching `contrib/install-tetron-suite.sh`'s convention in
//! the tetron core repo).
//!
//! Unlike tetron-webui, the unit's `ExecStart` runs `tetron-sync-receiver
//! run`, not the bare binary: `run` execs directly into the system `rsync
//! --daemon --no-detach` process (see `main.rs`), so systemd/launchd
//! supervise the actual rsync daemon process, and `Restart=on-failure`
//! covers crash-restart with no bespoke supervision logic of our own.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

fn run_cmd(program: &str, args: &[&str]) {
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("warning: `{program}` exited with {status}"),
        Err(e) => eprintln!("warning: failed to run `{program}`: {e}"),
    }
}

#[allow(dead_code)]
fn run_cmd_quiet(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(target_os = "linux")]
fn unit_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not determine config directory")?
        .join("systemd/user");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tetron-sync-receiver.service"))
}

#[cfg(target_os = "macos")]
fn plist_path() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("could not determine home directory")?
        .join("Library/LaunchAgents");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("com.tetron.sync-receiver.plist"))
}

/// `tetron-sync-receiver install`: write the unit/plist (substituting the
/// path of the binary currently running, same idempotent-on-every-install
/// pattern tetron-webui/tetron-systray use), enable it, and wait for the
/// rsync port to actually come up before declaring success.
pub fn install(port: u16) -> Result<()> {
    println!("installing tetron-sync-receiver {}", crate::FULL_VERSION);
    let exe = std::env::current_exe()
        .context("failed to determine current executable path")?
        .to_string_lossy()
        .into_owned();

    #[cfg(target_os = "linux")]
    {
        let path = unit_path()?;
        let unit = include_str!("../contrib/tetron-sync-receiver.service")
            .replace("/usr/local/bin/tetron-sync-receiver", &exe);
        std::fs::write(&path, unit).with_context(|| format!("failed to write {}", path.display()))?;
        run_cmd("systemctl", &["--user", "daemon-reload"]);
        run_cmd("systemctl", &["--user", "enable", "tetron-sync-receiver"]);
        // `enable --now` no-ops a restart on an already-active unit, so a
        // reinstall over a running instance would never pick up a new
        // binary/config -- explicit restart instead, same fix as
        // tetron-webui/tetron-systray's own service.rs.
        run_cmd("systemctl", &["--user", "restart", "tetron-sync-receiver"]);
    }

    #[cfg(target_os = "macos")]
    {
        let path = plist_path()?;
        let plist = include_str!("../contrib/com.tetron.sync-receiver.plist")
            .replace("/usr/local/bin/tetron-sync-receiver", &exe);
        std::fs::write(&path, plist).with_context(|| format!("failed to write {}", path.display()))?;
        run_cmd_quiet("launchctl", &["unload", &path.to_string_lossy()]);
        run_cmd("launchctl", &["load", "-w", &path.to_string_lossy()]);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("per-user service install not supported on this platform");

    eprintln!("waiting for tetron-sync-receiver to come up on port {port}…");
    if wait_for_port(port, Duration::from_secs(10)) {
        println!("tetron-sync-receiver service installed and running on port {port}.");
        Ok(())
    } else {
        anyhow::bail!(
            "service was installed but never became reachable on port {port}.\n\
             Check the service logs (journalctl --user -u tetron-sync-receiver on Linux, \
             or /tmp/tetron-sync-receiver.log on macOS)."
        );
    }
}

/// `tetron-sync-receiver uninstall`: stop, disable, and remove the unit/plist.
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let path = unit_path()?;
        if path.exists() {
            run_cmd("systemctl", &["--user", "disable", "--now", "tetron-sync-receiver"]);
            std::fs::remove_file(&path)?;
            run_cmd("systemctl", &["--user", "daemon-reload"]);
            println!("Removed systemd --user service.");
        } else {
            println!("Service not installed.");
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let path = plist_path()?;
        if path.exists() {
            run_cmd("launchctl", &["unload", "-w", &path.to_string_lossy()]);
            std::fs::remove_file(&path)?;
            println!("Removed launchd LaunchAgent.");
        } else {
            println!("Service not installed.");
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("per-user service uninstall not supported on this platform");
    }
}

#[cfg(target_os = "linux")]
pub fn is_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "tetron-sync-receiver"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn is_active() -> bool {
    Command::new("launchctl")
        .args(["list", "com.tetron.sync-receiver"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn is_active() -> bool {
    false
}

/// `enable`/`disable`: start/stop the already-installed per-user service
/// without touching its registration -- `install`/`uninstall` own
/// registering/removing the unit itself.
pub fn start() -> Result<()> {
    #[cfg(target_os = "linux")]
    run_cmd("systemctl", &["--user", "start", "tetron-sync-receiver"]);
    #[cfg(target_os = "macos")]
    {
        let path = plist_path()?;
        run_cmd("launchctl", &["load", "-w", &path.to_string_lossy()]);
    }
    Ok(())
}

pub fn stop() -> Result<()> {
    #[cfg(target_os = "linux")]
    run_cmd("systemctl", &["--user", "stop", "tetron-sync-receiver"]);
    #[cfg(target_os = "macos")]
    {
        let path = plist_path()?;
        run_cmd_quiet("launchctl", &["unload", &path.to_string_lossy()]);
    }
    Ok(())
}

fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let addr = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + timeout;
    loop {
        if std::net::TcpStream::connect(&addr).is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
