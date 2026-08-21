# tetron-sync-receiver

Headless, CLI-first home-side rsync receiver for
[tetron-mobile-sync](https://github.com/ErikAllanKincaid/tetron-mobile-sync)
(the GPL-3.0 Android photo-backup addon for the
[tetron](https://github.com/ErikAllanKincaid/tetron) mesh). MPL-2.0, same
license as `tetron-webui`/`tetron-systray` -- it embeds no GPL code, only
generates `rsyncd.conf` and supervises the system's own stock
`rsync --daemon`.

Status: built and manually verified end to end (real `systemd --user`
service, real rsync transfer, real `tetron-webui` integration). Tagged
`v0.11.0`; no published GitHub release yet (Actions has not run on this
repo -- check Settings -> Actions before relying on
`install-tetron-suite.sh --install-sync-receiver`, which needs a real
release to fetch). Until then, install from source (below).

## What it does

- Runs as a per-user service (`systemd --user` on Linux, a launchd
  LaunchAgent on macOS) -- no root needed to *run* it, only to place the
  binary in `/usr/local/bin` alongside `tetron`/`tetron-webui`/
  `tetron-systray`.
- Manages its own `rsyncd.conf`: modules (name -> path) and a mesh-IP
  allow-list, both editable live with no restart -- rsync's daemon
  re-reads its config on every new connection, confirmed in practice, not
  just in theory.
- Talks to the local `tetron` daemon directly over `tetron-proto`'s IPC
  socket to resolve a peer's mesh IP by hostname (`allow add-peer`) -- no
  dependency on `tetron-webui` being installed or running.
- Denies every connection by default (`hosts deny = *`) until at least one
  IP is explicitly allowed.

## Installing

**From source** (until a GitHub release exists):

```
git clone https://github.com/ErikAllanKincaid/tetron-sync-receiver
cd tetron-sync-receiver
cargo build --release
sudo install -m 0755 target/release/tetron-sync-receiver /usr/local/bin/tetron-sync-receiver
```

**Once a release exists**, via the suite installer (tetron core repo) --
`sync-receiver` is opt-in, never installed by default:

```
curl -fsSL https://raw.githubusercontent.com/ErikAllanKincaid/tetron/main/contrib/install-tetron-suite.sh \
  | bash -s -- --install-sync-receiver
```

Either way, the binary alone does nothing until you also register and
start the per-user service:

```
tetron-sync-receiver install --port 8873
```

`--port` must be >1024 -- this process never runs as root, so it can
never bind a privileged port. `install` writes/enables the `systemd
--user` unit (or launchd LaunchAgent), starts it, and waits for the port
to actually come up before printing success.

## Using it

Nothing is reachable yet at this point -- no modules, no allowed IPs.
Two things to configure:

```
# Expose a directory as an rsync module (name -> path):
tetron-sync-receiver module add photos /home/user/Pictures/phone-backup

# Allow a phone by its mesh hostname (resolved via the local tetron
# daemon's own peer roster -- no need to know or copy its IP by hand):
tetron-sync-receiver allow add-peer my-phone

# ...or allow a raw IP directly, if you'd rather not depend on the local
# tetron daemon being reachable:
tetron-sync-receiver allow add 10.88.0.42
```

Both take effect immediately -- no restart needed. Check on things any time:

```
tetron-sync-receiver receiver status       # running? port? module/allow counts?
tetron-sync-receiver module list
tetron-sync-receiver allow list
```

Add `--json` before any subcommand for machine-readable output (one
global flag, works everywhere -- e.g. `tetron-sync-receiver --json
receiver status`). Start/stop the already-installed service without
touching its registration:

```
tetron-sync-receiver receiver disable
tetron-sync-receiver receiver enable
```

Remove things with `module remove <name>` / `allow remove <ip>`. Tear the
whole service down with `tetron-sync-receiver uninstall`.

## tetron-webui integration

If [`tetron-webui`](https://github.com/ErikAllanKincaid/tetron-webui) is
also installed, its Add-ons panel gets a **Sync Receiver** row:

- **Not installed yet**: clicking the row's button shows the exact
  terminal command to run (webui never has root, so it can't place the
  binary itself -- same as every other release-binary addon).
- **Installed**: the row shows an Install/Uninstall toggle plus a
  **Configure** button. Configure opens a live panel with:
  - a **Modules** table (add/remove, same as `module add`/`remove`),
  - an **Allowed mesh IPs** table (add by peer hostname or raw IP,
    remove -- same as `allow add`/`add-peer`/`remove`),
  - a **Start/Stop** toggle (same as `receiver enable`/`disable`).

Every action in that panel is webui shelling out to this binary's own
`--json` CLI (`src/sync_receiver.rs` on the webui side) -- webui never
reimplements any `rsyncd.conf`/module/allow-list logic itself. The CLI
above is the single source of truth either way; the panel is just a
convenience for people who'd rather click than type.

## Design record

`DO-NOT-COMMIT/PLAN_tetron-sync-receiver_2026-08-21.md` (gitignored, local
only) has the full design history and decisions.
