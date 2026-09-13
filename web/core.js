/* ═══════════ STEALTHNET CORE: icons, helpers, router, modal system ═══════════ */
'use strict';

/* ── icon registry (lucide-style, stroke 1.8) ── */
const ICONS = {
  arrowRight:'<path d="M4 12h16m-6-6 6 6-6 6"/>',
  github:'<path d="M9 19c-4.3 1.3-4.3-2.1-6-2.5M15 22v-3.9c0-1.1.1-1.5-.5-2.2 3.3-.4 6.7-1.6 6.7-7.2a5.6 5.6 0 0 0-1.5-3.9 5.2 5.2 0 0 0-.1-3.8S18.4.6 15.7 2a13.4 13.4 0 0 0-7.4 0C5.6.6 4.4 1 4.4 1a5.2 5.2 0 0 0-.1 3.8 5.6 5.6 0 0 0-1.5 3.9c0 5.6 3.4 6.8 6.7 7.2-.6.7-.6 1.4-.5 2.2V22"/>',
  arrowUp:'<path d="m6 10 6-6 6 6M12 4v16"/>',
  arrowDown:'<path d="m6 14 6 6 6-6M12 4v16"/>',
  menu:'<path d="M4 6h16M4 12h16M4 18h16"/>',
  dash:'<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>',
  card:'<path d="M20 7H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2Z"/><path d="M2 11h20"/>',
  layers:'<path d="M12 2 2 7l10 5 10-5-10-5Z"/><path d="m2 17 10 5 10-5"/><path d="m2 12 10 5 10-5"/>',
  percent:'<path d="M19 5 5 19"/><circle cx="6.5" cy="6.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/>',
  users2:'<path d="M17 11a4 4 0 1 0-4-4"/><circle cx="9" cy="11" r="4"/><path d="M3 21v-1a6 6 0 0 1 12 0v1"/><path d="M16 15a6 6 0 0 1 5 5.6v.4"/>',
  send:'<path d="M22 2 11 13"/><path d="M22 2 15 22l-4-9-9-4Z"/>',
  user:'<circle cx="12" cy="8" r="4"/><path d="M4 21v-1a8 8 0 0 1 16 0v1"/>',
  chat:'<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z"/>',
  server:'<rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><circle cx="6.5" cy="6.5" r=".6" fill="currentColor"/><circle cx="6.5" cy="17.5" r=".6" fill="currentColor"/>',
  host:'<path d="M12 2 2 7l10 5 10-5-10-5Z"/><path d="m2 17 10 5 10-5"/>',
  activity:'<path d="M22 12h-4l-3 8-6-16-3 8H2"/>',
  chart:'<path d="M3 17 9 11l4 4 8-8"/><path d="M15 7h6v6"/>',
  plug:'<path d="M12 22v-5"/><path d="M9 8V2"/><path d="M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/>',
  json:'<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6"/><path d="M10 12a1.5 1.5 0 0 0-1.5 1.5v1A1.5 1.5 0 0 1 7 16a1.5 1.5 0 0 1 1.5 1.5v1A1.5 1.5 0 0 0 10 20"/><path d="M14 12a1.5 1.5 0 0 1 1.5 1.5v1A1.5 1.5 0 0 0 17 16a1.5 1.5 0 0 0-1.5 1.5v1A1.5 1.5 0 0 1 14 20"/>',
  squads:'<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>',
  globe:'<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18Z"/>',
  sliders:'<path d="M4 21v-7"/><path d="M4 10V3"/><path d="M12 21v-9"/><path d="M12 8V3"/><path d="M20 21v-5"/><path d="M20 12V3"/><path d="M1 14h6"/><path d="M9 8h6"/><path d="M17 16h6"/>',
  file:'<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6"/>',
  browser:'<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M2 9h20"/><circle cx="5.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="8" cy="6.5" r=".5" fill="currentColor"/>',
  reply:'<path d="M9 17H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-6l-4 4Z"/><path d="m8 9 3 3-3 3"/>',
  fingerprint:'<path d="M12 11a4 4 0 0 1 4 4c0 2-.5 4-1.5 6"/><path d="M8 15c0-2.2 1.8-4 4-4"/><path d="M12 7a8 8 0 0 1 8 8c0 1.4-.2 2.7-.5 4"/><path d="M4.6 19A8 8 0 0 1 4 15a8 8 0 0 1 4-6.9"/><path d="M12 3a12 12 0 0 1 5 1.1"/>',
  history:'<path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 3v5h5"/><path d="M12 7v5l3 3"/>',
  eye:'<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z"/><circle cx="12" cy="12" r="3"/>',
  magnet:'<path d="M6 15a6 6 0 0 0 12 0V3h-4v12a2 2 0 0 1-4 0V3H6Z"/><path d="M6 7h4"/><path d="M14 7h4"/>',
  pulse:'<path d="M3 12h4l2-7 4 14 2-7h6"/>',
  dollar:'<path d="M12 2v20"/><path d="M17 5.5c-1-1-2.7-1.5-5-1.5-3 0-5 1.3-5 3.5S9 11 12 11s5 1.3 5 3.5-2 3.5-5 3.5c-2.3 0-4-.5-5-1.5"/>',
  settings:'<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.6-1.1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9V9a1.7 1.7 0 0 0 1.5 1h.1a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z"/>',
  shield:'<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/>',
  shieldCheck:'<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/>',
  search:'<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  bell:'<path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/>',
  plus:'<path d="M12 5v14M5 12h14"/>',
  x:'<path d="M18 6 6 18M6 6l12 12"/>',
  check:'<path d="m5 13 4 4L19 7"/>',
  chevD:'<path d="m6 9 6 6 6-6"/>',
  chevR:'<path d="m9 6 6 6-6 6"/>',
  chevL:'<path d="m15 6-6 6 6 6"/>',
  more:'<circle cx="5" cy="12" r="1" fill="currentColor"/><circle cx="12" cy="12" r="1" fill="currentColor"/><circle cx="19" cy="12" r="1" fill="currentColor"/>',
  copy:'<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
  edit:'<path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/>',
  trash:'<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M10 11v6M14 11v6"/>',
  refresh:'<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v5h-5"/>',
  qr:'<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><path d="M14 14h3v3h-3zM20 14h1M14 20h1M18 18h3v3h-3z"/>',
  key:'<circle cx="8" cy="15" r="5"/><path d="m11.5 11.5 9-9"/><path d="M17 4l3 3"/><path d="M14 7l3 3"/>',
  link:'<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>',
  power:'<path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.8 0"/>',
  play:'<path d="m6 3 14 9-14 9Z"/>',
  terminal:'<path d="m4 17 6-6-6-6"/><path d="M12 19h8"/>',
  download:'<path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M5 21h14"/>',
  upload:'<path d="M12 21V9"/><path d="m7 14 5-5 5 5"/><path d="M5 3h14"/>',
  grip:'<circle cx="9" cy="6" r="1" fill="currentColor"/><circle cx="15" cy="6" r="1" fill="currentColor"/><circle cx="9" cy="12" r="1" fill="currentColor"/><circle cx="15" cy="12" r="1" fill="currentColor"/><circle cx="9" cy="18" r="1" fill="currentColor"/><circle cx="15" cy="18" r="1" fill="currentColor"/>',
  clock:'<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 3"/>',
  calendar:'<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>',
  alert:'<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4"/><circle cx="12" cy="17" r=".4" fill="currentColor"/>',
  info:'<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><circle cx="12" cy="8" r=".4" fill="currentColor"/>',
  ban:'<circle cx="12" cy="12" r="9"/><path d="m5.5 5.5 13 13"/>',
  cpu:'<rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>',
  smartphone:'<rect x="6" y="2" width="12" height="20" rx="2.5"/><path d="M11 18h2"/>',
  monitor:'<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>',
  wifi:'<path d="M2 8.8a15 15 0 0 1 20 0"/><path d="M5.5 12.5a10 10 0 0 1 13 0"/><path d="M9 16.2a5 5 0 0 1 6 0"/><circle cx="12" cy="19.5" r="1" fill="currentColor"/>',
  logout:'<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>',
  arrowUp:'<path d="M12 19V5"/><path d="m5 12 7-7 7 7"/>',
  arrowDown:'<path d="M12 5v14"/><path d="m5 12 7 7 7-7"/>',
  external:'<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><path d="M15 3h6v6"/><path d="M10 14 21 3"/>',
  tag:'<path d="M12.6 2.6 21.4 11.4a2 2 0 0 1 0 2.8l-7.2 7.2a2 2 0 0 1-2.8 0L2.6 12.6A2 2 0 0 1 2 11.2V4a2 2 0 0 1 2-2h7.2a2 2 0 0 1 1.4.6Z"/><circle cx="7.5" cy="7.5" r="1.2"/>',
  mail:'<rect x="2" y="4" width="20" height="16" rx="2"/><path d="m2 7 10 6 10-6"/>',
  gift:'<rect x="3" y="8" width="18" height="4" rx="1"/><path d="M12 8v13"/><path d="M19 12v7a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-7"/><path d="M7.5 8a2.5 2.5 0 0 1 0-5C11 3 12 8 12 8s1-5 4.5-5a2.5 2.5 0 0 1 0 5"/>',
  zap:'<path d="M13 2 3 14h9l-1 8 10-12h-9l1-8Z"/>',
  book:'<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/>',
  star:'<path d="m12 2 3.1 6.3 6.9 1-5 4.9 1.2 6.9L12 17.8 5.8 21l1.2-6.9-5-4.9 6.9-1Z"/>',
  rotate:'<path d="M21 12a9 9 0 1 1-9-9c2.5 0 4.8 1 6.4 2.6L21 8"/><path d="M21 3v5h-5"/>',
  database:'<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.7 4 3 9 3s9-1.3 9-3V5"/><path d="M3 12c0 1.7 4 3 9 3s9-1.3 9-3"/>',
  hash:'<path d="M4 9h16M4 15h16M10 3 8 21M16 3l-2 18"/>',
  filter:'<path d="M22 3H2l8 9.5V19l4 2v-8.5Z"/>',
  columns:'<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18M15 3v18"/>',
  help:'<circle cx="12" cy="12" r="9"/><path d="M9 9.5a3 3 0 0 1 5.9.8c0 2-3 2.2-3 4"/><circle cx="11.9" cy="17.4" r=".4" fill="currentColor"/>',
  sun:'<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon:'<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"/>',
  lock:'<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  home:'<path d="m3 10 9-7 9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/><path d="M9 22V12h6v10"/>',
};
const I = (name, size=16) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" style="width:${size}px;height:${size}px">${ICONS[name]||ICONS.help}</svg>`;

