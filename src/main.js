import { Background } from './render/background.js';
import { DEFAULT_PALETTE, paletteFromImage } from './render/palette.js';
import { settings, saveSettings } from './settings.js';
import { LyricsView } from './ui/lyrics.js';
import { Menu } from './ui/menu.js';
import { Splash } from './ui/splash.js';
import { Visualizer } from './render/visualizer.js';
import { STYLE_CYCLE, styleFromFeatures, styleFromGenre } from './render/genre.js';
import { isSupported, onLanguageChange, setLanguage, systemLanguage, t } from './i18n/index.js';
import { LanguagePicker } from './ui/language.js';
import { Onboarding } from './ui/onboarding.js';
import * as glass from './ui/glass.js';
import { DONATE_AFTER_MS, DONATE_URL } from './config.js';
import { DonateDialog } from './ui/donate.js';

const tauri = window.__TAURI__;
const demo = !tauri; // открыли index.html в обычном браузере — показываем демо
const demoParams = new URLSearchParams(location.search);
if (demo) {
  // ?viz=rock — посмотреть конкретный стиль визуализатора в демо
  const forced = demoParams.get('viz');
  if (forced) settings.vizStyle = forced;
  // ?ob=1 — пройти первоначальную настройку заново; ?lang=de — язык интерфейса
  settings.onboarded = !demoParams.get('ob');
  const lang = demoParams.get('lang');
  if (lang && isSupported(lang)) settings.language = lang;
}

const DEMO_TRACK = { id: 1, title: 'Demo Track', artist: 'AutoLyrics', album: '', durationMs: 60000, cover: null, source: 'demo' };
const demoOnboarding = demo && !settings.onboarded;

// Тексты для проверки определения языка (?lyrics=de и т. д.)
const DEMO_LYRICS = {
  ru: ['Мягкий свет на стенах дома', 'Цвета плывут как облака', 'Каждая нота — как волна', 'Музыка рядом, и ты не один', 'Слушай, как дышит рассвет', 'Тихо, но ярко звучит тишина'],
  en: ['I will hold you in the light of the morning', 'And you know that I am here with you', 'All the colors are moving in the sky', 'Do not be afraid of what is coming', 'We are the ones that make the music', 'Close your eyes and feel it with me'],
  de: ['Ich halte dich im Licht des Morgens', 'Und du weißt, dass ich bei dir bin', 'Alle Farben ziehen über den Himmel', 'Hab keine Angst vor dem, was kommt', 'Wir sind es, die die Musik machen', 'Mach die Augen zu und fühl es mit mir'],
  fr: ['Je te tiens dans la lumière du matin', 'Et tu sais que je suis là avec toi', 'Toutes les couleurs glissent dans le ciel', 'N’aie pas peur de ce qui arrive', 'C’est nous qui faisons la musique', 'Ferme les yeux et sens-le avec moi'],
  pl: ['Trzymam cię w świetle poranka', 'I wiesz, że jestem tu z tobą', 'Wszystkie kolory płyną po niebie', 'Nie bój się tego, co nadchodzi', 'To my tworzymy tę muzykę', 'Zamknij oczy i poczuj to ze mną'],
  zh: ['我在清晨的光里抱着你', '你知道我就在你身边', '所有的颜色在天空中流动', '不要害怕即将到来的一切', '是我们在创造这首音乐', '闭上眼睛和我一起感受'],
  ja: ['朝の光の中であなたを抱きしめる', 'あなたのそばにいることを知っているでしょう', 'すべての色が空を流れていく', 'これから来るものを怖がらないで', '音楽を作るのは私たちだから', '目を閉じて一緒に感じてみて'],
};

// Язык интерфейса: выбранный, а до выбора — язык системы.
setLanguage(settings.language && isSupported(settings.language) ? settings.language : systemLanguage());

const invoke = tauri ? tauri.core.invoke : demoInvoke;
const listen = tauri ? tauri.event.listen : () => Promise.resolve(() => {});
const win = tauri ? tauri.window.getCurrentWindow() : null;

const background = new Background(document.getElementById('bg'));
// Визуализатор берёт цвета у фона и «качает» фон в такт.
const viz = new Visualizer(
  document.getElementById('viz'),
  () => background.current,
  (s) => background.setEnergy(Math.min(1, s.bass * 0.7 + s.beatEnv * 0.6)),
);
viz.setIntensity(settings.vizIntensity);

