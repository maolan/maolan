use crate::midi::io::MidiEvent;
use std::cell::UnsafeCell;
use vst3::Steinberg::Vst::ControllerNumbers_::{
    kAfterTouch, kCtrlProgramChange, kPitchBend,
};
use vst3::Steinberg::Vst::DataEvent_::DataTypes_;
use vst3::Steinberg::Vst::Event_::EventTypes_;
use vst3::Steinberg::Vst::{
    CtrlNumber, DataEvent, Event, Event__type0, IEventList, IEventListTrait, IMidiMapping,
    IMidiMappingTrait, IParamValueQueue, IParamValueQueueTrait, IParameterChanges,
    IParameterChangesTrait, LegacyMIDICCOutEvent, NoteOffEvent, NoteOnEvent, ParamID,
    PolyPressureEvent,
};
use vst3::Steinberg::{kInvalidArgument, kResultFalse, kResultOk};
use vst3::{Class, ComPtr, ComWrapper};

pub struct EventBuffer {
    events: UnsafeCell<Vec<Event>>,
    sysex_data: UnsafeCell<Vec<Vec<u8>>>,
}

impl Class for EventBuffer {
    type Interfaces = (IEventList,);
}

impl Default for EventBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBuffer {
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(Vec::new()),
            sysex_data: UnsafeCell::new(Vec::new()),
        }
    }

    pub fn clear(&mut self) {
        self.events_mut().clear();
        self.sysex_data_mut().clear();
    }

    pub fn from_midi_events(midi_events: &[MidiEvent], bus_index: i32) -> Self {
        let buffer = Self::new();
        for midi_event in midi_events {
            buffer.push_midi_event(midi_event, bus_index);
        }
        buffer
    }

    pub fn to_midi_events(&self) -> Vec<MidiEvent> {
        self.events_ref()
            .iter()
            .filter_map(vst3_event_to_midi)
            .collect()
    }

    pub fn event_count(&self) -> usize {
        self.events_ref().len()
    }

    pub fn event_list_ptr(list: &ComWrapper<Self>) -> *mut IEventList {
        list.as_com_ref::<IEventList>()
            .map(|r| r.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }

    #[allow(clippy::mut_from_ref)]
    fn events_mut(&self) -> &mut Vec<Event> {
        unsafe { &mut *self.events.get() }
    }

    fn events_ref(&self) -> &Vec<Event> {
        unsafe { &*self.events.get() }
    }

    #[allow(clippy::mut_from_ref)]
    fn sysex_data_mut(&self) -> &mut Vec<Vec<u8>> {
        unsafe { &mut *self.sysex_data.get() }
    }

    fn push_midi_event(&self, midi_event: &MidiEvent, bus_index: i32) {
        if let Some(event) = midi_to_vst3_event(midi_event, bus_index, self.sysex_data_mut()) {
            self.events_mut().push(event);
        }
    }
}

impl IEventListTrait for EventBuffer {
    unsafe fn getEventCount(&self) -> i32 {
        self.event_count().min(i32::MAX as usize) as i32
    }

    unsafe fn getEvent(&self, index: i32, e: *mut Event) -> i32 {
        if index < 0 || e.is_null() {
            return kInvalidArgument;
        }
        let Some(event) = self.events_ref().get(index as usize).copied() else {
            return kResultFalse;
        };
        unsafe {
            *e = event;
        }
        kResultOk
    }

    unsafe fn addEvent(&self, e: *mut Event) -> i32 {
        if e.is_null() {
            return kInvalidArgument;
        }
        let event = unsafe { *e };
        if event.r#type as u32 == EventTypes_::kDataEvent
            && let Some(bytes) = copy_sysex_event(&event)
        {
            self.sysex_data_mut().push(bytes);
            if let Some(last) = self.sysex_data_mut().last() {
                self.events_mut().push(Event {
                    __field0: Event__type0 {
                        data: DataEvent {
                            size: last.len().min(u32::MAX as usize) as u32,
                            r#type: DataTypes_::kMidiSysEx,
                            bytes: last.as_ptr(),
                        },
                    },
                    ..event
                });
                return kResultOk;
            }
        }
        self.events_mut().push(event);
        kResultOk
    }
}

