'use strict';

(async () => {
  const $ = (id) => document.getElementById(id);

  const cfg = await getCfg();
  $('enabled').checked = cfg.enabled;
  $('enabled').addEventListener('change', (e) => setCfg({ enabled: e.target.checked }));
  $('open-options').addEventListener('click', (e) => {
    e.preventDefault();
    api.runtime.openOptionsPage();
  });

  // connection status
  const st = $('status');
  try {
    await rdmFetch('/ping');
    st.textContent = 'متصل';
    st.className = 'status ok';
  } catch (e) {
    st.className = 'status bad';
    st.textContent = e.message === 'RDM_NOT_RUNNING'
      ? 'برنامه‌ی RDM باز نیست'
      : 'کد اتصال درست نیست';
  }

  // detected media
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  const list = tab ? await getMedia(tab.id) : [];
  const ul = $('media');
  $('empty').hidden = list.length > 0;

  const fmt = (n) => {
    if (!n) return '';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0, v = n;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return v.toFixed(i ? 1 : 0) + ' ' + u[i];
  };
  const nameOf = (url) => {
    try {
      const last = new URL(url).pathname.split('/').filter(Boolean).pop() || url;
      return decodeURIComponent(last);
    } catch (_) { return url; }
  };

  for (const m of list.slice().reverse()) {
    const li = document.createElement('li');
    const info = document.createElement('div');
    info.className = 'info';
    const name = document.createElement('div');
    name.className = 'name';
    name.textContent = nameOf(m.url);
    name.title = m.url;
    const meta = document.createElement('div');
    meta.className = 'meta';
    meta.textContent = [m.type.split(';')[0], fmt(m.size)].filter(Boolean).join(' · ');
    info.append(name, meta);

    const btn = document.createElement('button');
    btn.textContent = 'دانلود';
    btn.addEventListener('click', async () => {
      btn.disabled = true;
      try {
        await sendToRdm({ url: m.url, referrer: tab && tab.url });
        btn.textContent = '✓';
      } catch (e) {
        btn.textContent = 'خطا';
        btn.title = e.message === 'RDM_NOT_RUNNING' ? 'برنامه‌ی RDM باز نیست' : e.message;
        btn.disabled = false;
      }
    });
    li.append(info, btn);
    ul.appendChild(li);
  }
})();
