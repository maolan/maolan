use super::{audio::track::AudioTrack, midi::track::MIDITrack};
use crate::{
    audio::io::AudioIO,
    lv2::Lv2Processor,
    message::{Lv2GraphConnection, Lv2GraphNode, Lv2GraphPlugin},
};
use std::sync::Arc;

#[derive(Debug)]
pub struct Lv2Instance {
    pub id: usize,
    pub processor: Lv2Processor,
}

#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub audio: AudioTrack,
    pub midi: MIDITrack,
    pub lv2_processors: Vec<Lv2Instance>,
    pub next_lv2_instance_id: usize,
    pub sample_rate: f64,
}

impl Track {
    pub fn new(
        name: String,
        audio_ins: usize,
        audio_outs: usize,
        midi_ins: usize,
        midi_outs: usize,
        buffer_size: usize,
        sample_rate: f64,
    ) -> Self {
        Self {
            name,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            audio: AudioTrack::new(audio_ins, audio_outs, buffer_size),
            midi: MIDITrack::new(midi_ins, midi_outs),
            lv2_processors: Vec::new(),
            next_lv2_instance_id: 0,
            sample_rate,
        }
        .with_default_audio_passthrough()
    }

    pub fn setup(&mut self) {
        self.audio.setup();
        for instance in &self.lv2_processors {
            instance.processor.setup_audio_ports();
        }
    }

    pub fn process(&mut self) {
        self.midi.process();
        for audio_in in &self.audio.ins {
            audio_in.process();
        }

        let frames = self
            .audio
            .ins
            .first()
            .map(|audio_in| audio_in.buffer.lock().len())
            .or_else(|| {
                self.audio
                    .outs
                    .first()
                    .map(|audio_out| audio_out.buffer.lock().len())
            })
            .unwrap_or(0);

        if !self.lv2_processors.is_empty() {
            let mut processed = vec![false; self.lv2_processors.len()];
            let mut remaining = self.lv2_processors.len();

            while remaining > 0 {
                let mut progressed = false;
                for (idx, already_processed) in processed.iter_mut().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    let all_inputs_ready = self.lv2_processors[idx]
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if !all_inputs_ready {
                        continue;
                    }

                    for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                        audio_in.process();
                    }
                    self.lv2_processors[idx].processor.process_with_audio_io(frames);
                    *already_processed = true;
                    remaining -= 1;
                    progressed = true;
                }

                if !progressed {
                    break;
                }
            }

