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
      tabStatus: 'وضعیت دانلود', tabSpeed: 'محدودکننده‌ی سرعت', tabDone: 'بعد از اتمام',
      tabInfo: 'جزئیات', tabConn: 'اتصال‌ها',
      kUrl: 'آدرس', kStatus: 'وضعیت', kSize: 'حجم فایل', kDownloaded: 'دریافت‌شده',
      kRate: 'سرعت فعلی', kAvg: 'میانگین سرعت', kEta: 'زمان باقی‌مانده', kElapsed: 'زمان سپری‌شده',
      kResume: 'قابلیت ادامه', kConns: 'اتصال‌ها', kPath: 'مسیر ذخیره', kCategory: 'دسته',
      kPriority: 'اولویت', prio_1: 'بالا', prio_0: 'معمولی', prio__1: 'پایین',
      kQueue: 'صف', kStart: 'زمان شروع', kLast: 'آخرین فعالیت', kRetries: 'تعداد تلاش مجدد',
      kSegDone: 'بخش‌های کامل', kSegActive: 'بخش‌های فعال', kSegFailed: 'بخش‌های ناموفق',
      kFileName: 'نام فایل', kFileType: 'نوع فایل', kContentType: 'Content-Type', kHttp: 'کد HTTP',
      kServer: 'سرور', kAcceptRanges: 'Accept-Ranges', kEtag: 'ETag', kModified: 'Last-Modified',
      kCreated: 'زمان افزودن', kCompleted: 'زمان اتمام', kLimit: 'محدودیت سرعت', kIntegrity: 'صحت فایل',
      mapTitle: 'نقشه‌ی دانلود (نمایش واقعی بخش‌های فایل)',
      yes: 'دارد', no: 'ندارد', none: '—', unlimited: 'بدون محدودیت',
      active: 'فعال', of: 'از',
      s_receiving: 'در حال دریافت داده…', s_connecting: 'در حال اتصال…', s_retrying: 'تلاش مجدد…',
      s_verifying: 'در حال بررسی صحت…', s_finalizing: 'در حال نهایی‌سازی…', s_preparing: 'در حال آماده‌سازی…',
      s_paused: 'متوقف', s_completed: 'تمام شد', s_failed: 'ناموفق', s_queued: 'در صف',
      s_stopping: 'در حال توقف…',
      retryLine: 'تلاش {a} از {b} — انتظار {s} ثانیه',
      limEnable: 'محدودیت سرعت فعال باشد', limApply: 'اعمال برای',
      limThis: 'همین دانلود', limAll: 'همه‌ی دانلودها',
      limQueueHint: 'محدودیت برای هر صف در مرحله‌ی چندصفی اضافه می‌شود.',
      apply: 'اعمال', limCurThis: 'محدودیت همین دانلود', limCurAll: 'محدودیت کلی برنامه',
      limSaved: 'محدودیت اعمال شد.',
      doneTitle: 'بعد از تمام شدن همین دانلود', doneNone: 'کاری نشود',
      doneOpenFile: 'فایل باز شود', doneOpenFolder: 'پوشه باز شود',
      doneShutdown: 'کامپیوتر خاموش شود (با ۶۰ ثانیه مهلت برای انصراف)', doneHibernate: 'هایبرنیت',
      doneHint: 'این کارها فقط وقتی انجام می‌شوند که برنامه‌ی RDM باز باشد و دانلود واقعاً کامل شود.',
      doneSaved: 'ذخیره شد.',
      integrity_not_verified: 'تأیید نشده',
      integrity_verifying: 'در حال بررسی…', integrity_verified: 'تأیید شد ✓', integrity_mismatch: 'هش با مقدار مورد انتظار نمی‌خواند ✗',
      integrity_computed: 'هش محاسبه شد (مرجعی برای مقایسه نیست)', integrity_failed: 'بررسی انجام نشد',
      integrityNone: 'اطلاعات صحت از سمت سرور در دسترس نیست. می‌توانید هش را خودتان وارد کنید.',
      integrityServer: 'سرور هش کل فایل را اعلام کرده است.',
      kExpected: 'هش مورد انتظار', kActual: 'هش محاسبه‌شده', kSource: 'منبع هش', kScan: 'اسکن آنتی‌ویروس',
      src_server: 'سرور (Repr-Digest)', src_user: 'واردشده توسط شما', src_none: '—',
      scan_none: 'انجام نشده', scan_scanning: 'در حال اسکن…', scan_clean: 'سالم ✓', scan_threat: 'تهدید پیدا شد ✗', scan_failed: 'اسکن ناموفق',
      verifyBtn: 'بررسی صحت', scanBtn: 'اسکن دوباره',
      chkHint: 'هش SHA-256 یا SHA-512 را وارد کنید (فقط ارقام هگز، یا sha256:… ). خالی بگذارید تا هش خود فایل محاسبه شود.',
      verifyStarted: 'بررسی شروع شد.', needDone: 'این بررسی فقط برای دانلود کامل‌شده است.',
      cDown: 'دریافت‌شده', cRange: 'بازه', cProgress: 'پیشرفت', cSpeed: 'سرعت', cStatus: 'وضعیت',
      cRetry: 'تلاش مجدد', cHttp: 'HTTP',
      c_waiting: 'منتظر', c_connecting: 'در حال اتصال', c_receiving: 'دریافت', c_retrying: 'تلاش مجدد',
      c_completed: 'کامل', c_failed: 'ناموفق',
      cdReq: 'بازه‌ی درخواستی', cdPos: 'موقعیت فعلی', cdBytes: 'دریافت‌شده در این بازه',
      cdTotal: 'کل دریافتی این اتصال', cdSpeed: 'سرعت فعلی', cdAvg: 'میانگین سرعت', cdSegs: 'بخش‌های تمام‌شده',
      cdStart: 'شروع', cdLast: 'آخرین فعالیت', cdErr: 'خطا',
      noConns: 'این دانلود در حال اجرا نیست؛ اطلاعات اتصال‌ها فقط هنگام دانلود نمایش داده می‌شود.',
      mapTip: 'بخش {i} از {n} — بایت {a} تا {b} — {p}٪', mapNoOwner: 'هیچ اتصالی الان روی این بخش نیست.',
      mapOwner: 'اتصال {c}',
      pause: 'توقف', resume: 'ادامه', retry: 'تلاش دوباره', openFile: 'باز کردن فایل',
      openFolder: 'نمایش در پوشه', close: 'بستن', copied: 'کپی شد',
      noData: 'این دانلود دیگر وجود ندارد.',
    },
    en: {
      tabStatus: 'Download status', tabSpeed: 'Speed limiter', tabDone: 'On completion',
      tabInfo: 'Details', tabConn: 'Connections',
      kUrl: 'URL', kStatus: 'Status', kSize: 'File size', kDownloaded: 'Downloaded',
      kRate: 'Transfer rate', kAvg: 'Average rate', kEta: 'Time left', kElapsed: 'Elapsed time',
      kResume: 'Resume capability', kConns: 'Connections', kPath: 'Save path', kCategory: 'Category',
      kPriority: 'Priority', prio_1: 'High', prio_0: 'Normal', prio__1: 'Low',
      kQueue: 'Queue', kStart: 'Start time', kLast: 'Last activity', kRetries: 'Retries',
      kSegDone: 'Completed segments', kSegActive: 'Active segments', kSegFailed: 'Failed segments',
      kFileName: 'File name', kFileType: 'File type', kContentType: 'Content-Type', kHttp: 'HTTP status',
      kServer: 'Server', kAcceptRanges: 'Accept-Ranges', kEtag: 'ETag', kModified: 'Last-Modified',
      kCreated: 'Added', kCompleted: 'Completed', kLimit: 'Speed limit', kIntegrity: 'Integrity',
      mapTitle: 'Download map (the real ranges of the file)',
      yes: 'Yes', no: 'No', none: '—', unlimited: 'Unlimited',
      active: 'active', of: 'of',
      s_receiving: 'Receiving data…', s_connecting: 'Connecting…', s_retrying: 'Retrying…',
      s_verifying: 'Verifying…', s_finalizing: 'Finalizing…', s_preparing: 'Preparing…',
      s_paused: 'Paused', s_completed: 'Completed', s_failed: 'Failed', s_queued: 'Queued',
      s_stopping: 'Stopping…',
      retryLine: 'Attempt {a} of {b} — waiting {s} sec',
      limEnable: 'Enable speed limit', limApply: 'Apply to',
      limThis: 'This download', limAll: 'All downloads',
      limQueueHint: 'Per-queue limits arrive with the multiple-queues stage.',
      apply: 'Apply', limCurThis: 'Limit of this download', limCurAll: 'Global limit',
      limSaved: 'Limit applied.',
      doneTitle: 'When this download completes', doneNone: 'Do nothing',
      doneOpenFile: 'Open the file', doneOpenFolder: 'Open the folder',
      doneShutdown: 'Shut down the computer (60 seconds to cancel)', doneHibernate: 'Hibernate',
      doneHint: 'These actions only run while RDM is open and the download really completes.',
      doneSaved: 'Saved.',
      integrity_not_verified: 'Not verified',
      integrity_verifying: 'Verifying…', integrity_verified: 'Verified ✓', integrity_mismatch: 'Hash does not match the expected value ✗',
      integrity_computed: 'Hash calculated (nothing to compare with)', integrity_failed: 'Check could not run',
      integrityNone: 'Server integrity information unavailable. You can enter a checksum yourself.',
      integrityServer: 'The server announced a checksum for the whole file.',
      kExpected: 'Expected hash', kActual: 'Calculated hash', kSource: 'Hash source', kScan: 'Antivirus scan',
      src_server: 'Server (Repr-Digest)', src_user: 'Entered by you', src_none: '—',
      scan_none: 'Not scanned', scan_scanning: 'Scanning…', scan_clean: 'Clean ✓', scan_threat: 'Threat found ✗', scan_failed: 'Scan failed',
      verifyBtn: 'Verify integrity', scanBtn: 'Scan again',
      chkHint: 'Enter a SHA-256 or SHA-512 hash (hex digits, or sha256:…). Leave empty to just calculate the file hash.',
      verifyStarted: 'Check started.', needDone: 'Only finished downloads can be checked.',
      cDown: 'Downloaded', cRange: 'Range', cProgress: 'Progress', cSpeed: 'Speed', cStatus: 'Status',
      cRetry: 'Retries', cHttp: 'HTTP',
      c_waiting: 'Waiting', c_connecting: 'Connecting', c_receiving: 'Receiving', c_retrying: 'Retrying',
      c_completed: 'Completed', c_failed: 'Failed',
      cdReq: 'Requested range', cdPos: 'Current position', cdBytes: 'Received in this range',
      cdTotal: 'Total received by this connection', cdSpeed: 'Current speed', cdAvg: 'Average speed',
      cdSegs: 'Finished segments', cdStart: 'Started', cdLast: 'Last activity', cdErr: 'Error',
      noConns: 'This download is not running; connection data is only available while it downloads.',
      mapTip: 'Slice {i} of {n} — bytes {a} to {b} — {p}%', mapNoOwner: 'No connection is on this part right now.',
      mapOwner: 'Connection {c}',
      pause: 'Pause', resume: 'Resume', retry: 'Retry', openFile: 'Open file',
      openFolder: 'Show in folder', close: 'Close', copied: 'Copied',
      noData: 'This download no longer exists.',
    },
  };

  let lang = localStorage.getItem('rdm-lang') || 'fa';
  if (!I18N[lang]) lang = 'fa';
  const t = (k, vars) => {
    let s = I18N[lang][k] ?? k;
    if (vars) for (const [a, b] of Object.entries(vars)) s = s.replace('{' + a + '}', b);
    return s;
  };

  /* ------------------------------------------------------------------ */
  /* helpers                                                             */
  /* ------------------------------------------------------------------ */
  const $ = (id) => document.getElementById(id);
  const label = window.__TAURI__.window.getCurrentWindow().label;
  const id = Number(String(label).replace('details-', ''));
  const locale = () => (lang === 'fa' ? 'fa-IR' : 'en-US');

  // The engine counts in 1024s, so the units are KiB/MiB/GiB.
  function fmtBytes(n) {
    if (n == null) return '—';
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return v.toFixed(i === 0 ? 0 : v >= 100 ? 0 : v >= 10 ? 1 : 2) + ' ' + units[i];
  }
  const fmtSpeed = (s) => (s > 0 ? fmtBytes(s) + '/s' : '—');
  const pad2 = (n) => String(n).padStart(2, '0');
  function fmtClock(sec) {
    const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = Math.floor(sec % 60);
    return `${pad2(h)}:${pad2(m)}:${pad2(s)}`;
  }
  function fmtEta(sec) {
    if (sec == null) return '—';
    if (sec < 60) return `${sec} ${lang === 'fa' ? 'ثانیه' : 'sec'}`;
    if (sec < 3600) return `${Math.floor(sec / 60)} ${lang === 'fa' ? 'دقیقه' : 'min'} ${sec % 60} ${lang === 'fa' ? 'ثانیه' : 'sec'}`;
    return fmtClock(sec);
  }
  function fmtDate(secs) {
    if (!secs) return '—';
    try {
      return new Date(secs * 1000).toLocaleString(locale(), { dateStyle: 'short', timeStyle: 'medium' });
    } catch (_) { return '—'; }
  }
  const setText = (el, v) => { if (el && el.textContent !== v) el.textContent = v; };
  const orDash = (v) => (v === null || v === undefined || v === '' ? '—' : String(v));

  let toastTimer = null;
  function toast(msg) {
    const el = $('toast');
    el.textContent = msg;
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.hidden = true; }, 3500);
  }
  async function call(cmd, args) {
    try { return await invoke(cmd, args); } catch (e) { toast(String(e)); return undefined; }
  }

  /* ------------------------------------------------------------------ */
  /* state shown in the window                                           */
  /* ------------------------------------------------------------------ */
  let d = null;               // the latest details payload
  let expanded = new Set();   // connection ids with an open detail row
  let highlightConn = null;   // connection highlighted from a map click
  let tab = 'status';
  const rowEls = new Map();

  function statusKind(x) {
    if (x.stopping) return 'stopping';
    switch (x.status) {
      case 'completed': return 'completed';
      case 'error': return 'failed';
      case 'paused': return 'paused';
      case 'queued': return 'queued';
      default: {
        const p = (x.engine && x.engine.phase) || x.phase || '';
        if (p === 'connecting') return 'connecting';
        if (p === 'retrying') return 'retrying';
        if (p === 'verifying') return 'verifying';
        if (p === 'finalizing') return 'finalizing';
        if (p === 'preparing') return 'preparing';
        return 'receiving';
      }
    }
  }
  const pillClass = (k) =>
    ({ completed: 'st-completed', failed: 'st-error', paused: 'st-paused', queued: 'st-queued', stopping: 'st-paused' }[k] || 'st-downloading');

  function percent(x) {
    if (x.status === 'completed') return 100;
    if (x.total) return Math.min(100, (x.downloaded / x.total) * 100);
    return null;
  }

  /* ------------------------------------------------------------------ */
  /* download map                                                        */
  /* ------------------------------------------------------------------ */
  function renderMap(el, cells, done) {
    if (!cells || !cells.length) {
      if (el.children.length) el.innerHTML = '';
      return;
    }
    if (el.children.length !== cells.length) {
      el.innerHTML = '';
      for (let k = 0; k < cells.length; k++) {
        const c = document.createElement('i');
        c.dataset.k = String(k);
        el.appendChild(c);
      }
      el.style.gridTemplateColumns = `repeat(${cells.length}, 1fr)`;
    }
    el.classList.toggle('done', done);
    for (let k = 0; k < cells.length; k++) {
      const v = done ? 100 : cells[k];
      const c = el.children[k];
      if (c._v !== v) {
        c._v = v;
        c.style.setProperty('--p', v + '%');
        c.classList.toggle('full', v >= 100);
      }
    }
  }

  function cellInfo(k, n) {
    const total = d.total || 0;
    const a = Math.floor((k * total) / n);
    const b = Math.max(a, Math.floor(((k + 1) * total) / n) - 1);
    return { a, b };
  }

  function connAt(offset) {
    const conns = (d.engine && d.engine.connections) || [];
    return conns.find((c) => c.status !== 'waiting' && offset >= c.start && offset <= c.end);
  }

  const tip = $('tip');
  function bindMapEvents(el) {
    el.addEventListener('mousemove', (e) => {
      const c = e.target.closest('i');
      if (!c || !d || !d.engine || !d.engine.map.length) { tip.hidden = true; return; }
      const n = d.engine.map.length;
      const k = Number(c.dataset.k);
      const { a, b } = cellInfo(k, n);
      let text = t('mapTip', { i: k + 1, n, a: fmtBytes(a), b: fmtBytes(b), p: d.engine.map[k] });
      const owner = connAt(a);
      text += '\n' + (owner ? t('mapOwner', { c: owner.id }) : t('mapNoOwner'));
      tip.textContent = text;
      tip.hidden = false;
      const w = tip.offsetWidth;
      tip.style.left = Math.max(4, Math.min(e.clientX + 12, window.innerWidth - w - 4)) + 'px';
      tip.style.top = Math.min(e.clientY + 16, window.innerHeight - tip.offsetHeight - 4) + 'px';
    });
    el.addEventListener('mouseleave', () => { tip.hidden = true; });
    el.addEventListener('click', (e) => {
      const c = e.target.closest('i');
      if (!c || !d || !d.engine || !d.engine.map.length) return;
      const k = Number(c.dataset.k);
      const { a } = cellInfo(k, d.engine.map.length);
      const owner = connAt(a);
      setText($('map-info'), owner ? t('mapOwner', { c: owner.id }) + ' — ' + fmtBytes(owner.start) + ' → ' + fmtBytes(owner.end) : t('mapNoOwner'));
      if (owner) {
        highlightConn = owner.id;
        expanded.add(owner.id);
        selectTab('conn');
        renderConnections();
        const row = rowEls.get(owner.id);
        if (row) row.main.scrollIntoView({ block: 'nearest' });
      }
    });
  }
  bindMapEvents($('map1'));
  bindMapEvents($('map2'));

  /* ------------------------------------------------------------------ */
  /* connections table (keyed rows, only changed text is touched)        */
  /* ------------------------------------------------------------------ */
  function connRows(c) {
    const size = c.end >= c.start ? c.end - c.start + 1 : 0;
    const pct = size ? Math.min(100, (c.downloaded / size) * 100) : 0;
    return { size, pct };
  }

  function createConnRow(cid) {
    const main = document.createElement('tr');
    main.className = 'crow';
    main.tabIndex = 0;
    main.dataset.id = String(cid);
    main.setAttribute('aria-expanded', 'false');
    main.innerHTML = `
      <td class="num n"></td><td class="num dl"></td><td class="num rg ltr"></td>
      <td class="c-p"><div class="progwrap"><div class="bar mini"><i class="fill"></i></div><span class="num pct"></span></div></td>
      <td class="num sp"></td><td class="stc"></td><td class="num rt"></td><td class="num ht"></td>`;
    const extra = document.createElement('tr');
    extra.className = 'cextra';
    extra.hidden = true;
    extra.innerHTML = '<td colspan="8"><div class="kv dense"></div></td>';
    return {
      main, extra,
      n: main.querySelector('.n'), dl: main.querySelector('.dl'), rg: main.querySelector('.rg'),
      bar: main.querySelector('.bar'), fill: main.querySelector('.fill'), pct: main.querySelector('.pct'),
      sp: main.querySelector('.sp'), st: main.querySelector('.stc'), rt: main.querySelector('.rt'),
      ht: main.querySelector('.ht'), box: extra.querySelector('.kv'), boxSig: '',
    };
  }

  function detailHtml(c) {
    const kv = (k, v) => `<div><span>${k}</span><b class="${/^[\d\s.:/→\-A-Za-z%]+$/.test(v) ? 'num' : ''}">${v}</b></div>`;
    const esc = (s) => String(s).replace(/[&<>"']/g, (m) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[m]));
    const lines = [
      kv(t('cdReq'), `${fmtBytes(c.start)} → ${fmtBytes(c.end)}`),
      kv(t('cdPos'), fmtBytes(c.pos)),
      kv(t('cdBytes'), fmtBytes(c.downloaded)),
      kv(t('cdTotal'), fmtBytes(c.totalBytes)),
      kv(t('cdSpeed'), fmtSpeed(c.speed)),
      kv(t('cdAvg'), fmtSpeed(c.avgSpeed)),
      kv(t('cRetry'), String(c.retries)),
      kv(t('cHttp'), c.httpStatus ? String(c.httpStatus) : '—'),
      kv(t('cdSegs'), String(c.segmentsDone)),
      kv(t('cdStart'), fmtDate(c.startedAt)),
      kv(t('cdLast'), fmtDate(c.lastActivity)),
    ];
    if (c.retryInMs > 0) lines.push(kv(t('kRetries'), t('retryLine', { a: c.retries, b: d.engine.retryLimit, s: (c.retryInMs / 1000).toFixed(1) })));
    if (c.error) lines.push(`<div class="wide"><span>${t('cdErr')}</span><b>${esc(c.error)}</b></div>`);
    return lines.join('');
  }

  function renderConnections() {
    const conns = (d.engine && d.engine.connections) || [];
    const visible = conns.filter((c) => c.status !== 'waiting' || c.totalBytes > 0 || c.segmentsDone > 0);
    $('no-conns').hidden = visible.length > 0;
    const tbody = $('ctbody');
    const seen = new Set();
    let prev = null;
    for (const c of visible) {
      let r = rowEls.get(c.id);
      if (!r) { r = createConnRow(c.id); rowEls.set(c.id, r); }
      const { pct } = connRows(c);
      setText(r.n, String(c.id));
      setText(r.dl, fmtBytes(c.downloaded));
      setText(r.rg, `${fmtBytes(c.start)} – ${fmtBytes(c.end)}`);
      r.fill.style.width = pct + '%';
      r.bar.className = 'bar mini' + (c.status === 'receiving' ? '' : ' idle') + (c.status === 'completed' ? ' done' : '') + (c.status === 'failed' ? ' err' : '') + (c.status === 'retrying' ? ' paused' : '');
      setText(r.pct, Math.floor(pct) + '%');
      setText(r.sp, c.status === 'receiving' ? fmtSpeed(c.speed) : '—');
      setText(r.st, t('c_' + c.status));
      r.st.className = 'stc cs-' + c.status;
      setText(r.rt, String(c.retries));
      setText(r.ht, c.httpStatus ? String(c.httpStatus) : '—');

      const open = expanded.has(c.id);
      r.extra.hidden = !open;
      r.main.setAttribute('aria-expanded', String(open));
      r.main.classList.toggle('open', open);
      r.main.classList.toggle('hl', highlightConn === c.id);
      if (open) {
        const html = detailHtml(c);
        if (r.boxSig !== html) { r.box.innerHTML = html; r.boxSig = html; }
      }
      const ref = prev ? prev.extra.nextSibling : tbody.firstChild;
      if (r.main !== ref) tbody.insertBefore(r.main, ref);
      if (r.extra.previousSibling !== r.main) tbody.insertBefore(r.extra, r.main.nextSibling);
      prev = r;
      seen.add(c.id);
    }
    for (const [cid, r] of rowEls) {
      if (!seen.has(cid)) { r.main.remove(); r.extra.remove(); rowEls.delete(cid); }
    }
  }

  function toggleConn(cid) {
    if (expanded.has(cid)) expanded.delete(cid); else expanded.add(cid);
    highlightConn = null;
    if (d) renderConnections();
  }
  $('ctbody').addEventListener('click', (e) => {
    const tr = e.target.closest('tr.crow');
    if (tr) toggleConn(Number(tr.dataset.id));
  });
  $('ctbody').addEventListener('keydown', (e) => {
    const tr = e.target.closest('tr.crow');
    if (tr && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); toggleConn(Number(tr.dataset.id)); }
  });

  /* ------------------------------------------------------------------ */
  /* main render                                                         */
  /* ------------------------------------------------------------------ */
  let limUiInit = false;
  let doneUiInit = false;

  function render() {
    const x = d;
    const kind = statusKind(x);
    const pct = percent(x);
    const eng = x.engine;
    const running = x.status === 'downloading';

    // header + big bar
    setText($('h-pct'), pct === null ? '…' : Math.floor(pct) + '%');
    setText($('h-name'), x.filename);
    document.title = `${pct === null ? '' : Math.floor(pct) + '% '}${x.filename}`;
    const statusText = t('s_' + kind);
    setText($('h-status'), statusText);
    $('h-status').className = 'pill ' + pillClass(kind);

    const bar = $('bigbar');
    bar.className = 'bigbar ' + ({ completed: 'done', failed: 'err', paused: 'paused', stopping: 'paused', verifying: 'verify', finalizing: 'verify', retrying: 'warn' }[kind] || (running ? 'live' : 'idle'));
    $('bigfill').style.width = (pct === null ? 0 : pct) + '%';
    bar.setAttribute('aria-valuenow', pct === null ? '0' : String(Math.floor(pct)));
    bar.setAttribute('aria-valuetext', `${pct === null ? '' : Math.floor(pct) + '%'} ${statusText}`);
    setText($('b-done'), fmtBytes(x.downloaded));
    setText($('b-total'), fmtBytes(x.total));
    setText($('b-speed'), running ? fmtSpeed(x.speed) : '');
    setText($('b-eta'), running && x.eta != null ? fmtEta(x.eta) : '');

    // status tab
    setText($('v-url'), x.url);
    setText($('v-status'), statusText);
    setText($('v-size'), fmtBytes(x.total));
    setText($('v-done'), `${fmtBytes(x.downloaded)}${pct === null ? '' : ' (' + pct.toFixed(2) + '%)'}`);
    setText($('v-rate'), running ? fmtSpeed(x.speed) : '—');
    setText($('v-avg'), running ? fmtSpeed(x.avgSpeed) : '—');
    setText($('v-eta'), running && x.eta != null ? fmtEta(x.eta) : '—');
    setText($('v-elapsed'), fmtClock(x.elapsedSecs || 0));
    setText($('v-resume'), x.resumable ? t('yes') : t('no'));
    const act = eng ? eng.activeConnections : 0;
    const max = eng ? eng.maxConnections : 0;
    const target = eng ? eng.targetConnections : 0;
    setText($('v-conns'), running && eng ? `${act} / ${target} ${t('active')} (max ${max})` : '—');
    setText($('v-path'), x.savePath);
    setText($('v-cat'), x.category || '—');
    setText($('v-queue'), x.queue || '—');
    setText($('v-priority'), t('prio_' + String(x.priority || 0).replace('-', '_')));
    setText($('v-start'), fmtDate(x.startedAt));
    setText($('v-last'), eng && eng.lastActivity ? fmtDate(eng.lastActivity) : '—');
    setText($('v-retries'), String((x.retries || 0) + (eng ? eng.retries : 0)));
    setText($('v-segdone'), eng ? `${eng.segmentsCompleted} / ${eng.segmentsTotal}` : '—');
    setText($('v-segactive'), eng ? String(eng.segmentsActive) : '—');
    setText($('v-segfailed'), eng ? String(eng.segmentsFailed) : '—');

    // retry banner (real data from the connections that are waiting to retry)
    const rb = $('retry-box');
    const retrying = eng ? eng.connections.filter((c) => c.status === 'retrying') : [];
    if (retrying.length && running) {
      const c = retrying[0];
      rb.textContent = `${t('s_retrying')}  ${t('retryLine', { a: c.retries, b: eng.retryLimit, s: (c.retryInMs / 1000).toFixed(1) })}` + (c.error ? `\n${c.error}` : '');
      rb.hidden = false;
    } else rb.hidden = true;
    const eb = $('error-box');
    if (x.status === 'error' && x.error) { eb.textContent = x.error; eb.hidden = false; } else eb.hidden = true;

    // map (engine map while running, plain fill when finished)
    let cells = eng && eng.map && eng.map.length ? eng.map : null;
    if (!cells && x.segments && x.segments.length) cells = x.segments;
    if (!cells && x.status === 'completed') cells = new Array(64).fill(100);
    renderMap($('map1'), cells, x.status === 'completed');
    renderMap($('map2'), cells, x.status === 'completed');

    // details tab
    setText($('i-name'), x.filename);
    const dot = x.filename.lastIndexOf('.');
    setText($('i-ext'), dot > 0 ? x.filename.slice(dot + 1).toUpperCase() : '—');
    setText($('i-ctype'), orDash(x.contentType));
    setText($('i-http'), x.httpStatus ? String(x.httpStatus) : '—');
    setText($('i-server'), orDash(x.server));
    setText($('i-ranges'), orDash(x.acceptRanges));
    setText($('i-etag'), orDash(x.etag));
    setText($('i-modified'), orDash(x.lastModified));
    setText($('i-created'), fmtDate(x.added));
    setText($('i-completed'), fmtDate(x.completedAt));
    setText($('i-limit'), x.speedLimitKbps > 0 ? `${x.speedLimitKbps} KiB/s` : t('unlimited'));
    setText($('i-integrity'), t('integrity_' + (x.integrity || 'not_verified')));
    setText($('integrity-note'), x.integritySource === 'server' ? t('integrityServer') : (x.expectedHash ? '' : t('integrityNone')));
    setText($('i-expected'), orDash(x.expectedHash));
    setText($('i-actual'), orDash(x.actualHash));
    setText($('i-source'), t('src_' + (x.integritySource || 'none')));
    setText($('i-scan'), (t('scan_' + (x.scan || 'none'))) + (x.scanNote ? ' — ' + x.scanNote : ''));
    const done = x.status === 'completed';
    $('chk-verify').disabled = !done || x.integrity === 'verifying';
    $('chk-scan').disabled = !done || x.scan === 'scanning';
    const rn = $('resume-note');
    if (x.resumeNote) { rn.textContent = x.resumeNote; rn.hidden = false; } else rn.hidden = true;

    // connections tab
    if (eng) renderConnections();
    else { $('no-conns').hidden = false; }

    // speed / completion tabs: fill once, then only the read-only "current" values
    setText($('lim-cur-this'), x.speedLimitKbps > 0 ? `${x.speedLimitKbps} KiB/s` : t('unlimited'));
    if (!limUiInit) {
      limUiInit = true;
      $('lim-on').checked = x.speedLimitKbps > 0;
      if (x.speedLimitKbps > 0) $('lim-kbps').value = x.speedLimitKbps;
    }
    if (!doneUiInit) {
      doneUiInit = true;
      const cur = x.onComplete || 'none';
      const radio = document.querySelector(`input[name="done"][value="${cur}"]`);
      if (radio) radio.checked = true;
    }

    // footer buttons
    const tog = $('f-toggle');
    if (x.status === 'downloading' || x.status === 'queued') {
      tog.textContent = t('pause'); tog.dataset.act = 'pause'; tog.hidden = false; tog.disabled = !!x.stopping;
    } else if (x.status === 'paused') {
      tog.textContent = t('resume'); tog.dataset.act = 'resume'; tog.hidden = false; tog.disabled = false;
    } else if (x.status === 'error') {
      tog.textContent = t('retry'); tog.dataset.act = 'resume'; tog.hidden = false; tog.disabled = false;
    } else tog.hidden = true;
    $('f-file').hidden = x.status !== 'completed';
  }

  /* ------------------------------------------------------------------ */
  /* tabs                                                                */
  /* ------------------------------------------------------------------ */
  const TABS = ['status', 'speed', 'done', 'info', 'conn'];
  function selectTab(name, focus) {
    tab = name;
    for (const n of TABS) {
      const on = n === name;
      const btn = $('tab-' + n);
      btn.setAttribute('aria-selected', String(on));
      btn.tabIndex = on ? 0 : -1;
      $('p-' + n).hidden = !on;
      if (on && focus) btn.focus();
    }
    tip.hidden = true;
  }
  $('tabs').addEventListener('click', (e) => {
    const b = e.target.closest('[role=tab]');
    if (b) selectTab(b.id.replace('tab-', ''));
  });
  $('tabs').addEventListener('keydown', (e) => {
    const i = TABS.indexOf(tab);
    const rtl = document.documentElement.dir === 'rtl';
    let n = null;
    if (e.key === 'ArrowRight') n = i + (rtl ? -1 : 1);
    else if (e.key === 'ArrowLeft') n = i + (rtl ? 1 : -1);
    else if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = TABS.length - 1;
    if (n !== null) {
      e.preventDefault();
      selectTab(TABS[(n + TABS.length) % TABS.length], true);
    }
  });

  /* ------------------------------------------------------------------ */
  /* actions                                                             */
  /* ------------------------------------------------------------------ */
  $('f-toggle').addEventListener('click', async (e) => {
    const act = e.currentTarget.dataset.act;
    if (act === 'pause') await call('pause_download', { id });
    else if (act === 'resume') await call('resume_download', { id });
  });
  $('f-file').addEventListener('click', () => call('open_file', { id }));
  $('f-folder').addEventListener('click', () => call('open_folder', { id }));
  $('f-close').addEventListener('click', () => call('close_details', { id }));
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') call('close_details', { id }); });

  for (const el of [$('v-url'), $('v-path')]) {
    const copy = async () => {
      try { await navigator.clipboard.writeText(el.textContent); toast(t('copied')); } catch (_) { /* clipboard unavailable */ }
    };
    el.addEventListener('dblclick', copy);
    el.addEventListener('keydown', (e) => { if (e.key === 'Enter') copy(); });
  }

  // speed limiter: this download or the whole application (real engine limiters)
  async function refreshGlobalLimit() {
    const s = await call('get_settings');
    if (s) setText($('lim-cur-all'), s.speedLimitKbps > 0 ? `${s.speedLimitKbps} KiB/s` : t('unlimited'));
    return s;
  }
  $('lim-apply').addEventListener('click', async () => {
    const on = $('lim-on').checked;
    const v = Math.max(1, parseInt($('lim-kbps').value, 10) || 0);
    const kbps = on ? v : 0;
    const scope = document.querySelector('input[name="lim-scope"]:checked').value;
    if (scope === 'this') {
      await call('set_item_speed_limit', { id, kbps });
    } else {
      const s = await call('get_settings');
      if (!s) return;
      s.speedLimitKbps = kbps;
      try { await invoke('save_settings', { settings: s }); } catch (e) { toast(String(e)); return; }
      await refreshGlobalLimit();
    }
    toast(t('limSaved'));
  });

  // completion action: saved the moment it is chosen
  document.querySelectorAll('input[name="done"]').forEach((r) => {
    r.addEventListener('change', async () => {
      await call('set_on_complete', { id, action: r.value });
      toast(t('doneSaved'));
    });
  });

  /* ------------------------------------------------------------------ */
  /* language + start                                                    */
  /* ------------------------------------------------------------------ */
  function applyLang() {
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === 'fa' ? 'rtl' : 'ltr';
    document.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
  }

  let gone = false;
  function onDetails(p) {
    if (gone) return;
    if (!p) { gone = true; document.body.innerHTML = `<p class="gone">${t('noData')}</p>`; return; }
    d = p;
    render();
  }

  async function init() {
    applyLang();
    await listen('details', (ev) => onDetails(ev.payload));
    refreshGlobalLimit();
    const first = await call('get_details', { id });
    onDetails(first);
  }
  init();

  /* integrity actions */
  $('chk-verify').addEventListener('click', async () => {
    const r = await call('verify_download', { id, expected: $('chk-input').value.trim() || null });
    if (r !== undefined) toast(t('verifyStarted'));
  });
  $('chk-scan').addEventListener('click', async () => {
    const r = await call('rescan_download', { id });
    if (r !== undefined) toast(t('verifyStarted'));
  });
})();
