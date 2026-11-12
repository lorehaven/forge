# songbook (service)

The library of every song. Songbook is meant to replace a streaming service for a personal collection; this is its first module, **admin**: the index of songs, artists, albums and tags, the Deezer sync that fills it, the YouTube
lookup, download and conversion that give every song a file, and the web interface that drives them. It is a Quench service like the others of Forge
(`core` / `app` / `web` / `service`), and runs in Docker.

## How the library is kept

* **One index, one writer.** The tables of the `songbook` schema in Postgres (`DATABASE_URL`, `DB_SCHEMA`; the tables are
  Foundry's, module `songbook`) are the single source of truth; this service is the only thing that writes them. Followed
  artists, categories, the album blacklist and the last suggestions live in them too (no text files). Up to 0.1 the index was a
  SQLite file; [moving a 0.1 library over](#moving-a-01-library-over-from-sqlite) is one command.
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

The tables are made by Foundry (`services/foundry/service/migrations/songbook`, `0001-library` and `0002-listeners`); the service
creates nothing and only needs the schema to exist. No foreign keys are declared, as before: the service removes dependants
itself. Ids are identity columns, `BIGINT` throughout (Deezer's ids do not fit 32 bits). `yt_status` is `NULL` (not searched),
`ok` or `miss`. The listeners' tables are `playlists`, `playlist_songs`, `plays`, `contexts` and `resume`, kept apart from the
library so that writing one never drops the cached copy of the library. Text is sorted bytewise (`COLLATE "C"`) wherever the
page shows an order, so the order does not depend on the locale of the database.

## The listener API

Under `/api/v1` (not `/admin`), for any grant of `songbook`. Only songs that have a file are listed. Shapes in
[`core/src/listen.rs`](../core/src/listen.rs).

| Method and path | Body / query | Answer |
|---|---|---|
| `GET /library/overview` | | counts, artists and categories with how many playable songs |
| `GET /library/albums` | `artist`, `category`, `q`, `limit` | albums |
| `GET /library/songs` | `ids`, `playlist`, `album`, `artist`, `category`, `q`, `limit` | songs (at most 500 unless `limit`) |
| `GET /library/ids` | the same | the ids only, in play order (up to 20,000): a queue |
| `GET /library/search?q=` | | a few songs, albums and artists |
| `GET /library/recent` | `limit` | what this person played last, newest first, each song once (the same on every device) |
| `POST /history` | `{song_id}` | remembers that the person started a song (the player and the Android app tell it; the last 500 are kept in the `plays` table) |
| `GET /history/contexts` | `limit` | what this person chose to start playing (a mix, an artist, an album, a playlist, a category or one song), newest first, each once: the home page's "Recently played" |
| `POST /history/context` | a `RecentItem` (`kind`, `name`, `sub`, `art_kind`, `art_key`, `query`, `shuffle`) | remembers that the person chose to start it; the same thing again moves to the front (the last 60 are kept) |
| `GET /resume` | | where this person stopped listening on any device: `{song_id, position_ms, queue, index, device, at}` (`song_id` 0: nowhere yet) |
| `PUT /resume` | `{song_id, position_ms, queue, index, device}` | replaces it (`queue[index]` must be `song_id`; up to 2000 songs of the list are kept); the answer has the service's `at` |
| `GET /stream/{id}` | `Range` | the audio file; `206` for a range, `bytes=N-` is answered in 4 MiB pieces |
| `GET/POST /playlists`, `PUT/DELETE /playlists/{id}` | `{name}` | the signed-in person's playlists |
| `POST /playlists/{id}/songs` | `{song_ids}` | append; `PUT` replaces the order |

| `GET /art/album/{id}`, `/art/song/{id}`, `/art/artist?name=` | `w` (64, 160 or 320; none: the saved 500 px) | the picture (`404` when there is none); `ETag`, a week's `Cache-Control` |

A song's picture is its album's cover (a single's own cover), else its artist's. The pictures are what the **images** job saved in `meta/images` (`album-<deezer id>.jpg`, `artist-<deezer id>.jpg`; an empty `<name>.none` marks what Deezer has none of, `retry_missing` clears them); smaller copies are made with ffmpeg on first request. The job is a Pipeline step, the last stage of "run everything" and a stage of the nightly run. The player keeps pictures, its files, its last page and the library answers on the device with a service worker (`/sw.js`), so it opens without the network.

The catalog is built from the index and a look at the audio folder, kept for 30 s and rebuilt when the index changes.
Playlists and the listeners' plays and resume points are in tables of their own, apart from the index, so writing one does not drop the index's cached copy.

## The admin API

All under `/api/v1/admin`, JSON, behind Gatehouse sign-in (`Auth`: bearer token or the session cookie). The shapes are in
[`core/src/api.rs`](core/src/api.rs).

| Method and path | Body / query | Answer |
|---|---|---|
| `GET /state` | | artists (with counts), categories, genres, suggestions |
| `GET /browse` | `artist`, `category`, `genre`, `q`, `links` (`has` / `missing` / `unofficial`) | albums with tracklists; `total`, `linked`, `shown` (at most 300) |
| `GET /stats` | | the numbers on the main page: artists (followed, and all credited), albums, songs, downloaded and missing (and how many of the missing have a link); the audio folder is looked at on every call |
| `POST /link` | `{track_id, url}` (empty url clears) | `{updated}` |
| `POST /replace` | `{track_id, url}` | `{started}`: a job (`replace`) that downloads the link, converts it to mp3 in `<audio>/.replace`, and only then removes the song's old files and moves the new mp3 in; the link becomes the song's. A link that fails leaves the old file |
| `POST /albums/blacklist` | `{artist, album_id}` | `{removed}` |
| `GET /blacklist` | | `[{id, artist, title}]`: every album kept out of the syncs |
| `POST /blacklist` | `{id, artist, title}` (`id` 0: add by name) | `{removed}`: the album and its other editions leave the index now; an entry already there is refused |
| `POST /blacklist/remove` | `{id}` | `{removed: 1}`; the next sync brings the album back |
| `POST /artists/add` | `{items: [{name, id, category}]}` | `{added: [names]}` |
| `POST /artists/remove` | `{name}` | `{}` |
| `POST /artists/category` | `{name, category}` | `{retagged}` |
| `POST /categories/add` | `{name}` | `{added: [name]}` (empty when it exists) |
| `POST /jobs` | `{name: sync / tags / similar / youtube / download / convert, singles, artist, limit, top, per_artist, category, retry_missing}` | `{started}` (`false`: a job is running) |
| `GET /schedule`, `POST /schedule` | the [nightly run](#the-nightly-run): `{enabled, time, zone, sync, links, download, convert, fresh_ratings, singles}` | the saved schedule with `next_run`, `last_run`, `last_result` |
| `POST /artists/genres` | `{name, genres}` | `{}`; applied at the next sync |
| `GET /job?from=N` | | `{name, running, error, log, offset}`: the log from byte `N`; poll with the last `offset` |

`link`, `albums/blacklist` and `artists/category` answer `409` while a job runs. Errors are `{"error": "..."}`.

### Jobs

One job at a time, in the background, with its log kept in memory (the last 4 MB).

* **sync** reads each followed artist's albums and tracks from Deezer (`api.deezer.com`, no key; about 7 requests a second, 4
  track lookups at once), writes songs, artists, albums and links, drops what left Deezer or was blacklisted, rebuilds the tags.
  Albums and tracks are cached as JSON in `$MUSIC_HOME/.deezer_cache`, so a repeat run is fast (the cache of the Python helper
  is reused as is). `singles` also fetches singles.
* An artist can have **genres** (the dialog of the pencil next to the artist; `; `-separated Deezer genre names, compared without
  regard to case). Sync then keeps only the albums that have one of them and drops the others, including those already in the
  index. This is for a Deezer profile that several people of one name share (the rapper WC has Polish punk and Thai singles on
  his id). An album with no genre never matches; empty keeps every album. Blacklisting is for single albums.
* **tags** rebuilds category and genre tags without fetching.
* **similar** asks Deezer for the related artists of the followed ones and ranks them by how many of yours point at them, then
  fetches each one's 5 best songs (`artist/{id}/top`, scored like the library's) and keeps them with the suggestion.
* **youtube** searches YouTube (`yt-dlp --flat-playlist ytsearch20:...`, metadata only) for each song without a link or file, and
  keeps the best result when its score is 4 or more: the artist's "Topic" channel, VEVO or an artist-named channel, a matching
  title and a duration within a few seconds win; live, cover, remix, karaoke and similar lose. A song with no good match is
  marked `miss` (`retry_missing` looks again). Also writes `meta/youtube_misses.csv`.
* **download** fetches each linked song that has no file into the audio folder (audio only, `-f bestaudio/best`).
* **convert** turns every media file in the audio folder into mp3 (VBR quality 0) in place and removes the source; a leftover
  source whose mp3 exists is removed too when both last the same within 2 s. A download with no audio stream is reported and left.
* **fix-links** (`POST /jobs {name: "fix-links", dry_run, artist, limit}`) repairs links that do not match their song. It is not a step of
  the pipeline or the nightly run; the Pipeline page has its own card with a *Preview* and a *Fix links* button. A song whose automatic
  link (not one set by hand) points at a video whose title does not contain the song's title (case and punctuation ignored, `(feat. X)`
  and remaster tags dropped) is a suspect. The preview lists them. Fixing moves each suspect's audio file to `<audio>/.mismatch-old/`
  (nothing is deleted), clears its link, then runs find links, download and convert. Find links looks at every song without a link,
  not only the cleared ones. A suspect for which no new match is found keeps no file; its old one is in `.mismatch-old/`.
* **purge** deletes the audio files of blacklisted albums. Blacklisting only takes an album out of the index, so its files stay and
  the index no longer knows their names; purge asks Deezer for each blacklisted album's tracklist again, rebuilds the file names as
  sync does and deletes the files of those names (every extension) that are in the audio folder. A name that a song in the index
  still has (it is also on an album you keep) is left alone, as is anything else in the folder. `dry_run` only lists the files and
  their size. It is never part of the pipeline or the nightly run.

### The nightly run

The Schedule page (and `/schedule`) keeps one daily run: a time (`HH:MM`), an IANA time zone (empty: the server's own, from `TZ`)
and the stages to run, in this order: sync, find links (songs that had no match are tried again), download (songs that failed
before are tried again, as every run does), convert, refresh suggestions (the *similar* job with its defaults: 50 artists, 20 related per artist). It runs as the one job (named `scheduled`, log on the Activity page) once a
day: from the set time, or as soon as a running job is done. A stage that fails does not stop the next ones; the run's result is
`ok` or the failed stages. *Fresh ratings* makes sync ask Deezer for every track again (saved track files older than 12 hours) so
`songs.rank` is current; it costs one request per track. Saving a time that has already passed today waits for tomorrow; *Run now*
starts the stages at once. The schedule is the `schedule` key of the `meta` table.

The library's orders: *Top* (songs by popularity), *Top artists* (artists with the most Deezer fans first, each with their 10 best
songs) and *Suggested* (the most popular songs not yet downloaded). *Popularity* (the score behind them) is Deezer's `rank` and the views of the song's YouTube video; views are
only recorded when a link is found, so a song that already has a link keeps Deezer's rank alone until its link is looked up again.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` (or `POSTGRES_URL`) | none, **required** | the Postgres holding the `songbook` schema; the service refuses to start without it |
| `DB_SCHEMA` | `songbook` | the schema Foundry installed the module in (the connections are opened with it on the `search_path`) |
| `MUSIC_HOME` | working directory | holds `meta/` (`cookies.txt`, `images/`, `youtube_misses.csv`) and `.deezer_cache/` |
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
| `YTDLP_UPDATE` | `true` | keep a current yt-dlp of the service's own in `$MUSIC_HOME/bin/yt-dlp` (see below); `false` uses the one on the `PATH` |
| `RUST_LOG`, `LOG_*` | | logging, as for every quench service |

Every other platform variable (`GATEHOUSE_TLS_VERIFY`, `AUTH_COOKIE_*`, ...) is the quench-starter / quench-auth one, listed in
the [common service runtime](../../../docs/services/common-runtime.md). **`SERVICE_AUTH_ENABLED` defaults to `false`: set
it to `true` in any deployment**, or `/api/v1` answers anyone.

**yt-dlp is kept current by the service.** YouTube changes often enough that a yt-dlp a few months old is refused (`HTTP Error 403: Forbidden`
on every download), and the copy a distribution puts in an image only ages: the one in the image got 403 on 12 of 12 sample links where a current
one got all 12. So before Find links and Download (and so before the pipeline) the service downloads the official standalone build to
`$MUSIC_HOME/bin/yt-dlp` (checked against the release's `SHA2-256SUMS`), and runs `yt-dlp -U` on it at most every 12 hours; the first lines of the log say
what happened. The pod needs to reach `github.com`. If that fails the job goes on with the copy it has, or the `yt-dlp` on the `PATH`. A download
stops after 5 refusals in a row (403) with a message, instead of working through the whole list: then it is the cookies (push fresh ones) or YouTube.

The service needs `yt-dlp` (with a JavaScript runtime such as node, for YouTube's challenges), `ffmpeg` and `ffprobe` on the
`PATH`; the image has them.

## Running

```sh
foreman start songbook     # builds, then serves https://localhost:16443/songbook/ behind the local Gatehouse
cargo test -p songbook     # unit tests; the ones that need the database skip themselves without SONGBOOK_TEST_DATABASE_URL (see below)
```

For a quick look without Gatehouse, run the binary from this directory with `GATEHOUSE_URL=http://127.0.0.1:9`,
`SERVER_ADDR=127.0.0.1:8080` and `MUSIC_HOME=<dir>`: sign-in is off unless `SERVICE_AUTH_ENABLED=true`, and the page is served to
anyone (`/health/ready` stays 503 because Gatehouse is not there). The browser client is the `songbook-web` crate:
`scripts/web.sh` at the root of the workspace builds it to `service/web/app.wasm`.

## Moving a 0.1 library over from SQLite

Songbook 0.2 keeps the library in Postgres. A 0.1 library (`meta/library.db` and `meta/user.db` under `MUSIC_HOME`) moves with
one command that is part of the service binary, and one wrapper script around it:

1. Stop the 0.1 service (it holds the SQLite files).
2. Install the tables: `foundry apply --catalog services/foundry/service/migrations --database-url "$DATABASE_URL" --install songbook`
   (the Foundry Job of a deployment does it when `songbook` is in its install list).
3. Run `services/songbook/scripts/migrate-to-postgres.sh` with `MUSIC_HOME`, `DATABASE_URL` and `SONGBOOK_BIN` (the 0.2
   `songbook-service`) or `SONGBOOK_IMAGE` (a 0.2 image, run with `docker run --network host`) set. It copies the two files to
   `meta/sqlite-backup-<time>/`, does a dry run (every table copied and checked, then rolled back), asks, and copies for real.
   `--yes` skips the question, `--dry-run` stops after the dry run, `--replace` empties the songbook tables first (without it a
   target that already holds rows is refused).
4. Start the 0.2 service with `DATABASE_URL`. Keep the SQLite files until you have looked at the library.

The command itself is `songbook-service migrate-sqlite [--home DIR] [--library FILE] [--user FILE] [--database-url URL] [--schema NAME]
[--replace] [--dry-run]` (`--help` lists them). Every row keeps its id, so playlists, plays and the apps' links keep working, and
the identity counters are moved past the largest id. It is one transaction: either every table is copied and its row count equals
the file's, or nothing changes. A file from the first release, without the later columns, moves too (the defaults fill them). The
audio folder is not touched.

The import of the Python helper's text files (`artists.txt`, ...) that 0.1 did on a first start is gone: a library that still comes
from the Python helper goes through Songbook 0.1 first.

## Docker

Built like every service: `anvil docker` with the `songbook` module of `.anvil/docker.toml`, on `images/Dockerfile.alpine`
(musl binary, the wasm client in `web/`; `RUNTIME_PACKAGES = "ffmpeg yt-dlp nodejs"`). yt-dlp needs a JavaScript runtime for YouTube's
challenges, so the deployment sets `YTDLP_ARGS=--js-runtimes node`. Volumes: `/data` (`MUSIC_HOME`) and `/music` (`MUSIC_DATA`); it runs as
root, as the hostPath volumes it is deployed with are created root-owned.
