/* Тема оформления: системная по умолчанию (светлая/тёмная), с возможностью ручного переключения.
 *
 * Файл подключается первым и до отрисовки: если выставлять атрибут
 * позже, человек с тёмной темой успевает увидеть вспышку белого экрана.
 * Пока выбор не сделан вручную, решает настройка системы через prefers-color-scheme.
 */
'use strict';

const THEME_KEY = 'sn_theme';

function storedTheme(){
  try { return localStorage.getItem(THEME_KEY); } catch (_) { return null; }
}

function prefersDark(){
  try { return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches; } catch (_) { return false; }
}

function isDark(){
  const s = storedTheme();
  if (s === 'dark') return true;
  if (s === 'light') return false;
  return prefersDark();
}

function applyTheme(t){
  const root = document.documentElement;
  const dark = t === 'dark' || (t !== 'light' && prefersDark());
  root.setAttribute('data-theme', dark ? 'dark' : 'light');
  root.style.colorScheme = dark ? 'dark' : 'light';
}

function setTheme(t){
  try { localStorage.setItem(THEME_KEY, t); } catch (_) {}
  applyTheme(t);
  // Перерисовываем шапку, чтобы значок сменился на противоположный.
  if (typeof route === 'function') route();
}

function toggleTheme(){ setTheme(isDark() ? 'light' : 'dark'); }

applyTheme(storedTheme());

try {
  if (window.matchMedia) {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (!storedTheme()) {
        applyTheme(null);
        if (typeof route === 'function') route();
      }
    });
  }
} catch (_) {}

