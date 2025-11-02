use libc::{MAP_SHARED, PROT_READ, mmap, munmap};
use nix::libc;
use std::{
    fs::File,
    mem,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};

// Format
pub const AFMT_QUERY: u32 = 0x00000000;
pub const AFMT_MU_LAW: u32 = 0x00000001;
pub const AFMT_A_LAW: u32 = 0x00000002;
pub const AFMT_IMA_ADPCM: u32 = 0x00000004;
pub const AFMT_U8: u32 = 0x00000008;
pub const AFMT_S16_LE: u32 = 0x00000010;
pub const AFMT_S16_BE: u32 = 0x00000020;
pub const AFMT_S8: u32 = 0x00000040;
pub const AFMT_U16_LE: u32 = 0x00000080;
pub const AFMT_U16_BE: u32 = 0x00000100;
pub const AFMT_MPEG: u32 = 0x00000200;
pub const AFMT_AC3: u32 = 0x00000400;
pub const AFMT_S32_LE: u32 = 0x00001000;
pub const AFMT_S32_BE: u32 = 0x00002000;
pub const AFMT_U32_LE: u32 = 0x00004000;
pub const AFMT_U32_BE: u32 = 0x00008000;
pub const AFMT_S24_LE: u32 = 0x00010000;
pub const AFMT_S24_BE: u32 = 0x00020000;
pub const AFMT_U24_LE: u32 = 0x00040000;
pub const AFMT_U24_BE: u32 = 0x00080000;
pub const AFMT_STEREO: u32 = 0x10000000;
pub const AFMT_WEIRD: u32 = 0x20000000;
pub const AFMT_FULLDUPLEX: u32 = 0x80000000;

// Triggers
pub const PCM_ENABLE_INPUT: u32 = 0x00000001;
pub const PCM_ENABLE_OUTPUT: u32 = 0x00000002;

#[derive(Debug)]
pub struct Config {
    pub dsp: File,
    pub channels: i32,
    pub rate: i32,
    pub bytes: i32,
    pub format: u32,
    pub samples: i32,
    pub chsamples: i32,
}

impl Config {
    pub fn new(path: &str, rate: i32, bits: i32, input: bool) -> Config {
        let mut binding = File::options();
        let options = binding.write(true);
        options.custom_flags(libc::O_RDONLY | libc::O_EXCL | libc::O_NONBLOCK);
        let mut c = Config {
            dsp: options.open(path).unwrap(),
            channels: 0,
            rate,
            bytes: 0,
            format: AFMT_S32_NE,
            samples: 0,
            chsamples: 0,
        };
        if bits == 32 {
            c.format = AFMT_S32_NE;
        } else if bits == 16 {
            c.format = AFMT_S16_NE;
        } else if bits == 8 {
            c.format = AFMT_S8;
        } else {
            panic!("No format with {} bits", bits);
        }
        let mut audio_info = AudioInfo::new();
        let mut buffer_info = BufferInfo::new();
        unsafe {
            let fd = c.dsp.as_raw_fd();
            let flags: i32 = 0;

            oss_get_info(fd, &mut audio_info).expect("Failed to get info on device");
            oss_get_caps(fd, &mut audio_info.caps)
                .expect("Failed to get capabilities of the device");
            oss_set_cooked(fd, &flags).expect("Failed to disable cooked mode");

            // Set number of channels, sample format and rate
            oss_set_format(fd, &mut c.format).expect("Failed to set format");
            oss_set_channels(fd, &mut audio_info.max_channels)
                .expect("Failed to set number of channels");
            oss_set_speed(fd, &mut c.rate).expect("Failed to set sample rate");

            // When it's all set and good to go, gather buffer size info
            oss_get_buffer_info(fd, &mut buffer_info).expect("Failed to get buffer size");
        }
        if buffer_info.fragments < 1 {
            buffer_info.fragments = buffer_info.fragstotal;
        }
        if buffer_info.bytes < 1 {
            buffer_info.bytes = buffer_info.fragstotal * buffer_info.fragsize;
        }
        if buffer_info.bytes < 1 {
            panic!(
                "OSS buffer error: buffer size can not be {}",
                buffer_info.bytes
            );
        }
        c.channels = audio_info.max_channels;
        c.bytes = buffer_info.bytes;
        c.samples = c.bytes / mem::size_of::<i32>() as i32;
        c.chsamples = c.samples / c.channels;
        unsafe {
            let fd = c.dsp.as_raw_fd();
            let trig: u32 = if input { PCM_ENABLE_INPUT } else { PCM_ENABLE_OUTPUT };

            oss_set_trigger(fd, &trig);
        }
        c
    }
}

#[repr(C)]
struct AudioInfo {
    pub dev: libc::c_int,
    pub name: [libc::c_char; 64],
    pub busy: libc::c_int,
    pub pid: libc::c_int,
    pub caps: libc::c_int,
    pub iformats: libc::c_int,
    pub oformats: libc::c_int,
    pub magic: libc::c_int,
    pub cmd: [libc::c_char; 64],
    pub card_number: libc::c_int,
    pub port_number: libc::c_int,
    pub mixer_dev: libc::c_int,
    pub legacy_device: libc::c_int,
    pub enabled: libc::c_int,
    pub flags: libc::c_int,
    pub min_rate: libc::c_int,
    pub max_rate: libc::c_int,
    pub min_channels: libc::c_int,
    pub max_channels: libc::c_int,
    pub binding: libc::c_int,
    pub rate_source: libc::c_int,
    pub handle: [libc::c_char; 32],
    pub nrates: libc::c_uint,
    pub rates: [libc::c_uint; 20],
    pub song_name: [libc::c_char; 64],
    pub label: [libc::c_char; 16],
    pub latency: libc::c_int,
    pub devnode: [libc::c_char; 32],
    pub next_play_engine: libc::c_int,
    pub next_rec_engine: libc::c_int,
    pub filler: [libc::c_int; 184],
}

