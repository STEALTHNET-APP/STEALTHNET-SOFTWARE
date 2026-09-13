/* Тема оформления: светлая по умолчанию, тёмная по выбору.
 *
 * Файл подключается первым и до отрисовки: если выставлять атрибут
 * позже, человек с тёмной темой успевает увидеть вспышку белого экрана.
 * Пока выбор не сделан, атрибута нет вовсе — тогда решает настройка
 * системы через prefers-color-scheme в таблице стилей.
 */
'use strict';

const THEME_KEY = 'sn_theme';

function storedTheme(){
  try { return localStorage.getItem(THEME_KEY); } catch (_) { return null; }
}

function applyTheme(t){
  const root = document.documentElement;
  root.setAttribute('data-theme', t === 'dark' ? 'dark' : 'light');
  // Системные части окна — полосы прокрутки, поле ввода браузера —
  // красятся по этому свойству. Без него светлая панель получает
  // тёмную полосу прокрутки, и это сразу заметно.
  root.style.colorScheme = isDark() ? 'dark' : 'light';
}

/* По умолчанию светлая: панель для работы днём, и тёмная — выбор, а не
   догадка по настройке операционной системы. */
function isDark(){ return storedTheme() === 'dark'; }

function setTheme(t){
  try { localStorage.setItem(THEME_KEY, t); } catch (_) {}
  applyTheme(t);
  // Перерисовываем шапку, чтобы значок сменился на противоположный.
  if (typeof route === 'function') route();
}

function toggleTheme(){ setTheme(isDark() ? 'light' : 'dark'); }

applyTheme(storedTheme());
