# songbook (service)

The library of every song. Songbook is meant to replace a streaming service for a personal collection; this is its first module, **admin**: the index of songs, artists, albums and tags, the Deezer sync that fills it, the YouTube
lookup, download and conversion that give every song a file, and the web interface that drives them. It is a Quench service like the others of Forge
(`core` / `app` / `web` / `service`), and runs in Docker.

## How the library is kept

* **One index, one writer.** `meta/library.db` (SQLite, read and written through `quench-sqlite`) is the single source of
  truth; this service is the only thing that writes it. Followed artists, categories, the album blacklist and the last
  suggestions live in it too (no text files).
* **One row per song.** A song is identified by its artists (compared without case and punctuation) and its title with
  `(feat. X)` and remaster tags removed (`songbook_core::naming::song_key`). The same track on an album, a deluxe edition, a
  compilation or a collaborator's discography is one row. `Song (Sped Up)`, `(Live)` and `(Remix)` stay separate songs.
* **One file per song**, in one flat folder: `<Artist A, Artist B> - <Title>.<ext>`, artists in alphabetical order, never
  starting with a dot, at most 200 bytes. If two songs would get the same name the second gets ` [xxxxxx]`. The name is stored
  in `songs.filename`; the file is looked up by that stem in any extension.
* **Artists** come from Deezer's per-track contributor list, so features are real artists. Compilation "artists" such as
  `Nate Dogg featuring Snoop Dogg` are split when every part is an artist known from the fetched data.
* **Tags** are derived and rebuilt on every sync: a `category` tag for each category of the followed artists on a song (and of
  the owners of its albums), a `genre` tag for each Deezer genre of its albums. Tags of kind `custom` are never touched.

### Tables

```
songs        id, key, title, filename, duration, yt_url, yt_channel, yt_title, yt_official, yt_score, yt_manual, yt_status, yt_candidate
artists      id, deezer_id, name, listed_as            (listed_as = the followed name; NULL for artists not followed)
albums       id, deezer_id, title, year, type, genre, owner_id
tags         id, kind ('category' | 'genre' | 'custom'), name
song_artist  song_id, artist_id, position              (position 0 = Deezer's main artist)
song_album   song_id, album_id, track_no
song_tag     song_id, tag_id
song_track   deezer_id, song_id                        (every Deezer track id folded into the song)
follows      name, pin, category, position, deezer_name, deezer_id, fans, note   (note NULL = not synced yet)
categories   name, position
blacklist    artist, album_key, title
suggestions  artist, deezer_id, fans, related_to_count, related_to, scope, position
meta         key, value                                (schema, key_scheme)
```

`service/src/schema.sql` is the one definition; the service runs it on start. `quench-sqlite` refuses foreign keys, so none are
declared and the service removes dependants itself. `yt_status` is `NULL` (not searched), `ok` or `miss`.

## The admin API

All under `/api/v1/admin`, JSON, behind Gatehouse sign-in (`Auth`: bearer token or the session cookie). The shapes are in
[`core/src/api.rs`](core/src/api.rs).

| Method and path | Body / query | Answer |
|---|---|---|
| `GET /state` | | artists (with counts), categories, genres, suggestions |
| `GET /browse` | `artist`, `category`, `genre`, `q`, `links` (`has` / `missing` / `unofficial`) | albums with tracklists; `total`, `linked`, `shown` (at most 300) |
| `POST /link` | `{track_id, url}` (empty url clears) | `{updated}` |
| `POST /albums/blacklist` | `{artist, album_id}` | `{removed}` |
| `POST /artists/add` | `{items: [{name, id, category}]}` | `{added: [names]}` |
| `POST /artists/remove` | `{name}` | `{}` |
| `POST /artists/category` | `{name, category}` | `{retagged}` |
| `POST /categories/add` | `{name}` | `{added: [name]}` (empty when it exists) |
| `POST /jobs` | `{name: sync / tags / similar / youtube / download / convert, singles, artist, limit, top, per_artist, category, retry_missing}` | `{started}` (`false`: a job is running) |
| `GET /job?from=N` | | `{name, running, error, log, offset}`: the log from byte `N`; poll with the last `offset` |

`link`, `albums/blacklist` and `artists/category` answer `409` while a job runs. Errors are `{"error": "..."}`.

### Jobs

One job at a time, in the background, with its log kept in memory (the last 4 MB).

* **sync** reads each followed artist's albums and tracks from Deezer (`api.deezer.com`, no key; about 7 requests a second, 4
  track lookups at once), writes songs, artists, albums and links, drops what left Deezer or was blacklisted, rebuilds the tags.
  Albums and tracks are cached as JSON in `$MUSIC_HOME/.deezer_cache`, so a repeat run is fast (the cache of the Python helper
  is reused as is). `singles` also fetches singles.
