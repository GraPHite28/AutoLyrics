/** Показ текста: крупная текущая строка, соседние приглушены, плавный скролл. */

const LOOKAHEAD_MS = 120; // подсвечиваем строку чуть раньше — глаз успевает прочитать

export class LyricsView {
  constructor(root) {
    this.root = root;
    this.lines = [];
    this.els = [];
    this.active = -2;
    this.inner = null;
    window.addEventListener('resize', () => this.#center());
  }

  clear() {
    this.root.replaceChildren();
    this.root.classList.remove('approx');
    this.lines = [];
    this.els = [];
    this.inner = null;
    this.active = -2;
  }

  /** @param lines [{t, text}] — время в мс; approx — время условное (несинхронный текст) */
  setLines(lines, { approx = false } = {}) {
    this.clear();
    if (!lines.length) return;
    this.lines = lines;
    this.root.classList.toggle('approx', approx);

    this.inner = document.createElement('div');
    this.inner.className = 'lines';
    this.els = lines.map((line) => {
      const el = textEl('div', line.text ? 'line' : 'line gap', line.text || '♪');
      el.dataset.d = '4';
      return el;
    });
    this.inner.append(...this.els);
    this.root.append(this.inner);
    this.#render(-1, true);
  }

  setFontScale(scale) {
    document.documentElement.style.setProperty('--ly-scale', scale);
    this.#center();
  }

  update(posMs) {
    if (!this.lines.length) return;
    const idx = this.#indexAt(posMs + LOOKAHEAD_MS);
    if (idx !== this.active) this.#render(idx, false);
  }

  /** Индекс последней строки с t <= pos (или -1, если трек ещё не дошёл до первой). */
  #indexAt(pos) {
    let lo = 0;
    let hi = this.lines.length - 1;
    let ans = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (this.lines[mid].t <= pos) {
        ans = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return ans;
  }

  #render(idx, instant) {
    this.active = idx;
    for (let i = 0; i < this.els.length; i++) {
      this.els[i].dataset.d = String(Math.min(Math.abs(i - idx), 4));
    }
    this.#center(instant);
  }

  #center(instant = false) {
    if (!this.inner || !this.els.length) return;
    const el = this.els[Math.max(0, this.active)];
    const y = this.root.clientHeight / 2 - (el.offsetTop + el.offsetHeight / 2);
    if (instant) this.inner.style.transition = 'none';
    this.inner.style.transform = `translateY(${y}px)`;
    if (instant) {
      void this.inner.offsetHeight; // применить без анимации
      this.inner.style.transition = '';
    }
  }
}

function textEl(tag, className, text) {
  const el = document.createElement(tag);
  el.className = className;
  el.textContent = text;
  return el;
}
