//! Анализ звука для визуализатора: спектр, удар (бит), темп и общие признаки трека.
//!
//! Берём последние 2048 отсчётов системного звука, считаем FFT и раз в 25 мс отправляем
//! во фронтенд событие `spectrum`: 48 логарифмических полос (0..1, относительно
//! громкости самого трека — системный регулятор громкости не мешает), басы/средние/верха,
//! силу удара и темп. Признаки (`centroid`, `bassRatio`, `dynamics`) копятся медленно
//! и нужны, чтобы угадать жанр, когда его не сообщил ни один сервис.

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio_capture;

const FFT_SIZE: usize = 2048;
const BANDS: usize = 48;
const FRAME: Duration = Duration::from_millis(25);
const F_MIN: f32 = 35.0;
const F_MAX: f32 = 16_000.0;
/// Полосы [0, BASS_END) — басы, [BASS_END, MID_END) — середина, остальное — верха.
const BASS_END: usize = 10;
const MID_END: usize = 30;
/// Диапазон динамики полос ниже пика трека, дБ.
const RANGE_DB: f32 = 40.0;
/// Тише этого (RMS) считаем тишиной.
const SILENCE_RMS: f32 = 0.0008;
/// Минимум между двумя ударами.
const MIN_BEAT_GAP: Duration = Duration::from_millis(220);
/// Постоянная времени накопления признаков жанра (в кадрах).
const FEATURE_ALPHA: f32 = 0.004;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Spectrum {
    bands: Vec<f32>,
    bass: f32,
    mid: f32,
    treble: f32,
    level: f32,
    /// Сила удара в этом кадре, 0 — удара нет.
    beat: f32,
    /// Темп, уд/мин (0 — пока неизвестен).
    bpm: f32,
    /// Признаки для угадывания жанра.
    centroid: f32,
    bass_ratio: f32,
    dynamics: f32,
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let mut state = State::new(fft);
        let mut last_silent_emit = Instant::now();
        loop {
            let started = Instant::now();
            match audio_capture::latest(FFT_SIZE) {
                Some((samples, rate)) => {
                    if let Some(frame) = state.process(&samples, rate) {
                        let _ = app.emit("spectrum", frame);
                        last_silent_emit = started;
                    }
                }
                None => {}
            }
            // тишина: редкие нулевые кадры, чтобы визуализатор плавно затух
            if started.duration_since(last_silent_emit) > Duration::from_millis(250) {
                let _ = app.emit("spectrum", state.silence());
                last_silent_emit = started;
            }
            thread::sleep(FRAME.saturating_sub(started.elapsed()));
        }
    });
}

struct State {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buffer: Vec<Complex<f32>>,
    rate: u32,
    /// Диапазоны бинов FFT для каждой полосы и центральные частоты полос.
    edges: Vec<(usize, usize)>,
    centers: Vec<f32>,
    /// Опорный уровень (дБ): пик трека с медленным затуханием.
    ref_db: f32,
    bass_history: VecDeque<f32>,
    prev_bass: f32,
    last_beat: Option<Instant>,
    intervals: VecDeque<f32>,
    bpm: f32,
    centroid: f32,
    bass_ratio: f32,
    level_mean: f32,
    level_sq: f32,
}

