use crate::mutex::UnsafeMutex;
use std::sync::Arc;
use wavers::Samples;

#[derive(Debug, Clone)]
pub struct AudioIO {
    pub connections: Arc<UnsafeMutex<Vec<Arc<Self>>>>,
    pub buffer: Arc<UnsafeMutex<Samples<f32>>>,
}

impl AudioIO {
    pub fn new(size: usize) -> Self {
        Self {
            connections: Arc::new(UnsafeMutex::new(vec![])),
            buffer: Arc::new(UnsafeMutex::new(Samples::new(
                vec![0.0; size].into_boxed_slice(),
            ))),
        }
    }

    pub fn connect(&self, to: Arc<Self>) {
        self.connections.lock().push(to);
    }

    pub fn disconnect(&self, to: &Arc<Self>) -> Result<(), String> {
        let conns = self.connections.lock();
        let original_len = conns.len();

        conns.retain(|conn| !Arc::ptr_eq(conn, to));
        if conns.len() < original_len {
            Ok(())
        } else {
            Err("Connection not found".to_string())
        }
    }

    pub fn process(&self) {
        let local_buf = self.buffer.lock();
        let conns = self.connections.lock();

        local_buf.fill(0.0);
        for source in conns.iter() {
            let source_buf = source.buffer.lock();
            for (out_sample, in_sample) in local_buf.iter_mut().zip(source_buf.iter()) {
                *out_sample += *in_sample;
            }
        }
    }
}

impl PartialEq for AudioIO {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer)
    }
}

impl Eq for AudioIO {}
