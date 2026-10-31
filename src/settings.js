const KEY = 'autolyrics.settings';

const defaults = {
  /** Язык интерфейса (null — ещё не выбран, идёт первоначальная настройка). */
  language: null,
  /** Первоначальная настройка пройдена. */
  onboarded: false,
  /** Liquid Glass (преломление) у кнопок; выключено — обычное матовое стекло. */
  glass: true,
  fontScale: 1,
  soft: 1,
  alwaysOnTop: false,
  /** Определять трек по звуку, если плеер не сообщает его системе. */
  audioRecognition: true,
  /** Суммарное время работы программы, мс (кнопка пожертвования появляется после суток). */
  usageMs: 0,
  /** Пользователь подтвердил «не показывать больше» — кнопки пожертвования нет. */
  donateHidden: false,
  /** Заранее угадывать следующий трек альбома (заготовка + текст). */
  predictNext: false,
  /** Стиль визуализатора: auto (по жанру) | electronic | hiphop | rock | pop | chill | classical | default | off. */
  vizStyle: 'auto',
  vizIntensity: 1,
  /** Этапы анимации заставки: поиск, появление трека, переезд вверх при найденном тексте. */
  animSearch: true,
  animTrack: true,
  animDock: true,
  /** Ключ официального API Musixmatch (необязательно). */
  musixmatchKey: '',
  /** Ручной сдвиг синхронизации по трекам, мс. */
  offsets: {},
};

function load() {
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) || '{}') };
  } catch {
    return { ...defaults };
  }
}

export const settings = load();

export function saveSettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    /* хранилище недоступно — настройки просто не сохранятся */
  }
}
