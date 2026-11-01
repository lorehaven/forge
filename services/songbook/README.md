# Songbook

The library of every song: a personal replacement for a streaming service. Its first module is **admin**: the index of songs,
artists, albums and tags, the Deezer sync that fills it, the YouTube lookup, download and conversion that give every song a
file, and the web interface that drives them. A Quench service in the shape of Gauge and Workbench; one SQLite file is the
index and one flat folder holds the audio (`Artist A, Artist B - Title.mp3`).

| Crate | Path | Role |
|---|---|---|
| `songbook-core` | [`core/`](./core/) | Wire model, song identity and file names. No I/O. |
| `songbook-app` | [`app/`](./app/) | Screens (`quench-ui`) over `Request`/`Msg`. |
| `songbook-web` | [`web/`](./web/) | The WebAssembly client. |
| `songbook` | [`service/`](./service/) | The server: index, jobs, API, SSO. Binary `songbook-service`. |

Everything about the index, the API, the jobs and the configuration is in [`service/README.md`](./service/README.md) and in the
[documentation](../../docs/services/songbook/index.md).
