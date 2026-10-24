//! The small amount of browser behaviour the shell needs. All of it is progressive: without
//! JS the sidebar is simply hidden on mobile and the theme stays dark.

/// Set when someone picks a language in the switch. quench's own script writes
/// `qlocale` on first load (with the default), so that cookie alone can't say
/// whether anyone chose anything; this marker can.
pub const LOCALE_CHOSEN_COOKIE: &str = "qlocale_chosen";

// quench dispatches `localeChanged` only from the switch's `updateLocale`.
pub(crate) const MARK_LOCALE_CHOSEN: &str = "window.addEventListener('localeChanged',function(){document.cookie='qlocale_chosen=1; max-age=31536000; path=/; SameSite=Lax';});";

// quench's session watcher renews an expired session with a POST to /refresh. With
// several tabs open they all notice at about the same time, and a refresh token
// is single-use, so all but the first or second lose and land on the login page.
// Tabs on one origin take turns (Web Locks); whoever waited re-checks first,
// because the tab ahead of it has usually renewed the shared cookies already.
// Wraps quench's global `tryRefresh` (and reads its global `SESSION_STATUS_URL`);
// does nothing where either is missing.
pub(crate) const SERIALIZE_REFRESH: &str = "(function(){if(!navigator.locks||typeof tryRefresh!=='function'||typeof SESSION_STATUS_URL==='undefined')return;const original=tryRefresh;tryRefresh=function(){return navigator.locks.request('forge-session-refresh',async function(){try{const r=await fetch(SESSION_STATUS_URL,{credentials:'same-origin',headers:{'Accept':'application/json'},cache:'no-store'});if(r.ok){const s=await r.json();if(s&&s.authenticated===true)return true;}}catch(e){}return original();});};})();";

/// Runs in `<head>`, before first paint: mirrors quench's `qtheme` cookie onto
/// `html[data-fui-theme]` and the `fui_palette` cookie onto `html[data-fui-palette]` (which the tokens
/// key off), and follows later changes. `fuiSetTheme` / `fuiSetPalette` are the writers the theme menu uses.
pub(crate) const THEME_SYNC: &str = r#"(function(){
var root=document.documentElement;
function cookie(n){var m=document.cookie.match(new RegExp('(?:^|; )'+n+'=([^;]*)'));return m?decodeURIComponent(m[1]):null;}
function mode(){var t=cookie('qtheme');return t&&t.indexOf('light')!==-1?'light':'dark';}
function apply(){var m=mode();if(root.getAttribute('data-fui-theme')!==m)root.setAttribute('data-fui-theme',m);}
window.fuiSetTheme=function(m){document.cookie='qtheme=default-'+m+'; max-age=31536000; path=/; SameSite=Lax';apply();};
var PALETTES=['green','teal','blue','purple','rose','orange'];
function palette(){var p=cookie('fui_palette');return PALETTES.indexOf(p)!==-1?p:'green';}
function applyPalette(){var p=palette();if(root.getAttribute('data-fui-palette')!==p)root.setAttribute('data-fui-palette',p);}
window.fuiSetPalette=function(p){if(PALETTES.indexOf(p)===-1)return;document.cookie='fui_palette='+p+'; max-age=31536000; path=/; SameSite=Lax';applyPalette();};
window.fuiToggleTheme=function(){window.fuiSetTheme(mode()==='light'?'dark':'light');};
applyPalette();apply();window.addEventListener('themeChanged',apply);setInterval(apply,1000);
})();"#;

/// Drawer, active nav item, stacked-table labels and dropdown dismissal.
pub(crate) const SHELL_JS: &str = r#"(function(){
var root=document.documentElement;
window.fuiOpenDrawer=function(){root.classList.add('fui-drawer-open');};
window.fuiCloseDrawer=function(){root.classList.remove('fui-drawer-open');};
window.fuiOpenModal=function(id){var m=document.getElementById(id);if(m)m.classList.add('fui-open');};
window.fuiCloseModal=function(id){var m=document.getElementById(id);if(m)m.classList.remove('fui-open');};
document.addEventListener('keydown',function(e){
  if(e.key!=='Escape')return;
  window.fuiCloseDrawer();
  document.querySelectorAll('.fui-modal.fui-open').forEach(function(m){m.classList.remove('fui-open');});
  document.querySelectorAll('details.fui-dropdown[open]').forEach(function(d){d.removeAttribute('open');});
});
document.addEventListener('click',function(e){
  var keep=e.target.closest&&e.target.closest('details.fui-dropdown');
  document.querySelectorAll('details.fui-dropdown[open]').forEach(function(d){if(d!==keep)d.removeAttribute('open');});
  if(e.target.closest&&e.target.closest('.fui-nav-item,.fui-nav-subitem'))window.fuiCloseDrawer();
});
window.addEventListener('resize',function(){if(window.innerWidth>=1024)window.fuiCloseDrawer();});
function markActive(){
  var path=location.pathname.replace(/\/+$/,'');
  var best=null,bestLen=-1;
  document.querySelectorAll('.fui-nav-item[href]').forEach(function(a){
    var h=a.getAttribute('href').split('?')[0].replace(/\/+$/,'');
    if(h&&(path===h||path.indexOf(h+'/')===0)&&h.length>bestLen){best=a;bestLen=h.length;}
  });
  document.querySelectorAll('.fui-nav-item[aria-current]').forEach(function(a){a.removeAttribute('aria-current');});
  if(best)best.setAttribute('aria-current','page');
}
function labelTables(){
  document.querySelectorAll('table.fui-table-stack').forEach(function(t){
    var heads=Array.prototype.map.call(t.querySelectorAll('thead th'),function(th){return th.textContent.trim();});
    t.querySelectorAll('tbody tr').forEach(function(tr){
      Array.prototype.forEach.call(tr.children,function(td,i){if(heads[i])td.setAttribute('data-label',heads[i]);});
    });
  });
}
function init(){markActive();labelTables();setTimeout(labelTables,700);}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',init);else init();
window.addEventListener('localeChanged',function(){setTimeout(labelTables,100);});
document.addEventListener('htmx:afterSwap',function(){labelTables();});
})();"#;
