import { LANGUAGES, t } from '../i18n/index.js';

const INTERVAL_MS = 2800;
const SWAP_MS = 380;

/**
 * Общие часы смены языка: один номер языка и один таймер на все надписи. Благодаря этому
 * заголовок, подсказка и подпись на кнопке всегда показаны на одном и том же языке и
 * меняются одновременно — даже если одна из надписей включилась позже остальных.
 */
const clock = { index: 0, timer: null, members: new Set() };

function startClock() {
  if (clock.timer) return;
  clock.timer = setInterval(() => {
    clock.index = (clock.index + 1) % LANGUAGES.length;
    for (const member of clock.members) member.render();
  }, INTERVAL_MS);
}

function stopClockIfIdle() {
  if (clock.members.size === 0) {
    clearInterval(clock.timer);
    clock.timer = null;
  }
}

/**
 * Текст, который по кругу сменяется на всех языках: «Включите любимую песню…» →
 * “Play your favorite song…” → … Смена — плавное растворение с лёгким размытием.
 */
export class Cycle {
  constructor(element) {
    this.el = element;
    this.el.classList.add('cycle');
    this.key = null;
    this.params = undefined;
    this.token = 0;
  }

  /** Показывать строку `key` на всех языках по очереди (в ногу с остальными надписями). */
  start(key, params) {
    this.key = key;
    this.params = params;
    clock.members.add(this);
    startClock();
    this.render();
  }

  /** Показать один готовый текст без смены языков. */
  set(text) {
    this.stop();
    this.#swap(text);
  }

  stop() {
    clock.members.delete(this);
    stopClockIfIdle();
    this.token++;
  }

  /** Показать строку на текущем языке общих часов. */
  render() {
    const { code } = LANGUAGES[clock.index];
    this.#swap(t(this.key, this.params, code), code);
  }

  #swap(text, lang) {
    const token = ++this.token;
    this.el.classList.add('out');
    setTimeout(
      () => {
        if (token !== this.token) return;
        this.el.textContent = text;
        if (lang) this.el.lang = lang;
        this.el.classList.remove('out');
      },
      this.el.textContent ? SWAP_MS : 0,
    );
  }
}
