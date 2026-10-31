import { STRINGS } from './strings.js';

export { detectLanguage } from './detect.js';

/** Языки интерфейса; название — на самом языке. */
export const LANGUAGES = [
  { code: 'ru', native: 'Русский' },
  { code: 'en', native: 'English' },
  { code: 'de', native: 'Deutsch' },
  { code: 'fr', native: 'Français' },
  { code: 'pl', native: 'Polski' },
  { code: 'zh', native: '中文' },
  { code: 'ja', native: '日本語' },
];

let current = 'en';
const listeners = new Set();

export const isSupported = (code) => LANGUAGES.some((l) => l.code === code);

/** Язык системы, если он поддерживается, иначе английский. */
export function systemLanguage() {
  const code = (navigator.language || 'en').slice(0, 2).toLowerCase();
  return isSupported(code) ? code : 'en';
}

export const getLanguage = () => current;

export function nativeName(code = current) {
  return LANGUAGES.find((l) => l.code === code)?.native ?? code;
}

export function setLanguage(code) {
  if (!isSupported(code)) return;
  current = code;
  document.documentElement.lang = code;
  for (const listener of listeners) listener(code);
}

/** Подписка на смену языка; возвращает функцию отписки. */
export function onLanguageChange(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Строка на текущем языке (или на указанном) с подстановкой `{имя}`. */
export function t(key, params, lang = current) {
  const entry = STRINGS[key];
  const text = entry?.[lang] ?? entry?.en ?? key;
  return params ? text.replace(/\{(\w+)\}/g, (_, name) => params[name] ?? '') : text;
}
