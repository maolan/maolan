use crate::{
    hw::config,
    hw::traits::{HwMidiHub, HwWorkerDriver},
    message::{HwMidiEvent, Message},
    mutex::UnsafeMutex,
};
#[cfg(unix)]
use nix::libc;
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::error;

pub trait Backend: Send + Sync + 'static {
    type Driver: HwWorkerDriver + Send + 'static;
    type MidiHub: HwMidiHub + Send + 'static;

    const LABEL: &'static str;
    const WORKER_THREAD_NAME: &'static str;
    const ASSIST_THREAD_NAME: &'static str;
    const ASSIST_AUTONOMOUS_ENV: &'static str;
}

#[derive(Debug)]
pub struct HwWorker<B: Backend> {
    driver: Arc<UnsafeMutex<B::Driver>>,
    midi_hub: Arc<UnsafeMutex<B::MidiHub>>,
    rx: Receiver<Message>,
    tx: Sender<Message>,
    cycle_frames: u32,
    pending_midi_out_events: Vec<HwMidiEvent>,
    midi_in_events: Vec<HwMidiEvent>,
    pending_midi_out_sorted: bool,
    _backend: PhantomData<B>,
}

#[derive(Debug, Default)]
struct AssistState {
    shutdown: bool,
    request_seq: u64,
    done_seq: u64,
    last_error: Option<String>,
}

#[cfg(unix)]
const RT_POLICY: i32 = libc::SCHED_FIFO;
const RT_PRIORITY_WORKER: i32 = 18;
const RT_PRIORITY_ASSIST: i32 = 12;
const PROFILE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct AssistProfiler {
    report_at: Instant,
    cycle_count: u64,
    cycle_err_count: u64,
    cycle_time_ns: u128,
    step_count: u64,
    step_work_count: u64,
    step_err_count: u64,
    step_time_ns: u128,
    wait_count: u64,
    wait_time_ns: u128,
}

impl AssistProfiler {
    fn new() -> Self {
        Self {
            report_at: Instant::now() + PROFILE_INTERVAL,
            cycle_count: 0,
            cycle_err_count: 0,
            cycle_time_ns: 0,
            step_count: 0,
            step_work_count: 0,
            step_err_count: 0,
            step_time_ns: 0,
            wait_count: 0,
            wait_time_ns: 0,
        }
    }

    fn maybe_report(&mut self, cycle_samples: usize, sample_rate: i32, label: &str) {
        let now = Instant::now();
        if now < self.report_at {
            return;
        }
        let cycle_avg_us = if self.cycle_count > 0 {
            (self.cycle_time_ns / self.cycle_count as u128) as f64 / 1_000.0
        } else {
            0.0
        };
        let step_avg_us = if self.step_count > 0 {
            (self.step_time_ns / self.step_count as u128) as f64 / 1_000.0
        } else {
            0.0
        };
        let wait_avg_us = if self.wait_count > 0 {
            (self.wait_time_ns / self.wait_count as u128) as f64 / 1_000.0
        } else {
            0.0
        };
        let expected_cycles_per_sec = if cycle_samples > 0 && sample_rate > 0 {
            sample_rate as f64 / cycle_samples as f64
        } else {
            0.0
        };
        error!(
            "{} profile: expected_cps={:.1} cycles={} cycle_err={} cycle_avg_us={:.1} steps={} steps_work={} step_err={} step_avg_us={:.1} waits={} wait_avg_us={:.1}",
            label,
            expected_cycles_per_sec,
            self.cycle_count,
            self.cycle_err_count,
            cycle_avg_us,
            self.step_count,
            self.step_work_count,
            self.step_err_count,
            step_avg_us,
            self.wait_count,
            wait_avg_us
        );
        self.report_at = now + PROFILE_INTERVAL;
        self.cycle_count = 0;
        self.cycle_err_count = 0;
        self.cycle_time_ns = 0;
        self.step_count = 0;
        self.step_work_count = 0;
        self.step_err_count = 0;
        self.step_time_ns = 0;
        self.wait_count = 0;
        self.wait_time_ns = 0;
    }
}