            if remaining > 0 {
                for (idx, already_processed) in processed.iter().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                        audio_in.process();
                    }
                    self.lv2_processors[idx].processor.process_with_audio_io(frames);
                }
            }
        }

        let internal_sources = self.internal_audio_sources();
        for audio_out in &self.audio.outs {
            let out_samples = audio_out.buffer.lock();
            out_samples.fill(0.0);
            for source in audio_out.connections.lock().iter() {
                if !internal_sources
                    .iter()
                    .any(|internal| Arc::ptr_eq(internal, source))
                {
                    continue;
                }
                let source_buf = source.buffer.lock();
                for (out_sample, in_sample) in out_samples.iter_mut().zip(source_buf.iter()) {
                    *out_sample += *in_sample;
                }
            }
            *audio_out.finished.lock() = true;
        }

        self.audio.finished = true;
        self.audio.processing = false;
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn level(&self) -> f32 {
        self.level
    }
    pub fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    pub fn arm(&mut self) {
        self.armed = !self.armed;
    }
    pub fn mute(&mut self) {
        self.muted = !self.muted;
    }
    pub fn solo(&mut self) {
        self.soloed = !self.soloed;
    }

    pub fn load_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        let processor = Lv2Processor::new(self.sample_rate, uri)?;
        let id = self.next_lv2_instance_id;
        self.next_lv2_instance_id = self.next_lv2_instance_id.saturating_add(1);
        self.lv2_processors.push(Lv2Instance { id, processor });
        Ok(())
    }

    pub fn unload_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        let Some(index) = self
            .lv2_processors
            .iter()
            .position(|instance| instance.processor.uri() == uri)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 plugin loaded: {uri}",
                self.name
            ));
        };

        let removed = self.lv2_processors.remove(index);
        for port in removed.processor.audio_inputs() {
            Self::disconnect_all(port);
        }
        for port in removed.processor.audio_outputs() {
            Self::disconnect_all(port);
        }
        Ok(())
    }

    pub fn loaded_lv2_plugins(&self) -> Vec<String> {
        self.lv2_processors
            .iter()
            .map(|instance| instance.processor.uri().to_string())
            .collect()
    }

    pub fn loaded_lv2_instances(&self) -> Vec<(usize, String)> {
        self.lv2_processors
            .iter()
            .map(|instance| (instance.id, instance.processor.uri().to_string()))
            .collect()
    }

    pub fn lv2_graph_plugins(&self) -> Vec<Lv2GraphPlugin> {
        self.lv2_processors
            .iter()
            .map(|instance| Lv2GraphPlugin {
                instance_id: instance.id,
                uri: instance.processor.uri().to_string(),
                name: instance.processor.name().to_string(),
                audio_inputs: instance.processor.audio_input_count(),
                audio_outputs: instance.processor.audio_output_count(),
                midi_inputs: instance.processor.midi_input_count(),
                midi_outputs: instance.processor.midi_output_count(),
            })
            .collect()
    }

    pub fn lv2_graph_connections(&self) -> Vec<Lv2GraphConnection> {
        let mut source_ports: Vec<(Lv2GraphNode, usize, Arc<AudioIO>)> = self
            .audio
            .ins
            .iter()
            .enumerate()
            .map(|(idx, io)| (Lv2GraphNode::TrackInput, idx, io.clone()))
            .collect();
        for instance in &self.lv2_processors {
            source_ports.extend(
                instance
                    .processor
                    .audio_outputs()
                    .iter()
                    .enumerate()
                    .map(|(idx, io)| (Lv2GraphNode::PluginInstance(instance.id), idx, io.clone())),
            );
        }

        let mut connections = vec![];
        for (to_port, to_io) in self.audio.outs.iter().enumerate() {
            for conn in to_io.connections.lock().iter() {
                if let Some((from_node, from_port, _)) = source_ports
                    .iter()
                    .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                {
                    connections.push(Lv2GraphConnection {
                        from_node: from_node.clone(),
                        from_port: *from_port,
                        to_node: Lv2GraphNode::TrackOutput,
                        to_port,
                    });
                }
            }
        }
        for instance in &self.lv2_processors {
            for (to_port, to_io) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in to_io.connections.lock().iter() {
                    if let Some((from_node, from_port, _)) = source_ports
                        .iter()
                        .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                    {
                        connections.push(Lv2GraphConnection {
                            from_node: from_node.clone(),
                            from_port: *from_port,
                            to_node: Lv2GraphNode::PluginInstance(instance.id),
                            to_port,
                        });
                    }
                }
            }
        }
        connections
    }

    pub fn connect_lv2_audio(
        &self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.lv2_source_io(&from_node, from_port)?;
        let target = self.lv2_target_io(&to_node, to_port)?;
        AudioIO::connect(&source, &target);
        Ok(())
    }

    pub fn disconnect_lv2_audio(
        &self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.lv2_source_io(&from_node, from_port)?;
        let target = self.lv2_target_io(&to_node, to_port)?;
        AudioIO::disconnect(&source, &target)
    }

    pub fn show_lv2_plugin_ui(&mut self, uri: &str) -> Result<(), String> {
        let Some(instance) = self
            .lv2_processors
            .iter_mut()
            .find(|instance| instance.processor.uri() == uri)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 plugin loaded: {uri}",
                self.name
            ));
        };
        instance.processor.show_ui()
    }

    pub fn show_lv2_plugin_ui_instance(&mut self, instance_id: usize) -> Result<(), String> {
        let Some(instance) = self
            .lv2_processors
            .iter_mut()
            .find(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };
        instance.processor.show_ui()
    }

    fn with_default_audio_passthrough(self) -> Self {
        self.ensure_default_audio_passthrough();
        self
    }

    pub(crate) fn ensure_default_audio_passthrough(&self) {
        for (audio_in, audio_out) in self.audio.ins.iter().zip(self.audio.outs.iter()) {
            let exists = audio_out
                .connections
                .lock()
                .iter()
                .any(|conn| Arc::ptr_eq(conn, audio_in));
            if !exists {
                AudioIO::connect(audio_in, audio_out);
            }
        }
    }

    fn internal_audio_sources(&self) -> Vec<Arc<AudioIO>> {
        let mut sources = self.audio.ins.clone();
        for instance in &self.lv2_processors {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        sources
    }

    fn disconnect_all(port: &Arc<AudioIO>) {
        let connections = port.connections.lock().clone();
        for other in connections {
            let _ = AudioIO::disconnect(&other, port);
        }
    }

    fn lv2_source_io(&self, node: &Lv2GraphNode, port: usize) -> Result<Arc<AudioIO>, String> {
        match node {
            Lv2GraphNode::TrackInput => self
                .audio
                .ins
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track input port {port} not found")),
            Lv2GraphNode::TrackOutput => Err("Track output node cannot be source".to_string()),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| instance.processor.audio_outputs().get(port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} output port {port} missing")),
        }
    }

    fn lv2_target_io(&self, node: &Lv2GraphNode, port: usize) -> Result<Arc<AudioIO>, String> {
        match node {
            Lv2GraphNode::TrackInput => Err("Track input node cannot be target".to_string()),
            Lv2GraphNode::TrackOutput => self
                .audio
                .outs
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track output port {port} not found")),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| instance.processor.audio_inputs().get(port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} input port {port} missing")),
        }
    }
}