impl State {
    fn new(fft: Arc<dyn Fft<f32>>) -> Self {
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos())
            .collect();
        Self {
            fft,
            window,
            buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            rate: 0,
            edges: Vec::new(),
            centers: Vec::new(),
            ref_db: -50.0,
            bass_history: VecDeque::new(),
            prev_bass: 0.0,
            last_beat: None,
            intervals: VecDeque::new(),
            bpm: 0.0,
            centroid: 0.0,
            bass_ratio: 0.0,
            level_mean: 0.0,
            level_sq: 0.0,
        }
    }

    fn setup_bands(&mut self, rate: u32) {
        self.rate = rate;
        let bin_hz = rate as f32 / FFT_SIZE as f32;
        let f_max = F_MAX.min(rate as f32 * 0.45);
        let max_bin = FFT_SIZE / 2 - 1;
        self.edges.clear();
        self.centers.clear();
        for i in 0..BANDS {
            let f_lo = F_MIN * (f_max / F_MIN).powf(i as f32 / BANDS as f32);
            let f_hi = F_MIN * (f_max / F_MIN).powf((i + 1) as f32 / BANDS as f32);
            let lo = ((f_lo / bin_hz) as usize).clamp(1, max_bin);
            let hi = (((f_hi / bin_hz).ceil() as usize).saturating_sub(1)).clamp(lo, max_bin);
            self.edges.push((lo, hi));
            self.centers.push((f_lo * f_hi).sqrt());
        }
    }

    fn silence(&mut self) -> Spectrum {
        self.bpm = 0.0;
        self.intervals.clear();
        Spectrum {
            bands: vec![0.0; BANDS],
            centroid: self.centroid,
            bass_ratio: self.bass_ratio,
            dynamics: self.dynamics(),
            ..Default::default()
        }
    }

    fn dynamics(&self) -> f32 {
        if self.level_mean < 1e-4 {
            return 0.0;
        }
        let variance = (self.level_sq - self.level_mean * self.level_mean).max(0.0);
        variance.sqrt() / self.level_mean
    }

    fn process(&mut self, samples: &[f32], rate: u32) -> Option<Spectrum> {
        if rate != self.rate {
            self.setup_bands(rate);
        }
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        if rms < SILENCE_RMS {
            return None; // тишина — обработает вызывающий код
        }

        for (i, slot) in self.buffer.iter_mut().enumerate() {
            *slot = Complex::new(samples[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.buffer);

        // амплитуда: у синуса полной шкалы под окном Ханна пик равен N/4
        let norm = FFT_SIZE as f32 * 0.25;
        let mags: Vec<f32> = self.buffer[..FFT_SIZE / 2]
            .iter()
            .map(|c| c.norm() / norm)
            .collect();

        let mut lin = [0.0f32; BANDS];
        let mut db = [0.0f32; BANDS];
        for (i, &(lo, hi)) in self.edges.iter().enumerate() {
            let slice = &mags[lo..=hi];
            lin[i] = slice.iter().sum::<f32>() / slice.len() as f32;
            // лёгкий наклон +3 дБ/окт: у музыки верха всегда слабее, но глазу они нужны
            let tilt = 3.0 * (self.centers[i] / 100.0).log2();
            db[i] = 20.0 * (lin[i] + 1e-6).log10() + tilt;
        }

        let peak = db.iter().cloned().fold(f32::MIN, f32::max);
        self.ref_db = (self.ref_db - 0.012).max(peak).max(-50.0);
        let floor = self.ref_db - RANGE_DB;
        let mut bands = [0.0f32; BANDS];
        for i in 0..BANDS {
            bands[i] = ((db[i] - floor) / RANGE_DB).clamp(0.0, 1.0).powf(1.25);
        }

        let mean = |range: std::ops::Range<usize>| {
            let slice = &bands[range];
            slice.iter().sum::<f32>() / slice.len() as f32
        };
        let (bass, mid, treble) = (mean(0..BASS_END), mean(BASS_END..MID_END), mean(MID_END..BANDS));

        let beat = self.detect_beat(&lin);
        self.accumulate_features(&lin, rms);

        let round = |v: f32| (v * 100.0).round() / 100.0;
        Some(Spectrum {
            bands: bands.iter().map(|&v| round(v)).collect(),
            bass: round(bass),
            mid: round(mid),
            treble: round(treble),
            level: round((rms * 3.0).min(1.0)),
            beat: round(beat),
            bpm: self.bpm.round(),
            centroid: self.centroid.round(),
            bass_ratio: round(self.bass_ratio),
            dynamics: round(self.dynamics()),
        })
    }

    /// Удар: энергия басов заметно выше среднего за последние ~1,5 с и растёт.
    fn detect_beat(&mut self, lin: &[f32; BANDS]) -> f32 {
        let energy = lin[..BASS_END.min(8)].iter().sum::<f32>() / 8.0;
        self.bass_history.push_back(energy);
        if self.bass_history.len() > 60 {
            self.bass_history.pop_front();
        }
        let avg = self.bass_history.iter().sum::<f32>() / self.bass_history.len() as f32;
        let rising = energy > self.prev_bass;
        self.prev_bass = energy;

        let now = Instant::now();
        let gap_ok = self.last_beat.is_none_or(|t| now.duration_since(t) >= MIN_BEAT_GAP);
        if self.bass_history.len() < 20 || !(gap_ok && rising && energy > avg * 1.3 && energy > 0.004) {
            return 0.0;
        }

        if let Some(prev) = self.last_beat {
            let interval = now.duration_since(prev).as_secs_f32();
            if (0.3..=1.2).contains(&interval) {
                self.intervals.push_back(interval);
                if self.intervals.len() > 12 {
                    self.intervals.pop_front();
                }
                self.update_bpm();
            }
        }
        self.last_beat = Some(now);
        ((energy / avg - 1.3) / 1.2).clamp(0.15, 1.0)
    }

    fn update_bpm(&mut self) {
        if self.intervals.len() < 4 {
            return;
        }
        let mut sorted: Vec<f32> = self.intervals.iter().cloned().collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let mut bpm = 60.0 / sorted[sorted.len() / 2];
        while bpm < 70.0 {
            bpm *= 2.0;
        }
        while bpm > 180.0 {
            bpm /= 2.0;
        }
        self.bpm = if self.bpm > 0.0 { self.bpm * 0.8 + bpm * 0.2 } else { bpm };
    }

    fn accumulate_features(&mut self, lin: &[f32; BANDS], rms: f32) {
        let total: f32 = lin.iter().sum();
        if total <= 1e-6 {
            return;
        }
        let centroid = lin.iter().zip(&self.centers).map(|(l, f)| l * f).sum::<f32>() / total;
        let bass_ratio = lin[..BASS_END].iter().sum::<f32>() / total;
        let level = (rms * 3.0).min(1.0);
        let a = FEATURE_ALPHA;
        self.centroid += (centroid - self.centroid) * a;
        self.bass_ratio += (bass_ratio - self.bass_ratio) * a;
        self.level_mean += (level - self.level_mean) * a;
        self.level_sq += (level * level - self.level_sq) * a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, amp: f32) -> Vec<f32> {
        (0..FFT_SIZE)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn state() -> State {
        State::new(FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE))
    }

    #[test]
    fn sine_lands_in_the_right_band() {
        let mut s = state();
        let frame = s.process(&sine(1000.0, 48_000, 0.5), 48_000).expect("звук");
        let (peak, _) = frame
            .bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        let center = s.centers[peak];
        assert!((800.0..1250.0).contains(&center), "пик на {center} Гц");
        assert!(frame.bands[peak] > 0.9);
    }

    #[test]
    fn volume_does_not_change_the_picture() {
        let loud = state().process(&sine(440.0, 48_000, 0.8), 48_000).unwrap();
        let quiet = state().process(&sine(440.0, 48_000, 0.05), 48_000).unwrap();
        let top = |f: &Spectrum| f.bands.iter().cloned().fold(0.0, f32::max);
        assert!((top(&loud) - top(&quiet)).abs() < 0.05);
    }

    #[test]
    fn silence_is_not_processed() {
        assert!(state().process(&vec![0.0; FFT_SIZE], 48_000).is_none());
    }

    #[test]
    fn detects_regular_kicks_and_tempo() {
        let mut s = state();
        let rate = 48_000;
        let mut beats = 0;
        // 125 уд/мин = удар каждые 480 мс = каждые 19 кадров по 25 мс
        for frame in 0..400 {
            let kick = frame % 19 == 0;
            let samples = if kick { sine(60.0, rate, 0.8) } else { sine(2000.0, rate, 0.05) };
            // искусственно двигаем время: детектор считает по Instant, поэтому ждём реальные 25 мс
            std::thread::sleep(Duration::from_millis(25));
            if s.process(&samples, rate).is_some_and(|f| f.beat > 0.0) {
                beats += 1;
            }
        }
        assert!(beats >= 12, "найдено ударов: {beats}");
        assert!((100.0..150.0).contains(&s.bpm), "темп {}", s.bpm);
    }
}
