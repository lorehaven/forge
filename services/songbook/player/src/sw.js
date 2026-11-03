// Songbook's service worker: keeps what the player shows on the device, so it comes up at once and also without the network.
//   pictures (/api/v1/art/...)    from the device first, then the network (a picture of a song does not change)
//   the player's files (/ui/assets/)  from the device at once, and fetched again for the next time
//   the library and playlists (/api/v1/library/..., /api/v1/playlists)  from the network, else the last answer
//   the player's page (/ui, /ui/album/7, ...)  from the network, else the last one that came (it is the same page: the
//                                  client draws the screen from the address)
// Songs themselves are not here: the player keeps those (Cache Storage, "songbook-songs") when asked.
(function () {
  'use strict';
  var V = 'v1', ART = 'songbook-art-' + V, SHELL = 'songbook-shell-' + V, DATA = 'songbook-data-' + V;
  var MAX_ART = 800, MAX_DATA = 300;
  var scope = new URL(self.registration.scope).pathname;           // /songbook/ (or /)
  var ui = scope + 'ui';
  var PAGE = ui;                                                    // the key the page is kept under

  self.addEventListener('install', function () { self.skipWaiting(); });
  self.addEventListener('activate', function (e) {
    e.waitUntil(caches.keys().then(function (names) {
      return Promise.all(names.filter(function (n) { return /^songbook-(art|shell|data)-/.test(n) && n !== ART && n !== SHELL && n !== DATA; })
        .map(function (n) { return caches.delete(n); }));
    }).then(function () { return self.clients.claim(); }));
  });

  // The oldest pictures go when there are too many.
  function trim(cache, most) {
    return cache.keys().then(function (keys) {
      return Promise.all(keys.slice(0, Math.max(0, keys.length - most)).map(function (k) { return cache.delete(k); }));
    });
  }
  // What the service answered, kept for when it cannot be reached.
  function data(request) {
    return fetch(request).then(function (r) {
      if (r.ok) caches.open(DATA).then(function (c) { return c.put(request, r.clone()).then(function () { return trim(c, MAX_DATA); }); });
      return r;
    }).catch(function () {
      return caches.open(DATA).then(function (c) { return c.match(request); }).then(function (hit) { return hit || Response.error(); });
    });
  }
  function art(request) {
    return caches.open(ART).then(function (cache) {
      return cache.match(request).then(function (hit) {
        if (hit) return hit;
        return fetch(request).then(function (r) {
          if (r.ok) { cache.put(request, r.clone()).then(function () { return trim(cache, MAX_ART); }); }
          return r;
        });
      });
    });
  }
  function asset(request) {
    return caches.open(SHELL).then(function (cache) {
      return cache.match(request).then(function (hit) {
        var fresh = fetch(request).then(function (r) { if (r.ok) cache.put(request, r.clone()); return r; });
        if (hit) { fresh.catch(function () { /* offline: what is kept is used */ }); return hit; }
        return fresh;
      });
    });
  }
  function page(request) {
    return fetch(request).then(function (r) {
      // Only the page itself: a sign-in redirect is not kept.
      if (r.ok && r.type === 'basic') caches.open(SHELL).then(function (c) { c.put(PAGE, r.clone()); });
      return r;
    }).catch(function () {
      return caches.open(SHELL).then(function (c) { return c.match(PAGE); }).then(function (hit) { return hit || Response.error(); });
    });
  }

  self.addEventListener('fetch', function (e) {
    var r = e.request;
    if (r.method !== 'GET') return;
    var url = new URL(r.url);
    if (url.origin !== self.location.origin) return;
    var path = url.pathname;
    if (path.indexOf(scope + 'api/v1/art/') === 0) { e.respondWith(art(r)); return; }
    if (path.indexOf(scope + 'api/v1/library/') === 0 || path.indexOf(scope + 'api/v1/playlists') === 0) { e.respondWith(data(r)); return; }
    if (path.indexOf(ui + '/assets/') === 0) { e.respondWith(asset(r)); return; }
    var player = path === ui + '/home' || /^\/?.*\/ui\/(search|artists|artist|album|category|playlists|playlist|saved)(\/|$)/.test(path) && path.indexOf(ui + '/') === 0;
    if (r.mode === 'navigate' && player) e.respondWith(page(r));
  });
})();
