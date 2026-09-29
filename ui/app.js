(() => {
  'use strict';

  if (!window.__TAURI__) {
    document.body.textContent = 'Please open this page through the RDM desktop app.';
    return;
  }
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  /* ------------------------------------------------------------------ */
  /* i18n                                                                */
  /* ------------------------------------------------------------------ */
  const I18N = {
    fa: {
      add: 'افزودن لینک', resume: 'ادامه', pause: 'توقف', delete: 'حذف',
      resumeAll: 'ادامه‌ی همه', pauseAll: 'توقف همه', clearDone: 'پاک‌کردن تمام‌شده‌ها',
      settings: 'تنظیمات',
      fAll: 'همه‌ی دانلودها', fActive: 'در حال دانلود', fCompleted: 'تمام‌شده', fUnfinished: 'ناتمام',
      catTitle: 'دسته‌بندی',
      cat_Compressed: 'فشرده', cat_Documents: 'اسناد', cat_Music: 'موسیقی',
      cat_Video: 'ویدیو', cat_Programs: 'برنامه‌ها', cat_General: 'عمومی',
      colName: 'نام فایل', colSize: 'حجم', colProgress: 'پیشرفت', colSpeed: 'سرعت',
      colEta: 'زمان باقی‌مانده', colStatus: 'وضعیت', colAdded: 'زمان افزودن',
      st_queued: 'در صف', st_downloading: 'در حال دانلود', st_paused: 'متوقف',
      st_completed: 'تمام شد', st_error: 'خطا', st_pausing: 'در حال توقف…',
      addTitle: 'افزودن دانلود جدید', urlsLabel: 'لینک‌ها (هر خط یک لینک)',
      destLabel: 'پوشه‌ی مقصد (خالی = پوشه‌ی پیش‌فرض)', browse: 'انتخاب…',
      startNow: 'همین حالا شروع شود', cancel: 'انصراف', addBtn: 'افزودن',
      adding: 'در حال بررسی لینک‌ها…',
      settingsTitle: 'تنظیمات', setDir: 'پوشه‌ی پیش‌فرض دانلود',
      setConn: 'اتصال برای هر دانلود (۱ تا ۶۴)',
      setConc: 'دانلود هم‌زمان (بقیه در صف)',
      setLimit: 'محدودیت سرعت کل (KB/s، صفر = بدون محدودیت)',
      setRetries: 'تعداد تلاش مجدد',
      setCategorize: 'فایل‌ها در زیرپوشه‌ی دسته‌بندی (ویدیو، موسیقی، …) ذخیره شوند',
      setLang: 'زبان', save: 'ذخیره',
      delTitle: 'حذف از لیست', delQuestion: 'مورد انتخاب‌شده از لیست حذف شود؟',
      delQuestionN: 'مورد از لیست حذف شود؟',
      delFile: 'فایل دانلودشده هم از دیسک پاک شود',
      openFile: 'باز کردن فایل', openFolder: 'نمایش در پوشه',
      sbSpeed: 'سرعت کل', sbActive: 'فعال', sbTotal: 'همه',
      empty: 'هنوز دانلودی ندارید. روی «افزودن لینک» بزنید.',
      setTray: 'با بستن پنجره، برنامه در سینی سیستم بماند (دانلودها ادامه پیدا می‌کنند)',
      schedTitle: 'زمان‌بندی', schedEnable: 'زمان‌بندی فعال باشد',
      schedStart: 'شروع دانلودها در ساعت', schedStop: 'توقف دانلودها در ساعت',
      schedAfter: 'بعد از پایان دانلودهای زمان‌بندی‌شده',
      afterNone: 'کاری نشود', afterShutdown: 'خاموش شدن کامپیوتر', afterHibernate: 'هایبرنیت',
      intTitle: 'اتصال به مرورگر',
      intHint: 'این کد را در تنظیمات افزونه‌ی RDM در مرورگر وارد کنید (فقط یک‌بار).',
      intToken: 'کد اتصال', intPort: 'پورت',
      intRunning: 'سرور فعال است', intStopped: 'سرور اجرا نشد (پورت اشغال است)',
      copy: 'کپی', copied: 'کپی شد', addedToast: 'از مرورگر اضافه شد: ',
      powerBanner: 'کامپیوتر تا یک دقیقه‌ی دیگر خاموش می‌شود.',
    },
    en: {
      add: 'Add URL', resume: 'Resume', pause: 'Pause', delete: 'Delete',
      resumeAll: 'Resume all', pauseAll: 'Pause all', clearDone: 'Clear finished',
      settings: 'Settings',
      fAll: 'All downloads', fActive: 'Downloading', fCompleted: 'Completed', fUnfinished: 'Unfinished',
      catTitle: 'Categories',
      cat_Compressed: 'Compressed', cat_Documents: 'Documents', cat_Music: 'Music',
      cat_Video: 'Video', cat_Programs: 'Programs', cat_General: 'General',
      colName: 'File name', colSize: 'Size', colProgress: 'Progress', colSpeed: 'Speed',
      colEta: 'Time left', colStatus: 'Status', colAdded: 'Added',
      st_queued: 'Queued', st_downloading: 'Downloading', st_paused: 'Paused',
      st_completed: 'Completed', st_error: 'Error', st_pausing: 'Pausing…',
      addTitle: 'Add new download', urlsLabel: 'Links (one per line)',
      destLabel: 'Destination folder (empty = default folder)', browse: 'Browse…',
      startNow: 'Start immediately', cancel: 'Cancel', addBtn: 'Add',
      adding: 'Checking links…',
      settingsTitle: 'Settings', setDir: 'Default download folder',
      setConn: 'Connections per download (1-64)',
      setConc: 'Simultaneous downloads (rest wait in queue)',
      setLimit: 'Total speed limit (KB/s, 0 = unlimited)',
      setRetries: 'Retries per chunk',
      setCategorize: 'Sort files into sub-folders (Video, Music, ...)',
      setLang: 'Language', save: 'Save',
      delTitle: 'Remove from list', delQuestion: 'Remove the selected item from the list?',
      delQuestionN: 'items will be removed from the list.',
      delFile: 'Also delete the downloaded file from disk',
      openFile: 'Open file', openFolder: 'Show in folder',
      sbSpeed: 'Total speed', sbActive: 'Active', sbTotal: 'Total',
      empty: 'No downloads yet. Click "Add URL" to start.',
      setTray: 'Closing the window keeps RDM running in the system tray (downloads continue)',
      schedTitle: 'Scheduler', schedEnable: 'Enable scheduler',
      schedStart: 'Start downloads at', schedStop: 'Stop downloads at',
      schedAfter: 'When scheduled downloads finish',
      afterNone: 'Do nothing', afterShutdown: 'Shut down the computer', afterHibernate: 'Hibernate',
      intTitle: 'Browser integration',
      intHint: 'Paste this code into the RDM browser extension settings (one time only).',
      intToken: 'Pairing code', intPort: 'Port',
      intRunning: 'Server is running', intStopped: 'Server could not start (port in use)',
      copy: 'Copy', copied: 'Copied', addedToast: 'Added from browser: ',
      powerBanner: 'The computer will shut down in one minute.',
    },
  };

  let lang = localStorage.getItem('rdm-lang') || 'fa';
  if (!I18N[lang]) lang = 'fa';
  const t = (k) => I18N[lang][k] ?? k;

  /* ------------------------------------------------------------------ */
  /* helpers                                                             */
  /* ------------------------------------------------------------------ */
  const $ = (id) => document.getElementById(id);
  const esc = (s) =>
    String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  const ICONS = {
    plus: '<path d="M12 5v14M5 12h14"/>',
    play: '<polygon points="6 4 20 12 6 20 6 4"/>',
    pause: '<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>',
    trash: '<path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/>',
    sliders: '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    file: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/>',
    check: '<path d="M20 6L9 17l-5-5"/>',
  };
  const svg = (name) => `<svg class="i" viewBox="0 0 24 24">${ICONS[name] || ''}</svg>`;

  function fmtBytes(n) {
    if (n == null) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    const digits = i === 0 ? 0 : v >= 100 ? 0 : v >= 10 ? 1 : 2;
    return v.toFixed(digits) + ' ' + units[i];
  }
  const fmtSpeed = (s) => (s > 0 ? fmtBytes(s) + '/s' : '');
  const pad2 = (n) => String(n).padStart(2, '0');
  function fmtEta(sec) {
    if (sec == null) return '';
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    const s = Math.floor(sec % 60);
    return h > 0 ? `${h}:${pad2(m)}:${pad2(s)}` : `${m}:${pad2(s)}`;
  }
  function fmtDate(secs) {
    try {
      return new Date(secs * 1000).toLocaleString('en-GB', { dateStyle: 'short', timeStyle: 'short' });
    } catch (_) {
      return '';
    }
  }
  const setText = (el, v) => { if (el.textContent !== v) el.textContent = v; };

  let toastTimer = null;
  function toast(msg) {
    const el = $('toast');
    el.textContent = msg;
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.hidden = true; }, 4500);
  }

  /* ------------------------------------------------------------------ */
  /* state                                                               */
  /* ------------------------------------------------------------------ */
  let items = [];
  let selected = new Set();
  let filter = 'all';
  const rowEls = new Map();

  const FILTERS = [
    ['all', 'fAll'],
    ['active', 'fActive'],
    ['completed', 'fCompleted'],
    ['unfinished', 'fUnfinished'],
  ];
  const CATS = ['Compressed', 'Documents', 'Music', 'Video', 'Programs', 'General'];

  function matches(key, it) {
    if (key === 'all') return true;
    if (key === 'active') return it.status === 'downloading';
    if (key === 'completed') return it.status === 'completed';
    if (key === 'unfinished') return it.status !== 'completed';
    if (key.startsWith('cat:')) return it.category === key.slice(4);
    return true;
  }

  /* ------------------------------------------------------------------ */
  /* sidebar                                                             */
  /* ------------------------------------------------------------------ */
  const sideBtns = [];
  let catTitleEl = null;

  function buildSide() {
    const side = $('side');
    side.innerHTML = '';
    const make = (key, labelKey) => {
      const b = document.createElement('button');
      b.innerHTML = '<span class="lbl"></span><span class="cnt num"></span>';
      b.addEventListener('click', () => { filter = key; render(); });
      side.appendChild(b);
      sideBtns.push({ el: b, key, labelKey });
    };
    FILTERS.forEach(([key, label]) => make(key, label));
    catTitleEl = document.createElement('div');
    catTitleEl.className = 'title';
    side.appendChild(catTitleEl);
    CATS.forEach((c) => make('cat:' + c, 'cat_' + c));
  }

  function updateSide() {
    catTitleEl.textContent = t('catTitle');
    for (const b of sideBtns) {
      setText(b.el.querySelector('.lbl'), t(b.labelKey));
      setText(b.el.querySelector('.cnt'), String(items.filter((it) => matches(b.key, it)).length));
      b.el.classList.toggle('on', b.key === filter);
    }
  }

  /* ------------------------------------------------------------------ */
  /* list rendering (keyed, so clicks are never lost while updating)     */
  /* ------------------------------------------------------------------ */
  function createRow(id) {
    const tr = document.createElement('tr');
    tr.dataset.id = String(id);
    tr.innerHTML = `
      <td class="c-name"><span class="tag"></span><span class="name"></span></td>
      <td class="c-size"><span class="num size"></span></td>
      <td class="c-prog"><span class="bar"><span class="fill"></span></span><span class="num pct"></span></td>
      <td class="c-speed"><span class="num speed"></span></td>
      <td class="c-eta"><span class="num eta"></span></td>
      <td class="c-status"><span class="stt"></span><span class="sub"></span></td>
      <td class="c-added"><span class="num added"></span></td>
      <td class="c-act"></td>`;
    const q = (s) => tr.querySelector(s);
    tr._r = {
      tag: q('.tag'), name: q('.name'), size: q('.size'), bar: q('.bar'), fill: q('.fill'),
      pct: q('.pct'), speed: q('.speed'), eta: q('.eta'), stt: q('.stt'), sub: q('.sub'),
      added: q('.added'), act: q('.c-act'),
    };
    tr._sig = null;
    return tr;
  }

  function actionsHtml(it) {
    const b = (act, icon, key) =>
      `<button class="icon-btn" data-act="${act}" title="${esc(t(key))}">${svg(icon)}</button>`;
    let h = '';
    if ((it.status === 'downloading' && !it.stopping) || it.status === 'queued') h += b('pause', 'pause', 'pause');
    else if (it.status === 'paused' || it.status === 'error') h += b('resume', 'play', 'resume');
    if (it.status === 'completed') h += b('open', 'file', 'openFile');
    h += b('folder', 'folder', 'openFolder');
    h += b('delete', 'trash', 'delete');
    return h;
  }

  function updateRow(tr, it) {
    const r = tr._r;
    setText(r.tag, it.category ? t('cat_' + it.category) : '');
    setText(r.name, it.filename);
    r.name.title = `${it.savePath}\n${it.url}`;
    setText(r.size, fmtBytes(it.total));

    let pct = null;
    if (it.status === 'completed') pct = 100;
    else if (it.total) pct = Math.min(100, (it.downloaded / it.total) * 100);
    const indet = pct === null && it.status === 'downloading';
    r.bar.className = 'bar' +
      (indet ? ' indet' : '') +
      (it.status === 'completed' ? ' done' : '') +
      (it.status === 'error' ? ' err' : '') +
      (it.status === 'paused' ? ' paused' : '');
    r.fill.style.width = (pct === null ? 0 : pct) + '%';
    setText(r.pct, pct === null ? '' : Math.floor(pct) + '%');

    setText(r.speed, it.status === 'downloading' ? fmtSpeed(it.speed) : '');
    setText(r.eta, it.status === 'downloading' ? fmtEta(it.eta) : '');

    const label = it.stopping ? t('st_pausing') : t('st_' + it.status);
    setText(r.stt, label);
    r.stt.className = 'stt st-' + it.status;
    const shortErr = it.error ? (it.error.length > 70 ? it.error.slice(0, 70) + '…' : it.error) : '';
    setText(r.sub, shortErr);
    r.sub.title = it.error || '';
    setText(r.added, fmtDate(it.added));

    const sig = `${it.status}|${it.stopping}|${lang}`;
    if (tr._sig !== sig) { r.act.innerHTML = actionsHtml(it); tr._sig = sig; }
    tr.classList.toggle('sel', selected.has(it.id));
  }

  function render() {
    // forget selections of rows that no longer exist
    const ids = new Set(items.map((i) => i.id));
    for (const id of [...selected]) if (!ids.has(id)) selected.delete(id);

    const list = items.filter((it) => matches(filter, it)).reverse(); // newest first
    const tbody = $('rows');
    const seen = new Set();
    let prev = null;
    for (const it of list) {
      let tr = rowEls.get(it.id);
      if (!tr) { tr = createRow(it.id); rowEls.set(it.id, tr); }
      updateRow(tr, it);
      const ref = prev ? prev.nextSibling : tbody.firstChild;
      if (tr !== ref) tbody.insertBefore(tr, ref);
      prev = tr;
      seen.add(it.id);
    }
    for (const [id, tr] of rowEls) {
      if (!seen.has(id)) { tr.remove(); rowEls.delete(id); }
    }

    $('empty').hidden = list.length > 0;
    updateSide();
    updateToolbar();

    const active = items.filter((i) => i.status === 'downloading');
    setText($('sb-speed'), fmtBytes(active.reduce((s, i) => s + (i.speed || 0), 0)) + '/s');
    setText($('sb-active'), String(active.length));
    setText($('sb-total'), String(items.length));
  }

  function updateToolbar() {
    const sel = items.filter((i) => selected.has(i.id));
    const has = (list, ...st) => list.some((i) => st.includes(i.status));
    $('btn-resume').disabled = !has(sel, 'paused', 'error');
    $('btn-pause').disabled = !sel.some((i) => (i.status === 'downloading' && !i.stopping) || i.status === 'queued');
    $('btn-delete').disabled = sel.length === 0;
    $('btn-resume-all').disabled = !has(items, 'paused', 'error');
    $('btn-pause-all').disabled = !has(items, 'downloading', 'queued');
    $('btn-clear').disabled = !has(items, 'completed');
  }

  /* ------------------------------------------------------------------ */
  /* actions                                                             */
  /* ------------------------------------------------------------------ */
  async function call(cmd, args) {
    try {
      return await invoke(cmd, args);
    } catch (e) {
      toast(String(e));
      return undefined;
    }
  }

  async function rowAction(kind, id) {
    if (kind === 'pause') await call('pause_download', { id });
    else if (kind === 'resume') await call('resume_download', { id });
    else if (kind === 'open') await call('open_file', { id });
    else if (kind === 'folder') await call('open_folder', { id });
    else if (kind === 'delete') askDelete([id]);
  }

  function selectedIds() { return [...selected]; }

  $('rows').addEventListener('click', (e) => {
    const tr = e.target.closest('tr');
    if (!tr) return;
    const id = Number(tr.dataset.id);
    const btn = e.target.closest('button[data-act]');
    if (btn) { rowAction(btn.dataset.act, id); return; }
    if (e.ctrlKey || e.metaKey) {
      if (selected.has(id)) selected.delete(id); else selected.add(id);
    } else {
      selected = new Set([id]);
    }
    render();
  });

  $('rows').addEventListener('dblclick', (e) => {
    const tr = e.target.closest('tr');
    if (!tr || e.target.closest('button')) return;
    const id = Number(tr.dataset.id);
    const it = items.find((i) => i.id === id);
    if (it && it.status === 'completed') rowAction('open', id);
  });

  $('btn-resume').addEventListener('click', async () => {
    for (const id of selectedIds()) await call('resume_download', { id });
  });
  $('btn-pause').addEventListener('click', async () => {
    for (const id of selectedIds()) await call('pause_download', { id });
  });
  $('btn-delete').addEventListener('click', () => askDelete(selectedIds()));
  $('btn-resume-all').addEventListener('click', () => call('resume_all'));
  $('btn-pause-all').addEventListener('click', () => call('pause_all'));
  $('btn-clear').addEventListener('click', () => call('clear_completed'));

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Delete' && !document.querySelector('dialog[open]') && selected.size) {
      askDelete(selectedIds());
    }
  });

  /* ------------------------------------------------------------------ */
  /* dialogs                                                             */
  /* ------------------------------------------------------------------ */
  async function pickFolder(input) {
    const dlg = window.__TAURI__.dialog;
    if (!dlg) return;
    try {
      const res = await dlg.open({ directory: true, multiple: false, defaultPath: input.value || undefined });
      if (typeof res === 'string' && res) input.value = res;
    } catch (e) {
      toast(String(e));
    }
  }

  // ----- add -----
  const dlgAdd = $('dlg-add');
  $('btn-add').addEventListener('click', () => {
    $('add-error').hidden = true;
    $('add-ok').disabled = false;
    dlgAdd.showModal();
    $('add-urls').focus();
  });
  $('add-cancel').addEventListener('click', () => dlgAdd.close());
  $('add-browse').addEventListener('click', () => pickFolder($('add-dir')));
  $('add-ok').addEventListener('click', async () => {
    const urls = $('add-urls').value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    if (!urls.length) return;
    const okBtn = $('add-ok');
    const label = okBtn.textContent;
    okBtn.disabled = true;
    $('add-error').hidden = true;

    const failed = [];
    const errors = [];
    for (let i = 0; i < urls.length; i++) {
      okBtn.textContent = `${t('adding')} ${i + 1}/${urls.length}`;
      try {
        await invoke('add_download', {
          url: urls[i],
          dir: $('add-dir').value.trim() || null,
          startNow: $('add-start').checked,
        });
      } catch (e) {
        failed.push(urls[i]);
        errors.push(`${urls[i]}\n  -> ${e}`);
      }
    }
    okBtn.textContent = label;
    okBtn.disabled = false;

    if (failed.length) {
      $('add-urls').value = failed.join('\n');
      const box = $('add-error');
      box.textContent = errors.join('\n');
      box.hidden = false;
    } else {
      $('add-urls').value = '';
      dlgAdd.close();
    }
  });

  // ----- settings -----
  const dlgSet = $('dlg-settings');
  $('btn-settings').addEventListener('click', async () => {
    const s = await call('get_settings');
    if (!s) return;
    $('set-dir').value = s.downloadDir;
    $('set-conn').value = s.connections;
    $('set-conc').value = s.maxConcurrent;
    $('set-limit').value = s.speedLimitKbps;
    $('set-retries').value = s.retries;
    $('set-cat').checked = s.categorize;
    $('set-tray').checked = s.closeToTray;
    $('set-sched').checked = s.scheduleEnabled;
    $('set-sstart').value = s.scheduleStart || '';
    $('set-sstop').value = s.scheduleStop || '';
    $('set-after').value = s.afterFinish || 'none';
    $('set-lang').value = lang;
    const info = await call('get_integration');
    if (info) {
      $('int-token').value = info.token;
      setText($('int-port'), String(info.port));
      setText($('int-status'), info.running ? t('intRunning') : t('intStopped'));
    }
    $('set-error').hidden = true;
    dlgSet.showModal();
  });
  $('set-cancel').addEventListener('click', () => dlgSet.close());
  $('set-browse').addEventListener('click', () => pickFolder($('set-dir')));
  const num = (id, min, max, dflt) => {
    const v = parseInt($(id).value, 10);
    return Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : dflt;
  };
  $('set-ok').addEventListener('click', async () => {
    const settings = {
      downloadDir: $('set-dir').value.trim(),
      connections: num('set-conn', 1, 64, 16),
      maxConcurrent: num('set-conc', 1, 16, 3),
      speedLimitKbps: num('set-limit', 0, 100000000, 0),
      retries: num('set-retries', 0, 50, 8),
      categorize: $('set-cat').checked,
      closeToTray: $('set-tray').checked,
      scheduleEnabled: $('set-sched').checked,
      scheduleStart: $('set-sstart').value || '',
      scheduleStop: $('set-sstop').value || '',
      afterFinish: $('set-after').value,
    };
    try {
      await invoke('save_settings', { settings });
    } catch (e) {
      const box = $('set-error');
      box.textContent = String(e);
      box.hidden = false;
      return;
    }
    const newLang = $('set-lang').value;
    if (newLang !== lang) { lang = newLang; localStorage.setItem('rdm-lang', lang); applyLang(); }
    dlgSet.close();
  });

  $('int-copy').addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText($('int-token').value);
      toast(t('copied'));
    } catch (_) {
      $('int-token').select();
    }
  });

  $('banner-cancel').addEventListener('click', async () => {
    await call('cancel_shutdown');
    $('banner').hidden = true;
  });

  // ----- delete -----
  const dlgDel = $('dlg-delete');
  let pendingDelete = [];
  function askDelete(ids) {
    if (!ids.length) return;
    pendingDelete = ids;
    $('del-text').textContent = ids.length === 1 ? t('delQuestion') : `${ids.length} ${t('delQuestionN')}`;
    $('del-file').checked = false;
    dlgDel.showModal();
  }
  $('del-cancel').addEventListener('click', () => dlgDel.close());
  $('del-ok').addEventListener('click', async () => {
    const deleteFile = $('del-file').checked;
    dlgDel.close();
    for (const id of pendingDelete) {
      selected.delete(id);
      await call('remove_download', { id, deleteFile });
    }
    pendingDelete = [];
  });

  /* ------------------------------------------------------------------ */
  /* language                                                            */
  /* ------------------------------------------------------------------ */
  function applyLang() {
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === 'fa' ? 'rtl' : 'ltr';
    document.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
    // force action buttons (tooltips) to be rebuilt in the new language
    for (const tr of rowEls.values()) tr._sig = null;
    render();
  }

  /* ------------------------------------------------------------------ */
  /* start                                                               */
  /* ------------------------------------------------------------------ */
  async function init() {
    document.querySelectorAll('[data-icon]').forEach((el) => { el.outerHTML = svg(el.dataset.icon); });
    buildSide();
    applyLang();
    await listen('downloads', (ev) => { items = ev.payload || []; render(); });
    await listen('added', (ev) => toast(t('addedToast') + ev.payload));
    await listen('power', () => {
      setText($('banner-text'), t('powerBanner'));
      $('banner').hidden = false;
    });
    items = (await call('list_downloads')) || [];
    render();
  }
  init();
})();