/* ── брендовый знак STEALTHNET ── */
const LOGO_S = `<path fill="currentColor" d="M50.6 26.1c14.9 0 25 8.4 25.6 21.2H60.5c-.4-4.6-4-7.4-9.6-7.4-5.3 0-8.6 2.2-8.6 5.6 0 2.9 2.3 4.5 7.5 5.6l10.4 2.2c11.6 2.5 17.4 8.4 17.4 18.3 0 13.3-10.6 22-27 22-16.4 0-27-8.3-27.5-21.6h15.9c.5 4.8 4.5 7.7 10.7 7.7 5.7 0 9.3-2.3 9.3-5.9 0-2.9-2.2-4.6-7.4-5.7l-10.3-2.2C29.4 63.2 23.6 57 23.6 47.5c0-13 10.3-21.4 27-21.4Z"/>`;
const logoMark = (size=30) => `
  <span class="logo-mark" style="width:${size}px;height:${size}px">
    <svg viewBox="0 0 100 100" aria-label="STEALTHNET">
      <rect width="100" height="100" rx="22" fill="#000"/>
      <rect x=".75" y=".75" width="98.5" height="98.5" rx="21.4" fill="none" stroke="var(--border-hi)" stroke-width="1.5"/>
      <g color="#fff" transform="translate(50 50) scale(.76) translate(-50.35 -59.85)">${LOGO_S}</g>
    </svg>
  </span>`;

/* ── formatters ── */
const fmtN = n => Math.round(n).toLocaleString(currentLang());
/* Объём в гигабайтах — к человеческому виду.
 *
 * Всё, что меньше гигабайта, раньше округлялось до мегабайтов: 62 KiB
 * показывались как «0 MiB», и это читалось как «счётчик обнулился», хотя
 * трафик исправно считался. Мелкие значения опускаем до килобайтов и
 * байтов, а ноль так и называем нулём — чтобы «0 B» и «пока не считался»
 * не выглядели одинаково подозрительно. */
const fmtB = (gb) => {
  if (gb === null || gb === undefined) return '—';
  if (gb === Infinity) return '∞';
  if (gb >= 1024) return (gb / 1024).toFixed(2) + ' TiB';
  if (gb >= 1) return gb.toFixed(1).replace(/\.0$/, '') + ' GiB';
  const mib = gb * 1024;
  if (mib >= 1) return (mib >= 10 ? Math.round(mib) : +mib.toFixed(1)) + ' MiB';
  const kib = mib * 1024;
  if (kib >= 1) return (kib >= 10 ? Math.round(kib) : +kib.toFixed(1)) + ' KiB';
  return Math.round(kib * 1024) + ' B';
};
/* Суммы показываем в валюте системы. Жёстко зашитый знак доллара врал бы
   всем, кто продаёт в рублях, — а валюта в системе одна и известна. */
const CURRENCY_SIGN = { USD:'$', EUR:'€', RUB:'₽', UAH:'₴', KZT:'₸', TRY:'₺', GBP:'£', XTR:'★' };
const fmtMoney = (v, cur) => {
  const c = (cur || (typeof DB !== 'undefined' && DB.currency) || 'USD').toUpperCase();
  const sign = CURRENCY_SIGN[c] || c + ' ';
  // Звёзды целые: дробных не бывает.
  const digits = c === 'XTR' ? 0 : 2;
  const num = v.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return c === 'XTR' ? num + ' ★' : sign + num;
};
/* Скорость канала принято мерить в битах, объём — в байтах. Смешивать их
   нельзя: 100 Мбит/с и 100 МБ/с отличаются в восемь раз. */
