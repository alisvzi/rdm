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
      tAll: 'همه‌ی دانلودها', tStatus: 'وضعیت',
      fActive: 'در حال دانلود', fCompleted: 'تمام‌شده', fUnfinished: 'ناتمام',
      cat_Compressed: 'فشرده', cat_Documents: 'اسناد', cat_Music: 'موسیقی',
      cat_Video: 'ویدیو', cat_Programs: 'برنامه‌ها', cat_General: 'عمومی',
      colName: 'نام فایل', colSize: 'حجم', colProgress: 'پیشرفت', colSpeed: 'سرعت',
      colEta: 'زمان باقی‌مانده', colStatus: 'وضعیت', colAdded: 'زمان افزودن',
      verifyStarted: 'بررسی شروع شد.', badHash: 'هش نامطابق', badScan: 'تهدید',
      st_queued: 'در صف', st_downloading: 'در حال دانلود', st_paused: 'متوقف',
      st_completed: 'تمام شد', st_error: 'خطا', st_pausing: 'در حال توقف…',
      dSegments: 'نقشه‌ی دانلود (بخش‌هایی که با اتصال‌های هم‌زمان گرفته می‌شوند)',
      dUrl: 'آدرس', dPath: 'مسیر ذخیره', dDone: 'دریافت‌شده', dResume: 'قابلیت ادامه',
      yes: 'دارد', no: 'ندارد',
      addTitle: 'افزودن دانلود جدید', urlsLabel: 'لینک‌ها (هر خط یک لینک)',
      destLabel: 'پوشه‌ی مقصد (خالی = پوشه‌ی پیش‌فرض)', browse: 'انتخاب…',
      startNow: 'همین حالا شروع شود', cancel: 'انصراف', addBtn: 'افزودن',
      adding: 'در حال بررسی لینک‌ها…',
      settingsTitle: 'تنظیمات', setDir: 'پوشه‌ی پیش‌فرض دانلود',
      lastDirLbl: 'آخرین پوشه‌ی انتخابی (دانلود بعدی همین‌جا می‌رود)', clear: 'پاک‌کردن',
      setConn: 'تعداد اتصال برای هر دانلود',
      setConc: 'دانلود هم‌زمان (بقیه در صف)',
      setLimit: 'محدودیت سرعت کل (KB/s، صفر = بدون محدودیت)',
      setRetries: 'تعداد تلاش مجدد',
      setCategorize: 'فایل‌ها در زیرپوشه‌ی دسته‌بندی (ویدیو، موسیقی، …) ذخیره شوند',
      setConfirm: 'دانلودهای مرورگر همیشه قبل از شروع پنجره‌ی تأیید دارند (قابل خاموش‌کردن نیست)',
      setShowDetails: 'هنگام شروع دانلود، پنجره‌ی جزئیات خودکار باز شود',
      setCloseDetails: 'پنجره‌ی جزئیات بعد از اتمام دانلود خودکار بسته شود',
      setTray: 'با بستن پنجره، برنامه در سینی سیستم بماند (دانلودها ادامه پیدا می‌کنند)',
      scanTitle: 'اسکن آنتی‌ویروس بعد از دانلود (اختیاری)', scanPath: 'مسیر برنامه‌ی اسکنر (خالی = خاموش)',
      scanArgs: 'آرگومان‌ها ({file} جای مسیر فایل)', scanCodes: 'کدهای خروج «تهدید پیدا شد»',
      scanHint: 'مثال Windows Defender: آرگومان‌ها «-Scan -ScanType 3 -File {file} -DisableRemediation» و کد تهدید 2. اگر فایل هش نامعتبر یا تهدید داشته باشد، عملیات بعد از اتمام (باز کردن فایل، خاموشی) اجرا نمی‌شود.',
      verifyHash: 'بررسی صحت فایل (SHA-256)', scanAgain: 'اسکن آنتی‌ویروس',
      queuesBtn: 'صف‌ها', queuesTitle: 'مدیریت صف‌ها', qName: 'نام صف',
      qEnabled: 'صف فعال باشد (صف غیرفعال هیچ دانلودی را شروع نمی‌کند)', qPriority: 'اولویت (عدد بزرگ‌تر زودتر)',
      qMax: 'دانلود هم‌زمان در این صف', qLimit: 'محدودیت سرعت صف (KiB/s، ۰ = بدون محدودیت)',
      qSched: 'زمان‌بندی این صف', qSchedOn: 'زمان‌بندی فعال باشد', qStart: 'ساعت شروع', qStop: 'ساعت توقف',
      qAfter: 'بعد از پایان اجرای زمان‌بندی‌شده', qOnlyOk: 'فقط اگر همه‌ی دانلودها موفق بودند (بدون خطا و بدون مشکل هش یا اسکن)',
      qNew: 'صف جدید', qDelete: 'حذف صف', qStartNow: 'شروع صف', qStopNow: 'توقف صف', qSaved: 'ذخیره شد.',
      qMainNoDelete: 'صف اصلی حذف نمی‌شود.', qNewName: 'صف جدید', tQueues: 'صف‌ها', closeBtn: 'بستن', qDownloads: 'دانلود',
      prPriority: 'اولویت', prioHigh: 'بالا', prioNormal: 'معمولی', prioLow: 'پایین',
      propsTitle: 'مشخصات دانلود', prName: 'نام فایل', prUrl: 'آدرس', prPath: 'مسیر ذخیره', prQueue: 'صف',
      prLimit: 'محدودیت سرعت این دانلود (KiB/s، ۰ = بدون محدودیت)', prDone: 'بعد از اتمام',
      prOpenFile: 'باز کردن فایل', prOpenFolder: 'باز کردن پوشه',
      prRunning: 'برای تغییر صف، ابتدا دانلود را متوقف کنید.', properties: 'مشخصات',
      proxyTitle: 'پروکسی', proxyMode: 'حالت پروکسی', proxySystem: 'پروکسی سیستم', proxyNone: 'بدون پروکسی', proxyManual: 'دستی',
      proxyUrl: 'آدرس پروکسی (http://, https://, socks5://)', proxyUser: 'نام کاربری (اختیاری)',
      proxyPass: 'رمز عبور (خالی = بدون تغییر)', proxyBypass: 'استثناها (با ویرگول جدا کنید)',
      proxyHint: 'تغییر پروکسی روی دانلودهای جدید و ادامه‌ی دانلودها اثر می‌کند. دانلودهای در حال اجرا تا توقف و ادامه همان اتصال قبلی را دارند. رمز پروکسی رمزشده با DPAPI ذخیره می‌شود.',
      forgetHosts: 'فراموش کردن محدودیت اتصال سایت‌ها', hostsCleared: 'پاک شد.',
      listLabel: 'فهرست دانلودها', speedTitle: 'سرعت و پروتکل', httpMode: 'پروتکل HTTP',
      httpOne: 'HTTP/1.1 با چند اتصال (پیش‌فرض، بهترین برای دانلود چندبخشی)', httpAuto: 'خودکار (اجازه‌ی HTTP/2 به سرور)',
      limitRules: 'برنامه‌ی سرعت (هر خط: ساعت-ساعت سرعت KiB/s، ۰ = بدون محدودیت)',
      limitRulesHint: 'مثال: «22:00-06:00 0» یعنی شب‌ها بدون محدودیت و «08:00-17:00 500» یعنی روز حداکثر ۵۰۰ KiB/s. خارج از این بازه‌ها، محدودیت عادی تنظیمات اعمال می‌شود. HTTP/3 پشتیبانی نمی‌شود.',
      aDone: 'دانلود کامل شد', aError: 'خطا در دانلود', aBadHash: 'هش فایل نمی‌خواند', aThreat: 'اسکنر تهدید پیدا کرد',
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
      setLang: 'زبان', save: 'ذخیره',
      delTitle: 'حذف از لیست', delQuestion: 'مورد انتخاب‌شده از لیست حذف شود؟',
      delQuestionN: 'مورد از لیست حذف شود؟',
      delFile: 'فایل دانلودشده هم از دیسک پاک شود',
      openFile: 'باز کردن فایل', openFolder: 'نمایش در پوشه',
      sbSpeed: 'سرعت کل', sbActive: 'فعال', sbTotal: 'همه',
      empty: 'هنوز دانلودی ندارید. روی «افزودن لینک» بزنید.',
      detailsBtn: 'جزئیات', search: 'جستجو در نام، آدرس، دسته و وضعیت…', sortBy: 'مرتب‌سازی',
      sortDir: 'جهت مرتب‌سازی', sort_added: 'تاریخ افزودن', sort_name: 'نام', sort_size: 'حجم',
      sort_downloaded: 'دریافت‌شده', sort_progress: 'پیشرفت', sort_speed: 'سرعت',
      sort_eta: 'زمان باقی‌مانده', sort_status: 'وضعیت', sort_completed: 'تاریخ اتمام',
      retry: 'تلاش دوباره', restart: 'شروع مجدد', copyUrl: 'کپی آدرس', copyPath: 'کپی مسیر فایل',
      removeFromList: 'حذف از لیست', deleteFile: 'حذف فایل دانلودشده', connAuto: 'خودکار (هوشمند)',
    },
    en: {
      add: 'Add URL', resume: 'Resume', pause: 'Pause', delete: 'Delete',
      resumeAll: 'Resume all', pauseAll: 'Pause all', clearDone: 'Clear finished',
      settings: 'Settings',
      tAll: 'All downloads', tStatus: 'Status',
      fActive: 'Downloading', fCompleted: 'Completed', fUnfinished: 'Unfinished',
      cat_Compressed: 'Compressed', cat_Documents: 'Documents', cat_Music: 'Music',
      cat_Video: 'Video', cat_Programs: 'Programs', cat_General: 'General',
      colName: 'File name', colSize: 'Size', colProgress: 'Progress', colSpeed: 'Speed',
      colEta: 'Time left', colStatus: 'Status', colAdded: 'Added',
      verifyStarted: 'Check started.', badHash: 'hash mismatch', badScan: 'threat',
      st_queued: 'Queued', st_downloading: 'Downloading', st_paused: 'Paused',
      st_completed: 'Completed', st_error: 'Error', st_pausing: 'Pausing…',
      dSegments: 'Download map (parts fetched by parallel connections)',
      dUrl: 'URL', dPath: 'Save path', dDone: 'Downloaded', dResume: 'Resume support',
      yes: 'Yes', no: 'No',
      addTitle: 'Add new download', urlsLabel: 'Links (one per line)',
      destLabel: 'Destination folder (empty = default folder)', browse: 'Browse…',
      startNow: 'Start immediately', cancel: 'Cancel', addBtn: 'Add',
      adding: 'Checking links…',
      settingsTitle: 'Settings', setDir: 'Default download folder',
      lastDirLbl: 'Last chosen folder (the next download goes here)', clear: 'Clear',
      setConn: 'Connections per download',
      setConc: 'Simultaneous downloads (rest wait in queue)',
      setLimit: 'Total speed limit (KB/s, 0 = unlimited)',
      setRetries: 'Retries per chunk',
      setCategorize: 'Sort files into sub-folders (Video, Music, ...)',
      setConfirm: 'Browser downloads always ask for confirmation first (cannot be turned off)',
      setShowDetails: 'Open the details window automatically when a download starts',
      setCloseDetails: 'Close the details window automatically when the download finishes',
      setTray: 'Closing the window keeps RDM running in the system tray (downloads continue)',
      scanTitle: 'Antivirus scan after download (optional)', scanPath: 'Scanner program path (empty = off)',
      scanArgs: 'Arguments ({file} = file path)', scanCodes: 'Exit codes meaning "threat found"',
      scanHint: 'Windows Defender example: arguments "-Scan -ScanType 3 -File {file} -DisableRemediation", threat code 2. If the checksum does not match or a threat is found, completion actions (open file, shutdown) are skipped.',
      verifyHash: 'Verify file integrity (SHA-256)', scanAgain: 'Scan with antivirus',
      queuesBtn: 'Queues', queuesTitle: 'Manage queues', qName: 'Queue name',
      qEnabled: 'Queue enabled (a disabled queue starts nothing)', qPriority: 'Priority (higher starts first)',
      qMax: 'Simultaneous downloads in this queue', qLimit: 'Queue speed limit (KiB/s, 0 = unlimited)',
      qSched: 'Schedule of this queue', qSchedOn: 'Schedule enabled', qStart: 'Start time', qStop: 'Stop time',
      qAfter: 'After a scheduled run finishes', qOnlyOk: 'Only if every download succeeded (no errors, no checksum or scan problems)',
      qNew: 'New queue', qDelete: 'Delete queue', qStartNow: 'Start queue', qStopNow: 'Stop queue', qSaved: 'Saved.',
      qMainNoDelete: 'The main queue cannot be deleted.', qNewName: 'New queue', tQueues: 'Queues', closeBtn: 'Close', qDownloads: 'downloads',
      prPriority: 'Priority', prioHigh: 'High', prioNormal: 'Normal', prioLow: 'Low',
      propsTitle: 'Download properties', prName: 'File name', prUrl: 'URL', prPath: 'Save path', prQueue: 'Queue',
      prLimit: 'Speed limit of this download (KiB/s, 0 = unlimited)', prDone: 'When finished',
      prOpenFile: 'Open the file', prOpenFolder: 'Open the folder',
      prRunning: 'Pause the download first to move it to another queue.', properties: 'Properties',
      proxyTitle: 'Proxy', proxyMode: 'Proxy mode', proxySystem: 'System proxy', proxyNone: 'No proxy', proxyManual: 'Manual',
      proxyUrl: 'Proxy address (http://, https://, socks5://)', proxyUser: 'User name (optional)',
      proxyPass: 'Password (empty = unchanged)', proxyBypass: 'Bypass list (comma separated)',
      proxyHint: 'A proxy change applies to new downloads and to resumed ones. Running downloads keep their old connection until paused and resumed. The proxy password is stored encrypted with DPAPI.',
      forgetHosts: 'Forget per-site connection limits', hostsCleared: 'Cleared.',
      listLabel: 'Download list', speedTitle: 'Speed and protocol', httpMode: 'HTTP protocol',
      httpOne: 'HTTP/1.1 with many connections (default, best for segmented downloads)', httpAuto: 'Automatic (let the server use HTTP/2)',
      limitRules: 'Speed schedule (one per line: HH:MM-HH:MM KiB/s, 0 = unlimited)',
      limitRulesHint: 'Example: "22:00-06:00 0" = unlimited at night, "08:00-17:00 500" = at most 500 KiB/s by day. Outside the windows the normal limit from Settings applies. HTTP/3 is not supported.',
      aDone: 'Download completed', aError: 'Download failed', aBadHash: 'File hash does not match', aThreat: 'Scanner found a threat',
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
      setLang: 'Language', save: 'Save',
      delTitle: 'Remove from list', delQuestion: 'Remove the selected item from the list?',
      delQuestionN: 'items will be removed from the list.',
      delFile: 'Also delete the downloaded file from disk',
      openFile: 'Open file', openFolder: 'Show in folder',
      sbSpeed: 'Total speed', sbActive: 'Active', sbTotal: 'Total',
      empty: 'No downloads yet. Click "Add URL" to start.',
      detailsBtn: 'Details', search: 'Search name, URL, category, status…', sortBy: 'Sort by',
      sortDir: 'Sort direction', sort_added: 'Date added', sort_name: 'Name', sort_size: 'Size',
      sort_downloaded: 'Downloaded', sort_progress: 'Progress', sort_speed: 'Speed',
      sort_eta: 'Time left', sort_status: 'Status', sort_completed: 'Date completed',
      retry: 'Retry', restart: 'Restart', copyUrl: 'Copy URL', copyPath: 'Copy file path',
      removeFromList: 'Delete from list', deleteFile: 'Delete downloaded file', connAuto: 'Auto (adaptive)',
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
    logo: '<path d="M12 3v11m0 0l-4-4m4 4l4-4"/><path d="M4 15v3a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-3"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    play: '<polygon points="7 4 19 12 7 20 7 4"/>',
    playall: '<polygon points="4 5 12 12 4 19 4 5"/><polygon points="13 5 21 12 13 19 13 5"/>',
    pause: '<rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/>',
    pauseall: '<rect x="3" y="5" width="3.5" height="14" rx="1"/><rect x="9" y="5" width="3.5" height="14" rx="1"/><rect x="15" y="5" width="3.5" height="14" rx="1"/>',
    trash: '<path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/>',
    sliders: '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    file: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/>',
    check: '<path d="M20 6L9 17l-5-5"/>',
    all: '<path d="M4 6h16M4 12h16M4 18h16"/>',
    bolt: '<path d="M13 2L4 14h7l-1 8 9-12h-7z"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    box: '<path d="M21 8l-9-5-9 5 9 5z"/><path d="M3 8v8l9 5 9-5V8"/>',
    doc: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6M8 13h8M8 17h5"/>',
    music: '<path d="M9 18V5l11-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="17" cy="16" r="3"/>',
    video: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M10 9l5 3-5 3z"/>',
    app: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M9 9h6v6H9z"/>',
    dots: '<circle cx="5" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="19" cy="12" r="1.2"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.01"/>',
  };
  const svg = (name) => `<svg class="i" viewBox="0 0 24 24">${ICONS[name] || ''}</svg>`;

  function fmtBytes(n) {
    if (n == null) return '—';
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']; // the engine counts in 1024s
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
      return new Date(secs * 1000).toLocaleString(lang === 'fa' ? 'fa-IR' : 'en-US', { dateStyle: 'short', timeStyle: 'short' });
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
  const speedHist = [];

  const CATS = ['Compressed', 'Documents', 'Music', 'Video', 'Programs', 'General'];
  const CAT_ICON = { Compressed: 'box', Documents: 'doc', Music: 'music', Video: 'video', Programs: 'app', General: 'file' };

  function matches(key, it) {
    if (key.startsWith('q:')) return it.queueId === Number(key.slice(2));
    if (key === 'all') return true;
    if (key === 'active') return it.status === 'downloading';
    if (key === 'completed') return it.status === 'completed';
    if (key === 'unfinished') return it.status !== 'completed';
    if (key.startsWith('cat:')) return it.category === key.slice(4);
    return true;
  }

  /* ------------------------------------------------------------------ */
  /* sidebar (tree like IDM: All downloads > categories, then status)    */
  /* ------------------------------------------------------------------ */
  const sideNodes = [];
  let statusTitleEl = null;

  let queues = [];
  const queueName = (id) => (queues.find((q) => q.id === id) || { name: '' }).name;

  function buildSide() {
    const side = $('side');
    side.innerHTML = '';
    sideNodes.length = 0;
    const make = (key, labelKey, level, icon) => {
      const b = document.createElement('button');
      b.className = 'node' + (level ? ' lvl1' : '');
      b.innerHTML = `${svg(icon)}<span class="lbl"></span><span class="cnt num"></span>`;
      b.addEventListener('click', () => { filter = key; render(); });
      side.appendChild(b);
      sideNodes.push({ el: b, key, labelKey });
    };
    make('all', 'tAll', 0, 'all');
    CATS.forEach((c) => make('cat:' + c, 'cat_' + c, 1, CAT_ICON[c]));
    statusTitleEl = document.createElement('div');
    statusTitleEl.className = 'title';
    side.appendChild(statusTitleEl);
    make('active', 'fActive', 0, 'bolt');
    make('unfinished', 'fUnfinished', 0, 'clock');
    make('completed', 'fCompleted', 0, 'check');
    const qt = document.createElement('div');
    qt.className = 'title';
    qt.textContent = t('tQueues');
    qt.dataset.qtitle = '1';
    side.appendChild(qt);
    queues.forEach((q) => make('q:' + q.id, q.name, 0, 'clock'));
    if (filter.startsWith('q:') && !queues.some((q) => 'q:' + q.id === filter)) filter = 'all';
  }

  async function loadQueues() {
    queues = (await call('get_queues')) || [];
    buildSide();
    render();
  }

  function updateSide() {
    statusTitleEl.textContent = t('tStatus');
    const qtEl = $('side').querySelector('[data-qtitle]');
    if (qtEl) qtEl.textContent = t('tQueues');
    for (const n of sideNodes) {
      setText(n.el.querySelector('.lbl'), t(n.labelKey));
      setText(n.el.querySelector('.cnt'), String(items.filter((it) => matches(n.key, it)).length));
      n.el.classList.toggle('on', n.key === filter);
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
      <td class="c-prog"><div class="progwrap"><div class="bar"><i class="fill"></i></div><span class="num pct"></span></div></td>
      <td class="c-speed"><span class="num speed"></span></td>
      <td class="c-eta"><span class="num eta"></span></td>
      <td class="c-status"><span class="pill stt"></span><span class="sub"></span></td>
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
      `<button class="icon-btn" data-act="${act}" title="${esc(t(key))}" aria-label="${esc(t(key))}">${svg(icon)}</button>`;
    let h = '';
    if ((it.status === 'downloading' && !it.stopping) || it.status === 'queued') h += b('pause', 'pause', 'pause');
    else if (it.status === 'paused' || it.status === 'error') h += b('resume', 'play', 'resume');
    if (it.status === 'completed') h += b('open', 'file', 'openFile');
    h += b('details', 'info', 'detailsBtn');
    h += b('folder', 'folder', 'openFolder');
    h += b('delete', 'trash', 'delete');
    return h;
  }

  function percentOf(it) {
    if (it.status === 'completed') return 100;
    if (it.total) return Math.min(100, (it.downloaded / it.total) * 100);
    return null;
  }

  function updateRow(tr, it) {
    const r = tr._r;
    setText(r.tag, it.category ? t('cat_' + it.category) : '');
    r.tag.className = 'tag cat-' + (it.category || 'General');
    setText(r.name, it.filename);
    r.name.title = `${it.savePath}\n${it.url}`;
    setText(r.size, fmtBytes(it.total));

    const pct = percentOf(it);
    const active = it.status === 'downloading';
    r.bar.className = 'bar' +
      (pct === null && active ? ' indet' : '') +
      (active ? '' : ' idle') +
      (it.status === 'completed' ? ' done' : '') +
      (it.status === 'error' ? ' err' : '') +
      (it.status === 'paused' ? ' paused' : '');
    r.fill.style.width = (pct === null ? 0 : pct) + '%';
    setText(r.pct, pct === null ? '' : Math.floor(pct) + '%');

    setText(r.speed, active ? fmtSpeed(it.speed) : '');
    setText(r.eta, active ? fmtEta(it.eta) : '');

    let stText = it.stopping ? t('st_pausing') : t('st_' + it.status);
    if (it.status === 'completed') {
      if (it.integrity === 'mismatch') stText += ' ⚠ ' + t('badHash');
      else if (it.scan === 'threat') stText += ' ⚠ ' + t('badScan');
      else if (it.integrity === 'verifying' || it.scan === 'scanning') stText += ' …';
      else if (it.integrity === 'verified') stText += ' ✓';
    }
    setText(r.stt, stText);
    r.stt.className = 'pill stt st-' + it.status;
    const shortErr = it.error ? (it.error.length > 70 ? it.error.slice(0, 70) + '…' : it.error) : '';
    setText(r.sub, shortErr);
    r.sub.title = it.error || '';
    setText(r.added, fmtDate(it.added));

    const sig = `${it.status}|${it.stopping}|${lang}`;
    if (tr._sig !== sig) { r.act.innerHTML = actionsHtml(it); tr._sig = sig; }
    tr.classList.toggle('sel', selected.has(it.id));
    tr.setAttribute('aria-selected', String(selected.has(it.id)));
    tr.tabIndex = 0;
  }

  /* ---------------- details panel with the "connections map" ---------------- */
  let segCount = 0;

  function ensureSegCells(n) {
    if (segCount === n) return;
    const seg = $('seg');
    seg.innerHTML = '';
    for (let k = 0; k < n; k++) seg.appendChild(document.createElement('i'));
    seg.style.gridTemplateColumns = `repeat(${n}, 1fr)`;
    segCount = n;
  }

  function updateDetail() {
    const sel = items.filter((i) => selected.has(i.id));
    const panel = $('detail');
    if (sel.length !== 1) { panel.hidden = true; return; }
    const it = sel[0];
    panel.hidden = false;

    setText($('d-name'), it.filename);
    setText($('d-status'), it.stopping ? t('st_pausing') : t('st_' + it.status));
    $('d-status').className = 'pill st-' + it.status;
    setText($('d-url'), it.url);
    setText($('d-path'), it.savePath);
    setText($('d-size'), fmtBytes(it.total));
    setText($('d-done'), fmtBytes(it.downloaded));
    setText($('d-speed'), it.status === 'downloading' ? fmtSpeed(it.speed) : '—');
    setText($('d-eta'), it.status === 'downloading' && it.eta != null ? fmtEta(it.eta) : '—');
    setText($('d-resume'), it.resumable ? t('yes') : t('no'));
    setText($('d-added'), fmtDate(it.added));

    let cells;
    if (it.status === 'completed') {
      cells = new Array(64).fill(100);
    } else if (it.segments && it.segments.length) {
      cells = it.segments;
    } else {
      const pct = percentOf(it) || 0;
      cells = Array.from({ length: 64 }, (_, k) => Math.max(0, Math.min(1, (pct * 64) / 100 - k)) * 100);
    }
    ensureSegCells(cells.length);
    const seg = $('seg');
    seg.classList.toggle('done', it.status === 'completed');
    const nodes = seg.children;
    for (let k = 0; k < cells.length; k++) {
      const v = Math.round(cells[k]);
      const el = nodes[k];
      if (el._v !== v) {
        el._v = v;
        el.style.setProperty('--p', v + '%');
        el.classList.toggle('full', v >= 100);
      }
    }
  }

  function drawSpark() {
    const W = 120, H = 24;
    const pts = speedHist.slice(-60);
    if (pts.length < 2) { $('spark-line').setAttribute('d', ''); $('spark-area').setAttribute('d', ''); return; }
    const max = Math.max(...pts, 1);
    const step = W / (pts.length - 1);
    const xy = pts.map((v, k) => `${(k * step).toFixed(1)},${(H - 2 - (v / max) * (H - 5)).toFixed(1)}`);
    const line = 'M' + xy.join(' L');
    $('spark-line').setAttribute('d', line);
    $('spark-area').setAttribute('d', `${line} L${W},${H} L0,${H} Z`);
  }

  /* ---------------- search + sorting ---------------- */
  let query = '';
  let sortKey = 'added';
  let sortDir = -1; // -1 = descending (newest first)

  function matchesSearch(it) {
    if (!query) return true;
    const hay = [it.filename, it.url, it.category, t('cat_' + it.category), it.status, t('st_' + it.status), queueName(it.queueId)]
      .join(' ').toLowerCase();
    return query.split(/\s+/).every((w) => hay.includes(w));
  }

  const pctVal = (it) => (it.status === 'completed' ? 100 : it.total ? (it.downloaded / it.total) * 100 : 0);
  const etaVal = (it) => (it.eta == null ? Number.MAX_SAFE_INTEGER : it.eta);
  const SORTERS = {
    added: (a, b) => a.added - b.added,
    name: (a, b) => a.filename.localeCompare(b.filename, lang),
    size: (a, b) => (a.total || 0) - (b.total || 0),
    downloaded: (a, b) => a.downloaded - b.downloaded,
    progress: (a, b) => pctVal(a) - pctVal(b),
    speed: (a, b) => (a.speed || 0) - (b.speed || 0),
    eta: (a, b) => etaVal(a) - etaVal(b),
    status: (a, b) => a.status.localeCompare(b.status),
    completed: (a, b) => (a.completedAt || 0) - (b.completedAt || 0),
  };
  function sortItems(list) {
    const f = SORTERS[sortKey] || SORTERS.added;
    return list.slice().sort((a, b) => sortDir * f(a, b) || b.id - a.id);
  }

  function render() {
    const ids = new Set(items.map((i) => i.id));
    for (const id of [...selected]) if (!ids.has(id)) selected.delete(id);

    const list = sortItems(items.filter((it) => matches(filter, it) && matchesSearch(it)));
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
    updateDetail();

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
    $('btn-details').disabled = sel.length !== 1;
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
    else if (kind === 'details') await call('open_details', { id });
    else if (kind === 'delete') askDelete([id]);
  }

  const selectedIds = () => [...selected];

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
    if (it) rowAction(it.status === 'completed' ? 'open' : 'details', id);
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

  // Keyboard use of the list: arrows move, Space toggles, Enter opens, Ctrl+A selects all.
  $('rows').addEventListener('keydown', (e) => {
    const tr = e.target.closest('tr');
    if (!tr || e.target.closest('button')) return;
    const id = Number(tr.dataset.id);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = e.key === 'ArrowDown' ? tr.nextElementSibling : tr.previousElementSibling;
      if (next) {
        selected = new Set([Number(next.dataset.id)]);
        render();
        next.focus();
      }
    } else if (e.key === ' ') {
      e.preventDefault();
      if (selected.has(id)) selected.delete(id); else selected.add(id);
      render();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const it = items.find((i) => i.id === id);
      if (it) rowAction(it.status === 'completed' ? 'open' : 'details', id);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      selected = new Set([...rowEls.keys()]);
      render();
    }
  });

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
  $('btn-add').addEventListener('click', async () => {
    $('add-error').hidden = true;
    $('add-ok').disabled = false;
    const s = await call('get_settings');
    $('add-dir').value = (s && s.lastDir) || ''; // the folder used last time
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
  function syncProxyBox() { $('proxy-manual').hidden = $('set-proxy-mode').value !== 'manual'; }
  $('set-proxy-mode').addEventListener('change', syncProxyBox);
  $('forget-hosts').addEventListener('click', async () => {
    if ((await call('clear_host_limits')) !== undefined) toast(t('hostsCleared'));
  });
  const dlgSet = $('dlg-settings');
  $('btn-settings').addEventListener('click', async () => {
    const s = await call('get_settings');
    if (!s) return;
    $('set-dir').value = s.downloadDir;
    setText($('last-dir'), s.lastDir || '—');
    const connValue = String(s.connections);
    if (![...$('set-conn').options].some((o) => o.value === connValue)) {
      const o = document.createElement('option');
      o.value = connValue;
      o.textContent = connValue;
      $('set-conn').appendChild(o);
    }
    $('set-conn').value = connValue;
    $('set-conc').value = s.maxConcurrent;
    $('set-limit').value = s.speedLimitKbps;
    $('set-retries').value = s.retries;
    $('set-cat').checked = s.categorize;
    $('set-confirm').checked = true; // always on: nothing starts without the user's confirmation
    $('set-showdetails').checked = s.showDetailsOnStart !== false;
    $('set-closedetails').checked = !!s.closeDetailsOnDone;
    $('set-tray').checked = s.closeToTray;
    $('set-scan-path').value = s.scannerPath || '';
    $('set-scan-args').value = s.scannerArgs || '{file}';
    $('set-scan-codes').value = s.scannerThreatCodes || '1,2';
    $('set-http').value = s.httpMode || 'http1';
    $('set-rules').value = s.limitRules || '';
    $('set-proxy-mode').value = s.proxyMode || 'system';
    $('set-proxy-url').value = s.proxyUrl || '';
    $('set-proxy-user').value = s.proxyUser || '';
    $('set-proxy-pass').value = '';
    $('set-proxy-bypass').value = s.proxyBypass || '';
    syncProxyBox();
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
  $('last-clear').addEventListener('click', async () => {
    await call('clear_last_dir');
    setText($('last-dir'), '—');
  });
  const num = (id, min, max, dflt) => {
    const v = parseInt($(id).value, 10);
    return Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : dflt;
  };
  $('set-ok').addEventListener('click', async () => {
    const settings = {
      downloadDir: $('set-dir').value.trim(),
      connections: num('set-conn', 0, 64, 0),
      maxConcurrent: num('set-conc', 1, 16, 3),
      speedLimitKbps: num('set-limit', 0, 100000000, 0),
      retries: num('set-retries', 0, 50, 8),
      categorize: $('set-cat').checked,
      confirmDownloads: true,
      showDetailsOnStart: $('set-showdetails').checked,
      closeDetailsOnDone: $('set-closedetails').checked,
      closeToTray: $('set-tray').checked,
      scannerPath: $('set-scan-path').value.trim(),
      scannerArgs: $('set-scan-args').value.trim() || '{file}',
      scannerThreatCodes: $('set-scan-codes').value.trim() || '1,2',
      httpMode: $('set-http').value,
      limitRules: $('set-rules').value,
      proxyMode: $('set-proxy-mode').value,
      proxyUrl: $('set-proxy-url').value.trim(),
      proxyUser: $('set-proxy-user').value.trim(),
      proxyPassword: $('set-proxy-pass').value,
      proxyBypass: $('set-proxy-bypass').value.trim(),
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
  function askDelete(ids, withFile = false) {
    if (!ids.length) return;
    pendingDelete = ids;
    $('del-text').textContent = ids.length === 1 ? t('delQuestion') : `${ids.length} ${t('delQuestionN')}`;
    $('del-file').checked = withFile;
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
    document.querySelectorAll('[data-i18n-placeholder]').forEach((el) => { el.placeholder = t(el.dataset.i18nPlaceholder); });
    document.querySelectorAll('[data-i18n-aria]').forEach((el) => { el.setAttribute('aria-label', t(el.dataset.i18nAria)); });
    for (const tr of rowEls.values()) tr._sig = null; // rebuild row tooltips
    render();
  }

  /* ------------------------------------------------------------------ */
  /* start                                                               */
  /* ------------------------------------------------------------------ */
  /* ---------------- search, sort, details, context menu ---------------- */
  $('search').addEventListener('input', (e) => { query = e.target.value.trim().toLowerCase(); render(); });
  $('sort-key').addEventListener('change', (e) => { sortKey = e.target.value; render(); });
  $('sort-dir').addEventListener('click', () => {
    sortDir = -sortDir;
    $('sort-dir').innerHTML = sortDir < 0 ? '&darr;' : '&uarr;';
    render();
  });
  $('btn-details').addEventListener('click', () => {
    const ids = selectedIds();
    if (ids.length === 1) rowAction('details', ids[0]);
  });

  const ctx = $('ctx');
  function hideCtx() { ctx.hidden = true; }

  async function ctxAction(action, it) {
    hideCtx();
    if (action === 'pause') await call('pause_download', { id: it.id });
    else if (action === 'resume') await call('resume_download', { id: it.id });
    else if (action === 'restart') await call('restart_download', { id: it.id });
    else if (action === 'open') await call('open_file', { id: it.id });
    else if (action === 'folder') await call('open_folder', { id: it.id });
    else if (action === 'details') await call('open_details', { id: it.id });
    else if (action === 'props') openProps(it);
    else if (action === 'verify') { if ((await call('verify_download', { id: it.id, expected: null })) !== undefined) toast(t('verifyStarted')); }
    else if (action === 'rescan') { if ((await call('rescan_download', { id: it.id })) !== undefined) toast(t('verifyStarted')); }
    else if (action === 'delete') askDelete([it.id]);
    else if (action === 'deleteFile') askDelete([it.id], true);
    else if (action === 'copyUrl' || action === 'copyPath') {
      try {
        await navigator.clipboard.writeText(action === 'copyUrl' ? it.url : it.savePath);
        toast(t('copied'));
      } catch (_) { /* clipboard unavailable */ }
    }
  }

  function showCtx(x, y, it) {
    const st = it.status;
    const entries = [];
    if ((st === 'downloading' && !it.stopping) || st === 'queued') entries.push(['pause', 'pause']);
    if (st === 'paused') entries.push(['resume', 'resume']);
    if (st === 'error') entries.push(['retry', 'resume']);
    if (st !== 'downloading') entries.push(['restart', 'restart']);
    if (st === 'completed') entries.push(['openFile', 'open'], ['verifyHash', 'verify'], ['scanAgain', 'rescan']);
    entries.push(['openFolder', 'folder'], ['copyUrl', 'copyUrl'], ['copyPath', 'copyPath'], ['detailsBtn', 'details'], ['properties', 'props']);
    entries.push(null, ['removeFromList', 'delete']);
    if (st === 'completed') entries.push(['deleteFile', 'deleteFile', true]);

    ctx.innerHTML = '';
    for (const e of entries) {
      if (!e) {
        const hr = document.createElement('hr');
        ctx.appendChild(hr);
        continue;
      }
      const b = document.createElement('button');
      b.type = 'button';
      b.setAttribute('role', 'menuitem');
      b.textContent = t(e[0]);
      if (e[2]) b.className = 'danger';
      b.addEventListener('click', () => ctxAction(e[1], it));
      ctx.appendChild(b);
    }
    ctx.hidden = false;
    const r = ctx.getBoundingClientRect();
    ctx.style.left = Math.max(4, Math.min(x, window.innerWidth - r.width - 4)) + 'px';
    ctx.style.top = Math.max(4, Math.min(y, window.innerHeight - r.height - 4)) + 'px';
    const first = ctx.querySelector('button');
    if (first) first.focus();
  }

  $('rows').addEventListener('contextmenu', (e) => {
    const tr = e.target.closest('tr');
    if (!tr) return;
    e.preventDefault();
    const id = Number(tr.dataset.id);
    const it = items.find((i) => i.id === id);
    if (!it) return;
    if (!selected.has(id)) selected = new Set([id]);
    render();
    showCtx(e.clientX, e.clientY, it);
  });
  document.addEventListener('click', (e) => { if (!ctx.contains(e.target)) hideCtx(); });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') hideCtx(); });
  window.addEventListener('blur', hideCtx);


  /* ---------------- queues dialog ---------------- */
  const dlgQ = $('dlg-queues');
  let qSel = 0;
  const qErr = (msg) => { const b = $('q-error'); b.textContent = msg || ''; b.hidden = !msg; };

  function renderQList() {
    const ul = $('q-list');
    ul.innerHTML = '';
    for (const q of queues) {
      const li = document.createElement('li');
      li.tabIndex = 0;
      li.setAttribute('role', 'option');
      li.setAttribute('aria-selected', String(q.id === qSel));
      if (!q.enabled) li.classList.add('off');
      const n = items.filter((i) => i.queueId === q.id).length;
      li.innerHTML = '<span></span><span class="num"></span>';
      li.children[0].textContent = q.name;
      li.children[1].textContent = String(n);
      const pick = () => { qSel = q.id; fillQForm(q); renderQList(); };
      li.addEventListener('click', pick);
      li.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(); } });
      ul.appendChild(li);
    }
  }
  function fillQForm(q) {
    qErr('');
    $('q-name').value = q.name;
    $('q-enabled').checked = q.enabled;
    $('q-priority').value = q.priority;
    $('q-max').value = q.maxConcurrent;
    $('q-limit').value = q.speedLimitKbps;
    $('q-sched').checked = q.scheduleEnabled;
    $('q-start').value = q.scheduleStart || '';
    $('q-stop').value = q.scheduleStop || '';
    $('q-after').value = q.afterFinish || 'none';
    $('q-onlyok').checked = q.onlyIfAllSucceeded;
    $('q-delete').disabled = q.id === 1 || q.id === 0;
    $('q-startnow').disabled = q.id === 0;
    $('q-stopnow').disabled = q.id === 0;
  }
  const intIn = (id, lo, hi, d) => {
    const v = parseInt($(id).value, 10);
    return Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d;
  };
  function readQForm() {
    return {
      id: qSel,
      name: $('q-name').value.trim(),
      enabled: $('q-enabled').checked,
      priority: intIn('q-priority', -100, 100, 0),
      maxConcurrent: intIn('q-max', 1, 16, 3),
      speedLimitKbps: intIn('q-limit', 0, 100000000, 0),
      scheduleEnabled: $('q-sched').checked,
      scheduleStart: $('q-start').value || '',
      scheduleStop: $('q-stop').value || '',
      afterFinish: $('q-after').value,
      onlyIfAllSucceeded: $('q-onlyok').checked,
    };
  }
  async function openQueues() {
    await loadQueues();
    qSel = (queues[0] || {}).id || 0;
    if (queues[0]) fillQForm(queues[0]);
    renderQList();
    dlgQ.showModal();
  }
  $('btn-queues').addEventListener('click', openQueues);
  $('open-queues').addEventListener('click', () => { dlgSet.close(); openQueues(); });
  $('q-close').addEventListener('click', () => dlgQ.close());
  $('q-new').addEventListener('click', () => {
    qSel = 0;
    fillQForm({ id: 0, name: t('qNewName'), enabled: true, priority: 0, maxConcurrent: 3, speedLimitKbps: 0,
      scheduleEnabled: false, scheduleStart: '', scheduleStop: '', afterFinish: 'none', onlyIfAllSucceeded: true });
    renderQList();
    $('q-name').focus();
  });
  $('q-save').addEventListener('click', async () => {
    try {
      const id = await invoke('save_queue', { queue: readQForm() });
      qSel = id;
      await loadQueues();
      const q = queues.find((x) => x.id === id);
      if (q) fillQForm(q);
      renderQList();
      toast(t('qSaved'));
    } catch (e) { qErr(String(e)); }
  });
  $('q-delete').addEventListener('click', async () => {
    if (qSel === 1) { qErr(t('qMainNoDelete')); return; }
    try {
      await invoke('delete_queue', { id: qSel });
      await loadQueues();
      qSel = queues[0].id;
      fillQForm(queues[0]);
      renderQList();
    } catch (e) { qErr(String(e)); }
  });
  $('q-startnow').addEventListener('click', async () => { if (qSel) { await call('start_queue', { id: qSel }); await loadQueues(); renderQList(); } });
  $('q-stopnow').addEventListener('click', async () => { if (qSel) { await call('stop_queue', { id: qSel }); await loadQueues(); renderQList(); } });

  /* ---------------- properties dialog ---------------- */
  const dlgPr = $('dlg-props');
  let prItem = null;
  function openProps(it) {
    prItem = it;
    $('pr-error').hidden = true;
    $('pr-name').value = it.filename;
    $('pr-url').value = it.url;
    $('pr-path').value = it.savePath;
    const sel = $('pr-queue');
    sel.innerHTML = '';
    for (const q of queues) {
      const o = document.createElement('option');
      o.value = String(q.id);
      o.textContent = q.name;
      sel.appendChild(o);
    }
    sel.value = String(it.queueId);
    const running = it.status === 'downloading';
    sel.disabled = running;
    $('pr-note').hidden = !running;
    setText($('pr-note'), running ? t('prRunning') : '');
    $('pr-done').value = it.onComplete || 'none';
    $('pr-priority').value = String(it.priority || 0);
    $('pr-limit').value = it.speedLimitKbps || 0;
    dlgPr.showModal();
  }
  $('pr-cancel').addEventListener('click', () => dlgPr.close());
  $('pr-ok').addEventListener('click', async () => {
    if (!prItem) return;
    const id = prItem.id;
    try {
      const q = Number($('pr-queue').value);
      if (q !== prItem.queueId) await invoke('set_item_queue', { id, queueId: q });
      await invoke('set_item_priority', { id, priority: Number($('pr-priority').value) });
      await invoke('set_item_speed_limit', { id, kbps: intIn('pr-limit', 0, 100000000, 0) });
      await invoke('set_on_complete', { id, action: $('pr-done').value });
      dlgPr.close();
    } catch (e) {
      const b = $('pr-error');
      b.textContent = String(e);
      b.hidden = false;
    }
  });

  // Screen-reader announcements for results that matter.
  const lastState = new Map();
  function announce(msg) {
    const el = $('live');
    el.textContent = '';
    setTimeout(() => { el.textContent = msg; }, 50);
  }
  function watchStates() {
    for (const it of items) {
      const key = `${it.status}|${it.integrity || ''}|${it.scan || ''}`;
      const prev = lastState.get(it.id);
      lastState.set(it.id, key);
      if (prev === undefined || prev === key) continue;
      if (it.integrity === 'mismatch') announce(`${t('aBadHash')}: ${it.filename}`);
      else if (it.scan === 'threat') announce(`${t('aThreat')}: ${it.filename}`);
      else if (prev.split('|')[0] !== it.status && it.status === 'completed') announce(`${t('aDone')}: ${it.filename}`);
      else if (prev.split('|')[0] !== it.status && it.status === 'error') announce(`${t('aError')}: ${it.filename}`);
    }
  }

  function onDownloads(list) {
    items = list || [];
    watchStates();
    speedHist.push(items.filter((i) => i.status === 'downloading').reduce((s, i) => s + (i.speed || 0), 0));
    if (speedHist.length > 120) speedHist.shift();
    render();
    drawSpark();
  }

  async function init() {
    document.querySelectorAll('[data-icon]').forEach((el) => {
      el.outerHTML = svg(el.dataset.icon).replace('class="i"', `class="i ${el.className || ''}"`);
    });
    buildSide();
    applyLang();
    await listen('downloads', (ev) => onDownloads(ev.payload));
    await listen('added', (ev) => toast(t('addedToast') + ev.payload));
    await listen('power', () => {
      setText($('banner-text'), t('powerBanner'));
      $('banner').hidden = false;
    });
    queues = (await call('get_queues')) || [];
    buildSide();
    items = (await call('list_downloads')) || [];
    render();
  }
  init();
})();