* **tags** rebuilds category and genre tags without fetching.
* **similar** asks Deezer for the related artists of the followed ones and ranks them by how many of yours point at them.
* **youtube** searches YouTube (`yt-dlp --flat-playlist ytsearch20:...`, metadata only) for each song without a link or file, and
  keeps the best result when its score is 4 or more: the artist's "Topic" channel, VEVO or an artist-named channel, a matching
  title and a duration within a few seconds win; live, cover, remix, karaoke and similar lose. A song with no good match is
  marked `miss` (`retry_missing` looks again). Also writes `meta/youtube_misses.csv`.
* **download** fetches each linked song that has no file into the audio folder (audio only, `-f bestaudio/best`).
* **convert** turns every media file in the audio folder into mp3 (VBR quality 0) in place and removes the source; a leftover
  source whose mp3 exists is removed too when both last the same within 2 s. A download with no audio stream is reported and left.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `MUSIC_HOME` | working directory | holds `meta/` (`library.db`, `cookies.txt`) and `.deezer_cache/` |
| `MUSIC_DATA` | `$MUSIC_HOME/data` | the audio folder |
| `COOKIES_FILE` | `$MUSIC_HOME/meta/cookies.txt` | YouTube cookies (Netscape format) for yt-dlp; checked once per job against a public video and dropped, with a warning in the log, when YouTube rejects them |
| `COOKIES_BROWSER` | empty | a browser whose cookies yt-dlp reads when there is no cookies file |
| `WEB_DIR` | `web` | directory holding `app.wasm`, the browser client |
| `BASE_PATH` | `/` | mount prefix |
| `SERVER_ADDR` | `0.0.0.0:443` (image: `0.0.0.0:8080`) | listen address; plain HTTP when no `SERVER_CERT_PATH` / `SERVER_KEY_PATH` can be loaded |
| `GATEHOUSE_URL` | none, **required** | Gatehouse origin including its base path: JWKS, and `/health/ready` of this service turns ready once Gatehouse answers |
| `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET` | empty | the OAuth client of this service in Gatehouse (`songbook`) |
| `SERVICE_AUTH_ENABLED` | `false` | `true` turns the token check on; the page routes always require a session |
| `SERVICE_NAME` | `service` | the audience the token must carry (set `songbook`) |
| `REDIS_URL` | none | shared session store; sessions are held in the process when unset |
| `YTDLP_ARGS` | empty | extra options for every yt-dlp run, e.g. `--js-runtimes node` |
| `RUST_LOG`, `LOG_*` | | logging, as for every quench service |

Every other platform variable (`GATEHOUSE_TLS_VERIFY`, `AUTH_COOKIE_*`, ...) is the quench-starter / quench-auth one, listed in
the [common service runtime](../../../docs/services/common-runtime.md). **`SERVICE_AUTH_ENABLED` defaults to `false`: set
it to `true` in any deployment**, or `/api/v1` answers anyone.

The service needs `yt-dlp` (with a JavaScript runtime such as node, for YouTube's challenges), `ffmpeg` and `ffprobe` on the
`PATH`; the image has them.

## Running

```sh
foreman start songbook     # builds, then serves https://localhost:16443/songbook/ behind the local Gatehouse
cargo test -p songbook     # unit tests (the convert test needs ffmpeg and skips itself without)
```

For a quick look without Gatehouse, run the binary from this directory with `GATEHOUSE_URL=http://127.0.0.1:9`,
`SERVER_ADDR=127.0.0.1:8080` and `MUSIC_HOME=<dir>`: sign-in is off unless `SERVICE_AUTH_ENABLED=true`, and the page is served to
anyone (`/health/ready` stays 503 because Gatehouse is not there). The browser client is the `songbook-web` crate:
`scripts/web.sh` at the root of the workspace builds it to `service/web/app.wasm`.

## Moving an existing library over from the Python helper

1. Convert the index (the old file is only read; needs the `sqlite3` tool):
   `services/songbook/scripts/convert-index.sh <old>/meta/library.db <new home>/meta/library.db`
2. Copy `artists.txt`, `categories.txt`, `blacklist.txt`, `artist_matches.csv` and `suggestions.csv` next to it. On the first start
   with an empty `follows` table the service imports them once and never reads them again.
3. Start the service on that home. It recomputes every song key with the Unicode-aware rule (`meta.key_scheme`), once, so
   the titles in Polish or other non-ASCII scripts no longer collide, and keeps the old file names (`songs.filename` is not
   recomputed). Copy the audio folder to `MUSIC_DATA` as it is.

## Docker

Built like every service: `anvil docker` with the `songbook` module of `.anvil/docker.toml`, on `images/Dockerfile.alpine`
(musl binary, the wasm client in `web/`; `RUNTIME_PACKAGES = "ffmpeg yt-dlp nodejs"`). yt-dlp needs a JavaScript runtime for YouTube's
challenges, so the deployment sets `YTDLP_ARGS=--js-runtimes node`. Volumes: `/data` (`MUSIC_HOME`) and `/music` (`MUSIC_DATA`); it runs as
root, as the hostPath volumes it is deployed with are created root-owned.
