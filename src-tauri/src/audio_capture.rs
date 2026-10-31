//! Захват всего, что играет на ПК (WASAPI loopback через cpal).
//!
//! В cpal loopback включается, если построить *входной* поток на *выходном*
//! устройстве. Звук складывается в кольцевой буфер (моно, родная частота
//! устройства); распознавание и визуализатор берут из него последние секунды.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

const KEEP_SECONDS: usize = 24;
const DEVICE_POLL: Duration = Duration::from_secs(2);
/// Если за это время не пришло ни одного пакета — звука на ПК нет.
const STALE_AFTER: Duration = Duration::from_millis(1200);

struct Ring {
    rate: u32,
    data: VecDeque<f32>,
    last_packet: Option<Instant>,
}

static RING: Mutex<Ring> = Mutex::new(Ring {
    rate: 0,
    data: VecDeque::new(),
    last_packet: None,
});
/// Выставляется в колбэке ошибки потока — менеджер пересоздаёт поток.
static STREAM_FAILED: AtomicBool = AtomicBool::new(false);

/// Фрагмент звука из буфера.
pub struct Clip {
    pub samples: Vec<f32>,
    pub rate: u32,
    /// Момент, когда был получен последний пакет (конец фрагмента).
    pub end: Instant,
}

impl Clip {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / self.rate as f32
    }
}

pub fn start() {
    thread::spawn(|| {
        // Stream нельзя передавать между потоками — он живёт здесь же.
        let mut current: Option<(String, Stream)> = None;
        loop {
            let host = cpal::default_host();
            let device = host.default_output_device();
            #[allow(deprecated)]
            let name = device.as_ref().and_then(|d| d.name().ok());

            let changed = current.as_ref().map(|(n, _)| n) != name.as_ref();
            if changed || STREAM_FAILED.swap(false, Ordering::Relaxed) {
                current = None;
                if let (Some(device), Some(name)) = (device, name) {
                    match build_stream(&device) {
                        Ok(stream) => current = Some((name, stream)),
                        Err(err) => eprintln!("[audio_capture] {err}"),
                    }
                }
            }
            thread::sleep(DEVICE_POLL);
        }
    });
}

fn build_stream(device: &cpal::Device) -> Result<Stream, String> {
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: StreamConfig = supported.config();

    if let Ok(mut ring) = RING.lock() {
        ring.rate = config.sample_rate;
        ring.data.clear();
        ring.last_packet = None;
    }

    let stream = match supported.sample_format() {
        SampleFormat::F32 => make_stream::<f32>(device, &config),
        SampleFormat::I16 => make_stream::<i16>(device, &config),
        SampleFormat::I32 => make_stream::<i32>(device, &config),
        SampleFormat::U16 => make_stream::<u16>(device, &config),
        other => return Err(format!("формат звука {other:?} не поддерживается")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn make_stream<T>(device: &cpal::Device, config: &StreamConfig) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _| push(data, channels),
        |err| {
            eprintln!("[audio_capture] поток остановлен: {err}");
            STREAM_FAILED.store(true, Ordering::Relaxed);
        },
        None,
    )
}

fn push<T>(data: &[T], channels: usize)
where
    T: Sample,
    f32: FromSample<T>,
{
    let Ok(mut ring) = RING.lock() else { return };
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
        ring.data.push_back(sum / channels as f32);
    }
    let cap = ring.rate as usize * KEEP_SECONDS;
    while ring.data.len() > cap {
        ring.data.pop_front();
    }
    ring.last_packet = Some(Instant::now());
}

/// Последние `secs` секунд звука; `None`, если буфер ещё не накопился.
pub fn recent(secs: f32) -> Option<Clip> {
    let ring = RING.lock().ok()?;
    if ring.rate == 0 {
        return None;
    }
    let want = (secs * ring.rate as f32) as usize;
    if want == 0 || ring.data.len() < want {
        return None;
    }
    Some(Clip {
        samples: ring.data.iter().skip(ring.data.len() - want).copied().collect(),
        rate: ring.rate,
        end: ring.last_packet?,
    })
}

/// Последние `n` отсчётов (недостающие в начале — нули) для спектрального анализа.
/// `None` — звука на ПК сейчас нет (пакеты не приходят).
pub fn latest(n: usize) -> Option<(Vec<f32>, u32)> {
    let ring = RING.lock().ok()?;
    let fresh = ring.last_packet.is_some_and(|t| t.elapsed() < Duration::from_millis(300));
    if ring.rate == 0 || !fresh {
        return None;
    }
    let len = ring.data.len();
    let take = n.min(len);
    let mut out = vec![0.0f32; n];
    for (dst, src) in out[n - take..].iter_mut().zip(ring.data.iter().skip(len - take)) {
        *dst = *src;
    }
    Some((out, ring.rate))
}

/// Громкость (RMS) за последние `secs` секунд; 0 — если звука на ПК нет.
pub fn level(secs: f32) -> f32 {
    let Ok(ring) = RING.lock() else { return 0.0 };
    let fresh = ring.last_packet.is_some_and(|t| t.elapsed() < STALE_AFTER);
    if !fresh || ring.rate == 0 {
        return 0.0;
    }
    let n = ((secs * ring.rate as f32) as usize).min(ring.data.len());
    if n == 0 {
        return 0.0;
    }
    let sum: f32 = ring.data.iter().skip(ring.data.len() - n).map(|s| s * s).sum();
    (sum / n as f32).sqrt()
}

/// Моно f32 любой частоты → моно i16 16 кГц (формат, который ждёт Shazam).
/// Усреднение по окну заодно работает как простой фильтр от алиасинга.
pub fn to_16k_i16(samples: &[f32], rate: u32) -> Vec<i16> {
    const TARGET: f64 = 16_000.0;
    let ratio = rate as f64 / TARGET;
    let out_len = (samples.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = (i as f64 * ratio) as usize;
        let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, samples.len());
        let avg = samples[start..end].iter().sum::<f32>() / (end - start) as f32;
        out.push((avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
    out
}
