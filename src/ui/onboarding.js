import { detectLanguage, nativeName, setLanguage, t } from '../i18n/index.js';
import { Cycle } from './cycle.js';
import { Splash } from './splash.js';

const CLOSE_FADE_MS = 750;

const el = (tag, className) => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
};

/**
 * Первый запуск. Приветствие по кругу на всех языках просит включить любимую песню на
 * родном языке; песню находим, ищем её текст и по тексту определяем язык. Внизу — кнопка,
 * чтобы выбрать язык вручную.
 */
export class Onboarding {
  /**
   * @param root      контейнер `#onboarding`
   * @param openPicker открыть выбор языка
   * @param onDone    вызывается с кодом языка, когда экран закрылся
   * @param decorate  подключение Liquid Glass к новым элементам
   */
  constructor(root, { openPicker, onDone, decorate }) {
    this.root = root;
    this.onDone = onDone;
    this.active = false;
    this.finishing = false;
    this.revealed = Promise.resolve();

    this.prompt = el('div', 'ob-prompt');
    this.stage = el('div', 'ob-stage');
    this.hint = el('div', 'ob-hint');
    this.button = el('button', 'pill glass ob-lang');
    this.button.type = 'button';
    const globe = el('span', 'ob-globe');
    globe.textContent = '🌐';
    this.buttonLabel = el('span', 'ob-lang-label');
    this.button.append(globe, this.buttonLabel);
    this.button.addEventListener('click', () => openPicker());
    root.replaceChildren(this.prompt, this.stage, this.hint, this.button);
    decorate?.(root);

    this.promptCycle = new Cycle(this.prompt);
    this.hintCycle = new Cycle(this.hint);
    this.labelCycle = new Cycle(this.buttonLabel);
    // «сонар» ожидания и всплывающая карточка найденной песни — те же, что в основном окне
    this.splash = new Splash(this.stage, () => ({ search: true, track: true, dock: false }));
  }

  start() {
    this.active = true;
    this.finishing = false;
    this.root.classList.add('open');
    this.#waiting();
    this.labelCycle.start('ob.chooseLanguage');
  }

  #waiting() {
    this.promptCycle.start('ob.prompt');
    this.hintCycle.start('ob.hint');
    this.splash.reset();
    this.splash.search('');
  }

  /** Нашли песню: карточка вырастает из центра, пока ищем текст. */
  onTrack(track) {
    if (!this.active || this.finishing) return;
    this.revealed = this.splash.showTrack(track);
    this.promptCycle.start('ob.lyrics');
    this.hintCycle.set('');
  }

  /** Песня пропала (или сменилась на «ничего») — снова ждём. */
  onNoTrack() {
    if (!this.active || this.finishing) return;
    this.#waiting();
  }

  /** Текст найден (или нет): определяем язык по нему. */
  async onLyrics(result) {
    if (!this.active || this.finishing) return;
    await this.revealed; // сначала даём карточке песни появиться целиком
    if (!this.active || this.finishing) return;
    const text = result?.synced?.length ? result.synced.map((l) => l.text).join('\n') : result?.plain || '';
    const detected = result?.found && !result.instrumental ? detectLanguage(text) : null;
    if (detected) {
      this.finish(detected.lang);
    } else {
      this.promptCycle.start('ob.notFound');
      this.hintCycle.set('');
    }
  }

  /** Язык выбран (определён по тексту или вручную): показываем его и закрываем экран. */
  finish(code) {
    if (!this.active || this.finishing) return;
    this.finishing = true;
    for (const cycle of [this.promptCycle, this.hintCycle, this.labelCycle]) cycle.stop();
    setLanguage(code);
    this.promptCycle.set(t('ob.detected', { lang: nativeName(code) }));
    this.hintCycle.set(t('ob.welcome'));
    this.button.classList.add('gone');
    setTimeout(() => {
      this.root.classList.remove('open');
      setTimeout(() => {
        this.active = false;
        this.splash.reset();
        this.button.classList.remove('gone');
        this.onDone(code);
      }, CLOSE_FADE_MS);
    }, 2400);
  }
}
