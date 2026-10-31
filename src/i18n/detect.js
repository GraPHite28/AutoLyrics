/**
 * Определение языка по тексту песни: сначала по письменности (кириллица, кана, иероглифы),
 * а для латиницы — по характерным буквам и частым словам.
 * Возвращает { lang, confidence } или null, если текста мало.
 */

const STOP_WORDS = {
  en: ['the', 'you', 'and', 'i', 'to', 'it', 'my', 'me', 'in', 'is', 'of', 'that', 'for', 'with', 'your', 'this', 'on', 'be', 'we', 'but', 'not', 'all', 'no', 'so', 'just', 'like', 'don\'t', 'i\'m'],
  de: ['und', 'ich', 'nicht', 'der', 'die', 'das', 'du', 'ist', 'ein', 'mit', 'zu', 'den', 'dem', 'mir', 'mich', 'auch', 'von', 'wir', 'es', 'sie', 'aber', 'wie', 'wenn', 'nur', 'noch', 'dich', 'mein'],
  fr: ['je', 'tu', 'le', 'la', 'les', 'et', 'pas', 'que', 'est', 'un', 'une', 'des', 'du', 'pour', 'dans', 'mon', 'ma', 'moi', 'toi', 'qui', 'sur', 'plus', 'avec', 'nous', 'vous', 'ne', 'il', 'elle', 'j\'ai', 'c\'est'],
  pl: ['nie', 'się', 'to', 'jest', 'na', 'że', 'co', 'ja', 'ty', 'w', 'z', 'do', 'jak', 'tak', 'ale', 'mi', 'mnie', 'tylko', 'już', 'dla', 'przez', 'po', 'czy', 'bez', 'ten', 'być', 'jestem'],
};

/** Буквы, которых нет в английском: сильный признак языка. */
const MARKS = {
  de: /[äöüß]/g,
  fr: /[éèêàçùûôîœâëï]/g,
  pl: /[ąćęłńóśźż]/g,
};

const count = (text, pattern) => (text.match(pattern) || []).length;

export function detectLanguage(text) {
  if (!text) return null;
  const kana = count(text, /[぀-ヿ]/g);
  const han = count(text, /[一-鿿]/g);
  const cyrillic = count(text, /[Ѐ-ӿ]/g);
  const latin = count(text, /[A-Za-zÀ-ɏ]/g);
  const letters = kana + han + cyrillic + latin;
  if (letters < 20) return null;

  // Кана есть только в японском; иероглифы без каны — китайский.
  if (kana >= 5 && kana + han > letters * 0.25) return { lang: 'ja', confidence: 0.95 };
  if (han > letters * 0.4) return { lang: 'zh', confidence: 0.9 };
  if (cyrillic > letters * 0.5) return { lang: 'ru', confidence: 0.95 };
  if (latin < letters * 0.5) return null;

  // Латиница: английский, немецкий, французский или польский.
  const lower = text.toLowerCase();
  const words = lower.match(/[\p{L}']+/gu) || [];
  if (words.length < 8) return null;

  const scores = {};
  for (const lang of Object.keys(STOP_WORDS)) {
    const stop = new Set(STOP_WORDS[lang]);
    const hits = words.filter((w) => stop.has(w)).length;
    const marks = MARKS[lang] ? count(lower, MARKS[lang]) : 0;
    // частые слова (в процентах) + характерные буквы (на сотню слов, с весом)
    scores[lang] = (hits / words.length) * 100 + (marks / words.length) * 100 * 0.6;
  }
  const ranked = Object.entries(scores).sort((a, b) => b[1] - a[1]);
  const [best, second] = ranked;
  if (best[1] < 4) return { lang: 'en', confidence: 0.3 }; // ничего характерного — по умолчанию английский
  const confidence = Math.min(0.95, 0.4 + (best[1] - second[1]) / Math.max(best[1], 1));
  return { lang: best[0], confidence };
}