const fmtBps = (bps) => {
  if (bps == null) return '—';
  const u = ['b/s', 'Kb/s', 'Mb/s', 'Gb/s'];
  let i = 0, v = bps;
  while (v >= 1000 && i < u.length - 1) { v /= 1000; i++; }
  return v.toFixed(i === 0 ? 0 : 2) + ' ' + u[i];
};

/* Объём данных — двоичные приставки, как их показывают ОС. */
const fmtBytes = (b) => {
  if (b == null) return '—';
  const u = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  let i = 0, v = b;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return v.toFixed(i <= 1 ? 0 : 2) + ' ' + u[i];
};

/* Аптайм словами: «73д 6ч» читается быстрее, чем 6 350 400 секунд. */
const fmtUptime = (sec) => {
  if (sec == null) return '—';
  const d = Math.floor(sec / 86400), h = Math.floor((sec % 86400) / 3600);
  const m = Math.floor((sec % 3600) / 60);
  return d ? `${d}д ${h}ч` : h ? `${h}ч ${m}м` : `${m}м`;
};

/* «был 2 мин назад» — по этому полю сразу видно, отвалилась ли нода. */
const fmtAgo = (iso) => {
  if (!iso) return 'ни разу';
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  // «0 с назад» — это «только что», и так понятнее.
  if (s < 45) return 'только что';
  if (s < 60) return Math.round(s) + ' с';
  if (s < 3600) return Math.round(s / 60) + ' мин';
  if (s < 86400) return Math.round(s / 3600) + ' ч';
  return Math.round(s / 86400) + ' д';
};

/* Флаг страны из двухбуквенного кода.

   Эмодзи-флаг — это пара «региональных индикаторов»: буква A имеет код
   0x1F1E6, и так далее. Готовый набор картинок для этого держать незачем.
   Неизвестный или пустой код показываем глобусом, а не пустым местом:
   иначе строка «прыгает» и непонятно, страна не задана или не отрисовалась. */
function flag(cc){
  const code = String(cc || '').trim().toUpperCase();
  if (!/^[A-Z]{2}$/.test(code)) return '🌐';
  return String.fromCodePoint(...[...code].map(c => 0x1F1E6 + c.charCodeAt(0) - 65));
}

/* Готовый значок страны: флаг крупно, код — подсказкой при наведении. */
const ccChip = (cc) => `<span class="cc" title="${esc(cc || 'страна не указана')}">${flag(cc)}</span>`;

/* Значок провайдера инфраструктуры.

   Есть свой логотип — показываем его. Нет — рисуем монограмму, цвет
   которой выведен из названия: один и тот же хостер всегда одного
   цвета, и в списке из десятка нод он узнаётся, не читая подпись.

   За фавиконками на чужие сервисы не ходим: панель self-hosted, и такой
   запрос рассказал бы стороннему сервису, какие хостеры используются. */
/* Восемь цветов монограмм хостеров. Берём «чернильные» варианты: буква
   набрана девятью пикселями на бледной подложке, и светлый оттенок на
   белом фоне читается плохо. В тёмной теме эти же имена дают светлые
   цвета, поэтому список один на обе темы. */
const PROVIDER_COLORS = [
  'var(--info-ink)', 'var(--violet-ink)', 'var(--accent)', 'var(--pink)',
  'var(--warn-ink)', 'var(--ok-ink)',     'var(--err-ink)', 'var(--cyan)',
];

function providerColor(name){
  const s = String(name || '');
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return PROVIDER_COLORS[h % PROVIDER_COLORS.length];
}

function providerLogo(name, logoUrl, size = 18){
  if (logoUrl) {
    // onerror: битая ссылка не должна оставлять дыру в вёрстке.
    return `<img class="pv-logo" src="${esc(logoUrl)}" alt="" width="${size}" height="${size}"
      onerror="this.replaceWith(Object.assign(document.createElement('span'),{className:'pv-mono',style:'--pv:${providerColor(name)}',textContent:'${esc((name||'?')[0].toUpperCase())}'}))">`;
  }
  const letter = esc(String(name || '?').trim()[0] || '?').toUpperCase();
  return `<span class="pv-mono" style="--pv:${providerColor(name)}">${letter}</span>`;
}

/* Значок вместе с названием — то, что вставляют в строки и списки. */
const providerBadge = (name, logoUrl) => !name ? '' :
  `<span class="pv-badge">${providerLogo(name, logoUrl)}<span>${esc(name)}</span></span>`;

/* ── WebAuthn: перевод между base64url и ArrayBuffer ──────────────
   Браузерный API принимает и отдаёт двоичные данные, а JSON их не
   несёт. Сервер шлёт base64url — стандартное для WebAuthn кодирование:
   без набивки и с «-_» вместо «+/», иначе значения ломаются в URL. */
function b64urlToBuf(s){
  const pad = '='.repeat((4 - (s.length % 4)) % 4);
  const bin = atob((s + pad).replace(/-/g, '+').replace(/_/g, '/'));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
}

function bufToB64url(b){
  const bytes = new Uint8Array(b);
  let bin = '';
  for (const x of bytes) bin += String.fromCharCode(x);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/* Рекурсивно превращает поля, которые WebAuthn ждёт двоичными.
   Перечислять их приходится поимённо: отличить «строка-идентификатор»
   от «строка-имя» по значению нельзя. */
const WEBAUTHN_BINARY = new Set(['challenge', 'id', 'userHandle', 'rawId']);
function decodeWebauthn(obj){
  if (Array.isArray(obj)) return obj.map(decodeWebauthn);
  if (obj && typeof obj === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(obj)) {
      out[k] = WEBAUTHN_BINARY.has(k) && typeof v === 'string' ? b64urlToBuf(v) : decodeWebauthn(v);
    }
    return out;
  }
  return obj;
}

/* Ответ браузера — объект с геттерами, JSON.stringify его не возьмёт. */
function encodeCredential(c){
  const r = c.response;
  const out = {
    id: c.id,
    rawId: bufToB64url(c.rawId),
    type: c.type,
    extensions: c.getClientExtensionResults ? c.getClientExtensionResults() : {},
    response: { clientDataJSON: bufToB64url(r.clientDataJSON) },
  };
  if (r.attestationObject) out.response.attestationObject = bufToB64url(r.attestationObject);
  if (r.authenticatorData) out.response.authenticatorData = bufToB64url(r.authenticatorData);
  if (r.signature) out.response.signature = bufToB64url(r.signature);
  if (r.userHandle) out.response.userHandle = bufToB64url(r.userHandle);
  return out;
}

const passkeysSupported = () =>
  typeof PublicKeyCredential !== 'undefined' && !!navigator.credentials;

// Calendar fields use the same local date as the visible dates in the panel.
function dateInputValue(value){
  if (!value) return '';
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return '';
  return [d.getFullYear(),String(d.getMonth()+1).padStart(2,'0'),String(d.getDate()).padStart(2,'0')].join('-');
}

