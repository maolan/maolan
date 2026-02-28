use crate::midi::io::MidiEvent;

/// EventBuffer for MIDI events
///
/// NOTE: Full VST3 MIDI support requires implementing IEventList interface
/// which is complex due to the vst3 crate's opaque union types. This is a
/// simplified placeholder that provides the API but doesn't yet implement
/// full MIDI event translation.
///
/// TODO: Implement proper IEventList wrapper for full MIDI support
pub struct EventBuffer {
    midi_events: Vec<MidiEvent>,
}

impl Default for EventBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBuffer {
    pub fn new() -> Self {
        Self {
            midi_events: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.midi_events.clear();
    }

    /// Convert Maolan MIDI events to internal format
    ///
    /// Currently stores MIDI events as-is. Full implementation would convert
    /// to VST3 Event structures and implement IEventList interface.
    pub fn from_midi_events(midi_events: &[MidiEvent], _bus_index: i32) -> Self {
        Self {
            midi_events: midi_events.to_vec(),
        }
    }

    /// Convert back to Maolan MIDI events
    pub fn to_midi_events(&self) -> Vec<MidiEvent> {
        self.midi_events.clone()
    }

    pub fn event_count(&self) -> usize {
        self.midi_events.len()
    }

    /// Get MIDI event at index (internal format)
    pub fn get_midi_event(&self, index: usize) -> Option<&MidiEvent> {
        self.midi_events.get(index)
    }
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
    fn test_empty_buffer() {
        let buffer = EventBuffer::from_midi_events(&[], 0);
        assert_eq!(buffer.event_count(), 0);
        assert_eq!(buffer.to_midi_events().len(), 0);
    }
}
