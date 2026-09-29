'use strict';

(async () => {
  const $ = (id) => document.getElementById(id);
  const msg = (text, ok) => {
    $('msg').textContent = text;
    $('msg').className = 'msg ' + (ok ? 'ok' : 'bad');
  };

  const cfg = await getCfg();
  $('token').value = cfg.token;
  $('port').value = cfg.port;
  $('min').value = cfg.minSizeMB;
  $('enabled').checked = cfg.enabled;

  async function save() {
    await setCfg({
      token: $('token').value.trim(),
      port: parseInt($('port').value, 10) || 46873,
      minSizeMB: Math.max(0, parseInt($('min').value, 10) || 0),
      enabled: $('enabled').checked,
    });
  }

  $('save').addEventListener('click', async () => {
    await save();
    msg('ذخیره شد.', true);
  });

  $('test').addEventListener('click', async () => {
    await save();
    try {
      const r = await rdmFetch('/ping');
      msg(`اتصال برقرار است (نسخه‌ی برنامه: ${r.version}).`, true);
    } catch (e) {
      msg(e.message === 'RDM_NOT_RUNNING'
        ? 'برنامه‌ی RDM باز نیست یا پورت اشتباه است.'
        : 'کد اتصال درست نیست.', false);
    }
  });
})();
