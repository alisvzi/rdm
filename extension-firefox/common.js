'use strict';
// Shared by the background script, the popup and the options page.

const api = globalThis.browser ?? globalThis.chrome;
const actionApi = api.action ?? api.browserAction;

const DEFAULTS = { token: '', port: 46873, enabled: true, minSizeMB: 0 };

async function getCfg() {
  const s = await api.storage.local.get(DEFAULTS);
  return { ...DEFAULTS, ...s };
}

async function setCfg(patch) {
  await api.storage.local.set(patch);
}

// Talks to the RDM desktop app on this computer.
async function rdmFetch(path, options = {}) {
  const cfg = await getCfg();
  let res;
  try {
    res = await fetch(`http://127.0.0.1:${cfg.port}${path}`, {
      ...options,
      headers: { 'Content-Type': 'application/json', 'X-RDM-Token': cfg.token },
    });
  } catch (_) {
    throw new Error('RDM_NOT_RUNNING');
  }
  let data = null;
  try { data = await res.json(); } catch (_) { /* empty body */ }
  if (!res.ok) throw new Error((data && data.error) || `HTTP ${res.status}`);
  return data;
}

async function cookiesFor(url) {
  try {
    const list = await api.cookies.getAll({ url });
    return list.map((c) => `${c.name}=${c.value}`).join('; ');
  } catch (_) {
    return '';
  }
}

// Native Messaging: the browser itself talks to rdm-nm.exe, which talks to RDM (and starts it
// when needed). Used when the direct local connection fails. Needs the host installed
// (native-messaging\install.ps1); without it these calls simply fail.
const NM_HOST = 'com.rdm.downloadmanager';

async function nmCall(type, payload) {
  const res = await api.runtime.sendNativeMessage(NM_HOST, { type, payload });
  if (!res || !res.ok) {
    throw new Error((res && res.data && res.data.error) || (res && res.error) || 'NATIVE_FAILED');
  }
  return res.data;
}

// Sends a link to RDM together with the cookies/referrer, so logged-in downloads work.
async function sendToRdm({ url, referrer, startNow = true }) {
  const cookies = await cookiesFor(url);
  const body = { url, referrer, cookies, userAgent: navigator.userAgent, startNow, confirm: true };
  try {
    return await rdmFetch('/add', { method: 'POST', body: JSON.stringify(body) });
  } catch (e) {
    if (e.message !== 'RDM_NOT_RUNNING') throw e;
    try { return await nmCall('add', body); } catch (_) { throw e; }
  }
}

// Many links at once ("download all links"). Each link gets only the cookies of ITS OWN site.
async function sendBatchToRdm({ urls, referrer, startNow = true }) {
  const byOrigin = new Map();
  const items = [];
  for (const url of urls) {
    let key = url;
    try { key = new URL(url).origin; } catch (_) { /* keep the full url as key */ }
    if (!byOrigin.has(key)) byOrigin.set(key, await cookiesFor(url));
    items.push({ url, cookies: byOrigin.get(key) });
  }
  const body = { items, referrer, userAgent: navigator.userAgent, startNow };
  try {
    return await rdmFetch('/add-batch', { method: 'POST', body: JSON.stringify(body) });
  } catch (e) {
    if (e.message !== 'RDM_NOT_RUNNING') throw e;
    try { return await nmCall('add-batch', body); } catch (_) { throw e; }
  }
}

/* ---- detected media per tab (kept in session storage when available) ---- */
const mediaStore = api.storage.session ?? api.storage.local;
const mediaKey = (tabId) => `media:${tabId}`;

async function getMedia(tabId) {
  const key = mediaKey(tabId);
  const r = await mediaStore.get(key);
  return r[key] || [];
}

async function addMedia(tabId, entry) {
  const list = await getMedia(tabId);
  if (list.some((m) => m.url === entry.url)) return list;
  list.push(entry);
  while (list.length > 20) list.shift();
  await mediaStore.set({ [mediaKey(tabId)]: list });
  return list;
}

async function clearMedia(tabId) {
  await mediaStore.remove(mediaKey(tabId));
}
