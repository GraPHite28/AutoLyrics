/** Стили визуализатора и выбор стиля: по жанру, а если жанр неизвестен — по самому звуку. */

/** Порядок переключения стилей в меню: авто → стили → выкл. */
export const STYLE_CYCLE = ['auto', 'electronic', 'hiphop', 'rock', 'pop', 'chill', 'classical', 'default', 'off'];

// Порядок важен: «Hip-Hop/Rap» должен стать хип-хопом раньше, чем что-то ещё.
const RULES = [
  ['hiphop', /hip|rap|trap|r&b|rnb|drill|urban|хип|рэп|реп/],
  ['electronic', /electro|dance|house|techno|trance|edm|dubstep|drum|club|disco|synth|электрон|танц/],
  ['rock', /rock|metal|punk|grunge|hardcore|emo|alternativ|indie|рок|метал|панк/],
  ['chill', /ambient|chill|lounge|lo-?fi|new age|downtempo|relax|meditat|эмбиент|чилл/],
  ['classical', /classic|jazz|blues|folk|acoustic|singer|instrumental|soundtrack|opera|orchestr|классик|джаз|блюз|фолк/],
  ['pop', /pop|latin|reggaeton|поп|эстрад/],
];

/** Стиль по названию жанра от Shazam / iTunes; null — жанр не распознан. */
export function styleFromGenre(genre) {
  if (!genre) return null;
  const text = genre.toLowerCase();
  for (const [style, pattern] of RULES) {
    if (pattern.test(text)) return style;
  }
  return null;
}

/**
 * Грубая оценка по звуку (темп, доля баса, яркость тембра, перепады громкости),
 * когда жанр узнать не удалось. Это эвристика — для точности есть жанр от сервисов.
 */
export function styleFromFeatures(f) {
  if (!f || !f.centroid) return null;
  const { bpm, centroid, bassRatio, dynamics } = f;
  if (bpm >= 118 && bassRatio > 0.5) return 'electronic';
  if (bpm >= 65 && bpm <= 108 && bassRatio > 0.55) return 'hiphop';
  if (centroid > 2400 && bassRatio < 0.5 && bpm >= 90) return 'rock';
  if (dynamics > 0.55 && bpm < 100) return 'classical';
  if (bpm > 0 && bpm < 90 && bassRatio < 0.5) return 'chill';
  if (bpm >= 100) return 'pop';
  return 'default';
}