impl AudioInfo {
    pub fn new() -> AudioInfo {
        AudioInfo {
            dev: 0,
            name: [0; 64],
            busy: 0,
            pid: 0,
            caps: 0,
            iformats: 0,
            oformats: 0,
            magic: 0,
            cmd: [0; 64],
            card_number: 0,
            port_number: 0,
            mixer_dev: 0,
            legacy_device: 0,
            enabled: 0,
            flags: 0,
            min_rate: 0,
            max_rate: 0,
            min_channels: 0,
            max_channels: 0,
            binding: 0,
            rate_source: 0,
            handle: [0; 32],
            nrates: 0,
            rates: [0; 20],
            song_name: [0; 64],
            label: [0; 16],
            latency: 0,
            devnode: [0; 32],
            next_play_engine: 0,
            next_rec_engine: 0,
            filler: [0; 184],
        }
    }
}

#[repr(C)]
struct BufferInfo {
    pub fragments: libc::c_int,
    pub fragstotal: libc::c_int,
    pub fragsize: libc::c_int,
    pub bytes: libc::c_int,
}

impl BufferInfo {
    pub fn new() -> BufferInfo {
        BufferInfo {
            fragments: 0,
            fragstotal: 0,
            fragsize: 0,
            bytes: 0,
        }
    }
}

const SNDCTL_DSP_MAGIC: u8 = b'P';
const SNDCTL_DSP_SPEED: u8 = 2;
const SNDCTL_DSP_SETFMT: u8 = 5;
const SNDCTL_DSP_CHANNELS: u8 = 6;
const SNDCTL_DSP_GETOSPACE: u8 = 12;
const SNDCTL_DSP_GETCAPS: u8 = 15;
const SNDCTL_DSP_SETTRIGGER: u8 = 16;
const SNDCTL_DSP_COOKEDMODE: u8 = 30;
nix::ioctl_readwrite!(oss_set_channels, SNDCTL_DSP_MAGIC, SNDCTL_DSP_CHANNELS, i32);
nix::ioctl_read!(
    oss_get_buffer_info,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETOSPACE,
    BufferInfo
);
nix::ioctl_read!(oss_get_caps, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETCAPS, i32);
nix::ioctl_readwrite!(oss_set_format, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SETFMT, u32);
nix::ioctl_readwrite!(oss_set_speed, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SPEED, i32);
nix::ioctl_write_ptr!(oss_set_cooked, SNDCTL_DSP_MAGIC, SNDCTL_DSP_COOKEDMODE, i32);
nix::ioctl_write_ptr!(oss_set_trigger, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SETTRIGGER, u32);

const SNDCTL_INFO_MAGIC: u8 = b'X';
const SNDCTL_ENGINEINFO: u8 = 12;
nix::ioctl_readwrite!(
    oss_get_info,
    SNDCTL_INFO_MAGIC,
    SNDCTL_ENGINEINFO,
    AudioInfo
);

fn add_to_sync_group(fd: i32, group: i32) -> i32 {
	// oss_syncgroup sync_group = {0, 0, {0}};
	// sync_group.id = group;
	// sync_group.mode |= PCM_ENABLE_INPUT;
	// if (ioctl(fd, SNDCTL_DSP_SYNCGROUP, &sync_group) == 0)
	// 	return sync_group.id;
	// errx(1, "Failed to add %d to syncgroup", sync_group.id);
  0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::slice::from_raw_parts;
    use wavers::Wav;

    #[test]
    #[ignore]
    fn it_works() {
        let mut oss = Config::new("/dev/dsp", 48000, 32, false);
        let mut wav: Wav<i32> = Wav::from_path("./stereo32.wav").unwrap();
        let nchannels: usize = wav.n_channels().into();
        let mut out: Vec<i32> = vec![];
        let mut iter = wav.frames();

        'outer: loop {
            let mut i = 0;
            while i < oss.chsamples {
                match iter.next() {
                    Some(ref samples) => {
                        for ch in 0..nchannels {
                            out.push(samples[ch]);
                            i += 1;
                        }
                    }
                    None => break 'outer,
                }
            }
            let bytes = unsafe { from_raw_parts(out.as_ptr() as *const u8, out.len() * 4) };
            let _ret = oss.dsp.write(bytes);
            out.clear();
        }
        // assert_eq!(1, 2, "Fake error");
    }

    #[test]
    fn mmap_mode() {
        let device = "/dev/dsp4";
        let oss_in = Config::new(device, 48000, 32, true);
        let oss_out = Config::new(device, 48000, 32, false);
        let addr_in;
        let addr_out;
        unsafe {
            addr_in = mmap(
                std::ptr::null_mut(),
                oss_in.bytes.try_into().unwrap(),
                PROT_READ,
                MAP_SHARED,
                oss_in.dsp.as_raw_fd(),
                0,
            );
            addr_out = mmap(
                std::ptr::null_mut(),
                oss_out.bytes.try_into().unwrap(),
                PROT_READ,
                MAP_SHARED,
                oss_out.dsp.as_raw_fd(),
                0,
            );

        }
        assert_ne!(addr_in, libc::MAP_FAILED, "Memory-mapping of input failed");
        assert_ne!(addr_out, libc::MAP_FAILED, "Memory-mapping of output failed");

        unsafe {
            munmap(addr_in, oss_in.bytes.try_into().unwrap());
            munmap(addr_out, oss_out.bytes.try_into().unwrap());
        }
    }
}
