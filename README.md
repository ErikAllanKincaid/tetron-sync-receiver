# tetron-sync-receiver

A small program that turns a home computer into a photo backup destination for [tetron-mobile-sync](https://github.com/ErikAllanKincaid/tetron-mobile-sync), an Android app that backs up your phone's camera roll to a computer you own over the [tetron](https://github.com/ErikAllanKincaid/tetron) mesh network. This is the piece that runs on the receiving computer.

It runs in the background, accepts incoming transfers only from devices you've explicitly allowed, and organizes what it shares into named folders. Nothing is reachable from the phone until you tell it what to share and who is allowed to connect.

## How it works

- It runs as a background service on your computer (no terminal window needs to stay open).
- You expose one or more folders as **modules** -- a module is just a name paired with a folder path (e.g. a module called `photos` pointing at `/home/you/Pictures/phone-backup`).
- You **allow** specific devices to connect, either by picking them from your mesh network's device list by name, or by typing in their address directly. Nothing else can connect -- every other connection is refused.
- Changes to modules or allowed devices take effect immediately, with no need to restart anything.

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
tetron-sync-receiver install --port 8873
```

You can pick a different port if you like -- anything above 1024 works, since this program never needs administrator/root privileges to run.

## Using the command line

Right after installing, nothing is shared and nothing is allowed to connect. Set up a folder to share and a device allowed to reach it:

```
# Share a folder under the name "photos"
tetron-sync-receiver module add photos /home/you/Pictures/phone-backup

# Allow a device by its name on the mesh network
tetron-sync-receiver allow add-peer my-phone

# ...or allow it by address directly, if you prefer
tetron-sync-receiver allow add 10.88.0.42
```

Check on things at any time:

```
tetron-sync-receiver receiver status   # is it running? what port? how many folders/devices?
tetron-sync-receiver module list
tetron-sync-receiver allow list
```

Remove a folder or a device:

```
tetron-sync-receiver module remove photos
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

## License

MPL-2.0 licensed. It contains no code from the phone app -- it only writes a plain `rsync` daemon configuration file and runs the system's own `rsync` program, the same tool countless backup and mirroring scripts have used for decades.
