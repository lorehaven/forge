# Songbook for Android

A native music app for the library, and the app **Android Auto** shows in the car. What is played, and where it is chosen, is a
media session (the car, the phone's own screens, the lock screen, the notification).

* **Crossfade** (0-12 s, Settings): a second player finishes the song that is ending while the main one fades the next in
  (equal-power curves). Only a song that ends by itself fades; a skip cuts at once.
* **Keep on this device**: songs (a row, a list, the player) are downloaded and never evicted; they are listed under *On this
  device*, also in the car, and play with no network. What was merely played is cached too (1 GiB, oldest out first), and so are
  the pictures that were seen.
* **In the car**: a *Crossfade* button on the player (steps through off/3/6/9/12 s, and follows the phone's setting), *continue*
  (Android Auto's recent root and playback resumption put back the same list, song and place), and a suggested root.
* **Recently played is the same everywhere**: each song started is told to the service (`POST /history`), and the list comes
  from it (`GET /library/recent`), so the phone, the car and the web player agree.
* **Phone screens**: browse (the same tree as the car), search, a mini player, the player (seek, shuffle, repeat, keep, the queue: tap to go there, ✕ to take one out), and the
  settings (crossfade, kept songs, sign out).

| File | Role |
|---|---|
| `Session.kt`, `Vault.kt`, `Http.kt` | Sign in to Gatehouse as `songbook-android` (one long-lived session per user and app), the refresh token sealed with an Android Keystore key, short access tokens renewed as they run out. A server that cannot be reached never ends the session; only Gatehouse refusing the refresh token does. |
| `Library.kt` | The library API (`/songbook/api/v1/library`, `/playlists`) for the signed-in person. |
| `Tree.kt` | What a media browser sees: Recently played, Mixes, Popular, Artists (A-Z), Categories, Playlists, down to songs. A song's id says which list it is in, so choosing it plays that list from there. |
| `PlaybackService.kt` | A Media3 `MediaLibraryService`: ExoPlayer reading the songs with the token (and keeping what it read, up to 1 GiB, so a song heard once plays again offline), the browse tree, voice search, artwork with the token. |
| `Crossfader.kt` | The fade between songs. |
| `Offline.kt` | The three layers songs are read through (kept, played, network), the download manager and `SongDownloadService`, the kept list. |
| `Phone.kt`, `Ui.kt`, `MainActivity.kt` | The phone's screens (plain views, no layout files) and the sign-in. |

## Build and test

```sh
cd services/songbook/android
./gradlew :app:assembleDebug                 # app/build/outputs/apk/debug/app-debug.apk
./gradlew :app:connectedDebugAndroidTest     # on a running emulator or device
```

The instrumented tests (18) play the part of the car (`CarBrowseTest`: connect as a media browser, walk the tree, search, play,
skip, play a mix shuffled), test the real sign-in (`SignInTest`: wrong password, token renewal while playing, a revoked session),
the fade (`CrossfadeTest`: both songs heard, equal power, ends cleanly; no fade when off or on a skip), kept songs (`KeepTest`: they
play while the server is down), the car's extras (`CarControlsTest`: the crossfade button, continue where the music was left, history) and drive the phone's screens (`PhoneUiTest`, with UiAutomator; it leaves screenshots in
`/sdcard/Download/songbook-*.png`).
They need, on the computer, a Songbook service on `0.0.0.0:18080` that asks for no sign-in (`SERVICE_AUTH_ENABLED=false`; the app
treats a server that answers the library without a sign-in as an *open* session) and, for `SignInTest`, a stand-in Gatehouse on
`:18090` (user `ann`, password `secret`, access tokens of six seconds, the API only with a valid bearer, `/__down` and `/__up` to
simulate the server being away) that proxies to it. The
emulator reaches the computer as `10.0.2.2`; debug builds allow plain HTTP for that, release builds are HTTPS only.

## Try it in Android Auto

1. On the phone: install the app, sign in (server `https://host`, your account), allow notifications.
2. Android Auto's developer settings (tap the version ten times) -> **Unknown sources** (needed for an app that is not from Google Play).
3. Car, or the Desktop Head Unit (`sdkmanager "extras;google;auto"`, then `adb forward tcp:5277 tcp:5277`, start the head unit
   server in Android Auto's developer settings, run `desktop-head-unit`): Songbook is under **Music**.

## Release

`anvil artifact release songbook` (`.anvil/songbook.toml`) builds the signed APK with the version of `player/Cargo.toml`. It needs
`services/songbook/release.keystore` (alias `release`, like the other apps; `keytool -genkeypair ...`) and its passwords from
`anvil login` or `QUENCH_KEYSTORE_PASSWORD`. For CI add the secret `SONGBOOK_KEYSTORE_B64` (`scripts/ci-restore-keystores.sh`).
