/**
 * Контекстное меню по ПКМ. Весь интерфейс скрыт, пока пользователь не нажмёт правую кнопку мыши;
 * через несколько секунд бездействия меню исчезает. Настройки разложены по трём вкладкам:
 * «Трек» (песня, текст, распознавание), «Вид» (визуализатор, цвета, дзен-режим, анимации)
 * и «Программа» (окно, язык, автозапуск).
 */

import { nativeName, onLanguageChange, t } from '../i18n/index.js';

const AUTO_HIDE_MS = 5000;
const EDGE = 12;
const TABS = ['track', 'look', 'app'];

export class Menu {
  /**
   * @param root элемент #menu
   * @param api  { getTrack, getOffsetMs, shiftOffset, fontDelta, getCandidates, selectCandidate,
   *               refetch, recognizeNow, isAudio, toggleAudio, isRecheck, toggleRecheck,
   *               isPredict, togglePredict, getLyricsSource, openLyricsDir,
   *               getMusixmatchKey, setMusixmatchKey, commitMusixmatchKey,
   *               isZen, toggleZen, isZenInfo, toggleZenInfo,
   *               getVizMode, getVizInfo, cycleViz, getVizIntensity, setVizIntensity,
   *               getSoft, setSoft, isAnim, toggleAnim, isGlass, toggleGlass,
   *               getBgMode, toggleBgMode, getBgColors, setBgColor,
   *               getEqMode, toggleEqMode, getEqColors, setEqColor,
   *               toggleTop, isTop, toggleFullscreen, minimize, close, pickLanguage,
   *               isAutostart, toggleAutostart, donateVisible, openDonate,
   *               getMenuTab, setMenuTab, decorate }
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
    this.#place(x, y);
    this.#armTimer();
  }

  /** Ставит меню у курсора, не выходя за края окна; запоминает точку для смены вкладки. */
  #place(x, y) {
    this.anchor = [x, y];
    const w = this.root.offsetWidth;
    const h = this.root.offsetHeight;
    const left = Math.max(EDGE, Math.min(x, window.innerWidth - w - EDGE));
    const top = Math.max(EDGE, Math.min(y, window.innerHeight - h - EDGE));
    this.root.style.left = `${left}px`;
    this.root.style.top = `${top}px`;
    this.root.style.transformOrigin = `${x - left}px ${y - top}px`;
  }

  close() {
    clearTimeout(this.timer);
    this.root.classList.add('hidden');
  }

  /** Подтянуть актуальные данные трека и значения настроек. */
  refresh() {
    const { api } = this;
    const track = api.getTrack();
    this.cover.src = track?.cover || '';
    this.cover.style.visibility = track?.cover ? 'visible' : 'hidden';
    this.title.textContent = track?.title || t('menu.noTrack');
    this.artist.textContent =
      track?.artist || (api.isAudio() ? t('menu.listening') : t('menu.startMusic'));
    this.offsetValue.textContent = formatOffset(api.getOffsetMs());
    this.soft.value = api.getSoft();
    this.mxm.value = api.getMusixmatchKey();
    for (const [key, button] of Object.entries(this.anim)) {
      button.classList.toggle('on', api.isAnim(key));
    }
    const source = api.getLyricsSource();
    this.source.hidden = !source;
    this.source.textContent = source ? t('menu.lyricsSource', { source }) : '';
    this.#refreshViz();
    this.#refreshToggles();
    this.#refreshColors();
    this.donate.hidden = !api.donateVisible();
    this.renderCandidates();
  }

  #refreshToggles() {
    const { api } = this;
    const toggle = (button, on, onKey, offKey) => {
      button.classList.toggle('on', on);
      button.textContent = t(on ? onKey : offKey);
    };
    toggle(this.audio, api.isAudio(), 'menu.audioOn', 'menu.audioOff');
    toggle(this.recheck, api.isRecheck(), 'menu.recheckOn', 'menu.recheckOff');
    toggle(this.predict, api.isPredict(), 'menu.predictOn', 'menu.predictOff');
    toggle(this.zen, api.isZen(), 'menu.zenOn', 'menu.zenOff');
    toggle(this.zenInfo, api.isZenInfo(), 'menu.zenInfoOn', 'menu.zenInfoOff');
    toggle(this.autostart, api.isAutostart(), 'menu.autostartOn', 'menu.autostartOff');
    toggle(this.glass, api.isGlass(), 'menu.glassOn', 'menu.glassOff');
    this.top.classList.toggle('on', api.isTop());
    this.zenInfo.classList.toggle('dim', !api.isZen()); // настройка имеет смысл в дзен-режиме
  }

  #refreshViz() {
    const mode = this.api.getVizMode();
    const label =
      mode === 'auto' ? t('menu.vizAuto') : mode === 'off' ? t('menu.vizOff') : t(`style.${mode}`);
    this.vizButton.textContent = t('menu.viz', { mode: label });
    this.vizInfo.textContent = this.api.getVizInfo();
    this.vizSlider.value = this.api.getVizIntensity();
  }

  #refreshColors() {
    const { api } = this;
    const sync = (block, mode, colors, customMode, offKey) => {
      block.mode.textContent = t(mode === customMode ? 'menu.modeCustom' : offKey);
      block.mode.classList.toggle('on', mode === customMode);
      block.swatches.hidden = mode !== customMode;
      block.inputs.forEach((input, i) => (input.value = colors[i]));
    };
    sync(this.bgBlock, api.getBgMode(), api.getBgColors(), 'custom', 'menu.modeCover');
    sync(this.eqBlock, api.getEqMode(), api.getEqColors(), 'custom', 'menu.modeSameAsBg');
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

  #showTab(id) {
    const tab = TABS.includes(id) ? id : 'track';
    this.tab = tab;
    for (const [name, page] of Object.entries(this.pages)) page.hidden = name !== tab;
    for (const [name, button] of Object.entries(this.tabButtons)) {
      button.classList.toggle('on', name === tab);
      button.setAttribute('aria-selected', String(name === tab));
    }
  }

  /** Вкладки с разной высотой: после переключения меню остаётся в окне. */
  #switchTab(id) {
    this.#showTab(id);
    this.api.setMenuTab(id);
    if (this.anchor) this.#place(...this.anchor);
  }

  #build() {
    const tabBar = el('div', 'm-tabs');
    tabBar.setAttribute('role', 'tablist');
    this.tabButtons = {};
    for (const id of TABS) {
      const button = el('button', 'm-tab');
      button.type = 'button';
      button.setAttribute('role', 'tab');
      button.textContent = t(`tab.${id}`);
      button.addEventListener('click', () => this.#switchTab(id));
      this.tabButtons[id] = button;
      tabBar.append(button);
    }

    this.pages = {
      track: this.#trackPage(),
      look: this.#lookPage(),
      app: this.#appPage(),
    };
    const scroller = el('div', 'menu-scroll');
    scroller.append(tabBar, ...Object.values(this.pages));
    this.root.replaceChildren(scroller);
    this.api.decorate?.(this.root);
    this.#showTab(this.api.getMenuTab());
  }

  /** Вкладка «Трек»: песня, текст, распознавание. */
  #trackPage() {
    const { api } = this;
    const page = el('div', 'm-page');

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

    const fontRow = row(
      t('menu.fontSize'),
      pill('A−', 'small', () => api.fontDelta(-0.1)),
      pill('A+', 'small', () => api.fontDelta(0.1)),
    );
    this.offsetValue = el('span', 'm-value');
    const offsetRow = row(
      t('menu.offset'),
      pill('−', 'small', () => this.#shift(-100)),
      this.offsetValue,
      pill('+', 'small', () => this.#shift(100)),
    );

    this.mxm = el('input', 'm-key');
    this.mxm.type = 'password';
    this.mxm.placeholder = t('menu.keyPlaceholder');
    this.mxm.autocomplete = 'off';
    this.mxm.spellcheck = false;
    this.mxm.addEventListener('input', () => api.setMusixmatchKey(this.mxm.value.trim()));
    this.mxm.addEventListener('change', () => api.commitMusixmatchKey());
    const mxmRow = row('Musixmatch', this.mxm);

    this.audio = pill('', '', () => this.#toggle(api.toggleAudio));
    this.recheck = pill('', 'wide', () => this.#toggle(api.toggleRecheck));
    this.predict = pill('', 'wide', () => this.#toggle(api.togglePredict));

    const pills = el('div', 'm-pills');
    pills.append(
      pill(t('menu.refetch'), 'wide', () => {
        this.close();
        api.refetch();
      }),
      this.audio,
      pill(t('menu.recognizeNow'), '', () => {
        this.close();
        api.recognizeNow();
      }),
      this.recheck,
      this.predict,
      pill(t('menu.folder'), 'wide', () => {
        this.close();
        api.openLyricsDir();
      }),
    );
    page.append(track, this.source, this.cands, fontRow, offsetRow, mxmRow, pills);
    return page;
  }

  /** Вкладка «Вид»: дзен-режим, визуализатор, цвета, анимации. */
  #lookPage() {
    const { api } = this;
    const page = el('div', 'm-page');

    this.zen = pill('', 'wide', () => this.#toggle(api.toggleZen));
    this.zenInfo = pill('', 'wide', () => this.#toggle(api.toggleZenInfo));

    this.vizButton = pill('', 'wide', () => {
      api.cycleViz();
      this.#refreshViz();
    });
    this.vizInfo = el('div', 'm-source');
    this.vizSlider = slider(0.4, 1.6, 0.05, (v) => api.setVizIntensity(v));
    const vizBlock = el('div', 'm-anim');
    vizBlock.append(this.vizButton, this.vizInfo, row(t('menu.brightness'), this.vizSlider));

    this.soft = slider(0.7, 1.5, 0.05, (v) => api.setSoft(v));
    const softRow = row(t('menu.soft'), this.soft);

    this.bgBlock = this.#colorBlock(t('menu.bgLabel'), () => api.toggleBgMode(), (i, hex) => api.setBgColor(i, hex));
    this.eqBlock = this.#colorBlock(t('menu.eqLabel'), () => api.toggleEqMode(), (i, hex) => api.setEqColor(i, hex));

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

    this.glass = pill('', 'wide', () => this.#toggle(api.toggleGlass));

    page.append(
      this.zen, this.zenInfo, vizBlock, softRow,
      this.bgBlock.root, this.eqBlock.root, animBlock, this.glass,
    );
    return page;
  }

  /** Вкладка «Программа»: окно, язык, автозапуск, поддержка. */
  #appPage() {
    const { api } = this;
    const page = el('div', 'm-page');

    this.top = pill(t('menu.onTop'), '', () => {
      api.toggleTop().then(() => this.top.classList.toggle('on', api.isTop()));
    });
    this.autostart = pill('', 'wide', () => this.#toggle(api.toggleAutostart));
    this.donate = pill(t('menu.donate'), 'wide donate', () => {
      this.close();
      api.openDonate();
    });
    this.donate.hidden = true;

    const pills = el('div', 'm-pills');
    pills.append(
      pill(t('menu.fullscreen'), '', () => {
        this.close();
        api.toggleFullscreen();
      }),
      this.top,
      this.autostart,
      pill(t('menu.language', { name: nativeName() }), 'wide', () => {
        this.close();
        api.pickLanguage();
      }),
      this.donate,
      pill(t('menu.minimize'), '', () => {
        this.close();
        api.minimize();
      }),
      pill(t('menu.close'), 'danger', () => api.close()),
    );
    page.append(pills);
    return page;
  }

  /** Блок «заголовок + режим + три цвета»: цвета видны только в режиме «свои». */
  #colorBlock(label, onToggleMode, onColor) {
    const mode = pill('', 'small-wide', () => {
      onToggleMode();
      this.#refreshColors();
    });
    const swatches = el('div', 'm-swatches');
    const inputs = [0, 1, 2].map((i) => {
      const input = el('input', 'm-color');
      input.type = 'color';
      input.addEventListener('input', () => onColor(i, input.value));
      swatches.append(input);
      return input;
    });
    const root = el('div', 'm-anim');
    root.append(row(label, mode), swatches);
    return { root, mode, swatches, inputs };
  }

  #toggle(action) {
    action();
    this.#refreshToggles();
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

function slider(min, max, step, onInput) {
  const input = el('input');
  input.type = 'range';
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.addEventListener('input', () => onInput(Number(input.value)));
  return input;
}

function row(label, ...controls) {
  const r = el('div', 'm-row');
  const ctrl = el('div', 'm-ctrl');
  ctrl.append(...controls);
  r.append(el('span'), ctrl);
  r.firstChild.textContent = label;
  return r;
}
