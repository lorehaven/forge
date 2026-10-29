# hoist

A quench app: `app/` is the interface (it runs in the browser and is rendered on the server),
`core/` is logic with no interface, `server/` serves it.

```text
quench dev      # build, serve on http://127.0.0.1:3000 and reload the browser when a file changes
quench test     # the tests, and a check that the app builds for the browser
quench build --release   # dist/: the server with the app built in, one file
quench doctor   # what this machine is missing
```
## Native apps

`desktop/` is the Linux app, drawn by the native renderer in a Wayland window (`cargo run -p hoist-desktop`);
`surface/` is the Android program drawn the same way (`quench android build-native`, then Gradle in `android/`).
`net/` talks to gatehouse and warehouse, `app/` has the screens, `core/` the model. After upgrading the quench
crates, `quench plugin update` writes the Android host's Kotlin again.