pub struct ParameterChanges {
    queues: UnsafeCell<Vec<ComWrapper<ParameterValueQueue>>>,
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl Default for ParameterChanges {
    fn default() -> Self {
        Self::new()
    }
}

impl ParameterChanges {
    pub fn new() -> Self {
        Self {
            queues: UnsafeCell::new(Vec::new()),
        }
    }

    pub fn from_midi_events(
        midi_events: &[MidiEvent],
        mapping: &ComPtr<IMidiMapping>,
        bus_index: i32,
    ) -> Option<Self> {
        let changes = Self::new();
        for midi_event in midi_events {
            let Some((channel, controller, value)) = midi_to_controller_change(midi_event) else {
                continue;
            };
            let mut param_id: ParamID = 0;
            let result = unsafe {
                mapping.getMidiControllerAssignment(
                    bus_index,
                    channel,
                    controller,
                    &mut param_id,
                )
            };
            if result != kResultOk {
                continue;
            }
            changes.add_point(
                param_id,
                midi_event.frame.min(i32::MAX as u32) as i32,
                value as f64,
            );
        }

        (!changes.queues_ref().is_empty()).then_some(changes)
    }

    pub fn changes_ptr(changes: &ComWrapper<Self>) -> *mut IParameterChanges {
        changes
            .as_com_ref::<IParameterChanges>()
            .map(|r| r.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }

    fn add_point(&self, param_id: ParamID, sample_offset: i32, value: f64) {
        for queue in self.queues_ref() {
            if queue.parameter_id() == param_id {
                queue.push_point(sample_offset, value);
                return;
            }
        }

        let queue = ComWrapper::new(ParameterValueQueue::new(param_id));
        queue.push_point(sample_offset, value);
        self.queues_mut().push(queue);
    }

    #[allow(clippy::mut_from_ref)]
    fn queues_mut(&self) -> &mut Vec<ComWrapper<ParameterValueQueue>> {
        unsafe { &mut *self.queues.get() }
    }

    fn queues_ref(&self) -> &Vec<ComWrapper<ParameterValueQueue>> {
        unsafe { &*self.queues.get() }
    }
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.queues_ref().len().min(i32::MAX as usize) as i32
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        self.queues_ref()
            .get(index.max(0) as usize)
            .and_then(|queue| queue.as_com_ref::<IParamValueQueue>())
            .map(|queue| queue.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }

    unsafe fn addParameterData(&self, id: *const ParamID, index: *mut i32) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        let param_id = unsafe { *id };
        if let Some(existing_idx) = self
            .queues_ref()
            .iter()
            .position(|queue| queue.parameter_id() == param_id)
        {
            if !index.is_null() {
                unsafe {
                    *index = existing_idx as i32;
                }
            }
            return self
                .queues_ref()
                .get(existing_idx)
                .and_then(|queue| queue.as_com_ref::<IParamValueQueue>())
                .map(|queue| queue.as_ptr())
                .unwrap_or(std::ptr::null_mut());
        }

        let queue = ComWrapper::new(ParameterValueQueue::new(param_id));
        self.queues_mut().push(queue);
        let idx = self.queues_ref().len().saturating_sub(1);
        if !index.is_null() {
            unsafe {
                *index = idx as i32;
            }
        }
        self.queues_ref()[idx]
            .as_com_ref::<IParamValueQueue>()
            .map(|queue| queue.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }
}

pub struct ParameterValueQueue {
    param_id: ParamID,
    points: UnsafeCell<Vec<(i32, f64)>>,
}

impl Class for ParameterValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl ParameterValueQueue {
    fn new(param_id: ParamID) -> Self {
        Self {
            param_id,
            points: UnsafeCell::new(Vec::new()),
        }
    }

