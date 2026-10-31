/**
 * Liquid Glass для кнопок и плашек: настоящее преломление фона у краёв.
 *
 * Для каждого элемента с классом `.glass` рисуем «карту смещения» (у краёв стекла
 * фон сдвигается наружу, в середине — нет) и подключаем её как SVG-фильтр через
 * `backdrop-filter: url(#…)`. Работает в Chromium (WebView2); в других движках
 * остаётся обычное матовое стекло из CSS.
 */

const SVG_NS = 'http://www.w3.org/2000/svg';
const MAX_SHIFT = 16; // на сколько пикселей у самого края сдвигается фон

let defs = null;
let enabled = true;
let counter = 0;
const tracked = new WeakMap(); // элемент → { id, w, h }
const observer = new ResizeObserver((entries) => entries.forEach((e) => update(e.target)));

function ensureDefs() {
  if (!defs) {
    const svg = document.createElementNS(SVG_NS, 'svg');
    svg.setAttribute('width', '0');
    svg.setAttribute('height', '0');
    svg.setAttribute('aria-hidden', 'true');
    svg.style.cssText = 'position:fixed;width:0;height:0;pointer-events:none';
    defs = document.createElementNS(SVG_NS, 'defs');
    svg.append(defs);
    document.body.append(svg);
  }
  return defs;
}

/** Карта смещения для скруглённого прямоугольника w×h (R, G — направление и сила сдвига). */
function displacementMap(w, h, radius, bezel) {
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d');
  const image = ctx.createImageData(w, h);
  const px = image.data;
  const cx = w / 2;
  const cy = h / 2;
  const hx = w / 2 - radius;
  const hy = h / 2 - radius;

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const dx = x + 0.5 - cx;
      const dy = y + 0.5 - cy;
      const qx = Math.abs(dx) - hx;
      const qy = Math.abs(dy) - hy;
      const ox = Math.max(qx, 0);
      const oy = Math.max(qy, 0);
      const outside = Math.hypot(ox, oy);
      const depth = -(outside + Math.min(Math.max(qx, qy), 0) - radius); // расстояние от края внутрь

      let r = 128;
      let g = 128;
      if (depth < bezel) {
        const t = 1 - Math.max(depth, 0) / bezel; // 1 у края, 0 в глубине
        const strength = t * t * t; // выпуклая линза: сильное преломление только у самого края
        let nx;
        let ny;
        if (qx > 0 && qy > 0) {
          nx = (Math.sign(dx) * ox) / outside;
          ny = (Math.sign(dy) * oy) / outside;
        } else if (qx > qy) {
          nx = Math.sign(dx);
          ny = 0;
        } else {
          nx = 0;
          ny = Math.sign(dy);
        }
        r = 128 + 127 * nx * strength;
        g = 128 + 127 * ny * strength;
      }
      const i = (y * w + x) * 4;
      px[i] = r;
      px[i + 1] = g;
      px[i + 2] = 128;
      px[i + 3] = 255;
    }
  }
  ctx.putImageData(image, 0, 0);
  return canvas.toDataURL('image/png');
}

function clear(el) {
  el.style.removeProperty('backdrop-filter');
  el.style.removeProperty('-webkit-backdrop-filter');
}

function update(el) {
  if (!enabled) return clear(el);
  const info = tracked.get(el);
  const w = Math.round(el.offsetWidth);
  const h = Math.round(el.offsetHeight);
  if (!info || !w || !h) return;
  if (info.w === w && info.h === h && info.id) {
    return apply(el, info.id);
  }

  const radius = Math.min(parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0, w / 2, h / 2);
  const bezel = Math.max(6, Math.min(radius * 0.9, 22, Math.min(w, h) / 2));
  const href = displacementMap(w, h, radius, bezel);

  const id = info.id || `lg-${++counter}`;
  let filter = document.getElementById(id);
  if (!filter) {
    filter = document.createElementNS(SVG_NS, 'filter');
    filter.id = id;
    ensureDefs().append(filter);
  }
  filter.setAttribute('filterUnits', 'userSpaceOnUse');
  filter.setAttribute('x', '0');
  filter.setAttribute('y', '0');
  filter.setAttribute('width', String(w));
  filter.setAttribute('height', String(h));
  filter.setAttribute('color-interpolation-filters', 'sRGB');
  filter.innerHTML =
    `<feImage href="${href}" x="0" y="0" width="${w}" height="${h}" preserveAspectRatio="none" result="map"/>` +
    `<feDisplacementMap in="SourceGraphic" in2="map" scale="${MAX_SHIFT * 2}" xChannelSelector="R" yChannelSelector="G"/>`;

  tracked.set(el, { id, w, h });
  apply(el, id);
}

function apply(el, id) {
  const value = `url(#${id}) blur(1.5px) saturate(1.7) brightness(1.08)`;
  el.style.backdropFilter = value;
  el.style.webkitBackdropFilter = value;
}

/** Подключить Liquid Glass ко всем `.glass` внутри `root` (и к самому `root`). */
export function decorate(root) {
  const items = [...root.querySelectorAll('.glass')];
  if (root.classList?.contains('glass')) items.push(root);
  for (const el of items) {
    if (tracked.has(el)) continue;
    tracked.set(el, { id: null, w: 0, h: 0 });
    observer.observe(el);
  }
}

/** Включить или выключить эффект у всех подключённых элементов. */
export function setEnabled(on) {
  enabled = on;
  document.body.classList.toggle('glass-liquid', on);
  document.querySelectorAll('.glass').forEach((el) => {
    if (!tracked.has(el)) return;
    if (on) update(el);
    else clear(el);
  });
}