/* ---------- Язык, Liquid Glass, первый запуск ---------- */

glass.setEnabled(settings.glass);
const decorate = (root) => glass.decorate(root);

function changeLanguage(code) {
  setLanguage(code);
  settings.language = code;
  saveSettings();
}

const picker = new LanguagePicker(
  document.getElementById('lang-picker'),
  (code) => (onboarding.active ? onboarding.finish(code) : changeLanguage(code)),
  decorate,
);
const onboarding = new Onboarding(document.getElementById('onboarding'), {
  openPicker: () => picker.open(),
  onDone: (code) => {
    changeLanguage(code);
    settings.onboarded = true;
    saveSettings();
    menu.disabled = false;
  },
  decorate,
});

const donateDialog = new DonateDialog(document.getElementById('donate-dialog'), {
  onOpen: () => invoke('open_support_link', { url: DONATE_URL }).catch(() => {}),
  onHide: () => {
    settings.donateHidden = true;
    saveSettings();
  },
  decorate,
});

// Суммарное время работы программы (для кнопки пожертвования). Сон и гибернацию не считаем:
// за один шаг засчитывается не больше полутора минут.
let lastUsageTick = Date.now();
setInterval(() => {
  const now = Date.now();
  settings.usageMs += Math.min(now - lastUsageTick, 90_000);
  lastUsageTick = now;
  saveSettings();
}, 30_000);

const stageEl = document.getElementById('stage');
const lyrics = new LyricsView(document.getElementById('lyrics'));
const splash = new Splash(stageEl, () => ({
  search: settings.animSearch,
  track: settings.animTrack,
  dock: settings.animDock,
}));

/* ---------- Состояние ---------- */

let track = null; // последний трек из SMTC
let lyricsState = { found: false, synced: [], plain: null, instrumental: false };
let fetchSeq = 0; // защита от «гонки» ответов при быстрой смене трека
let fetchedWithDuration = 0;
let usingApproxLines = false;
/** Песни, найденные распознаванием по звуку (для выбора другого варианта). */
let candidates = { items: [], selected: 0 };
/** Часы воспроизведения: позиция на момент `at` + прошедшее время, пока играет. */
const clock = { pos: 0, at: performance.now(), playing: false };

const trackKey = (t) => `${t.artist}|${t.title}`.toLowerCase();
const currentOffset = () => (track ? settings.offsets[trackKey(track)] || 0 : 0);
const position = () =>
  clock.pos + (clock.playing ? performance.now() - clock.at : 0) + currentOffset();

/* ---------- Стиль визуализатора ---------- */

/** Что известно о жанре текущего трека и звуке (стиль «авто» выбирается из этого). */
const vizInfo = {
  genre: null,
  genreStyle: null,
  featureStyle: null,
  pendingFeature: null,
  features: null,
  /** Пока жанр нового трека не известен, остаётся стиль прошлого — без мигания. */
  fallback: 'default',
};
const genreCache = new Map(); // «артист|название» → жанр или null

const autoStyle = () => vizInfo.genreStyle || vizInfo.featureStyle || vizInfo.fallback;

function currentStyle() {
  return settings.vizStyle === 'auto' ? autoStyle() : settings.vizStyle;
}

function applyViz() {
  const style = currentStyle();
  viz.setEnabled(style !== 'off');
  if (style !== 'off') viz.setStyle(style);
}

function resetVizInfo(genre = null, hasTrack = true) {
  vizInfo.fallback = hasTrack ? autoStyle() : 'default';
  vizInfo.genre = genre;
  vizInfo.genreStyle = styleFromGenre(genre);
  vizInfo.featureStyle = null;
  vizInfo.pendingFeature = null;
  applyViz();
}

/** Жанр от Shazam уже в треке; для остальных спрашиваем iTunes. */
function lookUpGenre(current) {
  if (current.genre) return;
  const key = trackKey(current);
  if (genreCache.has(key)) {
    resetVizInfo(genreCache.get(key));
    return;
  }
  invoke('fetch_genre', { title: current.title, artist: current.artist })
    .then((genre) => {
      genreCache.set(key, genre || null);
      if (track === current) resetVizInfo(genre || null);
    })
    .catch(() => {});
}