const esc = s => String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
const daysLeft = dateStr => {
  if(!dateStr) return null;
  return Math.ceil((new Date(dateStr) - Date.now()) / 86400000);
};
/* Дата без года, если год текущий: «06 авг.» вместо «06 авг. 2026 г.».
   Полная запись занимала две строки в ячейке и раздувала всю таблицу,
   а год в девяти случаях из десяти и так нынешний. */
const fmtDate = d => {
  if (!d) return '—';
  const dt = new Date(d);
  const opts = dt.getFullYear() === new Date().getFullYear()
    ? {day:'2-digit', month:'short'}
    : {day:'2-digit', month:'short', year:'2-digit'};
  return dt.toLocaleDateString(currentLang(), opts).replace(/\s*г\.$/,'');
};
const fmtDT = d => d ? new Date(d).toLocaleDateString(currentLang(),{day:'2-digit',month:'short'}) + ' ' + new Date(d).toLocaleTimeString(currentLang(),{hour:'2-digit',minute:'2-digit'}) : '—';

const USER_STATUS = {
  ACTIVE:   {t:'Активен',   c:'ok'},
  DISABLED: {t:'Отключён',  c:'neutral'},
  LIMITED:  {t:'Лимит',     c:'warn'},
  EXPIRED:  {t:'Истёк',     c:'err'},
};
const statusBadge = s => { const st = USER_STATUS[s] || {t:s,c:'neutral'}; return `<span class="bdg ${st.c}"><span class="dot"></span>${st.t}</span>`; };

/* ── toasts ── */
function toast(msg, type='ok'){
  const el = document.createElement('div');
  el.className = `toast ${type}`;
  const ic = type==='ok' ? 'check' : type==='err' ? 'alert' : 'info';
  el.innerHTML = I(ic,15) + `<span>${esc(msg)}</span>`;
  document.getElementById('toasts').appendChild(el);
  requestAnimationFrame(()=>el.classList.add('in'));
  setTimeout(()=>{ el.classList.remove('in'); setTimeout(()=>el.remove(), 300); }, 3400);
}
/* Копирование в буфер.

   Раньше сообщение «Скопировано» показывалось всегда: writeText —
   асинхронный, и его отказ (нет https, окно не в фокусе, старый браузер)
   проходил мимо try/catch. Человек видел успех, вставлял — а там пусто.
   Теперь при отказе показываем текст, чтобы его можно было выделить
   руками. */
function copyText(t, label='Скопировано в буфер'){
  const fallback = () => {
    // Старый способ: скрытое поле и execCommand. Работает там, где
    // Clipboard API недоступен, — например, при доступе по http.
    try {
      const ta = document.createElement('textarea');
      ta.value = t;
      ta.setAttribute('readonly', '');
      ta.style.cssText = 'position:fixed;top:-1000px;opacity:0';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand('copy');
      ta.remove();
      return ok;
    } catch (_) { return false; }
  };

  const failed = () => {
    if (fallback()) { toast(label); return; }
    toast('Скопировать не удалось — выделите текст и скопируйте вручную', 'err');
  };

  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(t).then(() => toast(label), failed);
  } else {
    failed();
  }
}

/* Копирование по атрибуту.

   Раньше строку для копирования подставляли прямо в onclick. В HTML-
   атрибуте обратный слэш ничего не экранирует, поэтому любая кавычка
   внутри команды разрывала обработчик, и кнопка падала с синтаксической
   ошибкой вместо копирования. Через data-атрибут кавычки безопасны:
   браузер разбирает их как текст, а не как код. */
document.addEventListener('click', (e) => {
  const el = e.target.closest && e.target.closest('[data-copy]');
  if (!el) return;
  e.preventDefault();
  copyText(el.dataset.copy, el.dataset.copyMsg || 'Скопировано в буфер');
});

/* Ссылка подписки клиента.

   Адрес сервиса задаётся в настройках панели и у каждой установки свой.
   Раньше он был прошит в карточке клиента строкой «sub.stealthnet.app»,
   и скопированная ссылка вела в никуда. Если адрес не задан — говорим об
   этом прямо, а не выдаём заведомо неверный. */
function subLink(shortUuid){
  const base = (DB.subPublicUrl || '').trim();
  if (!base) return '';
  return base.replace(/\/+$/, '') + '/s/' + shortUuid;
}

