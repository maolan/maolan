use super::{PCM_ENABLE_INPUT, PCM_ENABLE_OUTPUT};
use nix::libc;

#[repr(C)]
#[derive(Debug)]
pub struct AudioInfo {
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
    pub fn new() -> Self {
        Self {
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
#[derive(Debug)]
pub struct BufferInfo {
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

#[repr(C)]
#[derive(Debug, Default)]
pub(super) struct CountInfo {
    pub(super) bytes: libc::c_int,
    pub(super) blocks: libc::c_int,
    pub(super) ptr: libc::c_int,
}

#[repr(C)]
#[derive(Debug, Default)]
pub(super) struct OssCount {
    pub(super) samples: i64,
    pub(super) fifo_samples: libc::c_int,
    pub(super) filler: [libc::c_int; 32],
}

#[repr(C)]
#[derive(Debug, Default)]
pub(super) struct AudioErrInfo {
    pub(super) play_underruns: libc::c_int,
    pub(super) rec_overruns: libc::c_int,
    pub(super) play_ptradjust: libc::c_uint,
    pub(super) rec_ptradjust: libc::c_uint,
    pub(super) play_errorcount: libc::c_int,
    pub(super) rec_errorcount: libc::c_int,
    pub(super) play_lasterror: libc::c_int,
    pub(super) rec_lasterror: libc::c_int,
    pub(super) play_errorparm: libc::c_long,
    pub(super) rec_errorparm: libc::c_long,
    pub(super) filler: [libc::c_int; 16],
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct OssSysInfo {
    pub(super) product: [libc::c_char; 32],
    pub(super) version: [libc::c_char; 32],
    pub(super) versionnum: libc::c_int,
    pub(super) options: [libc::c_char; 128],
    pub(super) numaudios: libc::c_int,
    pub(super) openedaudio: [libc::c_int; 8],
    pub(super) numsynths: libc::c_int,
    pub(super) nummidis: libc::c_int,
    pub(super) numtimers: libc::c_int,
    pub(super) nummixers: libc::c_int,
    pub(super) openedmidi: [libc::c_int; 8],
    pub(super) numcards: libc::c_int,
    pub(super) numaudioengines: libc::c_int,
    pub(super) license: [libc::c_char; 16],
    pub(super) revision_info: [libc::c_char; 256],
    pub(super) filler: [libc::c_int; 172],
}

impl Default for OssSysInfo {
    fn default() -> Self {
        Self {
            product: [0; 32],
            version: [0; 32],
            versionnum: 0,
            options: [0; 128],
            numaudios: 0,
            openedaudio: [0; 8],
            numsynths: 0,
            nummidis: 0,
            numtimers: 0,
            nummixers: 0,
            openedmidi: [0; 8],
            numcards: 0,
            numaudioengines: 0,
            license: [0; 16],
            revision_info: [0; 256],
            filler: [0; 172],
        }
    }
}

#[repr(C)]
#[derive(Debug)]
struct OssSyncGroup {
    pub id: libc::c_int,
    pub mode: libc::c_int,
    pub filler: [libc::c_int; 16],
}

impl OssSyncGroup {
    pub fn new() -> Self {
        Self {
            id: 0,
            mode: 0,
            filler: [0; 16],
        }
    }
}

const SNDCTL_DSP_MAGIC: u8 = b'P';
const SNDCTL_DSP_SPEED: u8 = 2;
const SNDCTL_DSP_SETFMT: u8 = 5;
const SNDCTL_DSP_CHANNELS: u8 = 6;
const SNDCTL_DSP_SETFRAGMENT: u8 = 10;
const SNDCTL_DSP_GETOSPACE: u8 = 12;
const SNDCTL_DSP_GETISPACE: u8 = 13;
const SNDCTL_DSP_GETCAPS: u8 = 15;
const SNDCTL_DSP_SETTRIGGER: u8 = 16;
const SNDCTL_DSP_GETIPTR: u8 = 17;
const SNDCTL_DSP_GETOPTR: u8 = 18;
const SNDCTL_DSP_GETERROR: u8 = 25;
const SNDCTL_DSP_SYNCGROUP: u8 = 28;
const SNDCTL_DSP_SYNCSTART: u8 = 29;
const SNDCTL_DSP_COOKEDMODE: u8 = 30;
const SNDCTL_DSP_CURRENT_IPTR: u8 = 35;
const SNDCTL_DSP_CURRENT_OPTR: u8 = 36;

nix::ioctl_readwrite!(oss_set_channels, SNDCTL_DSP_MAGIC, SNDCTL_DSP_CHANNELS, i32);
nix::ioctl_readwrite!(
    oss_set_fragment,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SETFRAGMENT,
    i32
);
nix::ioctl_read!(
    oss_output_buffer_info,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETOSPACE,
    BufferInfo
);
nix::ioctl_read!(
    oss_input_buffer_info,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETISPACE,
    BufferInfo
);
nix::ioctl_read!(oss_get_caps, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETCAPS, i32);
nix::ioctl_readwrite!(oss_set_format, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SETFMT, u32);
nix::ioctl_readwrite!(oss_set_speed, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SPEED, i32);
nix::ioctl_write_ptr!(oss_set_cooked, SNDCTL_DSP_MAGIC, SNDCTL_DSP_COOKEDMODE, i32);
nix::ioctl_write_ptr!(
    oss_set_trigger,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SETTRIGGER,
    i32
);
nix::ioctl_read!(
    oss_get_iptr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETIPTR,
    CountInfo
);
nix::ioctl_read!(
    oss_get_optr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETOPTR,
    CountInfo
);
nix::ioctl_read!(
    oss_get_error,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETERROR,
    AudioErrInfo
);
nix::ioctl_read!(
    oss_current_iptr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_CURRENT_IPTR,
    OssCount
);
nix::ioctl_read!(
    oss_current_optr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_CURRENT_OPTR,
    OssCount
);
nix::ioctl_write_ptr!(oss_start_group, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SYNCSTART, i32);
nix::ioctl_readwrite!(
    oss_add_sync_group,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SYNCGROUP,
    OssSyncGroup
);

const SNDCTL_INFO_MAGIC: u8 = b'X';
const SNDCTL_ENGINEINFO: u8 = 12;
const SNDCTL_SYSINFO: u8 = 1;
nix::ioctl_readwrite!(
    oss_get_info,
    SNDCTL_INFO_MAGIC,
    SNDCTL_ENGINEINFO,
    AudioInfo
);
nix::ioctl_read!(
    oss_get_sysinfo,
    SNDCTL_INFO_MAGIC,
    SNDCTL_SYSINFO,
    OssSysInfo
);

pub fn add_to_sync_group(fd: i32, group: i32, input: bool) -> i32 {
    let mut sync_group = OssSyncGroup::new();
    sync_group.id = group;
    if input {
        sync_group.mode = PCM_ENABLE_INPUT;
    } else {
        sync_group.mode = PCM_ENABLE_OUTPUT;
    }
    unsafe {
        let _ = oss_add_sync_group(fd, &mut sync_group);
    }
    sync_group.id
}

pub fn start_sync_group(fd: i32, group: i32) -> std::io::Result<()> {
    let id = group;
    unsafe { oss_start_group(fd, &id) }
        .map(|_| ())
        .map_err(|_| std::io::Error::last_os_error())
}