    fn parameter_id(&self) -> ParamID {
        self.param_id
    }

    fn push_point(&self, sample_offset: i32, value: f64) {
        self.points_mut().push((sample_offset, value.clamp(0.0, 1.0)));
    }

    #[allow(clippy::mut_from_ref)]
    fn points_mut(&self) -> &mut Vec<(i32, f64)> {
        unsafe { &mut *self.points.get() }
    }

    fn points_ref(&self) -> &Vec<(i32, f64)> {
        unsafe { &*self.points.get() }
    }
}

impl IParamValueQueueTrait for ParameterValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.param_id
    }

    unsafe fn getPointCount(&self) -> i32 {
        self.points_ref().len().min(i32::MAX as usize) as i32
    }

    unsafe fn getPoint(&self, index: i32, sample_offset: *mut i32, value: *mut f64) -> i32 {
        let Some((offset, point_value)) = self.points_ref().get(index.max(0) as usize).copied() else {
            return kResultFalse;
        };
        if !sample_offset.is_null() {
            unsafe {
                *sample_offset = offset;
            }
        }
        if !value.is_null() {
            unsafe {
                *value = point_value;
            }
        }
        kResultOk
    }

    unsafe fn addPoint(&self, sample_offset: i32, value: f64, index: *mut i32) -> i32 {
        self.push_point(sample_offset, value);
        if !index.is_null() {
            unsafe {
                *index = self.points_ref().len().saturating_sub(1) as i32;
            }
        }
        kResultOk
    }
}

fn midi_to_vst3_event(
    midi_event: &MidiEvent,
    bus_index: i32,
    sysex_storage: &mut Vec<Vec<u8>>,
) -> Option<Event> {
    let status = *midi_event.data.first()?;
    let channel = (status & 0x0f) as i16;
    let kind = status & 0xf0;
    let sample_offset = midi_event.frame.min(i32::MAX as u32) as i32;

    match kind {
        0x80 => {
            let pitch = *midi_event.data.get(1)? as i16;
            let velocity = midi_velocity(midi_event.data.get(2).copied().unwrap_or(0));
            Some(Event {
                busIndex: bus_index,
                sampleOffset: sample_offset,
                ppqPosition: 0.0,
                flags: 0,
                r#type: EventTypes_::kNoteOffEvent as u16,
                __field0: Event__type0 {
                    noteOff: NoteOffEvent {
                        channel,
                        pitch,
                        velocity,
                        noteId: -1,
                        tuning: 0.0,
                    },
                },
            })
        }
        0x90 => {
            let pitch = *midi_event.data.get(1)? as i16;
            let velocity_byte = midi_event.data.get(2).copied().unwrap_or(0);
            if velocity_byte == 0 {
                return midi_to_vst3_event(
                    &MidiEvent::new(midi_event.frame, vec![0x80 | channel as u8, pitch as u8, 0]),
                    bus_index,
                    sysex_storage,
                );
            }
            Some(Event {
                busIndex: bus_index,
                sampleOffset: sample_offset,
                ppqPosition: 0.0,
                flags: 0,
                r#type: EventTypes_::kNoteOnEvent as u16,
                __field0: Event__type0 {
                    noteOn: NoteOnEvent {
                        channel,
                        pitch,
                        tuning: 0.0,
                        velocity: midi_velocity(velocity_byte),
                        length: 0,
                        noteId: -1,
                    },
                },
            })
        }
        0xA0 => {
            let pitch = *midi_event.data.get(1)? as i16;
            let pressure = midi_velocity(midi_event.data.get(2).copied().unwrap_or(0));
            Some(Event {
                busIndex: bus_index,
                sampleOffset: sample_offset,
                ppqPosition: 0.0,
                flags: 0,
                r#type: EventTypes_::kPolyPressureEvent as u16,
                __field0: Event__type0 {
                    polyPressure: PolyPressureEvent {
                        channel,
                        pitch,
                        pressure,
                        noteId: -1,
                    },
                },
            })
        }
        0xF0 if midi_event.data.first().copied() == Some(0xF0) => {
            sysex_storage.push(midi_event.data.clone());
            let bytes = sysex_storage.last()?;
            Some(Event {
                busIndex: bus_index,
                sampleOffset: sample_offset,
                ppqPosition: 0.0,
                flags: 0,
                r#type: EventTypes_::kDataEvent as u16,
                __field0: Event__type0 {
                    data: DataEvent {
                        size: bytes.len().min(u32::MAX as usize) as u32,
                        r#type: DataTypes_::kMidiSysEx,
                        bytes: bytes.as_ptr(),
                    },
                },
            })
        }
        _ => None,
    }
}