// Жанр неизвестен — угадываем по звуку (темп, бас, тембр). Меняем стиль, только если
// две проверки подряд (раз в 4 с) дали одно и то же — чтобы не метался.
setInterval(() => {
  if (!track || vizInfo.genreStyle) return;
  const suggestion = styleFromFeatures(vizInfo.features);
  if (suggestion && suggestion === vizInfo.pendingFeature && suggestion !== vizInfo.featureStyle) {
    vizInfo.featureStyle = suggestion;
    applyViz();
  }
  vizInfo.pendingFeature = suggestion;
}, 4000);

function describeStyle() {
  const style = currentStyle();
  if (style === 'off') return t('menu.vizOffLong');
  const name = t(`style.${style}`);
  if (settings.vizStyle !== 'auto') return t('menu.styleLine', { style: name });
  if (vizInfo.genreStyle) return t('menu.styleGenre', { style: name, genre: vizInfo.genre });
  if (vizInfo.featureStyle) return t('menu.styleAudio', { style: name });
  return t('menu.styleLine', { style: name });
}

/* ---------- Реакция на события ---------- */

function applyTrack(next) {
  if (!next) {
    track = null;
    fetchSeq++;
    lyrics.clear();
    stageEl.classList.remove('has-dock');
    splash.reset();
    background.setPalette(DEFAULT_PALETTE);
    resetVizInfo(null, false);
    onboarding.onNoTrack();
    return;
  }
  track = next;
  viz.setSearching(false);
  onboarding.onTrack(next);
  resetVizInfo(next.genre || null);
  lookUpGenre(next);
  clock.pos = 0;
  clock.at = performance.now();
  lyrics.clear();
  stageEl.classList.remove('has-dock');
  background.setPalette(DEFAULT_PALETTE);
  if (next.cover) applyCover(next.cover);
  // заставка появляется с анимацией; текст ждёт её окончания, прежде чем «заехать» вверх
  loadLyrics(false, splash.showTrack(next));
}

function applyCover(cover) {
  paletteFromImage(cover)
    .then((p) => background.setPalette(p))
    .catch(() => {});
}

function applyPlayback(pb) {
  if (!track || pb.trackId !== track.id) return;
  clock.pos = pb.positionMs;
  clock.at = performance.now();
  clock.playing = pb.playing;

  if (pb.durationMs && pb.durationMs !== track.durationMs) {
    track.durationMs = pb.durationMs;
    // Длительность стала известна позже — пробуем снова, если текст не нашли;
    // у несинхронного текста пересчитываем условные тайминги.
    if (!lyricsState.found && fetchedWithDuration === 0) loadLyrics();
    else if (usingApproxLines) showLyrics();
  }
}

/**
 * @param revealed промис окончания анимации появления заставки: результат
 *                 показываем только после неё, чтобы анимации не наезжали друг на друга
 */
async function loadLyrics(force = false, revealed = Promise.resolve()) {
  const seq = ++fetchSeq;
  const current = track;
  fetchedWithDuration = current.durationMs || 0;
  lyricsState = { found: false, synced: [], plain: null, instrumental: false };
  lyrics.clear();
  stageEl.classList.remove('has-dock');
  splash.undock();
  let settled = false;
  revealed.then(() => {
    if (seq === fetchSeq && !settled) splash.setNote(t('note.searching'));
  });

  try {
    const result = await invoke('fetch_lyrics', {
      title: current.title,
      artist: current.artist,
      album: current.album,
      durationMs: current.durationMs || 0,
      force,
    });
    if (seq !== fetchSeq) return;
    settled = true;
    lyricsState = result;
    onboarding.onLyrics(result); // первый запуск: по тексту определяем язык
    // Длительность от источника текста — по ней при распознавании по звуку
    // понимаем, что трек закончился, и сразу определяем следующий.
    if (current.source === 'audio' && result.durationMs > 0) {
      invoke('set_track_duration', { trackId: current.id, durationMs: result.durationMs });
    }
    await revealed;
    if (seq !== fetchSeq) return;
    showLyrics();
  } catch (err) {
    if (seq !== fetchSeq) return;
    settled = true;
    console.error(err);
    onboarding.onLyrics(null);
    await revealed;
    if (seq === fetchSeq) splash.setNote(t('note.loadError'));
  }
}

/** Текст найден: заставка уезжает наверх, строки занимают остальное место. */
function showLines(lines, options) {
  splash.setNote('');
  splash.dock();
  stageEl.classList.toggle('has-dock', settings.animDock);
  lyrics.setLines(lines, options);
}