/* ── layers: modals & drawers (stackable) ── */
const layers = [];
let panelFieldId = 0;
function enhancePanelUI(root){
  if (!root?.isConnected) return;
  root.querySelectorAll('.field').forEach(field=>{
    const label=field.querySelector('label');
    const control=field.querySelector('input:not([type=hidden]),select,textarea');
    if(label && control && !label.htmlFor && !label.contains(control)){
      if(!control.id) control.id='panel-field-'+(++panelFieldId);
      label.htmlFor=control.id;
    }
  });
  root.querySelectorAll('button[title]:not([aria-label])').forEach(button=>{
    if(!button.textContent.trim()) button.setAttribute('aria-label',button.title);
  });
  root.querySelectorAll('.tbl-wrap').forEach(box=>{
    if(!box.hasAttribute('tabindex')) box.tabIndex=0;
  });
  root.querySelectorAll('.tabs').forEach(tabs=>{
    if(tabs.dataset.panelEnhanced) return;
    tabs.dataset.panelEnhanced='true';
    tabs.setAttribute('role','group');
    if(!tabs.hasAttribute('aria-label')) tabs.setAttribute('aria-label','Вкладки раздела');
    const sync=()=>tabs.querySelectorAll('button').forEach(button=>button.setAttribute('aria-pressed',String(button.classList.contains('on'))));
    sync();
    tabs.addEventListener('click',()=>queueMicrotask(sync));
    tabs.addEventListener('keydown',event=>{
      if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
      const buttons=[...tabs.querySelectorAll('button:not([disabled])')];
      const index=buttons.indexOf(event.target);
      if(index<0) return;
      event.preventDefault();
      const next=event.key==='Home'?0:event.key==='End'?buttons.length-1:(index+(event.key==='ArrowRight'?1:-1)+buttons.length)%buttons.length;
      buttons[next].focus();buttons[next].click();
      buttons[next].scrollIntoView({block:'nearest',inline:'nearest'});
    });
  });
}
function setupDialogSections(pane){
  if(pane.classList.contains('sectioned-dialog')&&!pane.querySelector('.dialog-section-nav')){
    const scrollBody=pane.querySelector('.m-body');
    const bodyNode=scrollBody.querySelector('#pgBody')||scrollBody;
    const sections=[...bodyNode.children].filter(el=>el.classList.contains('form-section'));
    if(sections.length){
      const firstSection=document.createElement('section');firstSection.className='form-section';
      while(bodyNode.firstChild && bodyNode.firstChild!==sections[0]) firstSection.append(bodyNode.firstChild);
      if(firstSection.children.length||firstSection.textContent.trim()){
        const firstHeading=document.createElement('h4');firstHeading.textContent='Основное';firstSection.prepend(firstHeading);bodyNode.prepend(firstSection);sections.unshift(firstSection);
      }
      const nav=document.createElement('nav');nav.className='dialog-section-nav';nav.setAttribute('aria-label','Разделы формы');
      sections.forEach((section,i)=>{const button=document.createElement('button');button.type='button';button.textContent=section.querySelector('h4,summary')?.textContent||'Раздел '+(i+1);button.onclick=()=>{if(section.tagName==='DETAILS')section.open=true;section.scrollIntoView({block:'start',behavior:'auto'});};nav.append(button);});
      scrollBody.before(nav);
    }
  }
}
function _openLayer(kind, {title, sub, icon='settings', iconTone='', size='', className='', initialFocus='field', body, footer, onMount}){
  const host = document.getElementById('layers');
  const layer = document.createElement('div');
  layer.className = 'layer';
  const box = kind === 'drawer' ? `drawer ${size}` : `modal ${size}`;
  layer.innerHTML = `
    <div class="ovl"></div>
    <div class="${box}">
      <div class="m-head">
        <div class="m-ic ${iconTone}">${I(icon,16)}</div>
        <div style="min-width:0"><h3>${title}</h3>${sub?`<div class="m-sub">${sub}</div>`:''}</div>
        <button class="icon-btn x" data-close>${I('x',15)}</button>
      </div>
      <div class="m-body">${body}</div>
      ${footer !== undefined ? `<div class="m-foot">${footer}</div>` : ''}
    </div>`;
  host.appendChild(layer);
  layers.push(layer);
  const pane = layer.querySelector('.modal, .drawer');
  if(className) pane.classList.add(...className.split(/\s+/).filter(Boolean));
  pane.setAttribute('role', 'dialog');
  const heading = pane.querySelector('h3'); heading.id='dialog-'+Date.now()+'-'+layers.length;
  pane.setAttribute('aria-labelledby', heading.id);
  setupDialogSections(pane);
  layer._returnFocus=document.activeElement;
  layer.querySelector('[data-close]').setAttribute('aria-label','Закрыть окно');
  pane.setAttribute('aria-modal', 'true');
  requestAnimationFrame(()=>requestAnimationFrame(()=>layer.classList.add('in')));
  const close = () => closeLayer(layer);
  layer.querySelector('.ovl').addEventListener('click', close);
  layer.querySelectorAll('[data-close]').forEach(b=>b.addEventListener('click', close));
  if(onMount) Promise.resolve(onMount(layer, close)).then(()=>{enhancePanelUI(layer);setupDialogSections(pane);});
  enhancePanelUI(layer);

  const first = layer.querySelector('.m-body input:not([type=hidden]):not([disabled]), .m-body textarea, .m-body select');
  heading.tabIndex = -1;
  const focusTarget = initialFocus === 'title' || !first || first.readOnly ? heading : first;
  requestAnimationFrame(()=>{ if(layer.isConnected) focusTarget.focus({preventScroll:true}); });

  // Enter подтверждает — но только там, где подтверждение безобидно.
  // В окнах с опасным действием и в многострочных полях перевод строки
  // должен оставаться переводом строки.
  layer.addEventListener('keydown', e => {
    if (e.key !== 'Enter' || e.shiftKey || e.isComposing) return;
    const t = e.target;
    if (t && (t.tagName === 'TEXTAREA' || t.tagName === 'BUTTON' || t.isContentEditable)) return;
    const ok = layer.querySelector('.m-foot .btn.primary:not([disabled])');
    if (ok) { e.preventDefault(); ok.click(); }
  });

  // Табуляция не должна уводить за пределы окна: иначе фокус уходит в
  // страницу под ним, и человек печатает вслепую в чужое поле.
  layer.addEventListener('keydown', e => {
    if (e.key !== 'Tab') return;
    const items = [...layer.querySelectorAll(
      'a[href], button:not([disabled]), input:not([disabled]):not([type=hidden]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )].filter(el => el.offsetParent !== null);
    if (!items.length) return;
    const firstEl = items[0], lastEl = items[items.length - 1];
    if (e.shiftKey && document.activeElement === firstEl) { e.preventDefault(); lastEl.focus(); }
    else if (!e.shiftKey && document.activeElement === lastEl) { e.preventDefault(); firstEl.focus(); }
  });

  return {layer, close};
}
const openModal = opts => _openLayer('modal', opts);
const openDrawer = opts => _openLayer('drawer', opts);
function closeLayer(layer){
  const i = layers.indexOf(layer);
  if(i>-1) layers.splice(i,1);
  layer.classList.remove('in');
  if(layer._returnFocus?.isConnected) layer._returnFocus.focus({preventScroll:true});
  setTimeout(()=>layer.remove(), 240);
}
function closeAllLayers(){ [...layers].forEach(closeLayer); }
document.addEventListener('keydown', e=>{
  if(e.key==='Escape' && layers.length) closeLayer(layers[layers.length-1]);
  if((e.metaKey||e.ctrlKey) && e.key.toLowerCase()==='k'){ e.preventDefault(); openCmdK(); }
});
function confirmModal({title='Вы уверены?', text='', danger=true, okText='Подтвердить', onOk}){
  openModal({
    title, icon: danger?'alert':'help', iconTone: danger?'danger':'', size:'sm',
    body:`<p style="font-size:13px;color:var(--text-2);line-height:1.6">${text}</p>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn ${danger?'danger':'primary'}" data-ok>${okText}</button>`,
    onMount(layer, close){
      layer.querySelector('[data-ok]').addEventListener('click', ()=>{ close(); onOk && onOk(); });
    }
  });
}
function loaderModal(text='Выполняется…', doneMsg='Готово', ms=1400, after){
  const {layer, close} = openModal({
    title:'Подождите', icon:'clock', size:'sm',
    body:`<div class="loader-box"><div class="ring"></div><div style="font-size:13px;color:var(--text-2)">${text}</div></div>`,
    footer: undefined,
  });
  setTimeout(()=>{ close(); toast(doneMsg); after && after(); }, ms);
}

/* ── dropdown menus ── */
let openMenuEl = null;
function menu(anchor, items){
  killMenu();
  const m = document.createElement('div');
  m.className = 'menu';
  m.setAttribute('role','menu');
  m.addEventListener('keydown',e=>{const items=[...m.querySelectorAll('[role=menuitem]')],i=items.indexOf(document.activeElement);if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();items[(i+(e.key==='ArrowDown'?1:items.length-1))%items.length]?.focus();}if(e.key==='Enter'||e.key===' '){e.preventDefault();document.activeElement.click();}if(e.key==='Escape'){killMenu();anchor.focus();}});
  m.innerHTML = items.map(it=>{
    if(it === '-') return '<div class="sep"></div>';
    if(it.title) return `<div class="m-title">${it.title}</div>`;
    return `<div role="menuitem" tabindex="0" class="mi ${it.danger?'danger':''}" data-mi>${it.icon?I(it.icon,14):''}<span>${it.label}</span></div>`;
  }).join('');
  document.body.appendChild(m);
  const r = anchor.getBoundingClientRect();
  const mw = 230;
  let x = Math.min(r.left, innerWidth - mw - 12);
  let y = r.bottom + 6;
  m.style.left = x+'px'; m.style.top = y+'px';
  requestAnimationFrame(()=>{
    const mh = m.offsetHeight;
    if(y + mh > innerHeight - 10) m.style.top = Math.max(10, r.top - mh - 6)+'px';
    m.classList.add('in');
    m.querySelector('[role=menuitem]')?.focus();
  });
  const actions = items.filter(it=>it!=='-' && !it.title);
  m.querySelectorAll('[data-mi]').forEach((el,i)=>el.addEventListener('click', ()=>{ killMenu(); actions[i].onClick && actions[i].onClick(); }));
  openMenuEl = m;
  setTimeout(()=>document.addEventListener('click', killMenuOnce), 0);
}
function killMenu(){ if(openMenuEl){ openMenuEl.remove(); openMenuEl=null; document.removeEventListener('click', killMenuOnce);} }
function killMenuOnce(e){ if(openMenuEl && !openMenuEl.contains(e.target)) killMenu(); }

/* ── charts ── */
function sparkPath(data, w, h, pad=2){
  const min = Math.min(...data), max = Math.max(...data);
  const x = i => i/(data.length-1)*w;
  const y = v => h - pad - (v-min)/(max-min || 1)*(h - pad*2);
  return data.map((v,i)=>(i?'L':'M') + x(i).toFixed(1) + ' ' + y(v).toFixed(1)).join(' ');
}
function spark(data, w=100, h=28, color='var(--accent)', fill=false){
  const d = sparkPath(data, w, h, 3);
  const id = 'g'+Math.abs(data[0]*7919|0)+w;
  return `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" style="display:block">
    ${fill?`<defs><linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="color-mix(in srgb, var(--accent) 25%, transparent)"/><stop offset="1" stop-color="transparent"/></linearGradient></defs>
    <path d="${d} L ${w} ${h} L 0 ${h} Z" fill="url(#${id})"/>`:''}
    <path d="${d}" fill="none" stroke="${color}" stroke-width="1.6" stroke-linejoin="round"/></svg>`;
}
function barsChart(data, w, h, color='color-mix(in srgb, var(--accent) 35%, transparent)', rx=2.5){
  const max = Math.max(...data)*1.12;
  const bw = w/data.length*.55;
  return `<svg width="100%" height="${h}" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" style="display:block">` +
    data.map((v,i)=>{
      const bh = v/max*(h-6), x = i*(w/data.length) + (w/data.length-bw)/2;
      return `<rect x="${x.toFixed(1)}" y="${(h-bh).toFixed(1)}" width="${bw.toFixed(1)}" height="${bh.toFixed(1)}" rx="${rx}" fill="${color}"/>`;
    }).join('') + '</svg>';
}

/* ── router ── */
const PAGES = {};
const NAV = [];
function registerPage(p){ PAGES[p.id] = p; if(p.nav !== false) NAV.push(p); }
let currentPage = null;
let authed = false;   // до проверки токена считаем, что не вошли

function navigate(id){ location.hash = '#/' + id; }
function pageAllowed(id) {
  if (id==='team') return DB.admin?.role==='owner';
  if (id==='admin-profile') return true;
  if (DB.admin?.role !== 'support') return true;
  return ['home','users','support','nodes','nodes-metrics','nodes-stats','payments','tariffs','hwid-inspector','srh-inspector','sessions','torrent-reports','http-stats','login','404'].includes(id);
}
function route(){
  const id = (location.hash || '#/home').slice(2).split('?')[0] || 'home';
  if(!authed && id !== 'login'){ location.hash = '#/login'; return; }
  if (authed && !pageAllowed(id)) { navigate('home'); toast('Этот раздел недоступен вашей роли', 'info'); return; }
  const page = PAGES[id] || PAGES['404'];
  currentPage = page;
  try { renderShell(page); }
  catch(e) {
    console.error(e);
    renderShell({id:page.id,title:page.title,group:page.group,render:()=>`<div class="page-head"><h1>${esc(page.title)}</h1></div><div class="empty" role="alert"><b>Не удалось открыть раздел</b><p>${esc(e.message)}</p><button class="btn" onclick="refreshDB()">Повторить</button></div>`});
  }
  const tb = document.getElementById('themeBtn');
  if (tb) tb.addEventListener('click', toggleTheme);
  const lb = document.getElementById('langBtn');
  if (lb) lb.addEventListener('click', ()=>setLang(currentLang() === 'ru' ? 'en' : 'ru'));
  // Перевод накладываем после отрисовки: страницы написаны по-русски,
  // и переписывать полторы тысячи строк на вызовы t() значило бы
  // тронуть каждый экран ради одной задачи.
  translateNode(document.body);
}
function renderShell(page){
  const app = document.getElementById('app');
  killMenu(); closeAllLayers();
  /* Прокрутка бокового меню.

     Оболочка перерисовывается целиком на каждом переходе, и новое меню
     начинается сверху. Пункты нижней половины — инструменты, CRM,
     настройки — после нажатия уезжали из виду: человек щёлкал по
     «Торрент-репортам», а меню прыгало к «Командному центру», и чтобы
     перейти к соседнему пункту, приходилось прокручивать заново. */
  const прокрутка = document.querySelector('.side')?.scrollTop || 0;
  if(page.bare){
    app.className = 'app auth-mode';
    app.innerHTML = page.render();
    page.bind && page.bind(app);
    return;
  }
  app.className = 'app';
  app.innerHTML = `
    <aside class="side">
      <div class="logo" onclick="navigate('home')">
        ${logoMark(30)}
        <div><div class="logo-name">STEALTHNET</div><div class="logo-sub">CONTROL PANEL</div></div>
      </div>
      ${renderNav(page.id)}
      <div class="side-bottom">
        <div class="side-user" id="sideUser">
          <div class="avatar">${esc((DB.admin?.username||'A').slice(0,2).toUpperCase())}</div>
          <div style="flex:1"><b>${esc(DB.admin?.username||'Администратор')}</b><span>${esc(DB.admin?.role||'')}</span></div>
          ${I('chevD',13)}
        </div>
      </div>
    </aside>
    <header class="top">
      <button class="icon-btn mobile-nav-button" aria-label="Открыть меню" onclick="document.getElementById('app').classList.toggle('nav-open')">${I('menu',18)}</button>
      <div class="crumb">${page.group || 'Обзор'} ${I('chevR',12)} <b>${page.title}</b></div>
      <div class="search" onclick="openCmdK()">${I('search',14)} Поиск по панели… <span class="kbd">⌘K</span></div>
      <div class="env"><span class="dot"></span>${esc(location.hostname)}</div>
      <button class="panel-version" id="panelVersion" type="button" aria-label="Версия и обновления"></button>
      <button class="icon-btn" id="themeBtn" title="${isDark()?'Светлая тема':'Тёмная тема'}">${I(isDark()?'sun':'moon',16)}</button>
      <button class="icon-btn" id="langBtn" title="Язык интерфейса"
        style="font-family:var(--font-mono);font-size:11px;font-weight:600">${currentLang().toUpperCase()}</button>
      <button class="icon-btn" title="Инструкция по разделу" aria-label="Инструкция по разделу" onclick="sectionHelp()">${I('help',16)}</button>
      <button class="icon-btn" onclick="openNotifs(this)" title="Уведомления">${I('bell',16)}${notifItems().length ? '<span class="pip"></span>' : ''}</button>
    </header>
    <main class="main" id="main">${page.render()}</main>
    <footer class="statusbar">
      <span class="item"><span class="dot" style="background:${(PAGES._health?.db)?'var(--ok)':'var(--err)'}"></span>${(PAGES._health?.db)?'база отвечает':'нет связи с базой'}</span>
      <span class="item">клиентов <b>${fmtN(DB.dashboard?.clients?.total||0)}</b></span>
      <span class="item">нод онлайн <b>${DB.dashboard?.nodes?.online||0}/${DB.dashboard?.nodes?.total||0}</b></span>
      <span class="item">трафик за 30 дн <b>${(((DB.dashboard?.traffic?.month_bytes)||0)/1024**4).toFixed(2)} TiB</b></span>
      <div class="right">
        <span class="item">${PAGES._health?.brand||''} SOFTWARE <b>v${PAGES._health?.version||'—'}</b></span>
        <span class="item">${new Date().toLocaleString(currentLang(),{day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit'})}</span>
      </div>
    </footer>`;
  if (typeof PanelRelease !== 'undefined') PanelRelease.mount();
  document.getElementById('sideUser').addEventListener('click', e=>{
    menu(e.currentTarget, [
      {label:'Профиль администратора', icon:'user', onClick:()=>navigate('admin-profile')},
      ...(DB.admin?.role==='owner' ? [{label:currentLang()==='en'?'Team and access':'Команда и доступ',icon:'users2',onClick:()=>navigate('team')}] : []),
      ...(DB.admin?.role==='owner'||DB.admin?.role==='admin' ? [{label:'Настройки системы', icon:'settings', onClick:()=>navigate('settings')}] : []),
      '-',
      {label:'Выйти', icon:'logout', danger:true, onClick:async ()=>{ await API.logout(); API.setToken(null); authed=false; navigate('login'); toast('Вы вышли из системы','info'); }},
    ]);
  });
  // Возвращаем меню туда, где оно было. Делаем это до отрисовки
  // страницы: иначе на тяжёлых разделах меню успевает мигнуть сверху.
  const меню = document.querySelector('.side');
  if (меню) меню.scrollTop = прокрутка;

  const main=document.getElementById('main');
  const head=main.querySelector('.page-head');
  if (DB.admin?.role === 'readonly' || DB.admin?.role === 'support') {
    const note=document.createElement('div'); note.className='sub-note'; note.setAttribute('role','status');
    note.textContent=DB.admin.role==='readonly' ? 'Режим просмотра. Изменение данных недоступно; безопасность своего аккаунта можно настроить в профиле администратора.' : 'Роль поддержки: просмотр клиентов и сети, заметки, теги и ответы на обращения. Привязка аккаунтов, тарифы и лимиты доступны владельцу и администратору.';
    head?.after(note);
  }
  const failed=Object.entries(DB.loadErrors || {});
  if(failed.length){const note=document.createElement('div');note.className='sub-note';note.setAttribute('role','alert');note.textContent='Часть данных не загрузилась: '+failed.map(([path,error])=>path+' — '+error).join('; ')+'. ';const retry=document.createElement('button');retry.className='btn sm';retry.textContent='Повторить';retry.onclick=()=>refreshDB();note.append(retry);head?.after(note);}

  if(head && !head.querySelector('.section-help')){const help=document.createElement('button');help.className='btn section-help';help.innerHTML=I('help',16)+' Инструкция';help.title='Как пользоваться разделом';help.onclick=()=>sectionHelp(page.id);head.appendChild(help);}
  if(page.bind) Promise.resolve().then(()=>page.bind(main)).then(()=>enhancePanelUI(main)).catch(e=>{toast('Не удалось загрузить раздел: '+e.message,'err');console.error(e);});
  enhancePanelUI(main);
}
function renderNav(activeId){
  const groups = [];
  NAV.filter(p=>pageAllowed(p.id)).forEach(p=>{
    let g = groups.find(x=>x.name===p.group);
    if(!g){ g = {name:p.group, items:[]}; groups.push(g); }
    g.items.push(p);
  });
  return groups.map(g=>`
    <div class="nav-group">
      <div class="nav-title">${g.name}</div>
      ${g.items.map(p=>`
        <a class="nav-item ${p.id===activeId?'active':''}" href="#/${p.id}">
          ${I(p.icon,16)} ${p.title}
          ${p.badge ? `<span class="nav-badge ${p.badgeTone||''}">${typeof p.badge==='function'?p.badge():p.badge}</span>` : ''}
        </a>`).join('')}
    </div>`).join('');
}

/* ── global widgets ── */
function openCmdK(){
  openModal({title:'Быстрый поиск',icon:'search',size:'lg',
    body:'<label class="sr-only" for="ckInp">Поиск по панели</label><input class="inp" placeholder="Клиент, нода, платёж или раздел…" id="ckInp" autocomplete="off"><div id="ckRes" class="command-results" aria-live="polite"></div>',
    footer:'<span class="sub-note">⌘K — открыть · ↑ ↓ — выбрать результат · Enter — перейти</span>',
    onMount(layer,close){const input=layer.querySelector('#ckInp'),box=layer.querySelector('#ckRes');let timer,seq=0,actions=[];
      const draw=async()=>{const query=input.value.trim(),q=query.toLowerCase(),request=++seq;
        const pages=NAV.filter(p=>p.title.toLowerCase().includes(q)||p.id.includes(q)).slice(0,8);
        let result=pages.map(p=>({title:p.title,note:'Раздел панели',icon:p.icon||'chevR',action:()=>{close();navigate(p.id);}}));
        if(query.length>=2){box.textContent='Ищем…';
          result.push(...DB.nodes.filter(n=>[n.name,n.addr].join(' ').toLowerCase().includes(q)).slice(0,8).map(n=>({title:n.name,note:'Нода · '+n.addr,icon:'server',action:()=>{close();openNodeOverview(n);}})));
          const responses=await Promise.allSettled([API.call('/api/clients?'+new URLSearchParams({q:query,limit:8})),API.call('/api/payments?'+new URLSearchParams({search:query,limit:8,paginated:true}))]);
          if(request!==seq||!layer.isConnected)return;
          const [clients,payments]=responses;
          if(clients.status==='fulfilled')result.push(...clients.value.items.map(c=>({title:'@'+c.username,note:'Клиент · '+(c.tariff_code||'без тарифа'),icon:'user',action:()=>{close();openUserView({id:c.id});}})));
          if(payments.status==='fulfilled')result.push(...payments.value.items.map(raw=>{const p=mapPayment(raw);return {title:fmtMoney(p.amount,p.currency)+' · @'+p.user,note:'Платёж · '+p.method+' · '+fmtDT(p.at),icon:'card',action:()=>{close();payDetails(p);}};}));
          if(responses.some(r=>r.status==='rejected'))result.push({title:'Часть данных не загрузилась',note:'Измените запрос или повторите поиск.',icon:'alert'});
        }
        if(request!==seq||!layer.isConnected)return;actions=result;
        box.innerHTML=result.map((r,i)=>`<button class="command-result" data-result="${i}" ${r.action?'':'disabled'}>${I(r.icon,16)}<span><b>${esc(r.title)}</b><small>${esc(r.note)}</small></span>${r.action?I('chevR',13):''}</button>`).join('')||'<div class="empty"><b>Ничего не найдено</b><span>Попробуйте другое имя, адрес или номер платежа.</span></div>';
        box.querySelectorAll('[data-result]').forEach(b=>b.onclick=()=>actions[Number(b.dataset.result)].action?.());
      };
      input.oninput=()=>{++seq;clearTimeout(timer);timer=setTimeout(draw,180);};
      layer.addEventListener('keydown',e=>{const buttons=[...box.querySelectorAll('button:not([disabled])')];if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();const current=buttons.indexOf(document.activeElement);buttons[(current+(e.key==='ArrowDown'?1:buttons.length-1))%buttons.length]?.focus();}else if(e.key==='Enter'&&e.target===input){e.preventDefault();buttons[0]?.click();}});draw();
    }
  });
}
function openHelp(){
  const h = PAGES._health || {};
  const nodes = DB.dashboard?.nodes || {};
  const badNodes = (DB.nodes || []).filter(n => n.status === 'online' && n.engineOk === false).length;

  /* Состояние показываем только то, которое действительно проверено.
     Раньше здесь висел список из пяти строк с вечным «работает» — он
     выглядел как мониторинг, но ничего не проверял, и по нему нельзя
     было отличить рабочую систему от лежащей. */
  const row = (name, ok, note) => `
    <div class="info-row"><span class="k">${name}</span>
      <span class="v"><span class="bdg ${ok ? 'ok' : 'err'}"><span class="dot"></span>${ok ? 'работает' : 'не отвечает'}</span>
      ${note ? `<span class="sub-note">${note}</span>` : ''}</span></div>`;

  openDrawer({
    title:'Справка', sub:'Горячие клавиши, документация и состояние', icon:'book',
    body:`
      <div class="info-rows">
        <div class="info-row"><span class="k">Быстрый поиск</span><span class="v"><span class="kbd">⌘K</span></span></div>
        <div class="info-row"><span class="k">Закрыть окно</span><span class="v"><span class="kbd">Esc</span></span></div>
        <div class="info-row"><span class="k">Документация</span>
          <span class="v"><a class="link" href="https://stealthnet.software/" target="_blank" rel="noopener">stealthnet.software</a> ${I('external',12)}</span></div>
        <div class="info-row"><span class="k">Версия панели</span>
          <span class="v mono">v${esc(h.version || '—')}</span></div>
      </div>

      <div class="section-h"><h2>Состояние</h2><span>проверено сейчас</span></div>
      <div class="info-rows" id="helpState">
        ${row('API панели', h.status === 'ok')}
        ${row('База данных', !!h.db)}
        ${row('Ноды', (nodes.online || 0) > 0 && !badNodes,
              `${nodes.online || 0} из ${nodes.total || 0} на связи` +
              (badNodes ? `, на ${badNodes} не запустился движок` : ''))}
        <div class="info-row"><span class="k">Сервис подписок</span>
          <span class="v" id="helpSub"><span class="sub-note">проверяем…</span></span></div>
      </div>
      <div class="hint" style="margin-top:12px">${I('info',12)}
        Телеграм-бот и рассылки работают отдельными службами и своего адреса проверки не имеют —
        показывать их состояние здесь было бы догадкой.</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    onMount(layer){
      const cell = layer.querySelector('#helpSub');
      API.call('/api/dashboard/subscription-health')
        .then(r=>{if(!cell.isConnected)return;cell.innerHTML=r.status==='ready'
          ? '<span class="bdg ok"><span class="dot"></span>работает</span>'
          : r.status==='unconfigured'?'<span class="sub-note">адрес не задан в настройках</span>'
          : '<span class="bdg err"><span class="dot"></span>не отвечает</span>'+(r.http_status?'<span class="sub-note">HTTP '+Number(r.http_status)+'</span>':'');})
        .catch(()=>{if(cell.isConnected)cell.innerHTML='<span class="sub-note">Не удалось выполнить проверку. Откройте справку ещё раз.</span>';});
    },
  });
}
/* Уведомления собираются из состояния системы, а не из заготовленного
   списка: раньше здесь всегда висели одни и те же три строки с чужими
   именами, и по ним нельзя было понять, что происходит на самом деле. */
function notifItems(){
  const out = [];

  const dead = (DB.nodes || []).filter(n => n.status !== 'online' && n.status !== 'disabled');
  dead.slice(0, 3).forEach(n => out.push({
    label: `Нода ${n.name} не выходит на связь · ${fmtAgo(n.lastSeen)}`,
    icon: 'alert', onClick: () => navigate('nodes'),
  }));

  // Движок лёг, а агент отвечает — для клиента это такой же простой.
  (DB.nodes || []).filter(n => n.status === 'online' && n.engineOk === false)
    .slice(0, 3).forEach(n => out.push({
      label: `На ноде ${n.name} не запустился движок`,
      icon: 'alert', onClick: () => navigate('nodes'),
    }));

  const open = (DB.tickets || []).filter(t => t.status !== 'closed').length;
  if (open) out.push({
    label: `Открытых обращений: ${fmtN(open)}`, icon: 'chat', onClick: () => navigate('support'),
  });

  const failed = (DB.payments || []).filter(p => p.status === 'failed').length;
  if (failed) out.push({
    label: `Неуспешных платежей: ${fmtN(failed)}`, icon: 'card', onClick: () => navigate('payments'),
  });

  // Кому продлевать в ближайшие дни — это то, ради чего в панель заходят.
  const soon = (DB.users || []).filter(u => {
    if (!u.paidUntil) return false;
    const d = (new Date(u.paidUntil) - Date.now()) / 86400000;
    return d >= 0 && d <= 3;
  }).length;
  if (soon) out.push({
    label: `Подписок истекает за 3 дня: ${fmtN(soon)}`, icon: 'clock', onClick: () => navigate('users'),
  });

  return out;
}

function openNotifs(anchor){
  const items = notifItems();
  menu(anchor, [
    {title:'Уведомления'},
    ...(items.length ? items : [{label:'Всё спокойно — событий нет', icon:'check', onClick:()=>{}}]),
  ]);
}

/* ── boot ── */
function boot(){
  window.addEventListener('hashchange', route);
  if(!location.hash) location.hash = '#/home';
  route();
}
