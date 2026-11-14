use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use wasapi::*;

const BANDS: usize = 8;
const FFT_SIZE: usize = 2048;
const SAMPLE_RATE: f32 = 48000.0;
const MIN_HZ: f32 = 60.0;
const MAX_HZ: f32 = 12000.0;


pub type Bands = Arc<Mutex<Vec<f32>>>;

pub fn find_pid(name: &str) -> Option<u32> {
    let refreshes = RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing());
    let Ok(system) = System::new_with_specifics(refreshes) else {
        return None
    };
    let matches: Vec<_> = system
        .processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(name))
        .collect();
    // the top-level one: its parent isn't another copy of the same program
    let root = matches
        .iter()
        .filter(|p| !p.parent().map_or(false, |pp| matches.iter().any(|m| m.pid() == pp)))
        .max_by_key(|p| p.start_time());
    root.or(matches.first()).map(|p| p.pid().as_u32())
}

// Runs forever on its own thread. Looks for Apple Music, captures from it.
// and if that fails waits 2 seconds and tries agains
pub fn start(bands: Bands) {
    thread::spawn(move || loop {
        let names = crate::media::split_list(&crate::config::get().process_name);
        let index = crate::media::ACTIVE_APP
            .load(Ordering::Relaxed)
            .min(names.len().saturating_sub(1));
        if let Some(pid) = names.get(index).and_then(|n| find_pid(n)) {
            crate::log(&format!("Capturing audio from process {pid}"));
            if let Err(e) = capture_loop(pid, index, &bands) {
                crate::log(&format!("Capture stopped: {e}"));
            }
        }
        *bands.lock().unwrap() = vec![0.0; BANDS];
        thread::sleep(Duration::from_secs(2));
    });
}

fn capture_loop(pid: u32, index: usize, bands: &Bands) -> Result<(), Box<dyn Error>> {
    initialize_mta().ok()?;

    // 32-bit float, 48000 Hz, 2 channels (stereo)
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
    let mut client = AudioClient::new_application_loopback_client(pid, true)?;
    let mode = StreamMode::EventsShared {
        autoconvert: true, 
        buffer_duration_hns: 0,
    };
    client.initialize_client(&format, &Direction::Capture, &mode)?;
    let h_event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;

    // Prepare the FFT once: the planner, the window shape, and its scaling
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos()
        })
        .collect();
    let win_scale = 2.0 / window.iter().sum::<f32>();

    let mut queue: VecDeque<u8> = VecDeque::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut last_check = Instant::now();

    loop {
        if capture.get_next_packet_size()?.unwrap_or(0) > 0 {
            capture.read_from_device_to_deque(&mut queue)?;
        }

        // Stereo pairs (left, right) -> one mono sample
        for frame in queue.make_contiguous().chunks_exact(8) {
            let l = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
            let r = f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
            mono.push((l + r) / 2.0);
        }
        queue.clear();

        // Enough samples for one FFT: analyse them, then start collecting again
        if mono.len() >= FFT_SIZE * 2 {
            let start = mono.len() - FFT_SIZE;
            let result = compute_bands(&mono[start..], &fft, &window, win_scale);
            *bands.lock().unwrap() = result;
            mono.clear();
        }

        // If nothing arrives for a while (paused), show silence
        if h_event.wait_for_event(1000).is_err() {
            *bands.lock().unwrap() = vec![0.0; BANDS];
            mono.clear();
        }
        if last_check.elapsed() >= Duration::from_secs(3) {
            last_check = Instant::now();
            let names = crate::media::split_list(&crate::config::get().process_name);
            let active = crate::media::ACTIVE_APP.load(Ordering::Relaxed);
            if active != index || names.get(index).and_then(|n| find_pid(n)) != Some(pid) { 
                crate::log(&format!("Player process changed, reconnecting"));
                return Ok(());
            }
        }
    };
}


#[allow(non_snake_case)]
fn compute_bands(
    samples: &[f32],
    fft: &Arc<dyn Fft<f32>>,
    window: &[f32],
    win_scale: f32,
) -> Vec<f32> {
    let cfg = crate::config::get();
    let (DB_FLOOR, DB_CEIL, TILT_DB_PER_BAND) = (cfg.eq_floor_db, cfg.eq_ceil_db, cfg.eq_tilt_db);
    let range = (DB_CEIL - DB_FLOOR).max(1.0);  // guards against a bad config (ceil <= floor)
    let mut input: Vec<Complex<f32>> = samples
        .iter()
        .zip(window)
        .map(|(s, w) | Complex::new(s * w, 0.0))
        .collect();
    fft.process(&mut input);

    let mags: Vec<f32> = input[..FFT_SIZE / 2]
        .iter()
        .map(|c| c.norm() * win_scale)
        .collect();

    // 8 bands spaced evenly on a log scale between MIN_HZ and MAX_HZ
    let ratio = (MAX_HZ / MIN_HZ).powf(1.0 / BANDS as f32);
    let bin_hz = SAMPLE_RATE / FFT_SIZE as f32;

    let mut out = Vec::with_capacity(BANDS);
    for b in 0..BANDS {
        let lo_hz = MIN_HZ * ratio.powi(b as i32);
        let hi_hz = lo_hz * ratio;
        let lo = (lo_hz / bin_hz).floor() as usize;
        let hi = ((hi_hz / bin_hz).ceil() as usize).min(mags.len())
            .max(lo + 1)
            .min(mags.len());
        let slice = &mags[lo..hi];
        let avg = slice.iter().sum::<f32>() / slice.len() as f32;

        let db = 20.0 * (avg + 1e-9).log10() + TILT_DB_PER_BAND * b as f32;
        out.push(((db - DB_FLOOR) / range).clamp(0.0, 1.0));
    }
    out
}