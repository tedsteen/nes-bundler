use ringbuf::{
    SharedRb,
    storage::Heap,
    traits::{Consumer, Observer, Producer, Split},
    wrap::caching::Caching,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::Notify;

use crate::audio::MAX_AUDIO_LATENCY_MICROS;

/* ---------- ring aliases ---------- */
type Rb<T> = SharedRb<Heap<T>>;
type Cons<T> = Caching<Arc<Rb<T>>, false, true>;
type Prod<T> = Caching<Arc<Rb<T>>, true, false>;
pub type AudioConsumer = Cons<f32>;

/* ---------- constants ---------- */
/// Upstream ring capacity as a multiple of the max target buffer.
const UP_FACTOR: f64 = 6.0;
/// Maximum samples drained per pacer tick (~50 ms).
const MAX_TICK_S: f64 = 0.050;
/// Hard-stop margin: stop feeding downstream when this far above target.
const OVER_MARGIN_S: f64 = 0.010;

/* ---------- shared params ---------- */
/// All three PI parameters are updated together under a single lock so the
/// pacer thread never observes a partially-updated state.
#[derive(Clone, Copy)]
struct ParamsInner {
    target_s: f64,
    kp: f64,
    ki: f64,
}

struct Params(Mutex<ParamsInner>);

impl Params {
    fn new(target_s: f64, kp: f64, ki: f64) -> Self {
        Self(Mutex::new(ParamsInner { target_s, kp, ki }))
    }

    #[inline]
    fn get(&self) -> ParamsInner {
        *self.0.lock().unwrap()
    }

    fn set(&self, target_s: f64, kp: f64, ki: f64) {
        *self.0.lock().unwrap() = ParamsInner { target_s, kp, ki };
    }
}

/* ---------- helpers ---------- */
/// How many upstream samples the producer may queue before blocking.
/// 1× target keeps upstream transit latency equal to the configured target,
/// so total end-to-end latency ≈ 2× target rather than a large multiple.
#[inline]
fn up_thresh_samples(sr_hz: f64, target_s: f64) -> usize {
    (sr_hz * target_s).round() as usize
}

/* ---------- pacer ---------- */
struct Pacer {
    sr_hz: f64,
    integ: f64,
    frac: f64,
    last: Instant,
    params: Arc<Params>,
}
impl Pacer {
    fn new(sr_hz: f64, params: Arc<Params>) -> Self {
        Self {
            sr_hz,
            integ: 0.0,
            frac: 0.0,
            last: Instant::now(),
            params,
        }
    }

    /// How many samples to move downstream this tick.
    #[inline]
    fn compute(&mut self, dn_queued_samples: usize) -> usize {
        let ParamsInner { target_s, kp, ki } = self.params.get();

        // Hard guard: stop feeding when downstream is OVER_MARGIN_S above target.
        let over_margin = (self.sr_hz * OVER_MARGIN_S).round() as usize;
        let target_samp = (self.sr_hz * target_s).round() as usize;
        if dn_queued_samples > target_samp.saturating_add(over_margin) {
            self.frac = 0.0;
            self.integ = self.integ.min(0.0); // mild anti-windup
            self.last = Instant::now();
            return 0;
        }

        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64().max(1e-6);
        self.last = now;

        let dn_sec = (dn_queued_samples as f64) / self.sr_hz;
        let err = dn_sec - target_s;
        self.integ = (self.integ + err * dt).clamp(-target_s, target_s);

        let base = self.sr_hz * dt + self.frac;
        let corr = (kp * err + ki * self.integ) * self.sr_hz;
        let want = (base - corr).clamp(0.0, self.sr_hz * MAX_TICK_S);

        let n = want.floor() as usize;
        self.frac = want - n as f64;
        n
    }
}

/* ---------- producer ---------- */
pub struct AudioProducer {
    tx: Prod<f32>,
    space_notify: Arc<Notify>,
    params: Arc<Params>,
    sr_hz: f64,
    stop: Arc<AtomicBool>,
    _worker: Option<thread::JoinHandle<()>>,
}

impl Drop for AudioProducer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self._worker.take() {
            let _ = handle.join();
        }
    }
}

impl AudioProducer {
    #[inline]
    fn up_occ(&self) -> usize {
        self.tx.occupied_len()
    }

