/**
 * Визуализатор звука: рисует поверх фона, под текстом. Семь стилей (под жанр),
 * переключение между ними — плавное перекрёстное затухание.
 *
 * Данные приходят от Rust (`spectrum`): 48 полос 0..1, басы/середина/верха,
 * сила удара и темп. Здесь они сглаживаются: быстро растут и медленно спадают.
 */

const BANDS = 48;
const FADE_S = 1.1;
const TAU = Math.PI * 2;

const rgba = (c, a) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

/** Значение спектра в точке u (0..1) с линейной интерполяцией. */
function sample(bands, u) {
  const x = Math.min(BANDS - 1, Math.max(0, u * (BANDS - 1)));
  const i = Math.floor(x);
  const f = x - i;
  return bands[i] * (1 - f) + bands[Math.min(BANDS - 1, i + 1)] * f;
}

/** Плавная замкнутая кривая через точки (середины отрезков + квадратичные кривые). */
function closedSmoothPath(ctx, pts) {
  const n = pts.length;
  const mid = (a, b) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
  const start = mid(pts[n - 1], pts[0]);
  ctx.beginPath();
  ctx.moveTo(start[0], start[1]);
  for (let i = 0; i < n; i++) {
    const m = mid(pts[i], pts[(i + 1) % n]);
    ctx.quadraticCurveTo(pts[i][0], pts[i][1], m[0], m[1]);
  }
  ctx.closePath();
}

/* ---------- Стили ---------- */

/** Электроника: яркие «капсулы» по нижнему краю, кольца и вспышки на каждый удар. */
class Electronic {
  constructor() {
    this.rings = [];
    this.seen = 0;
  }
  draw(ctx, w, h, s, t, dt, c, k) {
    const n = BANDS;
    const gap = 5;
    const bw = Math.max(4, (w * 0.86) / n - gap);
    const x0 = (w - (n * (bw + gap) - gap)) / 2;
    const base = h - 30;
    const maxH = h * 0.46 * k;
    ctx.globalCompositeOperation = 'lighter';
    for (let i = 0; i < n; i++) {
      const v = s.bands[i];
      const bh = 6 + v ** 1.1 * maxH * (1 + s.beatEnv * 0.18);
      const col = c[i % c.length];
      const g = ctx.createLinearGradient(0, base - bh, 0, base);
      g.addColorStop(0, rgba(col, 0.92));
      g.addColorStop(1, rgba(col, 0.25));
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.roundRect(x0 + i * (bw + gap), base - bh, bw, bh, bw / 2);
      ctx.fill();
    }
    if (s.beatCount !== this.seen) {
      this.seen = s.beatCount;
      this.rings.push({ r: 30, a: 0.55 });
    }
    ctx.lineWidth = 3;
    this.rings = this.rings.filter((ring) => ring.a > 0.02);
    for (const ring of this.rings) {
      ring.r += dt * 380;
      ring.a *= Math.exp(-dt * 2.4);
      ctx.strokeStyle = rgba(c[0], ring.a);
      ctx.beginPath();
      ctx.arc(w / 2, h * 0.44, ring.r, 0, TAU);
      ctx.stroke();
    }
    if (s.beatEnv > 0.02) {
      ctx.fillStyle = `rgba(255,255,255,${s.beatEnv * 0.05 * k})`;
      ctx.fillRect(0, 0, w, h);
    }
  }
}

/** Хип-хоп: большой «дышащий» блоб, который качает бас. */
class HipHop {
  draw(ctx, w, h, s, t, dt, c, k) {
    const cx = w / 2;
    const cy = h * 0.5;
    const unit = Math.min(w, h);
    ctx.globalCompositeOperation = 'lighter';
    const blob = (scale, col1, col2, alpha, phase) => {
      const base = unit * 0.24 * scale * (1 + s.bass * 0.4 * k + s.beatEnv * 0.1);
      const pts = [];
      const N = 72;
      for (let j = 0; j < N; j++) {
        const th = (j / N) * TAU;
        const u = th < Math.PI ? th / Math.PI : (TAU - th) / Math.PI; // зеркально по оси
        const wobble = sample(s.bands, u * 0.8) * 0.3 * base * k + Math.sin(th * 3 + t * 0.9 + phase) * 0.035 * base;
        const r = base + wobble;
        pts.push([cx + Math.cos(th - Math.PI / 2) * r, cy + Math.sin(th - Math.PI / 2) * r]);
      }
      closedSmoothPath(ctx, pts);
      const g = ctx.createRadialGradient(cx, cy, base * 0.1, cx, cy, base * 1.5);
      g.addColorStop(0, rgba(col1, alpha));
      g.addColorStop(1, rgba(col2, alpha * 0.08));
      ctx.fillStyle = g;
      ctx.fill();
      // контур — иначе блоб тонет в фоне того же цвета
      ctx.lineWidth = 3 + s.beatEnv * 3;
      ctx.strokeStyle = rgba(col1, 0.8);
      ctx.stroke();
    };
    blob(1, c[0], c[1], 0.62, 0);
    blob(0.66, c[2], c[3], 0.5, 2);
  }
}

