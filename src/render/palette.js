/** Палитры в формате [r, g, b], каналы 0..1. */

export const DEFAULT_PALETTE = {
  colors: [
    [0.91, 0.73, 0.05], // жёлтый
    [0.69, 0.11, 0.88], // фиолетовый
    [0.17, 0.17, 0.84], // синий
    [0.85, 0.25, 0.69], // розовый
    [0.98, 0.45, 0.25], // коралловый
  ],
  base: [0.06, 0.055, 0.07],
};

/** Цвета ближе этого расстояния (RGB, 0..255) считаем одним. */
const MERGE_DISTANCE = 70;

function rgbToHsl(r, g, b) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h / 6, s, l];
}

function hslToRgb(h, s, l) {
  h = ((h % 1) + 1) % 1;
  const hue = (p, q, t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  if (s === 0) return [l, l, l];
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  return [hue(p, q, h + 1 / 3), hue(p, q, h), hue(p, q, h - 1 / 3)];
}

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

/**
 * Достаёт до 5 главных цветов обложки (с учётом чёрного и белого — они часть
 * обложки) и делает их «сочными, но мягкими» для фона.
 */
export async function paletteFromImage(src) {
  const img = new Image();
  img.src = src;
  await img.decode();

  const size = 48;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(img, 0, 0, size, size);
  const { data } = ctx.getImageData(0, 0, size, size);

  // 1. Считаем пиксели по грубым корзинам цвета (по 4 бита на канал).
  const buckets = new Map();
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) continue;
    const key = ((data[i] >> 4) << 8) | ((data[i + 1] >> 4) << 4) | (data[i + 2] >> 4);
    const b = buckets.get(key) || { n: 0, r: 0, g: 0, b: 0 };
    b.n++;
    b.r += data[i];
    b.g += data[i + 1];
    b.b += data[i + 2];
    buckets.set(key, b);
  }
  const colors = [...buckets.values()].map((b) => ({ n: b.n, r: b.r / b.n, g: b.g / b.n, b: b.b / b.n }));
  if (colors.length === 0) return DEFAULT_PALETTE;

  // 2. Вес цвета: сколько места занимает, с поправкой в пользу ярких —
  //    иначе белый фон обложки всегда победит и фон программы будет серым.
  //    Корень из площади: небольшое яркое пятно (надпись, силуэт) не тонет в большом фоне.
  const saturation = (c) => rgbToHsl(c.r / 255, c.g / 255, c.b / 255)[1];
  const weight = (c) => Math.sqrt(c.n) * (0.1 + 2.2 * saturation(c));
  colors.sort((a, b) => weight(b) - weight(a));

  // 3. Склеиваем близкие цвета, оставляем до 5 самых заметных.
  const picked = [];
  for (const c of colors) {
    const near = picked.find((p) => Math.hypot(p.r - c.r, p.g - c.g, p.b - c.b) < MERGE_DISTANCE);
    if (near) {
      const total = near.n + c.n;
      near.r = (near.r * near.n + c.r * c.n) / total;
      near.g = (near.g * near.n + c.g * c.n) / total;
      near.b = (near.b * near.n + c.b * c.n) / total;
      near.n = total;
    } else if (picked.length < 5) {
      // серых и белых тонов не больше двух — иначе фон выходит тусклым
      const neutral = saturation(c) < 0.12;
      if (neutral && picked.filter((p) => saturation(p) < 0.12).length >= 2) continue;
      picked.push({ ...c });
    }
  }
  picked.sort((a, b) => weight(b) - weight(a));

  // 4. Приводим к «мягкому и сочному»: оттенок обложки сохраняем, насыщенность
  //    слегка поднимаем, яркость держим в диапазоне, где белый текст читается.
  const hsl = picked.map((c) => rgbToHsl(c.r / 255, c.g / 255, c.b / 255));
  // Окрашенные цвета выпрямляем сильнее: на тонких деталях обложки (надпись, силуэт)
  // после усреднения с фоном они получаются «пыльными».
  const tuned = hsl.map(([h, s, l]) => {
    if (s < 0.12) return [h, s, clamp(l, 0.2, 0.68)]; // белый/серый/чёрный остаются собой
    return [h, clamp(s * 1.6, 0.5, 0.92), clamp(l, 0.38, 0.62)];
  });

  // 5. Мало цветов на обложке — добавляем светлее/темнее тех же оттенков,
  //    а не придумываем новые.
  for (let k = 0; tuned.length < 5; k++) {
    const [h, s, l] = tuned[k % picked.length];
    const shift = k < picked.length ? 0.14 : -0.12;
    tuned.push([h, s, clamp(l + shift, 0.15, 0.65)]);
  }

  const base = hslToRgb(tuned[0][0], Math.min(tuned[0][1], 0.4), 0.06);
  return { colors: tuned.map(([h, s, l]) => hslToRgb(h, s, l)), base };
}