    pub async fn push_all(&mut self, mut data: &[f32]) {
        while !data.is_empty() {
            let thresh = up_thresh_samples(self.sr_hz, self.params.get().target_s);

            // backpressure: wait while at or over threshold
            while self.up_occ() >= thresh {
                self.space_notify.notified().await;
            }

            let room = thresh - self.up_occ();
            let wrote = self.tx.push_slice(&data[..room.min(data.len())]);
            data = &data[wrote..];

            if wrote == 0 {
                // ring full; wait for worker to drain
                self.space_notify.notified().await;
            }
        }
    }
}

/* ---------- external control ---------- */
#[derive(Clone)]
pub struct BridgeCtl {
    params: Arc<Params>,
    max_s: f64,
}
impl BridgeCtl {
    pub fn set_latency_ms(&self, latency_ms: f64) {
        let s = (latency_ms / 1000.0).min(self.max_s);
        let (kp, ki) = gains_for(s);
        self.params.set(s, kp, ki);
    }
}

/* ---------- factory ---------- */
pub fn make_paced_bridge_ringbuf_bulk_async(
    latency_ms: f64,
    device_sr_hz: f64,
) -> (AudioProducer, AudioConsumer, BridgeCtl) {
    let max_s = (MAX_AUDIO_LATENCY_MICROS as f64) / 1_000_000.0;
    let target_s = (latency_ms / 1000.0).min(max_s);

    let (kp, ki) = gains_for(target_s);
    let (up_cap, dn_cap) = caps(device_sr_hz, max_s);

    let (up_tx, mut up_rx) = Rb::<f32>::new(up_cap).split();
    let (mut dn_tx, dn_rx) = Rb::<f32>::new(dn_cap).split();

    let notify = Arc::new(Notify::new());
    let notify_w = notify.clone();

    let params = Arc::new(Params::new(target_s, kp, ki));
    let ctl = BridgeCtl {
        params: params.clone(),
        max_s,
    };

    let mut pacer = Pacer::new(device_sr_hz, params.clone());

    // scratch buffer sized for one MAX_TICK_S window plus a small pad
    let scratch = (device_sr_hz * MAX_TICK_S).ceil() as usize + 64;
    let mut buf = vec![0.0f32; scratch];

    let stop = Arc::new(AtomicBool::new(false));
    let stop_w = stop.clone();

    let _worker = thread::Builder::new()
        .name("audio-pacer".into())
        .spawn(move || {
            while !stop_w.load(Ordering::Acquire) {
                // wake producers when upstream drops back to or below the write threshold
                let wake_threshold = up_thresh_samples(pacer.sr_hz, pacer.params.get().target_s);

                // move paced chunk downstream
                let want = pacer.compute(dn_tx.occupied_len()).min(buf.len());
                let pulled = if want > 0 {
                    up_rx.pop_slice(&mut buf[..want])
                } else {
                    0
                };

                if pulled > 0 {
                    let _ = dn_tx.push_slice(&buf[..pulled]);
                }

                // wake producers when upstream backlog is sufficiently low OR nothing was pulled
                if up_rx.occupied_len() <= wake_threshold || pulled == 0 {
                    notify_w.notify_waiters();
                }

                // 1ms tick is plenty; avoids hot spin
                thread::sleep(Duration::from_millis(1));
            }
        })
        .expect("failed to spawn audio-pacer thread");

    let producer = AudioProducer {
        tx: up_tx,
        space_notify: notify,
        params,
        sr_hz: device_sr_hz,
        stop,
        _worker: Some(_worker),
    };

    (producer, dn_rx, ctl)
}

/* ---------- heuristics ---------- */
#[inline]
fn gains_for(target_s: f64) -> (f64, f64) {
    // PI tuning: at 30 ms target kp ≈ 0.15; scales inversely with target.
    // ki is set ~100× target slower than kp, clamped to a safe range.
    let kp = (0.15 * 0.030) / target_s;
    let ki = (kp / (100.0 * target_s)).clamp(0.01, 0.12);
    (kp, ki)
}

#[inline]
fn caps(sr_hz: f64, max_target_s: f64) -> (usize, usize) {
    let up = (sr_hz * max_target_s * UP_FACTOR).ceil() as usize;
    let dn = (sr_hz * max_target_s * 3.0).ceil() as usize;
    (up.max(256), dn.max(256))
}
