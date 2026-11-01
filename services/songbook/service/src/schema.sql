-- The Songbook index (meta/library.db). One source of truth for the tables: the service runs it on start
-- (include_str!) and scripts/convert-index.sh runs it with the sqlite3 CLI. quench-sqlite refuses foreign keys,
-- so none are declared: the service deletes dependants itself.
CREATE TABLE IF NOT EXISTS artists(
  id INTEGER PRIMARY KEY, deezer_id INTEGER UNIQUE, name TEXT NOT NULL, listed_as TEXT UNIQUE);
CREATE TABLE IF NOT EXISTS albums(
  id INTEGER PRIMARY KEY, deezer_id INTEGER UNIQUE, title TEXT NOT NULL, year TEXT, type TEXT, genre TEXT, owner_id INTEGER);
CREATE TABLE IF NOT EXISTS songs(
  id INTEGER PRIMARY KEY, key TEXT NOT NULL UNIQUE, title TEXT NOT NULL, filename TEXT NOT NULL UNIQUE, duration INTEGER,
  yt_url TEXT NOT NULL DEFAULT '', yt_channel TEXT NOT NULL DEFAULT '', yt_title TEXT NOT NULL DEFAULT '',
  yt_official INTEGER NOT NULL DEFAULT 0, yt_score REAL, yt_manual INTEGER NOT NULL DEFAULT 0,
  yt_status TEXT, yt_candidate TEXT NOT NULL DEFAULT '');
CREATE TABLE IF NOT EXISTS song_artist(
  song_id INTEGER NOT NULL, artist_id INTEGER NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(song_id, artist_id));
CREATE TABLE IF NOT EXISTS song_album(
  song_id INTEGER NOT NULL, album_id INTEGER NOT NULL, track_no INTEGER, PRIMARY KEY(song_id, album_id));
CREATE TABLE IF NOT EXISTS song_track(
  deezer_id INTEGER PRIMARY KEY, song_id INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS tags(
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, UNIQUE(kind, name));
CREATE TABLE IF NOT EXISTS song_tag(
  song_id INTEGER NOT NULL, tag_id INTEGER NOT NULL, PRIMARY KEY(song_id, tag_id));
CREATE TABLE IF NOT EXISTS follows(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, pin TEXT NOT NULL DEFAULT '', category TEXT NOT NULL DEFAULT '',
  position INTEGER NOT NULL DEFAULT 0, deezer_name TEXT NOT NULL DEFAULT '', deezer_id TEXT NOT NULL DEFAULT '',
  fans TEXT NOT NULL DEFAULT '', note TEXT);
CREATE TABLE IF NOT EXISTS categories(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, position INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS blacklist(
  id INTEGER PRIMARY KEY, artist TEXT NOT NULL, album_key TEXT NOT NULL, title TEXT NOT NULL, UNIQUE(artist, album_key));
CREATE TABLE IF NOT EXISTS suggestions(
  id INTEGER PRIMARY KEY, artist TEXT NOT NULL, deezer_id TEXT NOT NULL DEFAULT '', fans TEXT NOT NULL DEFAULT '',
  related_to_count INTEGER NOT NULL DEFAULT 0, related_to TEXT NOT NULL DEFAULT '', scope TEXT NOT NULL DEFAULT '',
  position INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS meta(
  key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS song_artist_artist ON song_artist(artist_id);
CREATE INDEX IF NOT EXISTS song_album_album ON song_album(album_id);
CREATE INDEX IF NOT EXISTS song_tag_tag ON song_tag(tag_id);
CREATE INDEX IF NOT EXISTS albums_owner ON albums(owner_id);