/** Рок: резкие лучи из центра, вспышки и дрожь на ударах. */
class Rock {
  draw(ctx, w, h, s, t, dt, c, k) {
    const cx = w / 2;
    const cy = h * 0.5;
    const unit = Math.min(w, h);
    const inner = unit * 0.17 * (1 + s.bass * 0.2);
    const maxLen = unit * 0.32 * k;
    const rays = 120;
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineCap = 'round';
    ctx.lineWidth = 2.4;
    for (let i = 0; i < rays; i++) {
      const th = (i / rays) * TAU + t * 0.05;
      const u = Math.abs((i / rays) * 2 - 1);
      const v = sample(s.bands, u);
      const jitter = s.beatEnv > 0.25 ? Math.random() * 0.14 * maxLen * s.beatEnv : 0;
      const len = 6 + v ** 1.2 * maxLen * (1 + s.beatEnv * 0.4) + jitter;
      ctx.strokeStyle = rgba(c[i % c.length], 0.5 + 0.45 * v);
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(th) * inner, cy + Math.sin(th) * inner);
      ctx.lineTo(cx + Math.cos(th) * (inner + len), cy + Math.sin(th) * (inner + len));
      ctx.stroke();
    }
    ctx.lineWidth = 3 + s.beatEnv * 7;
    ctx.strokeStyle = rgba(c[0], 0.6);
    ctx.beginPath();
    ctx.arc(cx, cy, inner * 0.9, 0, TAU);
    ctx.stroke();
    if (s.beatEnv > 0.05) {
      const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, unit * 0.6);
      g.addColorStop(0, rgba(c[1], s.beatEnv * 0.38));
      g.addColorStop(1, rgba(c[1], 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
    }
  }
}

/** Поп: цветные мягкие пузыри всплывают на ударах + лёгкая волна снизу. */
class Pop {
  constructor() {
    this.parts = [];
    this.seen = 0;
  }
  spawn(w, h, c, n) {
    for (let i = 0; i < n && this.parts.length < 150; i++) {
      this.parts.push({
        x: Math.random() * w,
        y: h + 40,
        r: 10 + Math.random() * 38,
        vy: 50 + Math.random() * 110,
        phase: Math.random() * TAU,
        col: c[Math.floor(Math.random() * c.length)],
      });
    }
  }
  draw(ctx, w, h, s, t, dt, c, k) {
    ctx.globalCompositeOperation = 'lighter';
    if (s.beatCount !== this.seen) {
      this.seen = s.beatCount;
      this.spawn(w, h, c, 5 + Math.round(s.beatEnv * 9 * k));
    }
    if (Math.random() < s.level * dt * 10) this.spawn(w, h, c, 1);

    this.parts = this.parts.filter((p) => p.y > -60);
    for (const p of this.parts) {
      p.y -= p.vy * dt * (0.8 + s.level * 0.8);
      p.x += Math.sin(t * 1.4 + p.phase) * 24 * dt;
      const alpha = Math.min(1, p.y / (h * 0.25)) * 0.42;
      const g = ctx.createRadialGradient(p.x, p.y, p.r * 0.1, p.x, p.y, p.r);
      g.addColorStop(0, rgba(p.col, alpha));
      g.addColorStop(1, rgba(p.col, 0));
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, TAU);
      ctx.fill();
    }

    // мягкая волна по нижнему краю
    const g = ctx.createLinearGradient(0, h * 0.7, 0, h);
    g.addColorStop(0, rgba(c[3], 0.28));
    g.addColorStop(1, rgba(c[0], 0.02));
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.moveTo(0, h);
    const steps = 48;
    for (let i = 0; i <= steps; i++) {
      const u = i / steps;
      ctx.lineTo(u * w, h - 16 - sample(s.bands, u) * h * 0.2 * k);
    }
    ctx.lineTo(w, h);
    ctx.fill();
  }
}

