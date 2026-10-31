/**
 * Контекстное меню по ПКМ. Весь интерфейс скрыт, пока пользователь
 * не нажмёт правую кнопку мыши; через несколько секунд бездействия меню исчезает.
 */

import { nativeName, onLanguageChange, t } from '../i18n/index.js';

const AUTO_HIDE_MS = 5000;
const EDGE = 12;

export class Menu {
  /**
   * @param root элемент #menu
   * @param api  { getTrack, getOffsetMs, shiftOffset, fontDelta, setSoft, getSoft,
   *               refetch, toggleTop, isTop, toggleFullscreen, minimize, close,
   *               getCandidates, selectCandidate, recognizeNow, toggleAudio, isAudio,
   *               getLyricsSource, openLyricsDir, isAnim, toggleAnim,
   *               getMusixmatchKey, setMusixmatchKey, commitMusixmatchKey,
   *               getVizMode, getVizInfo, cycleViz, getVizIntensity, setVizIntensity,
   *               pickLanguage, isGlass, toggleGlass, decorate, isPredict, togglePredict,
   *               isAutostart, toggleAutostart, donateVisible, openDonate }
   */
  constructor(root, api) {
    this.root = root;
    this.api = api;
    this.timer = null;
    this.#build();
    // язык сменили — перестраиваем меню с новыми подписями
    onLanguageChange(() => {
      this.#build();
      if (this.isOpen) this.refresh();
    });

    document.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      if (this.disabled) return;
      this.open(e.clientX, e.clientY);
    });
    document.addEventListener('mousedown', (e) => {
      if (this.isOpen && !this.root.contains(e.target)) this.close();
    });
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.close();
    });
    for (const type of ['mousemove', 'mousedown', 'wheel', 'input']) {
      this.root.addEventListener(type, () => this.#armTimer());
    }
  }

  get isOpen() {
    return !this.root.classList.contains('hidden');
  }

  open(x, y) {
    this.refresh();
    this.root.classList.remove('hidden');
    // позиционируем после показа, чтобы знать размеры, и не выходим за края окна
    const w = this.root.offsetWidth;
    const h = this.root.offsetHeight;
    const left = Math.max(EDGE, Math.min(x, window.innerWidth - w - EDGE));
    const top = Math.max(EDGE, Math.min(y, window.innerHeight - h - EDGE));
    this.root.style.left = `${left}px`;
    this.root.style.top = `${top}px`;
    this.root.style.transformOrigin = `${x - left}px ${y - top}px`;
    this.#armTimer();
  }

  close() {
    clearTimeout(this.timer);
    this.root.classList.add('hidden');
  }

  /** Подтянуть актуальные данные трека и значения настроек. */
  refresh() {
    const track = this.api.getTrack();
    this.cover.src = track?.cover || '';
    this.cover.style.visibility = track?.cover ? 'visible' : 'hidden';
    this.title.textContent = track?.title || t('menu.noTrack');
    this.artist.textContent =
      track?.artist || (this.api.isAudio() ? t('menu.listening') : t('menu.startMusic'));
    this.offsetValue.textContent = formatOffset(this.api.getOffsetMs());
    this.soft.value = this.api.getSoft();
    this.top.classList.toggle('on', this.api.isTop());
    this.#refreshViz();
    this.mxm.value = this.api.getMusixmatchKey();
    for (const [key, button] of Object.entries(this.anim)) {
      button.classList.toggle('on', this.api.isAnim(key));
    }
    const source = this.api.getLyricsSource();
    this.source.hidden = !source;
    this.source.textContent = source ? t('menu.lyricsSource', { source }) : '';
    this.#refreshToggles();
    this.donate.hidden = !this.api.donateVisible();
    this.renderCandidates();
  }

  #refreshToggles() {
    const { api } = this;
    this.audio.classList.toggle('on', api.isAudio());
    this.audio.textContent = t(api.isAudio() ? 'menu.audioOn' : 'menu.audioOff');
    this.autostart.classList.toggle('on', api.isAutostart());
    this.autostart.textContent = t(api.isAutostart() ? 'menu.autostartOn' : 'menu.autostartOff');
    this.predict.classList.toggle('on', api.isPredict());
    this.predict.textContent = t(api.isPredict() ? 'menu.predictOn' : 'menu.predictOff');
    this.glass.classList.toggle('on', api.isGlass());
    this.glass.textContent = t(api.isGlass() ? 'menu.glassOn' : 'menu.glassOff');
  }

  #refreshViz() {
    const mode = this.api.getVizMode();
    const label =
      mode === 'auto' ? t('menu.vizAuto') : mode === 'off' ? t('menu.vizOff') : t(`style.${mode}`);
    this.vizButton.textContent = t('menu.viz', { mode: label });
    this.vizInfo.textContent = this.api.getVizInfo();
    this.vizSlider.value = this.api.getVizIntensity();
  }

  /** Список песен, которые распознавание нашло по звуку: можно выбрать другую. */
  renderCandidates() {
    const { items, selected } = this.api.getCandidates();
    this.cands.replaceChildren();
    this.cands.hidden = items.length === 0;
    if (!items.length) return;

    const label = el('div', 'm-label');
    label.textContent = t('menu.candidates');
    this.cands.append(label);
    items.forEach((item, i) => {
      const row = el('button', i === selected ? 'cand on glass' : 'cand glass');
      row.type = 'button';
      const cover = el('img', 'cand-cover');
      cover.alt = '';
      if (item.coverUrl) cover.src = item.coverUrl;
      const meta = el('div', 'm-meta');
      const title = el('div', 'm-title');
      title.textContent = item.title;
      const artist = el('div', 'm-artist');
      artist.textContent = item.artist;
      meta.append(title, artist);
      row.append(cover, meta);
      row.addEventListener('click', () => {
        this.api.selectCandidate(i);
        this.close();
      });
      this.cands.append(row);
    });
    this.api.decorate?.(this.cands);
  }

  #armTimer() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.close(), AUTO_HIDE_MS);
  }

  #build() {
    const { api } = this;

    this.cover = el('img', 'm-cover');
    this.cover.alt = '';
    this.title = el('div', 'm-title');
    this.artist = el('div', 'm-artist');
    const meta = el('div', 'm-meta');
    meta.append(this.title, this.artist);
    const track = el('div', 'm-track glass');
    track.append(this.cover, meta);

    this.source = el('div', 'm-source');
    this.source.hidden = true;
    this.cands = el('div', 'm-cands');
    this.cands.hidden = true;

    const fontMinus = pill('A−', 'small', () => api.fontDelta(-0.1));
    const fontPlus = pill('A+', 'small', () => api.fontDelta(0.1));
    const fontRow = row(t('menu.fontSize'), fontMinus, fontPlus);

    this.offsetValue = el('span', 'm-value');
    const offsetMinus = pill('−', 'small', () => this.#shift(-100));
    const offsetPlus = pill('+', 'small', () => this.#shift(100));
    const offsetRow = row(t('menu.offset'), offsetMinus, this.offsetValue, offsetPlus);

    this.soft = el('input');
    this.soft.type = 'range';
    this.soft.min = '0.7';
    this.soft.max = '1.5';
    this.soft.step = '0.05';
    this.soft.addEventListener('input', () => api.setSoft(Number(this.soft.value)));
    const softRow = row(t('menu.soft'), this.soft);

    // Визуализатор: стиль (авто — по жанру), яркость.
    this.vizButton = pill('', 'wide', () => {
      api.cycleViz();
      this.#refreshViz();
    });
    this.vizInfo = el('div', 'm-source');
    this.vizSlider = el('input');
    this.vizSlider.type = 'range';
    this.vizSlider.min = '0.4';
    this.vizSlider.max = '1.6';
    this.vizSlider.step = '0.05';
    this.vizSlider.addEventListener('input', () => api.setVizIntensity(Number(this.vizSlider.value)));
    const vizBlock = el('div', 'm-anim');
    vizBlock.append(this.vizButton, this.vizInfo, row(t('menu.brightness'), this.vizSlider));

    // Этапы анимации заставки можно отключать по отдельности.
    this.anim = {};
    const animBlock = el('div', 'm-anim');
    const animLabel = el('div', 'm-label');
    animLabel.textContent = t('menu.anim');
    const animPills = el('div', 'm-pills3');
    for (const [key, label] of [
      ['animSearch', t('menu.animSearch')],
      ['animTrack', t('menu.animTrack')],
      ['animDock', t('menu.animDock')],
    ]) {
      const button = pill(label, '', () => {
        api.toggleAnim(key);
        button.classList.toggle('on', api.isAnim(key));
      });
      this.anim[key] = button;
      animPills.append(button);
    }
    animBlock.append(animLabel, animPills);

    this.mxm = el('input', 'm-key');
    this.mxm.type = 'password';
    this.mxm.placeholder = t('menu.keyPlaceholder');
    this.mxm.autocomplete = 'off';
    this.mxm.spellcheck = false;
    this.mxm.addEventListener('input', () => api.setMusixmatchKey(this.mxm.value.trim()));
    this.mxm.addEventListener('change', () => api.commitMusixmatchKey());
    const mxmRow = row('Musixmatch', this.mxm);

    const pills = el('div', 'm-pills');
    this.audio = pill('', '', () => {
      api.toggleAudio();
      this.#refreshToggles();
    });
    this.autostart = pill('', 'wide', () => {
      api.toggleAutostart();
      this.#refreshToggles();
    });
    this.donate = pill(t('menu.donate'), 'wide donate', () => {
      this.close();
      api.openDonate();
    });
    this.donate.hidden = true;
    this.predict = pill('', 'wide', () => {
      api.togglePredict();
      this.#refreshToggles();
    });
    this.glass = pill('', '', () => {
      api.toggleGlass();
      this.#refreshToggles();
    });
    this.top = pill(t('menu.onTop'), '', () => {
      api.toggleTop().then(() => this.top.classList.toggle('on', api.isTop()));
    });
    const language = pill(t('menu.language', { name: nativeName() }), 'wide', () => {
      this.close();
      api.pickLanguage();
    });
    pills.append(
      this.donate,
      pill(t('menu.fullscreen'), '', () => {
        this.close();
        api.toggleFullscreen();
      }),
      this.top,
      pill(t('menu.refetch'), 'wide', () => {
        this.close();
        api.refetch();
      }),
      this.audio,
      pill(t('menu.recognizeNow'), '', () => {
        this.close();
        api.recognizeNow();
      }),
      this.predict,
      this.autostart,
      pill(t('menu.folder'), 'wide', () => {
        this.close();
        api.openLyricsDir();
      }),
      language,
      this.glass,
      pill(t('menu.minimize'), '', () => {
        this.close();
        api.minimize();
      }),
      pill(t('menu.close'), 'danger', () => api.close()),
    );

    const scroller = el('div', 'menu-scroll');
    scroller.append(
      track, this.source, this.cands, fontRow, offsetRow, softRow, vizBlock, animBlock, mxmRow, pills,
    );
    this.root.replaceChildren(scroller);
    api.decorate?.(this.root);
  }

  #shift(deltaMs) {
    this.api.shiftOffset(deltaMs);
    this.offsetValue.textContent = formatOffset(this.api.getOffsetMs());
  }
}

function formatOffset(ms) {
  const s = (ms / 1000).toFixed(1);
  return `${ms > 0 ? '+' : ''}${s} ${t('unit.sec')}`;
}

function el(tag, className = '') {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

function pill(text, extraClass, onClick) {
  const b = el('button', `pill glass ${extraClass}`.trim());
  b.type = 'button';
  b.textContent = text;
  b.addEventListener('click', onClick);
  return b;
}

function row(label, ...controls) {
  const r = el('div', 'm-row');
  const ctrl = el('div', 'm-ctrl');
  ctrl.append(...controls);
  r.append(el('span'), ctrl);
  r.firstChild.textContent = label;
  return r;
}