function showLyrics() {
  const { found, synced, plain, instrumental } = lyricsState;
  usingApproxLines = false;
  if (!found) {
    splash.setNote(t('note.notFound'));
  } else if (instrumental) {
    splash.setNote(t('note.instrumental'));
  } else if (synced.length) {
    showLines(synced);
  } else if (plain) {
    // Синхронизации нет — равномерно растягиваем строки по длительности трека.
    const rows = plain.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    const total = (track.durationMs || 180000) * 0.92;
    usingApproxLines = true;
    showLines(
      rows.map((text, i) => ({ t: Math.round((i / rows.length) * total), text })),
      { approx: true },
    );
  }
}

/* ---------- Окно ---------- */

async function toggleFullscreen() {
  if (win) await win.setFullscreen(!(await win.isFullscreen()));
  else if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen();
}

// Левая кнопка — перетаскивание окна, двойной клик — полный экран.
// (startDragging «съедает» dblclick, поэтому двойной клик ловим вручную.)
let lastDown = { t: 0, x: 0, y: 0 };
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0 || e.target.closest('#menu')) return;
  const dbl =
    e.timeStamp - lastDown.t < 350 &&
    Math.hypot(e.clientX - lastDown.x, e.clientY - lastDown.y) < 6;
  lastDown = { t: dbl ? 0 : e.timeStamp, x: e.clientX, y: e.clientY };
  if (dbl) toggleFullscreen();
  else if (win) win.startDragging();
});

// Курсор прячется, если мышь неподвижна.
let idleTimer;
const wake = () => {
  document.body.classList.remove('idle');
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => document.body.classList.add('idle'), 3000);
};
document.addEventListener('mousemove', wake);
wake();

/* ---------- Меню ---------- */

lyrics.setFontScale(settings.fontScale);
background.setSoft(settings.soft);
if (win && settings.alwaysOnTop) win.setAlwaysOnTop(true);

const menu = new Menu(document.getElementById('menu'), {
  getTrack: () => track,
  getCandidates: () => candidates,
  getVizMode: () => settings.vizStyle,
  getVizInfo: describeStyle,
  cycleViz: () => {
    const next = STYLE_CYCLE[(STYLE_CYCLE.indexOf(settings.vizStyle) + 1) % STYLE_CYCLE.length];
    settings.vizStyle = next;
    saveSettings();
    applyViz();
  },
  getVizIntensity: () => settings.vizIntensity,
  setVizIntensity: (value) => {
    settings.vizIntensity = value;
    viz.setIntensity(value);
    saveSettings();
  },
  isAnim: (key) => settings[key],
  toggleAnim: (key) => {
    settings[key] = !settings[key];
    saveSettings();
  },
  openLyricsDir: () => invoke('open_lyrics_dir').catch(() => {}),
  getMusixmatchKey: () => settings.musixmatchKey,
  setMusixmatchKey: (key) => {
    settings.musixmatchKey = key;
    invoke('set_musixmatch_key', { key });
    saveSettings();
  },
  // ключ введён до конца — если текста нет, пробуем снова уже с ним
  commitMusixmatchKey: () => {
    if (track && !lyricsState.found) loadLyrics(true);
  },
  getLyricsSource: () => {
    if (!lyricsState.found || lyricsState.instrumental) return '';
    const { source, origin } = lyricsState;
    if (origin === 'file') return t('origin.file');
    return origin === 'folder' || origin === 'partial' ? `${source} (${t(`origin.${origin}`)})` : source;
  },
  // кнопка пожертвования: только после суток работы программы и пока не нажато «не показывать больше»
  donateVisible: () =>
    !!DONATE_URL && !settings.donateHidden && (settings.usageMs >= DONATE_AFTER_MS || (demo && !!demoParams.get('donate'))),
  openDonate: () => donateDialog.open(),
  isAutostart: () => autostart,
  toggleAutostart: () => {
    autostart = !autostart;
    invoke('set_autostart', { enabled: autostart }).catch(() => (autostart = !autostart));
  },
  isPredict: () => settings.predictNext,
  togglePredict: () => {
    settings.predictNext = !settings.predictNext;
    invoke('set_predict_next', { enabled: settings.predictNext });
// Автозапуск живёт в системе (реестр), а не в настройках — спрашиваем, как есть на самом деле.
let autostart = false;
invoke('get_autostart').then((on) => (autostart = !!on)).catch(() => {});
// Подписи в меню трея — на языке интерфейса.
const syncTrayLabels = () => invoke('set_tray_labels', { show: t('tray.show'), quit: t('tray.quit') }).catch(() => {});
syncTrayLabels();
onLanguageChange(syncTrayLabels);
    saveSettings();
  },
  pickLanguage: () => picker.open(),
  isGlass: () => settings.glass,
  toggleGlass: () => {
    settings.glass = !settings.glass;
    glass.setEnabled(settings.glass);
    saveSettings();
  },
  decorate,
  selectCandidate: (index) => invoke('select_candidate', { index }),
  recognizeNow: () => invoke('recognize_now'),
  isAudio: () => settings.audioRecognition,
  toggleAudio: () => {
    settings.audioRecognition = !settings.audioRecognition;
    invoke('set_audio_recognition', { enabled: settings.audioRecognition });
    saveSettings();
  },
  getOffsetMs: currentOffset,
  shiftOffset: (delta) => {
    if (!track) return;
    const key = trackKey(track);
    const next = (settings.offsets[key] || 0) + delta;
    if (next === 0) delete settings.offsets[key];
    else settings.offsets[key] = next;
    saveSettings();
  },
  fontDelta: (delta) => {
    settings.fontScale = Math.min(1.8, Math.max(0.6, +(settings.fontScale + delta).toFixed(2)));
    lyrics.setFontScale(settings.fontScale);
    saveSettings();
  },
  getSoft: () => settings.soft,
  setSoft: (value) => {
    settings.soft = value;
    background.setSoft(value);
    saveSettings();
  },
  refetch: () => track && loadLyrics(true),
  isTop: () => settings.alwaysOnTop,
  toggleTop: async () => {
    settings.alwaysOnTop = !settings.alwaysOnTop;
    if (win) await win.setAlwaysOnTop(settings.alwaysOnTop);
    saveSettings();
  },
  toggleFullscreen,
  minimize: () => invoke('hide_window'), // в трей
  close: () => (win ? win.close() : window.close()),
});