fn vst3_event_to_midi(event: &Event) -> Option<MidiEvent> {
    let frame = event.sampleOffset.max(0) as u32;
    match event.r#type as u32 {
        EventTypes_::kNoteOnEvent => {
            let note = unsafe { event.__field0.noteOn };
            Some(MidiEvent::new(
                frame,
                vec![
                    0x90 | (note.channel as u8 & 0x0f),
                    note.pitch.clamp(0, 127) as u8,
                    midi_byte(note.velocity),
                ],
            ))
        }
        EventTypes_::kNoteOffEvent => {
            let note = unsafe { event.__field0.noteOff };
            Some(MidiEvent::new(
                frame,
                vec![
                    0x80 | (note.channel as u8 & 0x0f),
                    note.pitch.clamp(0, 127) as u8,
                    midi_byte(note.velocity),
                ],
            ))
        }
        EventTypes_::kPolyPressureEvent => {
            let pressure = unsafe { event.__field0.polyPressure };
            Some(MidiEvent::new(
                frame,
                vec![
                    0xA0 | (pressure.channel as u8 & 0x0f),
                    pressure.pitch.clamp(0, 127) as u8,
                    midi_byte(pressure.pressure),
                ],
            ))
        }
        EventTypes_::kDataEvent => {
            let data = unsafe { event.__field0.data };
            (data.r#type == DataTypes_::kMidiSysEx && !data.bytes.is_null()).then(|| {
                let bytes = unsafe {
                    std::slice::from_raw_parts(data.bytes, data.size.min(usize::MAX as u32) as usize)
                };
                MidiEvent::new(frame, bytes.to_vec())
            })
        }
        EventTypes_::kLegacyMIDICCOutEvent => {
            let cc = unsafe { event.__field0.midiCCOut };
            legacy_cc_to_midi(frame, cc)
        }
        _ => None,
    }
}

fn midi_to_controller_change(midi_event: &MidiEvent) -> Option<(i16, CtrlNumber, f32)> {
    let status = *midi_event.data.first()?;
    let channel = (status & 0x0f) as i16;
    let kind = status & 0xf0;
    match kind {
        0xB0 => Some((
            channel,
            midi_event.data.get(1).copied()? as CtrlNumber,
            midi_velocity(midi_event.data.get(2).copied().unwrap_or(0)),
        )),
        0xC0 => Some((
            channel,
            kCtrlProgramChange as CtrlNumber,
            midi_velocity(midi_event.data.get(1).copied().unwrap_or(0)),
        )),
        0xD0 => Some((
            channel,
            kAfterTouch as CtrlNumber,
            midi_velocity(midi_event.data.get(1).copied().unwrap_or(0)),
        )),
        0xE0 => {
            let lsb = midi_event.data.get(1).copied().unwrap_or(0) as u16;
            let msb = midi_event.data.get(2).copied().unwrap_or(0) as u16;
            let value = ((msb << 7) | lsb).min(16383) as f32 / 16383.0;
            Some((channel, kPitchBend as CtrlNumber, value))
        }
        _ => None,
    }
}

