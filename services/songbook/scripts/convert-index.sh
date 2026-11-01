#!/usr/bin/env bash
# Converts the index written by the Python music helper (meta/library.db, with foreign keys) into Songbook's
# foreign-key-free schema (service/src/schema.sql, the same file the service runs on start).
#
#   scripts/convert-index.sh <old library.db> <new library.db>
#
# The old file is only read. The new one must not exist. Needs the sqlite3 command-line tool. Afterwards start Songbook with
# MUSIC_HOME pointing at a directory whose meta/ holds the new file as library.db (and, still, the old artists.txt,
# categories.txt, blacklist.txt, artist_matches.csv and suggestions.csv, which the service imports once on first start).
set -euo pipefail
old="${1:?old library.db}"
new="${2:?new library.db}"
here="$(cd "$(dirname "$0")" && pwd)"
[ -f "$old" ] || { echo "no such file: $old" >&2; exit 1; }
[ ! -e "$new" ] || { echo "$new exists; refusing to overwrite" >&2; exit 1; }
command -v sqlite3 >/dev/null || { echo "sqlite3 is needed" >&2; exit 1; }

sqlite3 "$new" < "$here/../service/src/schema.sql"
sqlite3 "$new" <<SQL
ATTACH DATABASE '$old' AS o;
BEGIN;
INSERT INTO artists(id, deezer_id, name, listed_as) SELECT id, deezer_id, name, listed_as FROM o.artists;
INSERT INTO albums(id, deezer_id, title, year, type, genre, owner_id) SELECT id, deezer_id, title, year, type, genre, owner_id FROM o.albums;
INSERT INTO songs(id, key, title, filename, duration, yt_url, yt_channel, yt_title, yt_official, yt_score, yt_manual, yt_status, yt_candidate)
  SELECT id, key, title, filename, duration, yt_url, yt_channel, yt_title, yt_official, yt_score, yt_manual, yt_status, yt_candidate FROM o.songs;
INSERT INTO song_artist(song_id, artist_id, position) SELECT song_id, artist_id, position FROM o.song_artist;
INSERT INTO song_album(song_id, album_id, track_no) SELECT song_id, album_id, track_no FROM o.song_album;
INSERT INTO song_track(deezer_id, song_id) SELECT deezer_id, song_id FROM o.song_track;
INSERT INTO tags(id, kind, name) SELECT id, kind, name FROM o.tags;
INSERT INTO song_tag(song_id, tag_id) SELECT song_id, tag_id FROM o.song_tag;
INSERT INTO meta(key, value) VALUES ('schema', '1');
COMMIT;
DETACH DATABASE o;
SQL
echo "converted: $(sqlite3 "$new" 'SELECT COUNT(*) FROM songs') songs, $(sqlite3 "$new" 'SELECT COUNT(*) FROM artists') artists, $(sqlite3 "$new" 'SELECT COUNT(*) FROM albums') albums"
