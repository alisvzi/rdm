(() => {
  'use strict';

  const { invoke } = window.__TAURI__.core;
  const $ = (id) => document.getElementById(id);

  const T = {
    fa: {
      title: 'دانلود فایل جدید', sub: 'اجازه‌ی شروع دانلود را بدهید',
      url: 'آدرس', name: 'نام فایل', folder: 'ذخیره در پوشه',
      browse: 'انتخاب…', reset: 'پیش‌فرض',
      size: 'حجم', resume: 'قابلیت ادامه', cat: 'دسته', yes: 'دارد', no: 'ندارد', unknown: 'نامشخص',
      dont: 'دیگر این پنجره را نشان نده (بعداً از تنظیمات قابل تغییر است)',
      start: 'شروع دانلود', later: 'بعداً', cancel: 'انصراف',
      invalid: 'این درخواست دیگر معتبر نیست.',
      cats: { Compressed: 'فشرده', Documents: 'اسناد', Music: 'موسیقی', Video: 'ویدیو', Programs: 'برنامه‌ها', General: 'عمومی' },
    },
    en: {
      title: 'New download', sub: 'Allow this download to start',
      url: 'Address', name: 'File name', folder: 'Save to folder',
      browse: 'Browse…', reset: 'Default',
      size: 'Size', resume: 'Resume support', cat: 'Category', yes: 'Yes', no: 'No', unknown: 'Unknown',
      dont: "Don't show this window again (can be changed in Settings)",
      start: 'Start download', later: 'Download later', cancel: 'Cancel',
      invalid: 'This request is no longer valid.',
      cats: { Compressed: 'Compressed', Documents: 'Documents', Music: 'Music', Video: 'Video', Programs: 'Programs', General: 'General' },
    },
  };

  let lang = localStorage.getItem('rdm-lang') || 'fa';
  if (!T[lang]) lang = 'fa';
  const t = T[lang];
  document.documentElement.lang = lang;
  document.documentElement.dir = lang === 'fa' ? 'rtl' : 'ltr';

  $('t-title').textContent = t.title;
  $('t-sub').textContent = t.sub;
  $('t-url').textContent = t.url;
  $('t-name').textContent = t.name;
  $('t-folder').textContent = t.folder;
  $('browse').textContent = t.browse;
  $('reset').textContent = t.reset;
  $('t-size').textContent = t.size;
  $('t-resume').textContent = t.resume;
  $('t-cat').textContent = t.cat;
  $('t-dont').textContent = t.dont;
  $('start').textContent = t.start;
  $('later').textContent = t.later;
  $('cancel').textContent = t.cancel;

  function fmtBytes(n) {
    if (n == null) return t.unknown;
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return v.toFixed(i === 0 ? 0 : v >= 100 ? 0 : v >= 10 ? 1 : 2) + ' ' + units[i];
  }

  // The window is called "confirm-<id>"; the id tells us which download it is about.
  const label = window.__TAURI__.window.getCurrentWindow().label;
  const id = Number(String(label).replace('confirm-', ''));

  let info = null;
  let busy = false;

  function showError(msg) {
    const box = $('err');
    box.textContent = msg;
    box.hidden = false;
  }

  async function finish(start) {
    if (busy || !info) return;
    busy = true;
    $('err').hidden = true;
    for (const b of ['start', 'later', 'cancel']) $(b).disabled = true;
    try {
      await invoke('confirm_pending', {
        id,
        folder: $('folder').value.trim(),
        filename: $('name').value.trim(),
        start,
        dontAsk: $('dont').checked,
      });
      // the backend closes this window
    } catch (e) {
      showError(String(e));
      for (const b of ['start', 'later', 'cancel']) $(b).disabled = false;
      busy = false;
    }
  }

  async function cancel() {
    if (busy) return;
    busy = true;
    try { await invoke('cancel_pending', { id }); } catch (_) { /* window closes anyway */ }
  }

  $('start').addEventListener('click', () => finish(true));
  $('later').addEventListener('click', () => finish(false));
  $('cancel').addEventListener('click', cancel);
  $('reset').addEventListener('click', () => { if (info) $('folder').value = info.categoryFolder; });
  $('browse').addEventListener('click', async () => {
    const dlg = window.__TAURI__.dialog;
    if (!dlg) return;
    try {
      const res = await dlg.open({ directory: true, multiple: false, defaultPath: $('folder').value || undefined });
      if (typeof res === 'string' && res) $('folder').value = res;
    } catch (e) {
      showError(String(e));
    }
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') cancel();
    if (e.key === 'Enter' && e.target.tagName !== 'BUTTON') finish(true);
  });

  (async () => {
    try {
      info = await invoke('get_pending', { id });
    } catch (e) {
      showError(t.invalid);
      $('start').disabled = true;
      $('later').disabled = true;
      return;
    }
    $('url').textContent = info.url;
    $('url').title = info.url;
    $('name').value = info.filename;
    $('folder').value = info.folder;
    $('size').textContent = fmtBytes(info.total);
    $('resume').textContent = info.resumable ? t.yes : t.no;
    $('cat').textContent = t.cats[info.category] || info.category;
    $('name').focus();
    $('name').select();
  })();
})();