fn legacy_cc_to_midi(frame: u32, cc: LegacyMIDICCOutEvent) -> Option<MidiEvent> {
    let channel = (cc.channel as u8) & 0x0f;
    let control = cc.controlNumber as u16;
    let value = (cc.value as i16).clamp(0, 127) as u8;
    let value2 = (cc.value2 as i16).clamp(0, 127) as u8;

    let data = match control {
        x if x == kPitchBend as u16 => vec![0xE0 | channel, value, value2],
        x if x == kAfterTouch as u16 => vec![0xD0 | channel, value],
        x if x == kCtrlProgramChange as u16 => vec![0xC0 | channel, value],
        x if x <= 127 => vec![0xB0 | channel, x as u8, value],
        _ => return None,
    };
    Some(MidiEvent::new(frame, data))
}

fn copy_sysex_event(event: &Event) -> Option<Vec<u8>> {
    let data = unsafe { event.__field0.data };
    (data.r#type == DataTypes_::kMidiSysEx && !data.bytes.is_null()).then(|| unsafe {
        std::slice::from_raw_parts(data.bytes, data.size.min(usize::MAX as u32) as usize).to_vec()
    })
}

fn midi_velocity(value: u8) -> f32 {
    (value.min(127) as f32) / 127.0
}

fn midi_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_buffer_creation() {
        let buffer = EventBuffer::new();
        assert_eq!(buffer.event_count(), 0);
    }

    #[test]
    fn test_midi_events_conversion() {
        let midi = vec![
            MidiEvent::new(0, vec![0x90, 60, 100]),  // Note On C4
            MidiEvent::new(100, vec![0x80, 60, 64]), // Note Off C4
        ];

        let buffer = EventBuffer::from_midi_events(&midi, 0);
        assert_eq!(buffer.event_count(), 2);

        let output = buffer.to_midi_events();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].frame, 0);
        assert_eq!(output[0].data, vec![0x90, 60, 100]);
    }

    #[test]
    fn test_unsupported_events_are_ignored() {
        let midi = vec![MidiEvent::new(0, vec![0xB0, 74, 100])];
        let buffer = EventBuffer::from_midi_events(&midi, 0);
        assert_eq!(buffer.event_count(), 0);
    }

    #[test]
    fn test_sysex_roundtrip() {
        let midi = vec![MidiEvent::new(12, vec![0xF0, 0x7D, 0x10, 0xF7])];
        let buffer = EventBuffer::from_midi_events(&midi, 0);
        let output = buffer.to_midi_events();
        assert_eq!(output, midi);
    }

    #[test]
    fn test_controller_changes_map_to_parameter_changes() {
        struct TestMidiMapping;
        impl Class for TestMidiMapping {
            type Interfaces = (IMidiMapping,);
        }
        impl IMidiMappingTrait for TestMidiMapping {
            unsafe fn getMidiControllerAssignment(
                &self,
                _busIndex: i32,
                _channel: i16,
                midiControllerNumber: CtrlNumber,
                id: *mut ParamID,
            ) -> i32 {
                if midiControllerNumber == 74 {
                    unsafe {
                        *id = 1234;
                    }
                    kResultOk
                } else {
                    kResultFalse
                }
            }
        }

        let mapping = ComWrapper::new(TestMidiMapping)
            .to_com_ptr::<IMidiMapping>()
            .unwrap();
        let changes = ParameterChanges::from_midi_events(
            &[MidiEvent::new(64, vec![0xB0, 74, 100])],
            &mapping,
            0,
        )
        .unwrap();
        assert_eq!(unsafe { changes.getParameterCount() }, 1);
        let queue_ptr = unsafe { changes.getParameterData(0) };
        let queue = unsafe { ComPtr::from_raw(queue_ptr) }.unwrap();
        assert_eq!(unsafe { queue.getParameterId() }, 1234);
        assert_eq!(unsafe { queue.getPointCount() }, 1);
    }

    #[test]
    fn test_empty_buffer() {
        let buffer = EventBuffer::from_midi_events(&[], 0);
        assert_eq!(buffer.event_count(), 0);
        assert_eq!(buffer.to_midi_events().len(), 0);
    }
}
