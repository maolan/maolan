use std::ffi::c_uint;

type RubberBandOptions = i32;
type RubberBandLiveState = *mut RubberBandLiveStateOpaque;

#[repr(C)]
struct RubberBandLiveStateOpaque {
    _private: [u8; 0],
}

const RUBBERBAND_OPTION_PROCESS_REAL_TIME: RubberBandOptions = 0x00000001;
const RUBBERBAND_OPTION_THREADING_NEVER: RubberBandOptions = 0x00010000;
const RUBBERBAND_OPTION_WINDOW_SHORT: RubberBandOptions = 0x00100000;
const RUBBERBAND_OPTION_FORMANT_PRESERVED: RubberBandOptions = 0x01000000;
const RUBBERBAND_OPTION_PITCH_HIGH_CONSISTENCY: RubberBandOptions = 0x04000000;
const RUBBERBAND_OPTION_CHANNELS_TOGETHER: RubberBandOptions = 0x10000000;

#[link(name = "rubberband")]
unsafe extern "C" {
    fn rubberband_live_new(
        sample_rate: c_uint,
        channels: c_uint,
        options: RubberBandOptions,
    ) -> RubberBandLiveState;
    fn rubberband_live_delete(state: RubberBandLiveState);
    fn rubberband_live_reset(state: RubberBandLiveState);
    fn rubberband_live_set_pitch_scale(state: RubberBandLiveState, scale: f64);
    fn rubberband_live_set_formant_option(state: RubberBandLiveState, options: RubberBandOptions);
    fn rubberband_live_get_start_delay(state: RubberBandLiveState) -> c_uint;
    fn rubberband_live_get_block_size(state: RubberBandLiveState) -> c_uint;
    fn rubberband_live_shift(
        state: RubberBandLiveState,
        input: *const *const f32,
        output: *const *mut f32,
    );
}

#[derive(Debug)]
pub struct LivePitchShifter {
    state: RubberBandLiveState,
    block_size: usize,
    start_delay_remaining: usize,
    pending_offset: usize,
    pending_len: usize,
    next_input_frame: usize,
    next_output_frame: usize,
    formant_preserved: bool,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    input_ptrs: Vec<*const f32>,
    output_ptrs: Vec<*mut f32>,
}

unsafe impl Send for LivePitchShifter {}

impl LivePitchShifter {
    pub fn new(sample_rate: usize, channels: usize, formant_preserved: bool) -> Result<Self, String> {
        let mut options = RUBBERBAND_OPTION_PROCESS_REAL_TIME
            | RUBBERBAND_OPTION_THREADING_NEVER
            | RUBBERBAND_OPTION_WINDOW_SHORT
            | RUBBERBAND_OPTION_PITCH_HIGH_CONSISTENCY;
        if channels > 1 {
            options |= RUBBERBAND_OPTION_CHANNELS_TOGETHER;
        }
        if formant_preserved {
            options |= RUBBERBAND_OPTION_FORMANT_PRESERVED;
        }
        let state =
            unsafe { rubberband_live_new(sample_rate as c_uint, channels as c_uint, options) };
        if state.is_null() {
            return Err("Failed to initialize Rubber Band live shifter".to_string());
        }
        let block_size = unsafe { rubberband_live_get_block_size(state) as usize }.max(1);
        let start_delay_remaining =
            unsafe { rubberband_live_get_start_delay(state) as usize };
        let input = vec![vec![0.0; block_size]; channels.max(1)];
        let output = vec![vec![0.0; block_size]; channels.max(1)];
        let input_ptrs = input.iter().map(|channel| channel.as_ptr()).collect();
        let output_ptrs = output
            .iter()
            .map(|channel| channel.as_ptr() as *mut f32)
            .collect();
        Ok(Self {
            state,
            block_size,
            start_delay_remaining,
            pending_offset: 0,
            pending_len: 0,
            next_input_frame: 0,
            next_output_frame: 0,
            formant_preserved,
            input,
            output,
            input_ptrs,
            output_ptrs,
        })
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn reset(&mut self, output_frame: usize) {
        unsafe { rubberband_live_reset(self.state) };
        self.start_delay_remaining =
            unsafe { rubberband_live_get_start_delay(self.state) as usize };
        self.pending_offset = 0;
        self.pending_len = 0;
        self.next_input_frame = output_frame;
        self.next_output_frame = output_frame;
    }

    pub fn set_formant_preserved(&mut self, formant_preserved: bool) {
        if self.formant_preserved == formant_preserved {
            return;
        }
        self.formant_preserved = formant_preserved;
        let option = if formant_preserved {
            RUBBERBAND_OPTION_FORMANT_PRESERVED
        } else {
            0
        };
        unsafe { rubberband_live_set_formant_option(self.state, option) };
    }

    pub fn render<F>(
        &mut self,
        request_start_frame: usize,
        frames: usize,
        mut fill_input: F,
    ) -> Vec<Vec<f32>>
    where
        F: FnMut(usize, &mut [Vec<f32>]) -> f64,
    {
        let channels = self.output.len();
        let mut rendered = vec![vec![0.0; frames]; channels];
        if self.next_output_frame != request_start_frame {
            self.reset(request_start_frame);
        }

        let mut written = 0usize;
        while written < frames {
            if self.pending_offset < self.pending_len {
                let available = self.pending_len - self.pending_offset;
                let copy_len = available.min(frames - written);
                for (dst, src) in rendered.iter_mut().zip(self.output.iter()) {
                    dst[written..written + copy_len]
                        .copy_from_slice(&src[self.pending_offset..self.pending_offset + copy_len]);
                }
                self.pending_offset += copy_len;
                self.next_output_frame = self.next_output_frame.saturating_add(copy_len);
                written += copy_len;
                continue;
            }

            let pitch_scale = fill_input(self.next_input_frame, &mut self.input).max(0.01);
            unsafe {
                rubberband_live_set_pitch_scale(self.state, pitch_scale);
                rubberband_live_shift(
                    self.state,
                    self.input_ptrs.as_ptr(),
                    self.output_ptrs.as_ptr(),
                );
            }
            self.next_input_frame = self.next_input_frame.saturating_add(self.block_size);
            let drop = self.start_delay_remaining.min(self.block_size);
            self.start_delay_remaining = self.start_delay_remaining.saturating_sub(drop);
            self.pending_offset = drop;
            self.pending_len = self.block_size;
        }

        rendered
    }
}

impl Drop for LivePitchShifter {
    fn drop(&mut self) {
        if !self.state.is_null() {
            unsafe { rubberband_live_delete(self.state) };
        }
    }
}
