/* The navigation (a menu button on a narrow screen, the sections that open) and the search over the book's
   search-data.json. */
(function () {
  'use strict';
  var SEARCH = '__SEARCH_URL__';

  var bar = document.querySelector('.side-bar');
  var menu = document.getElementById('menu-button');
  if (bar && menu) {
    menu.addEventListener('click', function () {
      var open = bar.classList.toggle('nav-open');
      menu.setAttribute('aria-expanded', open ? 'true' : 'false');
    });
  }
  document.addEventListener('click', function (e) {
    var t = e.target.closest ? e.target.closest('.nav-list-expander') : null;
    if (!t) return;
    var item = t.parentNode;
    var open = item.classList.toggle('active');
    t.setAttribute('aria-pressed', open ? 'true' : 'false');
  });
  var current = document.querySelector('.nav-list-link.active');
  if (current && current.scrollIntoView) current.scrollIntoView({ block: 'center' });

  var input = document.getElementById('search-input');
  var results = document.getElementById('search-results');
  if (!input || !results) return;
  var docs = null;
  var loading = null;

  function load() {
    if (!loading) {
      loading = fetch(SEARCH).then(function (r) { return r.json(); }).then(function (data) {
        docs = Object.keys(data).map(function (k) { return data[k]; });
      }).catch(function () { docs = []; });
    }
    return loading;
  }
  function escape(text) {
    return text.replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; });
  }
  function score(doc, words) {
    var title = doc.title.toLowerCase();
    var body = doc.content.toLowerCase();
    var total = 0;
    for (var i = 0; i < words.length; i++) {
      var w = words[i];
      var inTitle = title.indexOf(w) !== -1;
      var inBody = body.indexOf(w) !== -1;
      if (!inTitle && !inBody) return 0;
      total += (inTitle ? 10 : 0) + (inBody ? 1 : 0) + (title === w ? 20 : 0);
    }
    return total;
  }
  function preview(doc, word) {
    var at = doc.content.toLowerCase().indexOf(word);
    if (at < 0) return doc.content.slice(0, 90);
    var from = Math.max(0, at - 30);
    return (from > 0 ? '… ' : '') + doc.content.slice(from, from + 100);
  }
  function show(list, words) {
    if (!list.length) {
      results.innerHTML = '<div class="search-no-result">No results found</div>';
    } else {
      results.innerHTML = '<ul class="search-results-list">' + list.map(function (doc) {
        return '<li><a class="search-result" href="' + escape(doc.url) + '"><span class="search-result-title">' + escape(doc.title) +
          '</span>' + (doc.doc && doc.doc !== doc.title ? '<span class="search-result-doc-title">' + escape(doc.doc) + '</span>' : '') +
          '<span class="search-result-preview">' + escape(preview(doc, words[0])) + '</span></a></li>';
      }).join('') + '</ul>';
    }
    results.classList.add('open');
  }
  function run() {
    var words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) { results.classList.remove('open'); return; }
    load().then(function () {
      var scored = docs.map(function (d) { return { d: d, s: score(d, words) }; }).filter(function (x) { return x.s > 0; });
      scored.sort(function (a, b) { return b.s - a.s; });
      show(scored.slice(0, 10).map(function (x) { return x.d; }), words);
    });
  }
  input.addEventListener('input', run);
  input.addEventListener('focus', function () { load(); if (input.value) run(); });
  document.addEventListener('keydown', function (e) {
    if (e.key === '/' && document.activeElement !== input && !/input|textarea/i.test(document.activeElement.tagName)) {
      e.preventDefault(); input.focus();
    } else if (e.key === 'Escape') {
      results.classList.remove('open'); input.blur();
    }
  });
  document.addEventListener('click', function (e) {
    if (!e.target.closest || !e.target.closest('.search')) results.classList.remove('open');
  });
})();
