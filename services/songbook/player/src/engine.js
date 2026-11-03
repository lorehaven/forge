// Songbook's audio engine. The screens (a quench-ui module) own everything you see; this owns the sound: the queue, two
// audio elements that fade into each other, the lock-screen controls, and the songs kept on the device.
//
// The two talk through one hidden element the screens draw:
//   #sb-engine       data-base (the service's path prefix) and data-cmd, a JSON array of {n, op, ...}; the commands with an
//                    `n` this script has not seen are done, in order.
//   #sb-engine-in    an <input>; this script sets its value to a JSON event and fires `input`.
// Events: state {playing,id,index,len,pos,dur,shuffle,repeat,fade,vol,loading}, queue {ids,index,len}, recent {ids}, cached {ids},
// saving {done,total}, storage {used,quota}, error {message}.
(function () {
  'use strict';
  var NS = 'songbook-songs';
  var S = { recent: [], ids: [], orig: [], index: -1, shuffle: false, repeat: 'off', fade: 0, vol: 1, meta: {}, cached: {}, last: 0 };
  var out = null, cmdEl = null, base = '';
  var ctx = null, els = [new Audio(), new Audio()], gains = [null, null], cur = 0;
  var fading = false, fadeTimer = 0, loading = false, saving = false, urls = [null, null];

  function save() {
    try {
      localStorage.setItem('sb.engine', JSON.stringify({ ids: S.ids, orig: S.orig, index: S.index, shuffle: S.shuffle,
        repeat: S.repeat, fade: S.fade, vol: S.vol, pos: els[cur].currentTime || 0, synced: S.synced || 0 }));
    } catch (e) { /* storage may be blocked */ }
  }
  function restore() {
    try {
      var v = JSON.parse(localStorage.getItem('sb.engine') || 'null');
      if (!v) return;
      S.ids = v.ids || []; S.orig = v.orig || S.ids; S.index = v.index; S.shuffle = !!v.shuffle;
      S.repeat = v.repeat || 'off'; S.fade = v.fade || 0; S.vol = v.vol == null ? 1 : v.vol; S.synced = v.synced || 0;
      if (S.index >= 0 && S.index < S.ids.length) { S.pending = v.pos || 0; }
    } catch (e) { /* a damaged copy is ignored */ }
    try { S.recent = JSON.parse(localStorage.getItem('sb.recent') || '[]').filter(Number.isFinite); } catch (e) { S.recent = []; }
  }

  function emit(o) {
    if (!out) return;
    out.value = JSON.stringify(o);
    out.dispatchEvent(new Event('input', { bubbles: true }));
  }
  function state() {
    var e = els[cur];
    emit({ ev: 'state', playing: !e.paused && !e.ended, id: S.ids[S.index] || 0, index: S.index, len: S.ids.length,
      pos: e.currentTime || 0, dur: isFinite(e.duration) ? e.duration : 0, shuffle: S.shuffle, repeat: S.repeat,
      fade: S.fade, vol: S.vol, loading: loading });
  }
  function queue() { emit({ ev: 'queue', ids: S.ids.slice(Math.max(S.index, 0), Math.max(S.index, 0) + 100), index: S.index, len: S.ids.length }); }
  // The songs heard last, newest first (kept on this device).
  function recent() { emit({ ev: 'recent', ids: S.recent }); }
  // What was played is told to the service too, so that "Recently played" is the same on every device; and what the service
  // knows (from the other devices) replaces what this browser kept, which is only for when the service cannot be reached.
  function tell(id) {
    try {
      fetch(base + '/api/v1/history', { method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ song_id: id }) }).catch(function () { /* offline: kept here only */ });
    } catch (e) { /* no fetch */ }
  }
  function syncRecent() {
    fetch(base + '/api/v1/library/recent?limit=40', { credentials: 'same-origin' }).then(function (r) { return r.ok ? r.json() : null; }).then(function (list) {
      if (!list) return;
      // The service's list is this person's: what this browser kept may be another person's, so it is replaced, not merged.
      S.recent = list.songs.map(function (s) { return s.id; }).slice(0, 40);
      try { localStorage.setItem('sb.recent', JSON.stringify(S.recent)); } catch (e) { /* storage may be blocked */ }
      recent();
    }).catch(function () { /* offline: what is here stays */ });
  }
  function heard(id) {
    if (!id) return;
    tell(id);
    S.recent = [id].concat(S.recent.filter(function (x) { return x !== id; })).slice(0, 40);
    try { localStorage.setItem('sb.recent', JSON.stringify(S.recent)); } catch (e) { /* storage may be blocked */ }
    recent();
  }

  // ---- where the person stopped, on every device ------------------------------------------------------------------
  // The service keeps one place per person: the song, the exact point in it and the list. This device writes it while it
  // plays (and when it pauses or the page goes away), and takes a newer one written by another device, paused at that point.
  function device() {
    try {
      var d = localStorage.getItem('sb.device');
      if (!d) { d = Math.random().toString(36).slice(2) + Date.now().toString(36); localStorage.setItem('sb.device', d); }
      return d;
    } catch (e) { return 'web'; }
  }
  var lastPush = 0;
  function pushResume(force) {
    var id = S.ids[S.index];
    if (!id || !S.touched) return;
    var now = Date.now();
    if (!force && now - lastPush < 10000) return;
    lastPush = now;
    // At most a window of the list, with the song's place in it.
    var from = Math.max(0, S.index - 100);
    var body = JSON.stringify({ song_id: id, position_ms: Math.round((els[cur].currentTime || 0) * 1000),
      queue: S.ids.slice(from, from + 2000), index: S.index - from, device: device() });
    try {
      fetch(base + '/api/v1/resume', { method: 'PUT', credentials: 'same-origin', keepalive: true,
        headers: { 'Content-Type': 'application/json' }, body: body })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (r) { if (r && r.at) { S.synced = r.at; } })
        .catch(function () { /* offline: the next write tells it */ });
    } catch (e) { /* no fetch */ }
  }
  // Seeks once the song is loaded far enough to be seekable.
  function seekWhenReady(e, pos) {
    if (!pos) return;
    var set = function () { try { e.currentTime = pos; } catch (x) { /* not seekable yet */ } };
    if (e.readyState >= 1) set();
    else e.addEventListener('loadedmetadata', function once() { e.removeEventListener('loadedmetadata', once); set(); });
  }
  function prepareAt(index, pos) { return prepare(cur, index).then(function () { seekWhenReady(els[cur], pos); }); }
  function pullResume() {
    fetch(base + '/api/v1/resume', { credentials: 'same-origin' }).then(function (r) { return r.ok ? r.json() : null; }).then(function (r) {
      if (!r || !r.song_id) return;
      // Written by this device, or already taken: nothing to do. Playing here: this device is the one listened to.
      if (r.device === device() || r.at <= (S.synced || 0)) { S.synced = Math.max(S.synced || 0, r.at); return; }
      var e = els[cur];
      if (!e.paused && !e.ended) return;
      S.ids = r.queue.slice(); S.orig = S.ids.slice(); S.shuffle = false; S.index = r.index;
      S.synced = r.at; S.touched = false;
      stopFade(); els[1 - cur].pause();
      prepareAt(S.index, r.position_ms / 1000).then(function () { meta(); state(); queue(); prepareNext(); save(); });
      state(); queue();
    }).catch(function () { /* offline: what is here stays */ });
  }
  function fail(message) { emit({ ev: 'error', message: message }); }

  // ---- the sound -------------------------------------------------------------------------------------------------
  function audio() {
    if (!ctx) {
      var C = window.AudioContext || window.webkitAudioContext;
      if (C) {
        try {
          ctx = new C();
          for (var i = 0; i < 2; i++) {
            gains[i] = ctx.createGain();
            ctx.createMediaElementSource(els[i]).connect(gains[i]);
            gains[i].connect(ctx.destination);
          }
        } catch (e) { ctx = null; gains = [null, null]; }
      }
    }
    if (ctx && ctx.state === 'suspended') ctx.resume();
  }
  // Sets how loud element `i` is, as a share (0-1) of the volume the person chose.
  function level(i, v) {
    if (gains[i]) { gains[i].gain.cancelScheduledValues(0); gains[i].gain.value = v * S.vol; els[i].volume = 1; }
    else els[i].volume = v * S.vol;
  }
  function url(id) { return base + '/api/v1/stream/' + id; }

  // The address to play a song from: the copy on this device when there is one, else the service.
  function source(i, id) {
    if (!S.cached[id] || !window.caches) return Promise.resolve(url(id));
    return caches.open(NS).then(function (c) { return c.match(url(id)); }).then(function (r) {
      return r ? r.blob() : null;
    }).then(function (b) {
      if (!b) return url(id);
      if (urls[i]) URL.revokeObjectURL(urls[i]);
      urls[i] = URL.createObjectURL(b);
      return urls[i];
    }).catch(function () { return url(id); });
  }
  function prepare(i, index) {
    var id = S.ids[index];
    var e = els[i];
    if (!id) { e.removeAttribute('src'); e.dataset.id = ''; return Promise.resolve(); }
    if (e.dataset.id === String(id) && e.getAttribute('src')) return Promise.resolve();
    e.dataset.id = String(id);
    return source(i, id).then(function (src) {
      if (e.dataset.id !== String(id)) return;
      e.preload = 'auto'; e.src = src; e.load();
    });
  }
  function nextIndex() {
    if (!S.ids.length) return -1;
    if (S.repeat === 'one') return S.index;
    if (S.index + 1 < S.ids.length) return S.index + 1;
    return S.repeat === 'all' ? 0 : -1;
  }
  function prepareNext() { var n = nextIndex(); if (n >= 0 && n !== S.index) prepare(1 - cur, n); }

  function stopFade() {
    clearTimeout(fadeTimer); fading = false;
    for (var i = 0; i < 2; i++) { if (i !== cur) { els[i].pause(); } level(i, i === cur ? 1 : 0); }
  }
  // Starts song `index` now.
  function go(index, at) {
    if (index < 0 || index >= S.ids.length) { stop(); return Promise.resolve(); }
    audio(); stopFade();
    S.index = index; loading = true; state(); queue();
    els[1 - cur].pause();
    var e = els[cur];
    return prepare(cur, index).then(function () {
      level(cur, 1);
      if (at) { try { e.currentTime = at; } catch (x) { /* not seekable yet */ } }
      return e.play();
    }).then(function () { loading = false; meta(); heard(S.ids[S.index]); state(); prepareNext(); save(); pushResume(true); })
      .catch(function (x) { loading = false; state(); fail('Cannot play this song' + (x && x.name === 'NotAllowedError' ? ': tap play' : '')); });
  }
  function stop() { stopFade(); els[cur].pause(); S.index = -1; state(); queue(); }
  function advance() {
    var n = nextIndex();
    if (n < 0) { els[cur].pause(); state(); return; }
    go(n);
  }
  function crossfade() {
    var n = nextIndex();
    if (n < 0 || n === S.index || fading) return;
    var from = cur, to = 1 - cur, a = els[from], b = els[to];
    var left = Math.max(0.3, Math.min(S.fade, (a.duration || 0) - a.currentTime));
    prepare(to, n).then(function () {
      if (fading) return;
      fading = true;
      b.currentTime = 0;
      level(to, 0);
      return b.play().then(function () {
        if (ctx && gains[0]) {
          var t = ctx.currentTime;
          gains[from].gain.cancelScheduledValues(t); gains[to].gain.cancelScheduledValues(t);
          gains[from].gain.setValueAtTime(S.vol, t); gains[from].gain.linearRampToValueAtTime(0, t + left);
          gains[to].gain.setValueAtTime(0, t); gains[to].gain.linearRampToValueAtTime(S.vol, t + left);
        } else {
          var step = 0, steps = Math.max(1, Math.round(left * 10));
          (function ramp() {
            step++; var k = step / steps;
            a.volume = Math.max(0, (1 - k) * S.vol); b.volume = Math.min(1, k * S.vol);
            if (step < steps) fadeTimer = setTimeout(ramp, 100);
          })();
        }
        clearTimeout(fadeTimer);
        fadeTimer = setTimeout(function () {
          a.pause(); fading = false; cur = to; S.index = n;
          level(from, 0); level(to, 1);
          meta(); heard(S.ids[S.index]); state(); queue(); prepareNext(); save();
        }, left * 1000 + 50);
      });
    }).catch(function () { fading = false; });
  }
  function tick() {
    var e = els[cur];
    if (S.fade > 0 && !fading && !e.paused && e.duration && e.duration - e.currentTime <= S.fade + 0.4 && nextIndex() >= 0 && S.repeat !== 'one') crossfade();
    state();
    if (ms() && e.duration) { try { navigator.mediaSession.setPositionState({ duration: e.duration, position: Math.min(e.currentTime, e.duration), playbackRate: 1 }); } catch (x) { /* some browsers refuse */ } }
  }
  function ms() { return 'mediaSession' in navigator; }
  function meta() {
    if (!ms()) return;
    var id = S.ids[S.index], m = S.meta[id];
    var pic = base + '/api/v1/art/song/' + id;
    navigator.mediaSession.metadata = m ? new MediaMetadata({ title: m.title, artist: m.artist, album: m.album,
      artwork: [{ src: pic + '?w=160', sizes: '160x160', type: 'image/jpeg' }, { src: pic + '?w=320', sizes: '320x320', type: 'image/jpeg' }, { src: pic, sizes: '500x500', type: 'image/jpeg' }] }) : null;
  }

  // ---- the queue ------------------------------------------------------------------------------------------------
  function shuffled(list, first) {
    var rest = list.filter(function (x, i) { return i !== first; });
    for (var i = rest.length - 1; i > 0; i--) { var j = Math.floor(Math.random() * (i + 1)); var t = rest[i]; rest[i] = rest[j]; rest[j] = t; }
    return first == null || first < 0 ? rest : [list[first]].concat(rest);
  }
  function load(c) {
    S.orig = (c.queue || []).slice();
    var start = Math.min(Math.max(c.index || 0, 0), Math.max(S.orig.length - 1, 0));
    if (c.shuffle != null) S.shuffle = !!c.shuffle;
    if (S.shuffle) { S.ids = shuffled(S.orig, c.index == null || c.index < 0 ? null : start); start = 0; }
    else S.ids = S.orig.slice();
    if (c.shuffle && (c.index == null || c.index < 0)) start = 0;
    S.index = -1; els[0].dataset.id = ''; els[1].dataset.id = '';
    return go(start);
  }
  function setShuffle(on) {
    on = !!on;
    if (on === S.shuffle) return;
    var now = S.ids[S.index];
    S.shuffle = on;
    if (on) { var at = S.index; S.ids = shuffled(S.ids, at < 0 ? null : at); S.index = at < 0 ? -1 : 0; }
    else { S.ids = S.orig.slice(); S.index = Math.max(S.ids.indexOf(now), S.index < 0 ? -1 : 0); }
    els[1 - cur].dataset.id = ''; prepareNext(); queue(); state(); save();
  }
  function enqueue(ids, next) {
    if (!ids.length) return;
    if (!S.ids.length) { load({ queue: ids, index: 0 }); return; }
    var at = next ? S.index + 1 : S.ids.length;
    S.ids.splice.apply(S.ids, [at, 0].concat(ids));
    if (next) { S.orig.splice.apply(S.orig, [S.orig.length, 0].concat(ids)); } else { S.orig = S.orig.concat(ids); }
    els[1 - cur].dataset.id = ''; prepareNext(); queue(); state(); save();
  }

  // ---- songs kept on the device ------------------------------------------------------------------------------------
  function cachedList() {
    if (!window.caches) return Promise.resolve();
    return caches.open(NS).then(function (c) { return c.keys(); }).then(function (keys) {
      S.cached = {};
      keys.forEach(function (r) { var m = /\/stream\/(\d+)$/.exec(new URL(r.url).pathname); if (m) S.cached[m[1]] = 1; });
      emit({ ev: 'cached', ids: Object.keys(S.cached).map(Number) });
      return storage();
    }).catch(function () { /* no storage: nothing is kept */ });
  }
  function storage() {
    if (navigator.storage && navigator.storage.estimate) {
      return navigator.storage.estimate().then(function (e) { emit({ ev: 'storage', used: e.usage || 0, quota: e.quota || 0 }); });
    }
  }
  function keep(ids) {
    if (!window.caches || saving) { if (!window.caches) fail('This browser cannot keep songs on the device'); return; }
    saving = true;
    var todo = ids.filter(function (id) { return !S.cached[id]; }), done = 0;
    emit({ ev: 'saving', done: 0, total: todo.length });
    return caches.open(NS).then(function (c) {
      return todo.reduce(function (p, id) {
        return p.then(function () {
          return fetch(url(id), { credentials: 'same-origin' }).then(function (r) {
            if (!r.ok) throw new Error('status ' + r.status);
            return c.put(url(id), r);
          }).then(function () {
            // Its picture too, in the sizes the player shows (the service worker keeps them), so the song looks the same
            // without the network.
            ['?w=160', '?w=320', ''].forEach(function (q) { fetch(base + '/api/v1/art/song/' + id + q, { credentials: 'same-origin' }).catch(function () { /* none */ }); });
            S.cached[id] = 1; done++; emit({ ev: 'saving', done: done, total: todo.length });
          });
        });
      }, Promise.resolve());
    }).catch(function (x) {
      fail(x && x.name === 'QuotaExceededError' ? 'The device has no room for more songs' : 'Could not keep the songs');
    }).then(function () { saving = false; emit({ ev: 'saving', done: todo.length, total: todo.length }); return cachedList(); });
  }
  function drop(ids) {
    if (!window.caches) return;
    return caches.open(NS).then(function (c) {
      return Promise.all(ids.map(function (id) { delete S.cached[id]; return c.delete(url(id)); }));
    }).then(cachedList);
  }

  // ---- commands -------------------------------------------------------------------------------------------------
  function run(c) {
    var e = els[cur];
    switch (c.op) {
      case 'load': return load(c);
      case 'play': audio(); if (S.index < 0 && S.ids.length) return go(0, S.pending); return e.play().then(state); 
      case 'pause': e.pause(); state(); save(); return;
      case 'toggle': return e.paused ? run({ op: 'play' }) : run({ op: 'pause' });
      case 'next': return advance();
      case 'prev': if ((e.currentTime || 0) > 3 || S.index <= 0) { e.currentTime = 0; state(); return; } return go(S.index - 1);
      case 'jump': return go(c.index);
      case 'seek': e.currentTime = Math.max(0, c.to); state(); return;
      case 'shuffle': return setShuffle(c.on);
      case 'repeat': S.repeat = c.mode; prepareNext(); state(); save(); return;
      case 'fade': S.fade = Math.max(0, Math.min(12, c.secs || 0)); state(); save(); return;
      case 'volume': S.vol = Math.max(0, Math.min(1, c.v)); level(cur, 1); state(); save(); return;
      case 'meta': c.songs.forEach(function (m) { S.meta[m.id] = m; }); meta(); return;
      case 'enqueue': return enqueue(c.ids || [], !!c.next);
      case 'remove': var i = c.index; if (i === S.index) return; var gone = S.ids.splice(i, 1)[0]; if (i < S.index) S.index--; var k = S.orig.indexOf(gone); if (k >= 0) S.orig.splice(k, 1); queue(); state(); save(); return;
      case 'keep': return keep(c.ids || []);
      case 'drop': return drop(c.ids || []);
      case 'ask': state(); queue(); recent(); return cachedList();
    }
  }
  var seen = 0;
  function commands() {
    var list;
    try { list = JSON.parse(cmdEl.getAttribute('data-cmd') || '[]'); } catch (e) { return; }
    list.forEach(function (c) { if (c.n > seen) { seen = c.n; try { run(c); } catch (x) { fail(String(x)); } } });
  }

  function hook(e, i) {
    e.addEventListener('ended', function () { if (i === cur && !fading) advance(); });
    e.addEventListener('play', function () { if (i === cur) S.touched = true; });
    e.addEventListener('pause', function () { if (i === cur && !fading) pushResume(true); });
    e.addEventListener('seeked', function () { if (i === cur) pushResume(false); });
    ['play', 'pause', 'waiting', 'playing', 'seeked', 'loadedmetadata'].forEach(function (n) { e.addEventListener(n, function () { if (i === cur) state(); }); });
    e.addEventListener('error', function () { if (i === cur && e.getAttribute('src')) { fail('Cannot play this song'); setTimeout(advance, 500); } });
  }
  hook(els[0], 0); hook(els[1], 1);
  setInterval(function () { if (!els[cur].paused) { tick(); } }, 250);
  setInterval(save, 5000);
  setInterval(function () { if (!els[cur].paused) pushResume(false); else if (!document.hidden) pullResume(); }, 10000);

  if (ms()) {
    var h = function (n, f) { try { navigator.mediaSession.setActionHandler(n, f); } catch (e) { /* not supported */ } };
    h('play', function () { run({ op: 'play' }); }); h('pause', function () { run({ op: 'pause' }); });
    h('previoustrack', function () { run({ op: 'prev' }); }); h('nexttrack', function () { run({ op: 'next' }); });
    h('seekto', function (d) { run({ op: 'seek', to: d.seekTime }); });
    h('seekbackward', function (d) { run({ op: 'seek', to: els[cur].currentTime - (d.seekOffset || 10) }); });
    h('seekforward', function (d) { run({ op: 'seek', to: els[cur].currentTime + (d.seekOffset || 10) }); });
  }
  // ---- the keyboard -----------------------------------------------------------------------------------------------
  // Space (or K) plays and pauses; the arrows (or J/L) seek 5 (10) seconds; N and P go to the next and the previous song;
  // M mutes. They leave a text field alone, and anything with Ctrl, Alt or Meta held (the browser's own).
  var muted = 0;
  function typing(t) {
    if (!t || !t.tagName) return false;
    var tag = t.tagName.toLowerCase();
    return tag === 'textarea' || tag === 'select' || t.isContentEditable ||
      (tag === 'input' && !/^(range|checkbox|radio|button|submit)$/.test(t.type || 'text'));
  }
  // A button or link reached with Tab keeps its own Space and Enter; the mouse leaves no focus behind (below).
  function interactive(t) {
    if (!t || !t.tagName) return false;
    var tag = t.tagName.toLowerCase();
    return tag === 'button' || tag === 'a' || tag === 'summary' || (tag === 'input' && /^(range|checkbox|radio|button|submit)$/.test(t.type)) ||
      (t.getAttribute && /^(button|link|menuitem|tab|switch|checkbox)$/.test(t.getAttribute('role') || ''));
  }
  document.addEventListener('click', function (ev) {
    var t = ev.target && ev.target.closest ? ev.target.closest('button') : null;
    if (t && ev.detail > 0 && t.blur) t.blur(); // a click, not Space or Enter on a focused button
  }, true);
  function seekBy(secs) { var e = els[cur]; if (S.index < 0) return; run({ op: 'seek', to: Math.min(isFinite(e.duration) ? e.duration : 1e9, (e.currentTime || 0) + secs) }); }
  document.addEventListener('keydown', function (ev) {
    if (ev.ctrlKey || ev.altKey || ev.metaKey || ev.defaultPrevented || typing(ev.target)) return;
    var k = ev.key, done = true;
    if ((k === ' ' || k === 'Spacebar') && interactive(ev.target)) return;
    if (k === ' ' || k === 'Spacebar' || k === 'k' || k === 'K') run({ op: 'toggle' });
    else if ((k === 'ArrowRight' || k === 'ArrowLeft') && ev.target && ev.target.type === 'range') done = false; // the slider moves by itself
    else if (k === 'ArrowRight') seekBy(5);
    else if (k === 'ArrowLeft') seekBy(-5);
    else if (k === 'l' || k === 'L') seekBy(10);
    else if (k === 'j' || k === 'J') seekBy(-10);
    else if (k === 'n' || k === 'N') run({ op: 'next' });
    else if (k === 'p' || k === 'P') run({ op: 'prev' });
    else if (k === 'm' || k === 'M') { if (S.vol > 0) { muted = S.vol; run({ op: 'volume', v: 0 }); } else { run({ op: 'volume', v: muted || 1 }); } }
    else done = false;
    if (done) ev.preventDefault();
  });
  // The space bar's release would also "click" a focused element: the key was already used.
  document.addEventListener('keyup', function (ev) {
    if ((ev.key === ' ' || ev.key === 'Spacebar') && !(ev.ctrlKey || ev.altKey || ev.metaKey) && !typing(ev.target) && !interactive(ev.target)) ev.preventDefault();
  });
  window.addEventListener('beforeunload', save);
  window.addEventListener('pagehide', function () { pushResume(true); });
  document.addEventListener('visibilitychange', function () { if (!document.hidden) pullResume(); });
  restore();

  // The screens draw their elements after this script has run: wait for them.
  function attach() {
    var c = document.getElementById('sb-engine'), i = document.getElementById('sb-engine-in');
    if (!c || !i || c === cmdEl) return;
    cmdEl = c; out = i; base = c.getAttribute('data-base') || '';
    // Pictures, the player's files and its last page are kept on the device by a service worker.
    if ('serviceWorker' in navigator) navigator.serviceWorker.register(base + '/sw.js', { scope: base + '/' }).catch(function () { /* not available here */ });
    new MutationObserver(commands).observe(c, { attributes: true, attributeFilter: ['data-cmd'] });
    commands(); state(); queue(); recent(); syncRecent(); cachedList();
    if (S.index >= 0) prepareAt(S.index, S.pending);
    pullResume();
  }
  new MutationObserver(attach).observe(document.documentElement, { childList: true, subtree: true });
  attach();
})();
