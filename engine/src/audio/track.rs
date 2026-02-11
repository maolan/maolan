use super::{clip::AudioClip, io::AudioIO};
use crate::mutex::UnsafeMutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct AudioTrack {
    pub clips: Vec<AudioClip>,
    pub ins: Vec<Arc<UnsafeMutex<Box<AudioIO>>>>,
    pub outs: Vec<Arc<UnsafeMutex<Box<AudioIO>>>>,
}

impl AudioTrack {
    pub fn new(ins: usize, outs: usize) -> Self {
        let mut ret = Self {
            clips: vec![],
            ins: vec![],
            outs: vec![],
        };
        for _ in 0..ins {
            ret.ins
                .push(Arc::new(UnsafeMutex::new(Box::new(AudioIO::new()))));
        }
        for _ in 0..outs {
            ret.outs
                .push(Arc::new(UnsafeMutex::new(Box::new(AudioIO::new()))));
        }

        ret
    }

    pub fn connect_in(
        &mut self,
        index: usize,
        to: Arc<UnsafeMutex<Box<AudioIO>>>,
    ) -> Result<(), String> {
        if index >= self.ins.len() {
            return Err(format!(
                "Index {} is too high, as there are only {} ins",
                index,
                self.ins.len()
            ));
        }
        let myin = self.ins[index].clone();
        myin.lock().connect(to);
        Ok(())
    }

    pub fn connect_out(
        &mut self,
        index: usize,
        to: Arc<UnsafeMutex<Box<AudioIO>>>,
    ) -> Result<(), String> {
        if index >= self.outs.len() {
            return Err(format!(
                "Index {} is too high, as there are only {} outs",
                index,
                self.outs.len()
            ));
        }
        let out = self.outs[index].clone();
        out.lock().connect(to);
        Ok(())
    }

    pub fn disconnect_in(
        &mut self,
        index: usize,
        to: &Arc<UnsafeMutex<Box<AudioIO>>>,
    ) -> Result<(), String> {
        if index >= self.ins.len() {
            return Err(format!(
                "Index {} is too high, as there are only {} ins",
                index,
                self.ins.len()
            ));
        }
        let myin = self.ins[index].clone();
        myin.lock().disconnect(to)
    }

    pub fn disconnect_out(
        &mut self,
        index: usize,
        to: &Arc<UnsafeMutex<Box<AudioIO>>>,
    ) -> Result<(), String> {
        if index >= self.outs.len() {
            return Err(format!(
                "Index {} is too high, as there are only {} outs",
                index,
                self.outs.len()
            ));
        }
        let out = self.outs[index].clone();
        out.lock().disconnect(to)
    }
}
