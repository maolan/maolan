use super::sync::ChannelState;
use super::{Audio, CountInfo, oss_get_iptr, oss_get_optr};
use std::os::fd::AsRawFd;

#[derive(Debug, Clone, Default)]
pub(super) struct Buffer {
    data: Vec<u8>,
    pos: usize,
}

impl Buffer {
    pub(super) fn with_size(size: usize) -> Self {
        Self {
            data: if size == 0 {
                Vec::new()
            } else {
                vec![0_u8; size]
            },
            pos: 0,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.data.len()
    }

    pub(super) fn progress(&self) -> usize {
        self.pos
    }

    pub(super) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(super) fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub(super) fn reset(&mut self) {
        self.pos = 0;
    }

    pub(super) fn clear(&mut self) {
        self.data.fill(0);
        self.pos = 0;
    }

    pub(super) fn advance(&mut self, bytes: usize) -> usize {
        let n = bytes.min(self.remaining());
        self.pos += n;
        n
    }

    pub(super) fn rewind(&mut self, bytes: usize) -> usize {
        let n = bytes.min(self.pos);
        self.pos -= n;
        n
    }

    pub(super) fn position(&mut self) -> &mut [u8] {
        let pos = self.pos;
        &mut self.data[pos..]
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub(super) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[derive(Debug, Clone)]
pub(super) struct BufferRecord {
    buffer: Buffer,
    end_frames: i64,
}

impl BufferRecord {
    fn empty() -> Self {
        Self {
            buffer: Buffer::default(),
            end_frames: 0,
        }
    }

    fn valid(&self) -> bool {
        self.buffer.len() > 0
    }
}

#[derive(Debug, Default)]
pub(super) struct ReadChannel {
    st: ChannelState,
    map_progress: i64,
    read_position: i64,
}
#[derive(Debug, Default)]
pub(super) struct WriteChannel {
    st: ChannelState,
    map_progress: i64,
    write_position: i64,
}

#[derive(Debug)]
pub(super) enum ChannelKind {
    Read(ReadChannel),
    Write(WriteChannel),
}

#[derive(Debug)]
pub(super) struct DoubleBufferedChannel {
    kind: ChannelKind,
    buffer_a: BufferRecord,
    buffer_b: BufferRecord,
}

impl DoubleBufferedChannel {
    pub(super) fn new_empty_read() -> Self {
        Self {
            kind: ChannelKind::Read(ReadChannel::default()),
            buffer_a: BufferRecord::empty(),
            buffer_b: BufferRecord::empty(),
        }
    }

    pub(super) fn new_empty_write() -> Self {
        Self {
            kind: ChannelKind::Write(WriteChannel::default()),
            buffer_a: BufferRecord::empty(),
            buffer_b: BufferRecord::empty(),
        }
    }

    pub(super) fn new_read(buffer_bytes: usize, frames: i64) -> Self {
        let mut s = Self::new_empty_read();
        s.set_buffer(Buffer::with_size(buffer_bytes), 0);
        s.set_buffer(Buffer::with_size(buffer_bytes), frames);
        s
    }

    pub(super) fn new_write(buffer_bytes: usize, frames: i64) -> Self {
        let mut s = Self::new_empty_write();
        s.set_buffer(Buffer::with_size(buffer_bytes), 0);
        s.set_buffer(Buffer::with_size(buffer_bytes), frames);
        s
    }

    pub(super) fn set_buffer(&mut self, buffer: Buffer, end_frames: i64) -> bool {
        if !self.buffer_b.valid() {
            self.buffer_b = BufferRecord { buffer, end_frames };
            if !self.buffer_a.valid() {
                std::mem::swap(&mut self.buffer_a, &mut self.buffer_b);
            }
            return true;
        }
        false
    }

    pub(super) fn take_buffer(&mut self) -> Buffer {
        std::mem::swap(&mut self.buffer_a, &mut self.buffer_b);
        std::mem::take(&mut self.buffer_b.buffer)
    }

    pub(super) fn reset_buffers(&mut self, end_frames: i64, frame_size: usize) {
        if self.buffer_a.valid() {
            self.buffer_a.buffer.clear();
            self.buffer_a.end_frames = end_frames;
        }
        if self.buffer_b.valid() {
            self.buffer_b.buffer.clear();
            self.buffer_b.end_frames =
                end_frames + (self.buffer_b.buffer.len() / frame_size) as i64;
        }
    }

    pub(super) fn end_frames(&self) -> i64 {
        if self.buffer_a.valid() {
            self.buffer_a.end_frames
        } else {
            0
        }
    }

    pub(super) fn process(&mut self, audio: &mut Audio, now: i64) -> std::io::Result<()> {
        let now = now - (now % audio.stepping());
        match &mut self.kind {
            ChannelKind::Read(read) => {
                Self::process_read(audio, read, &mut self.buffer_a, now)?;
                if self.buffer_a.buffer.done() && self.buffer_b.valid() {
                    Self::process_read(audio, read, &mut self.buffer_b, now)?;
                }
            }
            ChannelKind::Write(write) => {
                Self::process_write(audio, write, &mut self.buffer_a, now)?;
                if self.buffer_a.buffer.done() && self.buffer_b.valid() {
                    Self::process_write(audio, write, &mut self.buffer_b, now)?;
                }
            }
        }
        Ok(())
    }

    fn process_read(
        audio: &mut Audio,
        read: &mut ReadChannel,
        rec: &mut BufferRecord,
        now: i64,
    ) -> std::io::Result<()> {
        if read.st.last_processing != now {
            if audio.mapped {
                let mut info = CountInfo::default();
                let rc = unsafe { oss_get_iptr(audio.dsp.as_raw_fd(), &mut info) };
                if rc.is_ok() {
                    if let Some(delta) = audio.update_map_progress_from_count(&info) {
                        read.map_progress += (delta / audio.frame_size()) as i64;
                    }
                    let progress = read.map_progress - read.st.last_progress;
                    let available = read.st.last_progress + progress - read.read_position;
                    let loss = read.st.mark_loss(available - audio.buffer_frames());
                    read.st.mark_progress(progress, now, audio.stepping());
                    if loss > 0 {
                        read.read_position = read.st.last_progress - audio.buffer_frames();
                    }
                }
            } else {
                let queued = audio.queued_samples() as i64;
                let overdue = now - read.st.estimated_dropout(queued, audio.buffer_frames());
                if (overdue > 0 && audio.get_rec_overruns() > 0) || overdue > read.st.max_progress {
                    let progress = audio.buffer_frames() - queued;
                    let loss = read.st.mark_loss_from(progress, now);
                    read.st
                        .mark_progress(progress + loss, now, audio.stepping());
                    read.read_position = read.st.last_progress - audio.buffer_frames();
                } else {
                    let progress = queued - (read.st.last_progress - read.read_position);
                    read.st.mark_progress(progress, now, audio.stepping());
                    read.read_position = read.st.last_progress - queued;
                }
            }
        }

        let position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
        if position < read.read_position {
            let skip_frames = (read.read_position - position) as usize;
            let skip = rec.buffer.advance(skip_frames * audio.frame_size());
            if skip > 0 {
                rec.buffer.position().fill(0);
            }
        } else if position > read.read_position {
            let rewind_frames = (position - read.read_position) as usize;
            rec.buffer.rewind(rewind_frames * audio.frame_size());
        }

        if audio.mapped {
            let cur_position =
                rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            let mut oldest = read.st.last_progress - audio.buffer_frames();
            if read.map_progress < audio.buffer_frames() {
                oldest = read.st.last_progress - read.map_progress;
            }
            if cur_position >= oldest && cur_position < read.st.last_progress && !rec.buffer.done()
            {
                let offset = (read.st.last_progress - cur_position) as usize;
                let mut len = rec.buffer.remaining().min(offset * audio.frame_size());
                let pointer = (read.map_progress as usize).saturating_sub(offset)
                    % (audio.buffer_frames() as usize);
                len = audio.read_map(rec.buffer.position(), pointer * audio.frame_size(), len);
                rec.buffer.advance(len);
                read.read_position =
                    rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            }
        } else if audio.queued_samples() > 0 && !rec.buffer.done() {
            let mut bytes_read = 0_usize;
            let remaining = rec.buffer.remaining();
            audio.read_io(rec.buffer.position(), remaining, &mut bytes_read)?;
            read.read_position += (bytes_read / audio.frame_size()) as i64;
            rec.buffer.advance(bytes_read);
        }

        if read.st.freewheel() && now >= rec.end_frames + read.st.balance && !rec.buffer.done() {
            rec.buffer.position().fill(0);
            let advanced = rec.buffer.advance(rec.buffer.remaining());
            read.read_position += (advanced / audio.frame_size()) as i64;
        }

        Ok(())
    }

    fn process_write(
        audio: &mut Audio,
        write: &mut WriteChannel,
        rec: &mut BufferRecord,
        now: i64,
    ) -> std::io::Result<()> {
        if write.st.last_processing != now {
            if audio.mapped {
                let mut info = CountInfo::default();
                let rc = unsafe { oss_get_optr(audio.dsp.as_raw_fd(), &mut info) };
                if rc.is_ok() {
                    let delta = audio.update_map_progress_from_count(&info).unwrap_or(0);
                    let progress = (delta / audio.frame_size()) as i64;
                    if progress > 0 {
                        let start = (write.map_progress as usize % audio.buffer_frames() as usize)
                            * audio.frame_size();
                        audio.write_map(None, start, (progress as usize) * audio.frame_size());
                        write.map_progress += progress;
                    }
                    let loss = write
                        .st
                        .mark_loss(write.st.last_progress + progress - write.write_position);
                    write.st.mark_progress(progress, now, audio.stepping());
                    if loss > 0 {
                        write.write_position = write.st.last_progress;
                    }
                }
            } else {
                let queued = audio.queued_samples() as i64;
                let overdue = now - write.st.estimated_dropout(queued, audio.buffer_frames());
                if (overdue > 0 && audio.get_play_underruns() > 0)
                    || overdue > write.st.max_progress
                {
                    let progress = write.write_position - write.st.last_progress;
                    let loss = write.st.mark_loss_from(progress, now);
                    write
                        .st
                        .mark_progress(progress + loss, now, audio.stepping());
                    write.write_position = write.st.last_progress;
                } else {
                    let progress = (write.write_position - write.st.last_progress) - queued;
                    write.st.mark_progress(progress, now, audio.stepping());
                    write.write_position = write.st.last_progress + queued;
                }
            }
        }

        let position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
        if position > write.write_position {
            let rewind = rec
                .buffer
                .rewind(((position - write.write_position) as usize) * audio.frame_size());
            if rewind > 0 {
                let _ = rewind;
            }
        } else if position < write.write_position {
            rec.buffer
                .advance(((write.write_position - position) as usize) * audio.frame_size());
        }

        if audio.mapped {
            let pos = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            if !rec.buffer.done()
                && pos >= write.st.last_progress
                && pos < write.st.last_progress + audio.buffer_frames()
            {
                let offset = (pos - write.st.last_progress) as usize;
                let pointer =
                    ((write.map_progress as usize) + offset) % audio.buffer_frames() as usize;
                let mut len =
                    ((audio.buffer_frames() as usize).saturating_sub(offset)) * audio.frame_size();
                len = len.min(rec.buffer.remaining());
                let written = audio.write_map(
                    Some(rec.buffer.position()),
                    pointer * audio.frame_size(),
                    len,
                );
                rec.buffer.advance(written);
                write.write_position =
                    rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            }
        } else if audio.queued_samples() < audio.buffer_frames() as i32 && !rec.buffer.done() {
            let mut bytes_written = 0_usize;
            let remaining = rec.buffer.remaining();
            audio.write_io(rec.buffer.position(), remaining, &mut bytes_written)?;
            write.write_position += (bytes_written / audio.frame_size()) as i64;
            rec.buffer.advance(bytes_written);
        }

        if write.st.freewheel() && now >= rec.end_frames + write.st.balance && !rec.buffer.done() {
            rec.buffer.advance(rec.buffer.remaining());
        }

        Ok(())
    }

    pub(super) fn wakeup_time(&self, audio: &Audio, now: i64) -> i64 {
        let sync_frames = if self.buffer_a.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        } else {
            i64::MAX
        };

        match &self.kind {
            ChannelKind::Read(read) => read.st.wakeup_time(
                sync_frames,
                (read.st.last_progress - read.read_position).clamp(0, audio.buffer_frames()),
                audio.buffer_frames(),
                audio.stepping(),
            ),
            ChannelKind::Write(write) => write.st.wakeup_time(
                sync_frames,
                (write.st.last_progress + audio.buffer_frames() - write.write_position)
                    .clamp(0, audio.buffer_frames()),
                audio.buffer_frames(),
                audio.stepping(),
            ),
        }
        .max(now)
    }

    pub(super) fn finished(&self, now: i64) -> bool {
        if !self.buffer_a.valid() {
            return true;
        }
        match &self.kind {
            ChannelKind::Read(read) => {
                (self.buffer_a.end_frames + read.st.balance) <= now && self.buffer_a.buffer.done()
            }
            ChannelKind::Write(write) => {
                (self.buffer_a.end_frames + write.st.balance) <= now && self.buffer_a.buffer.done()
            }
        }
    }

    pub(super) fn total_finished(&self, now: i64) -> bool {
        if !self.buffer_a.valid() {
            return true;
        }
        let end = if self.buffer_b.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_b.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_b.end_frames + write.st.balance,
            }
        } else {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        };
        end <= now && self.buffer_a.buffer.done() && self.buffer_b.buffer.done()
    }

    pub(super) fn total_end(&self) -> i64 {
        if !self.buffer_a.valid() {
            return 0;
        }
        if self.buffer_b.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_b.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_b.end_frames + write.st.balance,
            }
        } else {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        }
    }

    pub(super) fn balance(&self) -> i64 {
        match &self.kind {
            ChannelKind::Read(read) => read.st.balance,
            ChannelKind::Write(write) => write.st.balance,
        }
    }

    pub(super) fn set_balance(&mut self, balance: i64) {
        match &mut self.kind {
            ChannelKind::Read(read) => read.st.balance = balance,
            ChannelKind::Write(write) => write.st.balance = balance,
        }
    }
}
