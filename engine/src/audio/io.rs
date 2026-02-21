use crate::mutex::UnsafeMutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wavers::Samples;

#[derive(Debug, Clone)]
pub struct AudioIO {
    pub connections: Arc<UnsafeMutex<Vec<Arc<Self>>>>,
    pub connection_count: Arc<AtomicUsize>,
    pub buffer: Arc<UnsafeMutex<Samples<f32>>>,
    pub finished: Arc<UnsafeMutex<bool>>,
}

impl AudioIO {
    pub fn new(size: usize) -> Self {
        Self {
            connections: Arc::new(UnsafeMutex::new(vec![])),
            connection_count: Arc::new(AtomicUsize::new(0)),
            buffer: Arc::new(UnsafeMutex::new(Samples::new(
                vec![0.0; size].into_boxed_slice(),
            ))),
            finished: Arc::new(UnsafeMutex::new(false)),
        }
    }

    pub fn connect(from: &Arc<Self>, to: &Arc<Self>) {
        let to_len = {
            let conns = to.connections.lock();
            conns.push(from.clone());
            conns.len()
        };
        to.connection_count.store(to_len, Ordering::Relaxed);

        let from_len = {
            let conns = from.connections.lock();
            conns.push(to.clone());
            conns.len()
        };
        from.connection_count.store(from_len, Ordering::Relaxed);
    }

    pub fn disconnect(from: &Arc<Self>, to: &Arc<Self>) -> Result<(), String> {
        let to_conns = to.connections.lock();
        let to_original_len = to_conns.len();
        to_conns.retain(|conn| !Arc::ptr_eq(conn, from));
        to.connection_count.store(to_conns.len(), Ordering::Relaxed);

        let from_conns = from.connections.lock();
        from_conns.retain(|conn| !Arc::ptr_eq(conn, to));
        from.connection_count
            .store(from_conns.len(), Ordering::Relaxed);

        if to_conns.len() < to_original_len {
            Ok(())
        } else {
            Err("Connection not found".to_string())
        }
    }

    pub fn process(&self) {
        let local_buf = self.buffer.lock();

        local_buf.fill(0.0);
        for source in self.connections.lock() {
            let source_buf = source.buffer.lock();
            for (out_sample, in_sample) in local_buf.iter_mut().zip(source_buf.iter()) {
                *out_sample += *in_sample;
            }
        }
        *self.finished.lock() = true;
    }

    pub fn setup(&self) {
        *self.finished.lock() = false;
    }

    pub fn ready(&self) -> bool {
        if *self.finished.lock() {
            return true;
        }
        if self.connection_count.load(Ordering::Relaxed) == 0 {
            return true;
        }
        for conn in self.connections.lock() {
            if !*conn.finished.lock() {
                return false;
            }
        }
        true
    }
}

impl PartialEq for AudioIO {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer)
    }
}

impl Eq for AudioIO {}
