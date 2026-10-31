import { t } from '../i18n/index.js';

const el = (tag, className, text) => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
};

/**
 * Окно «Поддержать проект». Сначала честно сказано, что пожертвование добровольное и ничего
 * не даёт взамен, и три кнопки: «Возможно позже» (просто закрыть), «Не показывать больше»
 * (с подтверждением) и «Открыть ссылку».
 */
export class DonateDialog {
  /**
   * @param root     контейнер `#donate-dialog`
   * @param onOpen   открыть ссылку пожертвования
   * @param onHide   пользователь подтвердил, что кнопка больше не нужна
   * @param decorate подключение Liquid Glass к кнопкам
   */
  constructor(root, { onOpen, onHide, decorate }) {
    this.root = root;
    this.onOpen = onOpen;
    this.onHide = onHide;
    this.decorate = decorate;
    root.addEventListener('mousedown', (e) => {
      if (e.target === root) this.close(); // клик мимо окна = «возможно позже»
    });
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.isOpen) this.close();
    });
  }

  get isOpen() {
    return this.root.classList.contains('open');
  }

  open() {
    this.#showAsk();
    this.root.classList.add('open');
  }

  close() {
    this.root.classList.remove('open');
  }

  #button(text, className, onClick) {
    const button = el('button', `pill glass ${className}`.trim(), text);
    button.type = 'button';
    button.addEventListener('click', onClick);
    return button;
  }

  #render(title, text, buttons) {
    const card = el('div', 'dlg-card');
    const body = el('div', 'dlg-body');
    body.append(el('div', 'dlg-title', title), el('div', 'dlg-text', text));
    const row = el('div', 'dlg-buttons');
    row.append(...buttons);
    body.append(row);
    card.append(body);
    this.root.replaceChildren(card);
    this.decorate?.(card);
  }

  /** Основной экран: честная формулировка и три кнопки. */
  #showAsk() {
    this.#render(t('menu.donate'), t('donate.text'), [
      this.#button(t('donate.later'), '', () => this.close()),
      this.#button(t('donate.never'), '', () => this.#showConfirm()),
      this.#button(t('donate.open'), 'primary', () => {
        this.close();
        this.onOpen();
      }),
    ]);
  }

  /** «Не показывать больше» — сначала предупреждаем и просим подтвердить. */
  #showConfirm() {
    this.#render(t('donate.never'), t('donate.confirmText'), [
      this.#button(t('ob.back'), '', () => this.#showAsk()),
      this.#button(t('donate.confirm'), 'danger', () => {
        this.close();
        this.onHide();
      }),
    ]);
  }
}