/* ---------- Запуск ---------- */

applyViz();

function tick() {
  lyrics.update(position());
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

listen('track', (e) => applyTrack(e.payload));
listen('track_cover', (e) => {
  if (track && e.payload.id === track.id) {
    track.cover = e.payload.cover;
    splash.setCover(track.cover);
    applyCover(track.cover);
  }
});
listen('playback', (e) => applyPlayback(e.payload));
listen('spectrum', (e) => {
  viz.feed(e.payload);
  vizInfo.features = e.payload;
});
// Короткие сообщения от распознавания («Трек подтверждён», «Не удалось распознать»…).
const toastEl = document.getElementById('toast');
let toastTimer;
function showToast(text) {
  toastEl.textContent = text;
  toastEl.classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toastEl.classList.add('hidden'), 3500);
}
const NOTICE_KEYS = {
  smtc: 'notice.smtc',
  disabled: 'notice.disabled',
  silent: 'notice.silent',
  recognizing: 'rec.recognizing',
  nomatch: 'rec.nomatch',
  noconn: 'rec.error',
  verifying: 'notice.verifying',
  confirmed: 'notice.confirmed',
  found: 'notice.found',
};
listen('notice', (e) => {
  const { code, artist, title } = e.payload;
  showToast(t(NOTICE_KEYS[code] || code, { artist, title }));
});

listen('candidates', (e) => {
  candidates = e.payload;
  if (menu.isOpen) menu.renderCandidates();
});

// Пока трек не определён, коротко сообщаем, что происходит.
const RECOGNITION_KEYS = {
  listening: 'rec.listening',
  recognizing: 'rec.recognizing',
  nomatch: 'rec.nomatch',
  error: 'rec.error',
};
function onRecognition(state) {
  const key = RECOGNITION_KEYS[state];
  // пока трек ищется, визуализатор сглажен и размыт, как фон
  viz.setSearching(!track && !!key);
  if (track) return;
  if (key) splash.search(t(key));
  else splash.reset();
}
listen('recognition', (e) => onRecognition(e.payload.state));

// Первый запуск: приветствие на всех языках и определение языка по песне пользователя.
if (!settings.onboarded) {
  menu.disabled = true;
  onboarding.start();
}

