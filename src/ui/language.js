import { LANGUAGES, getLanguage, t } from '../i18n/index.js';
import { Cycle } from './cycle.js';

/**
 * Выбор языка: полноэкранная подложка и семь «стеклянных» кнопок, которые
 * по очереди выпрыгивают из центра. Название каждого языка — на нём самом.
 */
export class LanguagePicker {
  /**
   * @param root   контейнер `#lang-picker`
   * @param onPick вызывается с кодом выбранного языка
   * @param decorate функция, подключающая Liquid Glass к новым кнопкам
   */
  constructor(root, onPick, decorate) {
    this.root = root;
    this.onPick = onPick;
    this.decorate = decorate;
    this.cancelable = true;
    this.title = document.createElement('div');
    this.title.className = 'lp-title';
    this.cycle = new Cycle(this.title);
    this.grid = document.createElement('div');
    this.grid.className = 'lp-grid';
    this.back = document.createElement('button');
    this.back.type = 'button';
    this.back.className = 'pill glass lp-back';
    this.back.addEventListener('click', () => this.close());
    this.root.replaceChildren(this.title, this.grid, this.back);

    for (const [i, { code, native }] of LANGUAGES.entries()) {
      const button = document.createElement('button');
      button.style.setProperty('--i', String(i));
      button.type = 'button';
      button.className = 'lp-lang glass';
      button.textContent = native;
      button.lang = code;
      button.dataset.code = code;
      button.addEventListener('click', () => this.#pick(code));
      this.grid.append(button);
    }
    this.decorate?.(this.root);
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.isOpen) this.close();
    });
  }

  get isOpen() {
    return this.root.classList.contains('open');
  }

  /** @param cancelable можно ли закрыть без выбора (при первом запуске — тоже можно: вернуться к ожиданию песни) */
  open({ cancelable = true } = {}) {
    this.cancelable = cancelable;
    this.back.textContent = t('ob.back');
    this.back.hidden = !cancelable;
    // заголовок на всех языках по очереди — пока язык не выбран, он неизвестен
    this.cycle.start('ob.pickTitle');
    const current = getLanguage();
    for (const button of this.grid.children) {
      button.classList.toggle('current', button.dataset.code === current);
    }
    this.root.classList.add('open');
  }

  close() {
    if (!this.isOpen) return;
    this.cycle.stop();
    this.root.classList.remove('open');
  }

  #pick(code) {
    this.close();
    this.onPick(code);
  }
}
