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

  document.addEventListener('click', function (e) {
    var t = e.target.closest ? e.target : e.target.parentNode;
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