impl<B: Backend> HwWorker<B> {
    fn profile_enabled() -> bool {
        config::env_flag(config::HW_PROFILE_ENV)
    }

    fn assist_autonomous_enabled() -> bool {
        config::env_flag(B::ASSIST_AUTONOMOUS_ENV)
    }

    fn configure_rt_thread(name: &str, priority: i32) -> Result<(), String> {
        #[cfg(unix)]
        {
            let thread = unsafe { libc::pthread_self() };
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let c_name = std::ffi::CString::new(name).map_err(|e| e.to_string())?;
            #[cfg(target_os = "linux")]
            unsafe {
                let _ = libc::pthread_setname_np(thread, c_name.as_ptr());
            }
            #[cfg(target_os = "freebsd")]
            unsafe {
                let _ = libc::pthread_set_name_np(thread, c_name.as_ptr());
            }

            let param = unsafe {
                let mut p = std::mem::zeroed::<libc::sched_param>();
                p.sched_priority = priority;
                p
            };
            let rc = unsafe { libc::pthread_setschedparam(thread, RT_POLICY, &param) };
            if rc != 0 {
                return Err(format!(
                    "pthread_setschedparam({}, prio {}) failed with errno {}",
                    name, priority, rc
                ));
            }

            let mut actual_policy = 0_i32;
            let mut actual_param = unsafe { std::mem::zeroed::<libc::sched_param>() };
            let rc = unsafe {
                libc::pthread_getschedparam(thread, &mut actual_policy, &mut actual_param)
            };
            if rc != 0 {
                return Err(format!(
                    "pthread_getschedparam({}) failed with errno {}",
                    name, rc
                ));
            }
            if actual_policy != RT_POLICY || actual_param.sched_priority != priority {
                return Err(format!(
                    "realtime verification failed for {}: policy {}, prio {}",
                    name, actual_policy, actual_param.sched_priority
                ));
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = name;
            let _ = priority;
            Err("Realtime thread priority is not supported on this platform".to_string())
        }
    }

    fn lock_memory_pages() -> Result<(), String> {
        #[cfg(unix)]
        {
            let rc = unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) };
            if rc == 0 {
                Ok(())
            } else {
                Err(format!(
                    "mlockall(MCL_CURRENT|MCL_FUTURE) failed: {}",
                    std::io::Error::last_os_error()
                ))
            }
        }
        #[cfg(not(unix))]
        {
            Err("mlockall is not supported on this platform".to_string())
        }
    }

    pub fn new(
        driver: Arc<UnsafeMutex<B::Driver>>,
        midi_hub: Arc<UnsafeMutex<B::MidiHub>>,
        rx: Receiver<Message>,
        tx: Sender<Message>,
    ) -> Self {
        let cycle_frames = {
            let d = driver.lock();
            d.cycle_samples() as u32
        };
        Self {
            driver,
            midi_hub,
            rx,
            tx,
            cycle_frames,
            pending_midi_out_events: vec![],
            midi_in_events: Vec::with_capacity(64),
            pending_midi_out_sorted: true,
            _backend: PhantomData,
        }
    }

    pub async fn work(mut self) {
        if let Err(e) = Self::lock_memory_pages() {
            error!("{} worker memory lock not enabled: {}", B::LABEL, e);
        }
        if let Err(e) = Self::configure_rt_thread(B::WORKER_THREAD_NAME, RT_PRIORITY_WORKER) {
            error!("{} worker realtime priority not enabled: {}", B::LABEL, e);
        }
        #[cfg(target_os = "macos")]
        unsafe {
            libc::pthread_set_qos_class_self_np(
                libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
                0,
            );
        }
        let assist_state = Arc::new((Mutex::new(AssistState::default()), Condvar::new()));
        let assist_handle = Self::start_assist_thread(self.driver.clone(), assist_state.clone());
        loop {
            match self.rx.recv().await {
                Some(msg) => match msg {
                    Message::Request(crate::message::Action::Quit) => {
                        Self::stop_assist_thread(&assist_state, assist_handle);
                        return;
                    }
                    Message::TracksFinished => {
                        {
                            let midi_hub = self.midi_hub.lock();
                            midi_hub.read_events_into(&mut self.midi_in_events);
                        }
                        spread_hw_event_frames(&mut self.midi_in_events, self.cycle_frames);
                        if !self.midi_in_events.is_empty() {
                            let cap = self.midi_in_events.capacity();
                            let out = std::mem::replace(
                                &mut self.midi_in_events,
                                Vec::with_capacity(cap.max(64)),
                            );
                            if let Err(e) = self.tx.send(Message::HWMidiEvents(out)).await {
                                error!(
                                    "{} worker failed to send HWMidiEvents to engine: {}",
                                    B::LABEL,
                                    e
                                );
                            }
                        }
                        {
                            if !self.pending_midi_out_events.is_empty() {
                                if !self.pending_midi_out_sorted {
                                    self.pending_midi_out_events.sort_by(|a, b| {
                                        a.event
                                            .frame
                                            .cmp(&b.event.frame)
                                            .then_with(|| a.device.cmp(&b.device))
                                    });
                                    self.pending_midi_out_sorted = true;
                                }
                                let midi_hub = self.midi_hub.lock();
                                midi_hub.write_events(&self.pending_midi_out_events);
                                self.pending_midi_out_events.clear();
                            }
                        }
                        if let Err(e) = Self::run_assist_cycle(&assist_state) {
                            error!("{} assist cycle error: {}", B::LABEL, e);
                        }
                        if let Err(e) = self.tx.send(Message::HWFinished).await {
                            error!(
                                "{} worker failed to send HWFinished to engine: {}",
                                B::LABEL,
                                e
                            );
                        }
                    }
                    Message::HWMidiOutEvents(mut events) => {
                        self.pending_midi_out_events.append(&mut events);
                        self.pending_midi_out_sorted = false;
                    }
                    _ => {}
                },
                None => {
                    Self::stop_assist_thread(&assist_state, assist_handle);
                    return;
                }
            }
        }
    }

    fn start_assist_thread(
        driver: Arc<UnsafeMutex<B::Driver>>,
        assist_state: Arc<(Mutex<AssistState>, Condvar)>,
    ) -> JoinHandle<()> {
        let profile = Self::profile_enabled();
        let autonomous = Self::assist_autonomous_enabled();
        std::thread::spawn(move || {
            if let Err(e) = Self::configure_rt_thread(B::ASSIST_THREAD_NAME, RT_PRIORITY_ASSIST) {
                error!("{} assist realtime priority not enabled: {}", B::LABEL, e);
            }
            #[cfg(target_os = "macos")]
            unsafe {
                libc::pthread_set_qos_class_self_np(
                    libc::qos_class_t::QOS_CLASS_USER_INITIATED,
                    0,
                );
            }
            let mut profiler = if profile {
                let (cycle_samples, sample_rate) = {
                    let d = driver.lock();
                    (d.cycle_samples(), d.sample_rate())
                };
                error!(
                    "{} profile enabled: cycle_samples={} sample_rate={} expected_cps={:.1}",
                    B::LABEL,
                    cycle_samples,
                    sample_rate,
                    if cycle_samples > 0 {
                        sample_rate as f64 / cycle_samples as f64
                    } else {
                        0.0
                    }
                );
                Some(AssistProfiler::new())
            } else {
                None
            };
            let (lock, cvar) = &*assist_state;
            loop {
                let (shutdown, has_request, target) = {
                    let st = lock.lock().expect("assist mutex poisoned");
                    (st.shutdown, st.request_seq > st.done_seq, st.request_seq)
                };
                if shutdown {
                    break;
                }
                if has_request {
                    let started = Instant::now();
                    let run_error = {
                        let d = driver.lock();
                        d.run_cycle_for_worker().err().map(|e| e.to_string())
                    };
                    if let Some(p) = profiler.as_mut() {
                        p.cycle_count += 1;
                        if run_error.is_some() {
                            p.cycle_err_count += 1;
                        }
                        p.cycle_time_ns += started.elapsed().as_nanos();
                        let (cycle_samples, sample_rate) = {
                            let d = driver.lock();
                            (d.cycle_samples(), d.sample_rate())
                        };
                        p.maybe_report(cycle_samples, sample_rate, B::LABEL);
                    }
                    let mut st = lock.lock().expect("assist mutex poisoned");
                    st.done_seq = st.done_seq.max(target);
                    st.last_error = run_error;
                    cvar.notify_all();
                    continue;
                }

                if !autonomous {
                    let st = lock.lock().expect("assist mutex poisoned");
                    if st.shutdown {
                        break;
                    }
                    let wait_started = Instant::now();
                    let _guard = cvar.wait(st).expect("assist condvar failed");
                    if let Some(p) = profiler.as_mut() {
                        p.wait_count += 1;
                        p.wait_time_ns += wait_started.elapsed().as_nanos();
                    }
                    continue;
                }

                let started = Instant::now();
                let did_work = {
                    let d = driver.lock();
                    match d.run_assist_step_for_worker() {
                        Ok(v) => v,
                        Err(e) => {
                            if let Some(p) = profiler.as_mut() {
                                p.step_err_count += 1;
                            }
                            let mut st = lock.lock().expect("assist mutex poisoned");
                            st.last_error = Some(e.to_string());
                            cvar.notify_all();
                            false
                        }
                    }
                };
                if let Some(p) = profiler.as_mut() {
                    p.step_count += 1;
                    if did_work {
                        p.step_work_count += 1;
                    }
                    p.step_time_ns += started.elapsed().as_nanos();
                    let (cycle_samples, sample_rate) = {
                        let d = driver.lock();
                        (d.cycle_samples(), d.sample_rate())
                    };
                    p.maybe_report(cycle_samples, sample_rate, B::LABEL);
                }
                if !did_work {
                    let st = lock.lock().expect("assist mutex poisoned");
                    if st.shutdown {
                        break;
                    }
                    let wait_started = Instant::now();
                    let _guard = cvar.wait(st).expect("assist condvar failed");
                    if let Some(p) = profiler.as_mut() {
                        p.wait_count += 1;
                        p.wait_time_ns += wait_started.elapsed().as_nanos();
                    }
                }
            }
        })
    }

    fn run_assist_cycle(assist_state: &Arc<(Mutex<AssistState>, Condvar)>) -> Result<(), String> {
        let (lock, cvar) = &**assist_state;
        let mut st = lock
            .lock()
            .map_err(|_| "assist mutex poisoned".to_string())?;
        st.request_seq = st.request_seq.saturating_add(1);
        let target = st.request_seq;
        cvar.notify_one();
        while st.done_seq < target && !st.shutdown {
            st = cvar
                .wait(st)
                .map_err(|_| "assist condvar wait failed".to_string())?;
        }
        if let Some(err) = st.last_error.take() {
            return Err(err);
        }
        Ok(())
    }

    fn stop_assist_thread(
        assist_state: &Arc<(Mutex<AssistState>, Condvar)>,
        assist_handle: JoinHandle<()>,
    ) {
        let (lock, cvar) = &**assist_state;
        if let Ok(mut st) = lock.lock() {
            st.shutdown = true;
            cvar.notify_all();
        }
        let _ = assist_handle.join();
    }
}

fn spread_hw_event_frames(events: &mut [HwMidiEvent], frames: u32) {
    if events.len() <= 1 || frames <= 1 {
        return;
    }
    let n = events.len() as u32;
    for (idx, event) in events.iter_mut().enumerate() {
        let pos = idx as u32;
        event.event.frame = ((pos as u64 * (frames - 1) as u64) / n as u64) as u32;
    }
}
