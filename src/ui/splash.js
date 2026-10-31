/**
 * Заставка трека: поиск → найденный трек → найденный текст.
 *
 *   search   — «сонар» и подпись, что происходит (слушаю / определяю…);
 *   track    — поиск быстро гаснет, из центра вырастает обложка, из неё
 *              выезжают название и исполнители;
 *   docked   — когда нашёлся текст, заставка уезжает вверх и остаётся приглушённой.
 *
 * Каждый этап можно отключить: см. флаги `search`, `track`, `dock`.
 */

const SEARCH_FADE_MS = 200;
const LEAVE_MS = 220;
/** Сколько длится появление трека (обложка + текст) — после этого можно «уезжать» вверх. */
const REVEAL_MS = 1250;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

export class Splash {
  /** @param flags функция, возвращающая { search, track, dock } — включённые этапы */
  constructor(root, flags) {
    this.flags = flags;
    this.state = 'empty'; // empty | search | track | docked
    this.token = 0;

    this.search_ = el('div', 'sp-search');
    this.search_.append(el('i'), el('i'), el('i'));
    const disc = el('div', 'sp-disc');
    disc.append(el('b'), el('b'), el('b'), el('b'));
    this.search_.append(disc);

    this.cover = el('div', 'sp-cover');
    this.coverImg = el('img');
    this.coverImg.alt = '';
    this.coverImg.addEventListener('load', () => this.coverImg.classList.add('loaded'));
    this.cover.append(this.coverImg);
    this.title = el('div', 'sp-title');
    this.artist = el('div', 'sp-artist');
    const inner = el('div', 'sp-text-inner');
    inner.append(this.title, this.artist);
    const text = el('div', 'sp-text');
    text.append(inner);
    this.track = el('div', 'sp-track');
    this.track.append(this.cover, text);

    this.note = el('div', 'sp-note');
    this.note.hidden = true;

    const stage = el('div', 'sp-stage');
    stage.append(this.search_, this.track, this.note);
    const stack = el('div', 'sp-stack');
    stack.append(stage);

    this.el = el('div', 'splash');
    this.el.append(stack);
    root.append(this.el);
  }

  /** Идёт поиск трека: показываем «сонар» (если этап включён) и подпись. */
  search(note) {
    if (this.state === 'track' || this.state === 'docked') return;
    this.state = 'search';
    this.token++;
    this.search_.classList.toggle('on', this.flags().search);
    this.el.classList.add('visible');
    this.setNote(note);
  }

  /** Всё убрать (трека нет). */
  reset() {
    this.token++;
    this.state = 'empty';
    this.search_.classList.remove('on');
    this.track.classList.remove('on');
    this.el.classList.remove('visible', 'docked', 'faded');
    this.setNote('');
  }

  /**
   * Показывает найденный трек. Возвращает промис, который выполнится, когда
   * анимация появления закончилась.
   */
  async showTrack(track) {
    const token = ++this.token;
    const animate = this.flags().track;
    const hadTrack = this.state === 'track' || this.state === 'docked';

    this.search_.classList.remove('on'); // поиск быстро затухает
    this.setNote('');

    if (hadTrack) {
      // предыдущая заставка уходит, затем появляется новая
      this.el.classList.add('leaving');
      await sleep(animate ? LEAVE_MS : 0);
      if (token !== this.token) return;
    }

    this.#instant(() => {
      this.el.classList.remove('leaving', 'docked', 'faded');
      this.track.classList.remove('on');
      this.title.textContent = track.title;
      this.artist.textContent = track.artist;
      this.setCover(track.cover);
      this.el.classList.add('visible');
    });
    this.state = 'track';

    if (!animate) {
      this.#instant(() => this.track.classList.add('on'));
      return;
    }
    await sleep(hadTrack ? 30 : SEARCH_FADE_MS);
    if (token !== this.token) return;
    this.track.classList.add('on'); // CSS: обложка вырастает, затем выезжает текст
    await sleep(REVEAL_MS);
  }

  setCover(url) {
    this.coverImg.classList.remove('loaded');
    if (url) this.coverImg.src = url;
    else this.coverImg.removeAttribute('src');
    this.cover.classList.toggle('empty', !url);
  }

  /** Текст найден: заставка уезжает вверх и остаётся приглушённой (или гаснет, если этап выключен). */
  /** @param keep не прятать заставку, даже если этап «уезжает вверх» выключен (дзен-режим) */
  dock(keep = false) {
    if (this.state !== 'track' && this.state !== 'docked') return;
    this.setNote('');
    this.el.classList.add(keep || this.flags().dock ? 'docked' : 'faded');
    this.state = 'docked';
  }

  /** Вернуть заставку в центр (например, поиск текста запущен заново). */
  undock() {
    if (this.state !== 'docked') return;
    this.el.classList.remove('docked', 'faded');
    this.state = 'track';
  }

  /** Маленькая подпись под заставкой; смена текста — без перерисовки всего блока. */
  setNote(text) {
    if (!text) {
      this.note.hidden = true;
      this.note.textContent = '';
      return;
    }
    if (this.note.textContent === text && !this.note.hidden) return;
    const wasHidden = this.note.hidden;
    this.note.textContent = text;
    this.note.hidden = false;
    // перезапуск короткой анимации смены подписи
    this.note.classList.remove('swap', 'show');
    void this.note.offsetWidth;
    this.note.classList.add(wasHidden ? 'show' : 'swap');
  }

  /** Выполнить изменения без переходов. */
  #instant(change) {
    this.el.classList.add('instant');
    change();
    void this.el.offsetWidth;
    this.el.classList.remove('instant');
  }
}
