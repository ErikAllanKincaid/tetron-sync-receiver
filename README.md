# tetron-sync-receiver

Headless, CLI-first home-side rsync receiver for
[tetron-mobile-sync](https://github.com/ErikAllanKincaid/tetron-mobile-sync)
(the GPL-3.0 Android photo-backup addon for the [tetron](https://github.com/ErikAllanKincaid/tetron)
mesh). MPL-2.0, same license as `tetron-webui`/`tetron-systray` -- it embeds
no GPL code, only generates `rsyncd.conf` and supervises the system's own
stock `rsync --daemon`.

Status: early scaffold, not yet released. See
`DO-NOT-COMMIT/PLAN_tetron-sync-receiver_2026-08-21.md` for the design
record.

## What it does

- Runs as a per-user service (`systemd --user` on Linux, a launchd
  LaunchAgent on macOS) -- no root needed to run, matching
  `tetron-webui`/`tetron-systray`.
- Manages its own `rsyncd.conf`: modules (name -> path) and a mesh-IP
  allow-list, both editable live with no restart (rsync's daemon re-reads
  its config on every new connection).
- Talks to the local `tetron` daemon directly over `tetron-proto`'s IPC
  socket to resolve a peer's mesh IP by hostname (`allow add-peer`) -- no
  dependency on `tetron-webui` being installed or running.
- Every subcommand works standalone over CLI/SSH on a fully headless box;
  `tetron-webui`, if installed, is expected to shell out to this same CLI
  (with `--json`) for its Add-ons panel rather than reimplementing any of
  this logic.

## Usage

```
tetron-sync-receiver install --port 8873
tetron-sync-receiver module add photos /home/user/Pictures/phone-backup
tetron-sync-receiver allow add-peer my-phone
tetron-sync-receiver receiver status --json
```

See `--help` on any subcommand for the full list.

## Ecosystem integration (planned, not yet built)

- A new opt-in component in the tetron core repo's
  `contrib/install-tetron-suite.sh` (`--install-sync-receiver`).
- A new `AddonSpec` entry in `tetron-webui`'s `src/addons.rs`.

See `TODO_tetron-webui.md`/`TODO_tetron.md` in the operator's own notes for
the full cross-repo plan.
