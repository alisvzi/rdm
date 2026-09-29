'use strict';

// Chrome/Edge run this as a service worker and need importScripts;
// Firefox loads common.js through the manifest "scripts" list.
if (typeof importScripts === 'function') {
  try { importScripts('common.js'); } catch (e) { console.error(e); }
}

/* ------------------------------------------------------------------ */
/* 1. Take over browser downloads                                      */
/* ------------------------------------------------------------------ */
api.downloads.onCreated.addListener(async (item) => {
  try {
    const cfg = await getCfg();
    if (!cfg.enabled || !cfg.token) return;
    if (item.byExtensionId) return; // started by an extension (maybe us): leave it
    const url = item.finalUrl || item.url;
    if (!/^https?:/i.test(url)) return; // blob:, data:, file: cannot be fetched by RDM
    if (cfg.minSizeMB > 0 && item.totalBytes > 0 && item.totalBytes < cfg.minSizeMB * 1048576) return;

    // Hand it to RDM first; only if that worked do we cancel the browser's own download.
    await sendToRdm({ url, referrer: item.referrer });
    await api.downloads.cancel(item.id);
    await api.downloads.erase({ id: item.id });
    flash('✓', '#16a34a');
  } catch (_) {
    // RDM is closed or refused the link: the browser simply keeps downloading it.
  }
});

/* ------------------------------------------------------------------ */
/* 2. Right-click menu: "Download with RDM"                            */
/* ------------------------------------------------------------------ */
api.contextMenus.removeAll().then(() => {
  api.contextMenus.create({
    id: 'rdm-send',
    title: 'دانلود با RDM',
    contexts: ['link', 'video', 'audio'],
  });
});

api.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== 'rdm-send') return;
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  try {
    await sendToRdm({ url, referrer: tab && tab.url });
    flash('✓', '#16a34a');
  } catch (_) {
    flash('!', '#dc2626');
  }
});

/* ------------------------------------------------------------------ */
/* 3. Detect videos / audio on the page (direct files only)            */
/* ------------------------------------------------------------------ */
const MEDIA_EXT = /\.(mp4|m4v|webm|mkv|mov|mp3|m4a|aac|ogg|opus|flac|wav)(\?|#|$)/i;

api.webRequest.onHeadersReceived.addListener(
  (d) => {
    if (d.tabId < 0 || !d.responseHeaders) return;
    const h = (name) => {
      const f = d.responseHeaders.find((x) => x.name.toLowerCase() === name);
      return (f && f.value) || '';
    };
    const type = h('content-type').toLowerCase();
    if (/mpegurl|dash\+xml/.test(type)) return; // streaming playlists are not supported
    const looksLikeMedia =
      type.startsWith('video/') ||
      type.startsWith('audio/') ||
      (MEDIA_EXT.test(d.url) && !type.startsWith('text/'));
    if (!looksLikeMedia) return;

    let size = parseInt(h('content-length') || '0', 10);
    const range = h('content-range'); // "bytes 0-1/123456"
    if (range.includes('/')) {
      const total = parseInt(range.split('/').pop(), 10);
      if (Number.isFinite(total)) size = total;
    }
    if (size && size < 100 * 1024) return; // ads, tiny sounds

    addMedia(d.tabId, { url: d.url, type, size }).then((list) => {
      try { actionApi.setBadgeText({ tabId: d.tabId, text: String(list.length) }); } catch (_) {}
    });
  },
  { urls: ['<all_urls>'] },
  ['responseHeaders']
);

// New page in a tab = forget what we found on the old one.
api.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === 'loading' && changeInfo.url) {
    clearMedia(tabId);
    try { actionApi.setBadgeText({ tabId, text: '' }); } catch (_) {}
  }
});
api.tabs.onRemoved.addListener((tabId) => clearMedia(tabId));

/* ------------------------------------------------------------------ */
function flash(text, color) {
  try {
    actionApi.setBadgeBackgroundColor({ color });
    actionApi.setBadgeText({ text });
    setTimeout(() => actionApi.setBadgeText({ text: '' }), 2500);
  } catch (_) { /* badge is only cosmetic */ }
}
