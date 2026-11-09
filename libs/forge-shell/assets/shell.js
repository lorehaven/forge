// forge-shell: what the page needs besides the client itself. Theme and palette follow the cookies the other forge
// services use (`qtheme`, `fui_palette`), the language follows `qlocale`, and links to gatehouse are filled in from
// the address this script was told about. The client only draws the elements; everything here is delegated.
(function () {
  var root = document.documentElement;
  var PALETTES = ['green', 'teal', 'blue', 'purple', 'rose', 'orange'];
  function cookie(name) {
    var m = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
    return m ? decodeURIComponent(m[1]) : null;
  }
  function put(name, value) {
    document.cookie = name + '=' + encodeURIComponent(value) + '; max-age=31536000; path=/; SameSite=Lax';
  }
  function mode() { var t = cookie('qtheme'); return t && t.indexOf('light') !== -1 ? 'light' : 'dark'; }
  function palette() { var p = cookie('fui_palette'); return PALETTES.indexOf(p) !== -1 ? p : 'green'; }
  function apply() {
    if (root.getAttribute('data-fui-theme') !== mode()) root.setAttribute('data-fui-theme', mode());
    if (root.getAttribute('data-fui-palette') !== palette()) root.setAttribute('data-fui-palette', palette());
  }
  apply();
  setInterval(apply, 1000);

  // The app keeps its language in the page's storage; start from the cookie when it has none yet.
  try {
    var chosen = cookie('qlocale');
    if (chosen && !localStorage.getItem('q:language')) localStorage.setItem('q:language', chosen);
  } catch (e) { /* storage may be blocked */ }

  // The side panel: folded to its icons or not, kept in the page's storage.
  function folded(on) {
    if (on) root.setAttribute('data-fui-nav', 'folded'); else root.removeAttribute('data-fui-nav');
  }
  try { folded(localStorage.getItem('fui-nav') === 'folded'); } catch (e) { /* storage may be blocked */ }
  // A folded item has no visible text: its name is the tooltip.
  document.addEventListener('mouseover', function (e) {
    var item = e.target.closest && e.target.closest('.fui-nav-item');
    if (item && !item.getAttribute('title')) item.setAttribute('title', item.textContent.trim());
  });

  document.addEventListener('click', function (e) {
    var t = e.target.closest ? e.target : e.target.parentNode;
    if (t.closest('[data-fui-nav-toggle]')) {
      var on = !root.hasAttribute('data-fui-nav');
      folded(on);
      try { localStorage.setItem('fui-nav', on ? 'folded' : 'open'); } catch (err) { /* storage may be blocked */ }
    }
    var m = t.closest('[data-fui-mode]');
    if (m) { put('qtheme', 'default-' + m.getAttribute('data-fui-mode')); apply(); }
    var p = t.closest('[data-fui-palette]');
    if (p) { put('fui_palette', p.getAttribute('data-fui-palette')); apply(); }
    var keep = t.closest('details.fui-dropdown');
    document.querySelectorAll('details.fui-dropdown[open]').forEach(function (d) { if (d !== keep) d.removeAttribute('open'); });
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') document.querySelectorAll('details.fui-dropdown[open]').forEach(function (d) { d.removeAttribute('open'); });
  });
  document.addEventListener('change', function (e) {
    if (e.target && e.target.id === 'language') { put('qlocale', e.target.value); put('qlocale_chosen', '1'); }
  });
  // An avatar that loaded replaces the icon; a missing one (404) leaves it.
  document.addEventListener('load', function (e) {
    var img = e.target;
    if (img && img.tagName === 'IMG' && img.parentNode && img.parentNode.classList.contains('fui-avatar')) img.parentNode.classList.add('has-avatar');
  }, true);

  // A canvas: `[data-canvas]` holds one thing (its first child) that zooms with the wheel or the `[data-canvas-zoom]`
  // buttons (`in`, `out`, `reset`; `[data-canvas-level]` shows the percentage) and is dragged around with the pointer.
  // The state lives on the canvas element, so a canvas drawn again starts fitted.
  var LIMITS = [0.1, 16];
  function view(c) { return c._v || (c._v = { s: 1, x: 0, y: 0 }); }
  function paint(c) {
    var v = view(c), item = c.firstElementChild;
    if (item) item.style.transform = 'translate(' + v.x + 'px,' + v.y + 'px) scale(' + v.s + ')';
    var n = document.querySelectorAll('[data-canvas-level]');
    for (var i = 0; i < n.length; i++) n[i].textContent = Math.round(v.s * 100) + '%';
  }
  // Zoom to `s`, keeping the point (px, py), measured from the canvas centre, where it is.
  function zoomAt(c, s, px, py) {
    var v = view(c);
    s = Math.min(LIMITS[1], Math.max(LIMITS[0], s));
    var k = s / v.s;
    v.x = px - k * (px - v.x);
    v.y = py - k * (py - v.y);
    v.s = s;
    paint(c);
  }
  function fromCentre(c, e) {
    var r = c.getBoundingClientRect();
    return [e.clientX - r.left - r.width / 2, e.clientY - r.top - r.height / 2];
  }
  document.addEventListener('wheel', function (e) {
    var c = e.target.closest && e.target.closest('[data-canvas]');
    if (!c) return;
    e.preventDefault();
    var p = fromCentre(c, e);
    zoomAt(c, view(c).s * Math.pow(1.0015, -e.deltaY), p[0], p[1]);
  }, { passive: false });
  var panning = null;
  document.addEventListener('pointerdown', function (e) {
    var c = e.target.closest && e.target.closest('[data-canvas]');
    if (!c || e.button !== 0) return;
    panning = { c: c, x: e.clientX, y: e.clientY };
    c.classList.add('dragging');
    if (c.setPointerCapture) c.setPointerCapture(e.pointerId);
    e.preventDefault();
  });
  document.addEventListener('pointermove', function (e) {
    if (!panning) return;
    var v = view(panning.c);
    v.x += e.clientX - panning.x;
    v.y += e.clientY - panning.y;
    panning.x = e.clientX;
    panning.y = e.clientY;
    paint(panning.c);
  });
  function release() { if (panning) panning.c.classList.remove('dragging'); panning = null; }
  document.addEventListener('pointerup', release);
  document.addEventListener('pointercancel', release);
  document.addEventListener('dblclick', function (e) {
    var c = e.target.closest && e.target.closest('[data-canvas]');
    if (c) { c._v = null; paint(c); }
  });
  document.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('[data-canvas-zoom]');
    var c = b && document.querySelector('[data-canvas]');
    if (!c) return;
    var how = b.getAttribute('data-canvas-zoom'), v = view(c);
    if (how === 'reset') { c._v = null; paint(c); }
    else zoomAt(c, v.s * (how === 'in' ? 1.25 : 0.8), 0, 0);
  });

  // Keyboard: an element with `data-shortcut="prev"` or `"next"` is clicked by the arrow keys (left and up, right and
  // down); the canvas zooms with `+` and `-` and fits with `0`.
  document.addEventListener('keydown', function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    var t = e.target;
    if (t && t.closest && t.closest('input, textarea, select, [contenteditable], audio, video')) return;
    var k = e.key, target = null;
    if (k === 'ArrowLeft' || k === 'ArrowUp') target = document.querySelector('[data-shortcut="prev"]');
    else if (k === 'ArrowRight' || k === 'ArrowDown') target = document.querySelector('[data-shortcut="next"]');
    else if (k === '+' || k === '=') target = document.querySelector('[data-canvas-zoom="in"]');
    else if (k === '-' || k === '_') target = document.querySelector('[data-canvas-zoom="out"]');
    else if (k === '0') target = document.querySelector('[data-canvas-zoom="reset"]');
    if (!target || target.disabled) return;
    e.preventDefault();
    target.click();
  });

  // Drag and drop: a `[data-drag-id]` element can be dropped on a `[data-drop-value]` one. The drop sets the element's
  // `<select>` (named by `data-drag-select`) to the value and fires its input event, so a drag does what the select does.
  var dragged = null;
  function over(e) { return e.target.closest && e.target.closest('[data-drop-value]'); }
  document.addEventListener('dragstart', function (e) {
    var card = e.target.closest && e.target.closest('[data-drag-id]');
    if (!card) return;
    dragged = card;
    e.dataTransfer.setData('text/plain', card.getAttribute('data-drag-id'));
    e.dataTransfer.effectAllowed = 'move';
    card.classList.add('fui-dragging');
  });
  document.addEventListener('dragend', function () {
    if (dragged) dragged.classList.remove('fui-dragging');
    dragged = null;
    document.querySelectorAll('.fui-drop-target').forEach(function (n) { n.classList.remove('fui-drop-target'); });
  });
  document.addEventListener('dragover', function (e) {
    if (dragged && over(e)) { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }
  });
  document.addEventListener('dragenter', function (e) {
    var t = over(e);
    if (dragged && t) t.classList.add('fui-drop-target');
  });
  document.addEventListener('dragleave', function (e) {
    var t = over(e);
    if (t && !t.contains(e.relatedTarget)) t.classList.remove('fui-drop-target');
  });
  document.addEventListener('drop', function (e) {
    var t = over(e);
    if (!dragged || !t) return;
    e.preventDefault();
    t.classList.remove('fui-drop-target');
    var select = document.getElementById(dragged.getAttribute('data-drag-select') || '');
    var value = t.getAttribute('data-drop-value');
    if (select && select.value !== value) {
      select.value = value;
      select.dispatchEvent(new Event('input', { bubbles: true }));
    }
  });

  // Copy: a `[data-copy-target]` element copies the text of the element whose id it names (a log) when it is clicked.
  document.addEventListener('click', function (e) {
    var button = e.target.closest && e.target.closest('[data-copy-target]');
    var source = button && document.getElementById(button.getAttribute('data-copy-target') || '');
    if (!source || !navigator.clipboard) return;
    navigator.clipboard.writeText(source.innerText).then(function () {
      button.classList.add('fui-copied');
      setTimeout(function () { button.classList.remove('fui-copied'); }, 1500);
    });
  });

  // Scroll: a `[data-scroll-to]` element scrolls the element whose id it names into view when it is clicked.
  document.addEventListener('click', function (e) {
    var dot = e.target.closest && e.target.closest('[data-scroll-to]');
    var target = dot && document.getElementById(dot.getAttribute('data-scroll-to') || '');
    if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });

  // Upload: an `<input type="file" data-upload-url="...">` sends each chosen file to that address as `multipart/form-data`
  // (the file as `file`, plus the fields in the JSON object of `data-upload-fields`), with the page's cookies, and reports what
  // came back to the input named by `data-upload-result` as a JSON `{name, status, body}` in its value and an `input` event.
  // The client cannot read a chosen file itself; this is how it gets one to the service.
  document.addEventListener('change', function (e) {
    var picker = e.target;
    var url = picker && picker.getAttribute && picker.getAttribute('data-upload-url');
    if (!url || !picker.files) return;
    var fields = {};
    try { fields = JSON.parse(picker.getAttribute('data-upload-fields') || '{}'); } catch (err) { /* no fields */ }
    var report = document.getElementById(picker.getAttribute('data-upload-result') || '');
    var files = Array.prototype.slice.call(picker.files);
    picker.value = '';
    files.forEach(function (file) {
      var form = new FormData();
      Object.keys(fields).forEach(function (name) { form.append(name, fields[name]); });
      form.append('file', file, file.name);
      var done = function (status, body) {
        if (!report) return;
        report.value = JSON.stringify({ name: file.name, status: status, body: body });
        report.dispatchEvent(new Event('input', { bubbles: true }));
      };
      fetch(url, { method: 'POST', body: form, credentials: 'same-origin' })
        .then(function (r) { return r.text().then(function (body) { done(r.status, body); }); })
        .catch(function (err) { done(0, String(err)); });
    });
  });

  // A scrolling element marked `data-follow` (a log) stays at its end while it grows, until the reader scrolls up from the end;
  // scrolling back down to the end makes it follow again. The attribute holds `end` or `free`.
  document.addEventListener('scroll', function (e) {
    var el = e.target;
    if (!el || !el.hasAttribute || !el.hasAttribute('data-follow')) return;
    el.setAttribute('data-follow', el.scrollHeight - el.scrollTop - el.clientHeight < 24 ? 'end' : 'free');
  }, true);
  function follow() {
    document.querySelectorAll('[data-follow]:not([data-follow="free"])').forEach(function (el) { el.scrollTop = el.scrollHeight; });
  }
  new MutationObserver(follow).observe(document.documentElement, { childList: true, subtree: true, characterData: true });
  follow();

  var script = document.currentScript;
  var gatehouse =((script && script.getAttribute('data-gatehouse')) || '/gatehouse').replace(/\/+$/, '');
  function links() {
    document.querySelectorAll('[data-gh-href]').forEach(function (el) { el.setAttribute('href', gatehouse + el.getAttribute('data-gh-href')); });
    document.querySelectorAll('[data-gh-src]').forEach(function (el) {
      var src = gatehouse + el.getAttribute('data-gh-src');
      if (el.getAttribute('src') !== src) el.setAttribute('src', src);
    });
  }
  new MutationObserver(links).observe(document.documentElement, { childList: true, subtree: true });
  links();
})();
