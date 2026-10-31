import { DEFAULT_PALETTE } from './palette.js';

const BLOBS = 5;

const VERT = `
attribute vec2 p;
void main() { gl_Position = vec4(p, 0.0, 1.0); }
`;

// Мягкие размытые «пятна» цвета на тёмной основе — как «Моя волна» в Яндекс Музыке.
const FRAG = `
precision highp float;
uniform vec2 uRes;
uniform float uTime;
uniform float uEnergy;
uniform float uSoft;
uniform vec3 uBase;
uniform vec3 uCol[${BLOBS}];

float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }

void main() {
  vec2 uv = gl_FragCoord.xy / uRes;
  float aspect = uRes.x / uRes.y;
  vec2 p = (uv - 0.5) * vec2(aspect, 1.0);

  vec3 col = uBase + uCol[0] * 0.05 * (1.0 - length(p));

  for (int i = 0; i < ${BLOBS}; i++) {
    float fi = float(i);
    float sp = 0.16 + fi * 0.045;
    vec2 c = vec2(
      sin(uTime * sp + fi * 2.1) * 0.36 * aspect,
      cos(uTime * sp * 0.83 + fi * 1.3) * 0.30
    );
    if (i == 0) c *= 0.25; // главное пятно держится ближе к центру
    float r = (0.23 + 0.05 * sin(uTime * 0.3 + fi * 1.7)) * uSoft * (1.0 + 0.18 * uEnergy);
    if (i == 0) r *= 1.5;
    float d = length(p - c);
    float w = exp(-(d * d) / (r * r));
    col = mix(col, uCol[i], clamp(w * 0.85, 0.0, 1.0));
  }

  col += (hash(gl_FragCoord.xy + uTime) - 0.5) / 160.0; // против бандинга
  gl_FragColor = vec4(col, 1.0);
}
`;

const RENDER_SCALE = 0.5; // фон и так размытый — считаем в половину разрешения

function compile(gl, type, src) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, src);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) || 'shader compile error');
  }
  return shader;
}

export class Background {
  constructor(canvas) {
    this.canvas = canvas;
    this.energy = 0;
    this.soft = 1;
    this.current = DEFAULT_PALETTE.colors.map((c) => [...c]);
    this.currentBase = [...DEFAULT_PALETTE.base];
    this.target = this.current.map((c) => [...c]);
    this.targetBase = [...this.currentBase];
    this.last = performance.now();

    const gl = canvas.getContext('webgl', { antialias: false, alpha: false });
    if (!gl) {
      this.gl = null;
      this.#fallbackCss();
      return;
    }
    this.gl = gl;

    const program = gl.createProgram();
    gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program));
    }
    gl.useProgram(program);

    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(program, 'p');
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

    this.u = {
      res: gl.getUniformLocation(program, 'uRes'),
      time: gl.getUniformLocation(program, 'uTime'),
      energy: gl.getUniformLocation(program, 'uEnergy'),
      soft: gl.getUniformLocation(program, 'uSoft'),
      base: gl.getUniformLocation(program, 'uBase'),
      col: gl.getUniformLocation(program, 'uCol'),
    };

    this.resize();
    window.addEventListener('resize', () => this.resize());
    requestAnimationFrame((t) => this.#frame(t));
  }

  /** Плавно перекрашивает фон в новую палитру. */
  setPalette({ colors, base }) {
    this.target = colors.map((c) => [...c]);
    this.targetBase = [...base];
    if (!this.gl) this.#fallbackCss();
  }

  setSoft(value) {
    this.soft = value;
  }

  /** Громкость/удар 0..1 — «дыхание» пятен (понадобится визуализатору). */
  setEnergy(value) {
    this.energy = value;
  }

  resize() {
    if (!this.gl) return;
    this.canvas.width = Math.max(2, Math.round(window.innerWidth * RENDER_SCALE));
    this.canvas.height = Math.max(2, Math.round(window.innerHeight * RENDER_SCALE));
    this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
  }

  #frame(now) {
    const dt = Math.min(0.1, (now - this.last) / 1000);
    this.last = now;

    const k = 1 - Math.exp(-dt * 1.8);
    for (let i = 0; i < BLOBS; i++) {
      for (let j = 0; j < 3; j++) {
        this.current[i][j] += (this.target[i][j] - this.current[i][j]) * k;
      }
    }
    for (let j = 0; j < 3; j++) {
      this.currentBase[j] += (this.targetBase[j] - this.currentBase[j]) * k;
    }

    const { gl, u } = this;
    gl.uniform2f(u.res, this.canvas.width, this.canvas.height);
    gl.uniform1f(u.time, now / 1000);
    gl.uniform1f(u.energy, this.energy);
    gl.uniform1f(u.soft, this.soft);
    gl.uniform3fv(u.base, this.currentBase);
    gl.uniform3fv(u.col, new Float32Array(this.current.flat()));
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    requestAnimationFrame((t) => this.#frame(t));
  }

  #fallbackCss() {
    const css = (c) => `rgb(${c.map((v) => Math.round(v * 255)).join(',')})`;
    const [a, b, c] = this.target;
    this.canvas.style.background = `radial-gradient(circle at 30% 40%, ${css(a)}, transparent 60%),
      radial-gradient(circle at 70% 35%, ${css(b)}, transparent 55%),
      radial-gradient(circle at 55% 75%, ${css(c)}, transparent 55%), ${css(this.targetBase)}`;
  }
}