/** Спокойный: медленные северные сияния, почти без резких движений. */
class Chill {
  draw(ctx, w, h, s, t, dt, c, k) {
    ctx.globalCompositeOperation = 'lighter';
    for (let L = 0; L < 3; L++) {
      const baseY = h * (0.56 + 0.12 * L);
      const amp = h * (0.05 + 0.05 * L) * (0.5 + s.level * 1.4 + s.bass * 0.6) * k;
      const col = c[(L + 1) % c.length];
      ctx.beginPath();
      ctx.moveTo(0, h);
      for (let x = 0; x <= w + 8; x += 8) {
        const y =
          baseY +
          Math.sin(x * 0.004 * (1 + L * 0.5) + t * 0.25 * (1 + L * 0.3) + L * 2) * amp +
          Math.sin(x * 0.011 - t * 0.18 + L) * amp * 0.4;
        ctx.lineTo(x, y);
      }
      ctx.lineTo(w, h);
      const g = ctx.createLinearGradient(0, baseY - amp, 0, h);
      g.addColorStop(0, rgba(col, 0.34));
      g.addColorStop(1, rgba(col, 0));
      ctx.fillStyle = g;
      ctx.fill();
    }
  }
}

/** Классика / джаз: тонкие плавные линии с мягким свечением. */
class Classical {
  draw(ctx, w, h, s, t, dt, c, k) {
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    const lines = 6;
    const cy = h * 0.62;
    for (let j = 0; j < lines; j++) {
      const band = sample(s.bands, (j + 0.5) / lines);
      const amp = h * 0.05 * (0.35 + band * 2.2) * k;
      const offset = (j - (lines - 1) / 2) * h * 0.034;
      const col = c[j % c.length];
      for (const [width, alpha] of [
        [6, 0.1],
        [1.6, 0.7],
      ]) {
        ctx.lineWidth = width;
        ctx.strokeStyle = rgba(col, alpha);
        ctx.beginPath();
        for (let x = 0; x <= w; x += 6) {
          const y =
            cy +
            offset +
            Math.sin(x * 0.006 * (1 + j * 0.12) + t * (0.4 + j * 0.07) + j) * amp +
            Math.sin(x * 0.0021 - t * 0.2 + j * 1.7) * amp * 0.6;
          if (x === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    }
  }
}

/** Мягкие блобы: плавный «горный» спектр снизу и пульсирующее свечение. */
class Soft {
  draw(ctx, w, h, s, t, dt, c, k) {
    ctx.globalCompositeOperation = 'lighter';
    const base = h - 18;
    const steps = 64;
    const fill = ctx.createLinearGradient(0, 0, w, 0);
    fill.addColorStop(0, rgba(c[0], 0.38));
    fill.addColorStop(0.5, rgba(c[1], 0.38));
    fill.addColorStop(1, rgba(c[2], 0.38));
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.moveTo(0, h);
    const ys = [];
    for (let i = 0; i <= steps; i++) {
      const y = base - sample(s.bands, i / steps) ** 1.1 * h * 0.34 * k;
      ys.push(y);
      ctx.lineTo((i / steps) * w, y);
    }
    ctx.lineTo(w, h);
    ctx.fill();

    ctx.lineWidth = 2;
    ctx.strokeStyle = rgba(c[3], 0.6);
    ctx.beginPath();
    ys.forEach((y, i) => (i === 0 ? ctx.moveTo(0, y) : ctx.lineTo((i / steps) * w, y)));
    ctx.stroke();

    const r = Math.min(w, h) * (0.16 + s.bass * 0.1 + s.beatEnv * 0.05);
    const g = ctx.createRadialGradient(w / 2, h * 0.5, 0, w / 2, h * 0.5, r * 2);
    g.addColorStop(0, rgba(c[0], 0.22 + s.beatEnv * 0.2));
    g.addColorStop(1, rgba(c[0], 0));
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, w, h);
  }
}

/* ---------- Менеджер ---------- */

export class Visualizer {
  /**
   * @param canvas   холст поверх фона
   * @param getColors функция, отдающая текущие цвета фона ([r,g,b] 0..1) — визуализатор в тон обложке
   * @param onFrame  необязательный колбэк с состоянием кадра (фон «дышит» в такт)
   */
  constructor(canvas, getColors, onFrame) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.getColors = getColors;
    this.onFrame = onFrame;
    this.styles = {
      electronic: new Electronic(),
      hiphop: new HipHop(),
      rock: new Rock(),
      pop: new Pop(),
      chill: new Chill(),
      classical: new Classical(),
      default: new Soft(),
    };
    this.current = 'default';
    this.previous = null;
    this.fade = 1;
    this.enabled = true;
    this.intensity = 1;
    this.raw = null;
    this.lastFeed = 0;
    this.presence = 0;
    this.searching = false;
    this.calm = 0;
    this.s = {
      bands: new Float32Array(BANDS),
      bass: 0,
      mid: 0,
      treble: 0,
      level: 0,
      beatEnv: 0,
      beatCount: 0,
      bpm: 0,
    };
    this.last = performance.now();
    this.w = 0;
    this.h = 0;
    this.resize();
    window.addEventListener('resize', () => this.resize());
    requestAnimationFrame((now) => this.#frame(now));
  }

  setEnabled(on) {
    this.enabled = on;
    if (!on) this.ctx.clearRect(0, 0, this.w, this.h);
  }

  setIntensity(value) {
    this.intensity = value;
  }

  /**
   * Пока трек ищется, визуализатор «успокаивается»: реагирует мягче и медленнее и
   * размывается, как фон (размытие — в CSS по классу `searching`).
   */
  setSearching(on) {
    this.searching = on;
    this.canvas.classList.toggle('searching', on);
  }

  /** Новый стиль включается плавно, прежний затухает. */
  setStyle(name) {
    if (!this.styles[name] || name === this.current) return;
    this.previous = this.current;
    this.current = name;
    this.fade = 0;
  }

  feed(payload) {
    this.raw = payload;
    this.lastFeed = performance.now();
    this.s.bpm = payload.bpm;
    if (payload.beat > 0) {
      this.s.beatEnv = Math.max(this.s.beatEnv, payload.beat);
      this.s.beatCount++;
    }
  }

  resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    this.w = window.innerWidth;
    this.h = window.innerHeight;
    this.canvas.width = Math.round(this.w * dpr);
    this.canvas.height = Math.round(this.h * dpr);
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  #colors() {
    // чуть осветляем цвета фона — на тёмном они должны светиться
    return this.getColors().map((col) => col.map((v) => Math.round(255 * Math.min(1, v * 1.1 + 0.12))));
  }

  #smooth(dt) {
    const s = this.s;
    const live = this.raw && performance.now() - this.lastFeed < 1000;
    const target = (key) => (live ? this.raw[key] : 0);
    // calm: 0 — обычная живость, 1 — поиск трека (плавно и лениво)
    this.calm += ((this.searching ? 1 : 0) - this.calm) * (1 - Math.exp(-dt * 2.5));
    const attack = 24 - 17 * this.calm;
    const release = 5 - 3.2 * this.calm;
    const follow = (current, goal) =>
      current + (goal - current) * (1 - Math.exp(-dt * (goal > current ? attack : release)));
    for (let i = 0; i < BANDS; i++) s.bands[i] = follow(s.bands[i], live ? this.raw.bands[i] : 0);
    s.bass = follow(s.bass, target('bass'));
    s.mid = follow(s.mid, target('mid'));
    s.treble = follow(s.treble, target('treble'));
    s.level = follow(s.level, target('level'));
    s.beatEnv *= Math.exp(-dt * (5 - 2.5 * this.calm));
    // появление: пока звука нет, визуализатор еле заметен
    this.presence += ((s.level > 0.03 ? 1 : 0) - this.presence) * (1 - Math.exp(-dt * 2));
  }

  #frame(now) {
    const dt = Math.min(0.1, (now - this.last) / 1000);
    this.last = now;
    requestAnimationFrame((t) => this.#frame(t));

    this.#smooth(dt);
    this.onFrame?.(this.s);
    if (!this.enabled) return;

    const { ctx, w, h } = this;
    ctx.clearRect(0, 0, w, h);
    this.fade = Math.min(1, this.fade + dt / FADE_S);
    const colors = this.#colors();
    const t = now / 1000;
    const visible = 0.3 + 0.7 * this.presence;

    const drawStyle = (name, alpha) => {
      if (alpha <= 0.01) return;
      ctx.save();
      ctx.globalAlpha = alpha * visible;
      this.styles[name].draw(ctx, w, h, this.s, t, dt, colors, this.intensity);
      ctx.restore();
    };
    if (this.previous && this.fade < 1) drawStyle(this.previous, 1 - this.fade);
    drawStyle(this.current, this.fade);
  }
}
