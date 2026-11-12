# Songbook

The library of every song: a personal replacement for a streaming service. Its first module is **admin**: the index of songs,
artists, albums and tags, the Deezer sync that fills it, the YouTube lookup, download and conversion that give every song a
file, and the web interface that drives them. A Quench service in the shape of Gauge and Workbench; the index is a
schema in Postgres (Foundry's `songbook` module) and one flat folder holds the audio (`Artist A, Artist B - Title.mp3`).

| Crate | Path | Role |
|---|---|---|
| `songbook-core` | [`core/`](./core/) | Wire model, song identity and file names. No I/O. |
| `songbook-app` | [`app/`](./app/) | Screens (`quench-ui`) over `Request`/`Msg`. |
| `songbook-web` | [`web/`](./web/) | The admin's WebAssembly client (`/ui/admin`). |
| `songbook-player` | [`player/`](./player/) | The listener's screens (`quench-ui`) and the audio engine (`engine.js`). |
| `songbook-player-web` | [`player-web/`](./player-web/) | The player's WebAssembly client (`/ui/home`). |
| `songbook-native` | [`native/`](./native/) | The Android app's brain, a program the app runs: sign-in, the API, the browse tree, history. |
| `songbook-surface` | [`surface/`](./surface/) | The Android phone screens: the player drawn by the native renderer, playing through the app's plugin. |
| Android app | [`android/`](./android/) | Kotlin: the media session (Android Auto), ExoPlayer, downloads, crossfade. |
| `songbook` | [`service/`](./service/) | The server: index, jobs, API, SSO. Binary `songbook-service`. |

Everything about the index, the API, the jobs and the configuration is in [`service/README.md`](./service/README.md) and in the
[documentation](../../docs/services/songbook/index.md).

## Two modes

* **The player**, at `/ui/home` (`https://host/songbook/ui/home`), is for everyone with a grant of `songbook` (`read` or `write`): browse by
  category, artist and album, search, play a song or a whole album, artist or category in order or shuffled, make playlists,
  keep songs on the device (Cache Storage) so they play without the network, and a crossfade between songs. It plays in the
  background too (lock-screen controls through the media session).
  Keyboard (in the browser): Space or K play and pause, the left and right arrows seek 5 seconds (J and L: 10), N and P go to
  the next and the previous song, M mutes. They leave text fields alone, and a button reached with Tab keeps its own Space.
  Where you stopped (the song, the exact point and the list) follows you to your other devices; "Recently played" lists what you
  chose to start (a mix, an artist, an album, a playlist, a song), not every song.

* **The admin**, at `/ui/admin/...` (`library`, `pipeline`, `discover`, `schedule`, `activity`), needs `songbook:write` and a
  computer: a phone or tablet is sent to the player.

`/api/v1/admin/*` needs `songbook:write`; the rest of `/api/v1` (library, `stream/{id}` with `Range`, playlists) needs any grant.
