# tetron-sync-receiver

A small program that turns a home computer into a photo backup destination for [tetron-mobile-sync](https://github.com/ErikAllanKincaid/tetron-mobile-sync), an Android app that backs up your phone's camera roll to a computer you own over the [tetron](https://github.com/ErikAllanKincaid/tetron) mesh network. This is the piece that runs on the receiving computer.

It runs in the background, accepts incoming transfers only from devices you've explicitly allowed, and writes everything under one backup folder you choose. Nothing is reachable from the phone until you pick that folder and say who is allowed to connect.

## How it works

- It runs as a background service on your computer (no terminal window needs to stay open).
- You choose **one backup destination folder**. The phone app writes to it by default -- there is nothing to name or configure on the phone.
- Each phone writes into its own sub-folder of that destination (`<destination>/<device-label>/...`, where the phone app picks the label), so **one folder holds backups from several phones**. The phone creates its sub-folder itself; you only pick the destination and allow the device.
- The destination accepts uploads only: a connected phone can push files but cannot list or download anything already there, and only one transfer at a time is accepted.
- You **allow** specific devices to connect, either by picking them from your mesh network's device list by name, or by typing in their address directly. Nothing else can connect -- every other connection is refused.
- Changes to the destination or the allowed devices take effect immediately, with no need to restart anything.
- Under the hood this is a single stock `rsync` daemon module named `tetron-sync`. That name is a coordinated default: the phone uses the same one automatically. Only advanced multi-folder setups ever change it (see [Advanced](#advanced)), and then it must be set to match on both sides by hand.

## Installing

Use the tetron install script.

Do not accept defaults, sync reciever will be a choice. 

```bash
curl -fsSL https://raw.githubusercontent.com/ErikAllanKincaid/tetron/main/contrib/install-tetron-suite.sh | bash
```

Or build and install the binary:

```
git clone https://github.com/ErikAllanKincaid/tetron-sync-receiver
cd tetron-sync-receiver
cargo build --release
sudo install -m 0755 target/release/tetron-sync-receiver /usr/local/bin/tetron-sync-receiver
```

Then register and start the background service:

```
tetron-sync-receiver install --port 28873
```

You can pick a different port if you like -- anything above 1024 works, since this program never needs administrator/root privileges to run.

## Using the command line

Right after installing, nothing is shared and nothing is allowed to connect. Setup is two commands: pick the backup destination and allow the phone.

```
# Set the backup destination folder (created if missing)
tetron-sync-receiver module set-dir /home/you/Pictures/phone-backup

# Allow a device by its name on the mesh network...
tetron-sync-receiver allow add my-phone

# ...or by address directly, if you prefer
tetron-sync-receiver allow add 10.88.0.42
```

The phone needs no matching setup -- it uses the `tetron-sync` module by default.

Check on things at any time:

```
tetron-sync-receiver receiver status   # is it running? what port? how many folders/devices?
tetron-sync-receiver module list
tetron-sync-receiver allow list
```

Change the destination later, or remove a device:

```
tetron-sync-receiver module set-dir /some/other/folder
tetron-sync-receiver allow remove 10.88.0.42
```

Start or stop the service without undoing your setup:

```
tetron-sync-receiver receiver disable
tetron-sync-receiver receiver enable
```

Remove everything, including the background service:

```
tetron-sync-receiver uninstall
```

Add `--json` right after `tetron-sync-receiver` on any command for machine-readable output, e.g. `tetron-sync-receiver --json receiver status`.

## Using the web dashboard (tetron-webui)

If you also run [`tetron-webui`](https://github.com/ErikAllanKincaid/tetron-webui) (a browser dashboard for the mesh network), you can do all of the above by clicking instead of typing:

1. Open the webui's Add-ons page. You'll see a **Sync Receiver** entry.
2. If it isn't installed yet, the row shows the exact command to run in a terminal (the dashboard can't install software on your computer for you -- you run that one command once).
3. Once installed, the row gets a **Configure** button. Clicking it opens a panel where you can add or remove shared folders, add or remove allowed devices (by name from your mesh network's device list, or by typing an address), and start or stop the service.

Everything in that panel does exactly the same thing as the command-line tool above -- it's just a point-and-click way to do it.

## Advanced

Most people never need this. `set-dir` manages a single module named `tetron-sync`, which is the name the phone app expects by default.

- **More than one destination.** `tetron-sync-receiver module add --name archive /home/you/archive` adds a second module. A phone only reaches it if you also set that same module name in the phone app's Settings, in the "Connection" section next to the port -- there is no discovery, the names must match exactly.
- **Rename the default.** Same mechanism: `module add --name <newname> <path>`, then `module remove tetron-sync`, then set `<newname>` on every phone. There is no reason to do this unless the name collides with something.
- **Remove a module.** `tetron-sync-receiver module remove <name>` (`<name>` defaults to `tetron-sync`).

The module name is exactly like the port: a coordinated value with a sensible default that only works if the receiver and every phone agree on it.

## License

MPL-2.0 licensed. It contains no code from the phone app -- it only writes a plain `rsync` daemon configuration file and runs the system's own `rsync` program, the same tool countless backup and mirroring scripts have used for decades.
