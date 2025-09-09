use ringbuf::{
    SharedRb,
    storage::Heap,
    traits::{Consumer, Observer, Producer, Split},
    wrap::caching::Caching,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::Notify;

type RingbufType<T> = SharedRb<Heap<T>>;
type SampleConsumer<T> = Caching<Arc<RingbufType<T>>, false, true>;
type SampleProducer<T> = Caching<Arc<RingbufType<T>>, true, false>;

pub type AudioConsumer = SampleConsumer<f32>;

pub struct PacerSec {
    pub device_sr_hz: f64,
    pub target_sec: f64,
    pub kp: f64,
    pub ki: f64,
    integ: f64,
    last: Instant,
    frac: f64,
}
impl PacerSec {
    pub fn new(sr: f64, target: f64, kp: f64, ki: f64) -> Self {
        Self {
            device_sr_hz: sr,
            target_sec: target,
            kp,
            ki,
            integ: 0.0,
            last: Instant::now(),
            frac: 0.0,
        }
    }
    pub fn compute(&mut self, queued_samples: usize) -> usize {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64().max(1e-6);
        self.last = now;

        let queued_sec = (queued_samples as f64) / self.device_sr_hz;
        let err = queued_sec - self.target_sec;
        self.integ = (self.integ + err * dt).clamp(-self.target_sec, self.target_sec);

        let base = self.device_sr_hz * dt + self.frac;
        let corr = (self.kp * err + self.ki * self.integ) * self.device_sr_hz;
        let want = base - corr;

        let n = want.floor().clamp(0.0, self.device_sr_hz * 0.050) as usize; // ≤50ms/tick
        self.frac = want - n as f64;
        n
    }
}

pub struct AudioProducer {
    tx: SampleProducer<f32>,
    space_notify: Arc<Notify>,
}
impl AudioProducer {
    pub async fn push_all(&mut self, mut data: &[f32]) {
        while !data.is_empty() {
            let wrote = self.tx.push_slice(data);
            data = &data[wrote..];
            if !data.is_empty() {
                self.space_notify.notified().await;
            }
        }
    }
}

pub struct BridgeGuard {
    _stop: Arc<AtomicBool>,
    _worker: thread::JoinHandle<()>,
}

pub fn make_paced_bridge_ringbuf_bulk_async(
    latency_ms: f64,
    device_sr_hz: f64,
) -> (AudioProducer, SampleConsumer<f32>, BridgeGuard) {
    let target_sec = (latency_ms) / 1000.0;
    let (kp, ki) = auto_gains_for_latency(target_sec);
    let (up_cap, dn_cap) = bridge_caps(device_sr_hz, target_sec);

    let (up_tx, mut up_rx) = RingbufType::<f32>::new(up_cap).split();
    let (mut dn_tx, dn_rx) = RingbufType::<f32>::new(dn_cap).split();

    let space_notify = Arc::new(Notify::new());
    let space_notify_worker = space_notify.clone();

    let _stop = Arc::new(AtomicBool::new(false));
    let stop_worker = _stop.clone();

    let mut pacer = PacerSec::new(device_sr_hz, target_sec, kp, ki);
    let mut scratch = vec![0.0f32; (device_sr_hz * 0.050).ceil() as usize + 64]; // ~≤50ms

    let worker = thread::spawn(move || {
        loop {
            if stop_worker.load(Ordering::Relaxed) {
                break;
            }

            let want = pacer.compute(dn_tx.occupied_len()).min(scratch.len());
            //println!("WAINT {want}");
            if want > 0 {
                // read up to `want` from upstream in one go
                let pulled = up_rx.pop_slice(&mut scratch[..want]);
                if pulled > 0 {
                    // write as much as fits downstream
                    let _pushed = dn_tx.push_slice(&scratch[..pulled]);

                    //println!("Shuffled {_pushed} bytes");

                    // if downstream couldn’t take it all, the remainder will be retried next loop
                    // signal the producer there is space upstream now
                    space_notify_worker.notify_waiters();
                }
            }

            // gentle cadence
            thread::sleep(Duration::from_millis(1));
        }
    });

    (
        AudioProducer {
            tx: up_tx,
            space_notify,
        },
        dn_rx,
        BridgeGuard {
            _stop,
            _worker: worker,
        },
    )
}

fn auto_gains_for_latency(target_sec: f64) -> (f64, f64) {
    let t = target_sec.clamp(0.010, 0.080);
    let kp = (0.15 * 0.030) / t; // same kp as above
    let gamma = 100.0; // integral time ~100× target
    let ki = (kp / (gamma * t)).clamp(0.01, 0.12);
    (kp, ki)
}
fn bridge_caps(device_sr_hz: f64, target_sec: f64) -> (usize, usize) {
    let up = (device_sr_hz * target_sec * 6.0).ceil() as usize; // upstream ~6× target
    let dn = (device_sr_hz * target_sec * 3.0).ceil() as usize; // downstream ~3× target
    (up.max(256), dn.max(256))
}