invoke('set_audio_recognition', { enabled: settings.audioRecognition });
invoke('set_musixmatch_key', { key: settings.musixmatchKey });
invoke('set_predict_next', { enabled: settings.predictNext });
// Автозапуск живёт в системе (реестр), а не в настройках — спрашиваем, как есть на самом деле.
let autostart = false;
invoke('get_autostart').then((on) => (autostart = !!on)).catch(() => {});
// Подписи в меню трея — на языке интерфейса.
const syncTrayLabels = () => invoke('set_tray_labels', { show: t('tray.show'), quit: t('tray.quit') }).catch(() => {});
syncTrayLabels();
onLanguageChange(syncTrayLabels);
invoke('get_candidates').then((view) => (candidates = view));

// Окно могло открыться позже первых событий — забираем последнее состояние.
invoke('current_state').then((snap) => {
  if (snap.track && !track) applyTrack(snap.track);
  if (snap.playback) applyPlayback(snap.playback);
});

/* ---------- Демо-режим (index.html в обычном браузере) ---------- */

async function demoInvoke(cmd) {
  if (cmd === 'get_candidates') {
    return {
      items: [
        { key: 'a', title: 'Demo Track', artist: 'AutoLyrics', coverUrl: null },
        { key: 'b', title: 'Demo Track (Sped Up)', artist: 'AutoLyrics', coverUrl: null },
        { key: 'c', title: 'Another Song', artist: 'Someone Else', coverUrl: null },
      ],
      selected: 0,
    };
  }
  if (cmd === 'fetch_genre') return 'Hip-Hop/Rap';
  if (cmd === 'get_autostart') return false;
  if (['set_audio_recognition', 'recognize_now', 'select_candidate', 'set_musixmatch_key', 'open_lyrics_dir', 'set_track_duration', 'set_predict_next', 'set_tray_labels', 'hide_window', 'set_autostart', 'open_support_link'].includes(cmd)) return null;
  if (cmd === 'current_state') {
    return {
      track: demoOnboarding ? null : DEMO_TRACK,
      playback: demoOnboarding ? null : { trackId: 1, positionMs: 0, durationMs: 60000, playing: true },
    };
  }
  if (demoParams.get('lyrics') === 'none') {
    return { found: false, source: '', origin: '', instrumental: false, plain: null, synced: [] };
  }
  const lines = DEMO_LYRICS[demoParams.get('lyrics') || 'ru'] || DEMO_LYRICS.ru;
  return {
    found: true,
    source: 'demo',
    origin: '',
    instrumental: false,
    plain: null,
    synced: lines.map((text, i) => ({ t: i * 4000, text })),
  };
}

if (demoOnboarding) {
  // Первый запуск в демо: поиск → трек найден → текст → язык определён.
  onRecognition('listening');
  setTimeout(() => onRecognition('recognizing'), 5500);
  setTimeout(() => {
    applyTrack(DEMO_TRACK);
    applyPlayback({ trackId: 1, positionMs: 0, durationMs: 60000, playing: true });
  }, 7000);
}
if (demo) {
  setInterval(() => {
    clock.pos = (clock.pos + 100) % 24000;
    clock.at = performance.now();
    clock.playing = false; // позицию ведёт сам интервал
  }, 100);

  // Синтетический «звук» ~120 уд/мин: проверка визуализатора без музыки.
  let frame = 0;
  setInterval(() => {
    frame++;
    const time = frame / 30;
    const kick = frame % 15 === 0;
    const punch = Math.max(0, 1 - (frame % 15) / 6);
    const bands = Array.from({ length: 48 }, (_, i) => {
      const body = 0.62 * Math.exp(-i / 16) * (0.65 + 0.35 * Math.sin(time * 3 + i * 0.4));
      const air = 0.28 * Math.max(0, Math.sin(time * 2.2 + i * 0.25));
      return Math.min(1, body + air * (i / 48) + (i < 8 ? punch * 0.45 : 0) + 0.06);
    });
    const avg = (a, b) => bands.slice(a, b).reduce((s, v) => s + v, 0) / (b - a);
    const payload = {
      bands,
      bass: avg(0, 10),
      mid: avg(10, 30),
      treble: avg(30, 48),
      level: 0.5,
      beat: kick ? 0.85 : 0,
      bpm: 120,
      centroid: 1800,
      bassRatio: 0.55,
      dynamics: 0.2,
    };
    viz.feed(payload);
    vizInfo.features = payload;
  }, 33);
}
