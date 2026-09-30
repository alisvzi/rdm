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

// Sends a link to RDM together with the cookies/referrer, so logged-in downloads work.
async function sendToRdm({ url, referrer, startNow = true }) {
  const cookies = await cookiesFor(url);
  return rdmFetch('/add', {
    method: 'POST',
    body: JSON.stringify({ url, referrer, cookies, userAgent: navigator.userAgent, startNow, confirm: true }),
  });
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
