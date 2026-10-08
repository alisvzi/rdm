'use strict';

(async () => {
  const $ = (id) => document.getElementById(id);
  const tabId = Number(new URLSearchParams(location.search).get('tab'));

  const CATS = {
    video: ['mp4', 'm4v', 'webm', 'mkv', 'mov', 'avi', 'wmv', 'flv', 'ts', 'm3u8'],
    audio: ['mp3', 'm4a', 'aac', 'ogg', 'opus', 'flac', 'wav', 'wma'],
    archive: ['zip', 'rar', '7z', 'tar', 'gz', 'tgz', 'bz2', 'xz', 'zst'],
    doc: ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'epub', 'csv', 'rtf', 'odt'],
    program: ['exe', 'msi', 'apk', 'dmg', 'iso', 'deb', 'appimage', 'msix'],
    image: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'avif'],
  };
  const LABEL = { all: 'همه', video: 'ویدیو', audio: 'صوت', archive: 'فشرده', doc: 'سند', program: 'برنامه', image: 'تصویر', other: 'سایر' };

  // Runs inside the page: collects every link and media address.
  function collectInPage() {
    const out = [];
    const add = (u, text) => { if (u) out.push({ url: u, text: (text || '').trim().slice(0, 120) }); };
    document.querySelectorAll('a[href]').forEach((a) => add(a.href, a.textContent || a.title));
    document.querySelectorAll('video[src], audio[src], source[src]').forEach((e) => add(e.src, ''));
    return { page: location.href, items: out };
  }

  async function collect() {
    if (api.scripting && api.scripting.executeScript) {
      const r = await api.scripting.executeScript({ target: { tabId }, func: collectInPage });
      return r[0].result;
    }
    const r = await api.tabs.executeScript(tabId, { code: `(${collectInPage.toString()})()` });
    return r[0];
  }

  const extOf = (url) => {
    try {
      const p = new URL(url).pathname;
      const m = /\.([A-Za-z0-9]{1,8})$/.exec(p);
      return m ? m[1].toLowerCase() : '';
    } catch (_) { return ''; }
  };
  const catOf = (ext) => {
    for (const [k, list] of Object.entries(CATS)) if (list.includes(ext)) return k;
    return ext ? 'other' : '';
  };
  const nameOf = (url) => {
    try {
      const last = new URL(url).pathname.split('/').filter(Boolean).pop() || url;
      return decodeURIComponent(last);
    } catch (_) { return url; }
  };

  let page = '';
  let items = [];
  let cat = 'all';
  const checked = new Set();

  try {
    const res = await collect();
    page = res.page;
    const seen = new Set();
    for (const it of res.items) {
      if (!/^https?:/i.test(it.url) || seen.has(it.url)) continue;
      seen.add(it.url);
      const ext = extOf(it.url);
      items.push({ url: it.url, text: it.text, ext, cat: catOf(ext) });
    }
  } catch (e) {
    $('result').textContent = 'خواندن لینک‌های صفحه ممکن نشد (صفحه‌های داخلی مرورگر قابل‌خواندن نیستند).';
  }
  $('page').textContent = page;

  // category buttons
  const catBox = $('cats');
  const catKeys = ['all', ...Object.keys(CATS), 'other'];
  for (const k of catKeys) {
    const b = document.createElement('button');
    b.type = 'button';
    b.dataset.cat = k;
    b.setAttribute('aria-pressed', String(k === cat));
    catBox.appendChild(b);
  }

  function visible() {
    const q = $('filter').value.trim().toLowerCase();
    let re = null;
    const rs = $('regex').value.trim();
    if (rs) { try { re = new RegExp(rs, 'i'); } catch (_) { re = null; } }
    const all = $('show-all').checked;
    return items.filter((it) => {
      if (!all && !it.cat) return false;
      if (cat !== 'all' && it.cat !== cat) return false;
      if (q && !(it.url.toLowerCase().includes(q) || it.text.toLowerCase().includes(q))) return false;
      if (re && !re.test(it.url)) return false;
      return true;
    });
  }

  const MAX_SHOWN = 2000;
  function render() {
    const list = visible();
    for (const b of catBox.children) {
      const k = b.dataset.cat;
      const n = k === 'all' ? items.length : items.filter((i) => i.cat === k).length;
      b.textContent = `${LABEL[k]} (${n})`;
      b.setAttribute('aria-pressed', String(k === cat));
    }
    const ul = $('list');
    ul.innerHTML = '';
    const frag = document.createDocumentFragment();
    for (const it of list.slice(0, MAX_SHOWN)) {
      const li = document.createElement('li');
      const label = document.createElement('label');
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = checked.has(it.url);
      cb.dataset.url = it.url;
      const box = document.createElement('div');
      box.className = 'info';
      const name = document.createElement('div');
      name.className = 'name ltr';
      name.textContent = nameOf(it.url);
      name.title = it.url;
      const meta = document.createElement('div');
      meta.className = 'meta';
      meta.textContent = it.text;
      box.append(name, meta);
      const tag = document.createElement('span');
      tag.className = 'tag';
      tag.textContent = it.ext ? it.ext.toUpperCase() : '';
      label.append(cb, box);
      li.append(label, tag);
      frag.appendChild(li);
    }
    ul.appendChild(frag);
    $('empty').hidden = list.length > 0;
    const shown = list.length > MAX_SHOWN ? `${MAX_SHOWN} از ${list.length}` : String(list.length);
    $('count').textContent = `${shown} لینک — ${checked.size} انتخاب‌شده`;
    $('send').disabled = checked.size === 0;
  }

  catBox.addEventListener('click', (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    cat = b.dataset.cat;
    render();
  });
  $('list').addEventListener('change', (e) => {
    const cb = e.target;
    if (cb.dataset && cb.dataset.url) {
      if (cb.checked) checked.add(cb.dataset.url); else checked.delete(cb.dataset.url);
      $('count').textContent = `${visible().length} لینک — ${checked.size} انتخاب‌شده`;
      $('send').disabled = checked.size === 0;
    }
  });
  for (const id of ['filter', 'regex', 'show-all']) $(id).addEventListener('input', render);
  $('sel-all').addEventListener('click', () => { visible().slice(0, MAX_SHOWN).forEach((i) => checked.add(i.url)); render(); });
  $('sel-none').addEventListener('click', () => { checked.clear(); render(); });

  $('send').addEventListener('click', async () => {
    const urls = [...checked];
    if (!urls.length) return;
    if (urls.length > 500) { $('result').textContent = 'حداکثر ۵۰۰ لینک در هر بار.'; return; }
    $('send').disabled = true;
    $('result').textContent = 'در حال ارسال…';
    try {
      const r = await sendBatchToRdm({ urls, referrer: page });
      $('result').textContent = `${r.added} لینک اضافه شد` + (r.failed ? `، ${r.failed} ناموفق` : '') + '.';
      if (r.errors && r.errors.length) $('result').title = r.errors.join('\n');
      if (!r.failed) checked.clear();
    } catch (e) {
      $('result').textContent = e.message === 'RDM_NOT_RUNNING' ? 'برنامه‌ی RDM باز نیست.' : e.message;
    }
    render();
  });

  render();
})();
