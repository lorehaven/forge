# Hoist
Installs and updates the estate's apps from the registry, on Android and Linux.

## What it is
Hoist is a Quench app: `app/` is the interface (it runs in the browser and is rendered on the server), `core/` is logic with no interface, `service/` serves the public catalogue. `desktop/` is the Linux app, drawn by the native renderer in a Wayland window; `surface/` is the Android program drawn the same way; `net/` talks to Gatehouse and Warehouse.

## Use cases
- Browse and install apps from Warehouse and the public catalogue.
- Be told when updates are available, without anything installing on its own.

## Development
```text
quench dev      # build, serve on http://127.0.0.1:3000 and reload the browser when a file changes
quench test     # the tests, and a check that the app builds for the browser
quench build --release   # dist/: the server with the app built in, one file
quench doctor   # what this machine is missing
```
### Native apps

`desktop/` is the Linux app, drawn by the native renderer in a Wayland window (`cargo run -p hoist-desktop`);
`surface/` is the Android program drawn the same way (`quench android build-native`, then Gradle in `android/`).
`net/` talks to gatehouse and warehouse, `app/` has the screens, `core/` the model. After upgrading the quench
crates, `quench plugin update` writes the Android host's Kotlin again.


### Looking for updates in the background

The setting **Check for updates in the background** (on by default) makes Hoist look for updates every few hours and
send a notification when there is news: when apps can be updated, once for each set of updates (a new version of one of
them is news again, a reminder is not). It never installs anything on its own.

- **Android:** a scheduled job (every 6 hours, when there is a network, kept across restarts) runs the app's program
  as `libquench_surface.so --check`: it signs in with the saved session, reads the registry, compares it with what the
  package manager says is installed, and prints one line of JSON; the app turns that into the notification. From
  Android 13 the notification needs your permission, which Hoist asks for the first time it opens.
- **Linux:** `hoist --check` does the same without a window and sends the notification with `notify-send`. To run it
  on a timer, copy the two files in `desktop/systemd/` to `~/.config/systemd/user/` and
  `systemctl --user enable --now hoist-update-check.timer`.
- **One session:** the window and the background check share the saved session. A refresh by either spends the token
  the other holds, so each looks at what the other saved first, and again if its own is refused.

## Documentation
Full documentation: [services/hoist](../../docs/services/hoist/index.md)
