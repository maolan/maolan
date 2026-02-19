use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

use crate::{
    audio::clip::AudioClip,
    hw::oss,
    kind::Kind,
    message::{Action, Message},
    midi::clip::MIDIClip,
    mutex::UnsafeMutex,
    oss_worker::OssWorker,
    state::State,
    track::Track,
    worker::Worker,
};

#[derive(Debug)]
struct WorkerData {
    tx: Sender<Message>,
    handle: JoinHandle<()>,
}

impl WorkerData {
    pub fn new(tx: Sender<Message>, handle: JoinHandle<()>) -> Self {
        Self { tx, handle }
    }
}

pub struct Engine {
    clients: Vec<Sender<Message>>,
    rx: Receiver<Message>,
    state: Arc<UnsafeMutex<State>>,
    tx: Sender<Message>,
    workers: Vec<WorkerData>,
    oss_in: Option<Arc<UnsafeMutex<oss::Audio>>>,
    oss_out: Option<Arc<UnsafeMutex<oss::Audio>>>,
    oss_worker: Option<WorkerData>,
    ready_workers: Vec<usize>,
    pending_requests: VecDeque<Action>,
    awaiting_hwfinished: bool,
}

impl Engine {
    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            rx,
            tx,
            clients: vec![],
            state: Arc::new(UnsafeMutex::new(State::default())),
            workers: vec![],
            oss_in: None,
            oss_out: None,
            oss_worker: None,
            ready_workers: vec![],
            pending_requests: VecDeque::new(),
            awaiting_hwfinished: false,
        }
    }

    pub async fn init(&mut self) {
        let max_threads = num_cpus::get();
        for id in 0..max_threads {
            let (tx, rx) = channel::<Message>(32);
            let tx_thread = self.tx.clone();
            let handler = tokio::spawn(async move {
                let wrk = Worker::new(id, rx, tx_thread);
                wrk.await.work().await;
            });
            self.workers.push(WorkerData::new(tx.clone(), handler));
        }
    }

    async fn notify_clients(&self, action: Result<Action, String>) {
        for client in &self.clients {
            client
                .send(Message::Response(action.clone()))
                .await
                .expect("Error sending response to client");
        }
    }

    async fn request_hw_cycle(&mut self) {
        if self.awaiting_hwfinished {
            return;
        }
        if let Some(worker) = &self.oss_worker {
            match worker.tx.send(Message::TracksFinished).await {
                Ok(_) => {
                    self.awaiting_hwfinished = true;
                }
                Err(e) => {
                    eprintln!("Error sending TracksFinished {e}");
                }
            }
        }
    }

    async fn send_tracks(&mut self) -> bool {
        let mut finished = true;
        for track in self.state.lock().tracks.values() {
            let t = track.lock();
            finished &= t.audio.finished;
            if !t.audio.finished && !t.audio.processing && t.audio.ready() {
                if self.ready_workers.is_empty() {
                    return false;
                }
                let worker_index = self.ready_workers.remove(0);
                t.audio.processing = true;
                let worker = &self.workers[worker_index];
                if let Err(e) = worker.tx.send(Message::ProcessTrack(track.clone())).await {
                    t.audio.processing = false;
                    self.notify_clients(Err(format!("Failed to send track to worker: {}", e)))
                        .await;
                }
            }
        }
        finished
    }

    pub fn check_if_leads_to_kind(
        &self,
        kind: Kind,
        current_track_name: &str,
        target_track_name: &str,
    ) -> bool {
        let mut visited = HashSet::new();
        self.check_if_leads_to_inner(kind, current_track_name, target_track_name, &mut visited)
    }

    fn check_if_leads_to_inner(
        &self,
        kind: Kind,
        current_track_name: &str,
        target_track_name: &str,
        visited: &mut HashSet<String>,
    ) -> bool {
        if current_track_name == target_track_name {
            return true;
        }

        if visited.contains(current_track_name) {
            return false;
        }
        visited.insert(current_track_name.to_string());

        let neighbors = self.connected_neighbors(kind, current_track_name);

        for neighbor in neighbors {
            if self.check_if_leads_to_inner(kind, &neighbor, target_track_name, visited) {
                return true;
            }
        }

        false
    }

    fn connected_neighbors(&self, kind: Kind, current_track_name: &str) -> Vec<String> {
        let state = self.state.lock();
        let mut found_neighbors = Vec::new();

        if let Some(current_track_handle) = state.tracks.get(current_track_name) {
            let current_track = current_track_handle.lock();

            match kind {
                Kind::Audio => {
                    for out_port in &current_track.audio.outs {
                        let conns = out_port.connections.lock();
                        for conn in conns.iter() {
                            for (name, next_track_handle) in &state.tracks {
                                let next_track = next_track_handle.lock();
                                let is_connected = next_track
                                    .audio
                                    .ins
                                    .iter()
                                    .any(|ins_port| Arc::ptr_eq(&ins_port.buffer, &conn.buffer));

                                if is_connected {
                                    found_neighbors.push(name.clone());
                                }
                            }
                        }
                    }
                }
                Kind::MIDI => {
                    for out_port in &current_track.midi.outs {
                        let conns = out_port.lock().connections.clone();
                        for conn in conns.iter() {
                            for (name, next_track_handle) in &state.tracks {
                                let next_track = next_track_handle.lock();
                                let is_connected = next_track
                                    .midi
                                    .ins
                                    .iter()
                                    .any(|ins_port| Arc::ptr_eq(ins_port, conn));

                                if is_connected {
                                    found_neighbors.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        found_neighbors
    }

    async fn handle_request(&mut self, a: Action) {
        match a {
            Action::Play => {}
            Action::Quit => {
                if let Some(worker) = self.oss_worker.take() {
                    worker
                        .tx
                        .send(Message::Request(a.clone()))
                        .await
                        .expect("Failed sending quit message to OSS worker");
                    worker
                        .handle
                        .await
                        .expect("Failed waiting for OSS worker to quit");
                }

                // Then shut down regular workers
                while !self.workers.is_empty() {
                    let worker = self.workers.remove(0);
                    worker
                        .tx
                        .send(Message::Request(a.clone()))
                        .await
                        .expect("Failed sending quit message to worker");
                    worker
                        .handle
                        .await
                        .expect("Failed waiting for worker to quit");
                }
            }
            Action::AddTrack {
                ref name,
                audio_ins,
                midi_ins,
                audio_outs,
                midi_outs,
            } => {
                let tracks = &mut self.state.lock().tracks;
                if tracks.contains_key(name) {
                    self.notify_clients(Err(format!("Track {} already exists", name)))
                        .await;
                    return;
                }
                match &self.oss_out {
                    Some(oss) => {
                        let chsamples = oss.lock().chsamples;
                        tracks.insert(
                            name.clone(),
                            Arc::new(UnsafeMutex::new(Box::new(Track::new(
                                name.clone(),
                                audio_ins,
                                audio_outs,
                                midi_ins,
                                midi_outs,
                                chsamples,
                                oss.lock().rate as f64,
                            )))),
                        );
                        if let Some(track) = tracks.get(name) {
                            track.lock().ensure_default_audio_passthrough();
                        }
                    }
                    None => {
                        self.notify_clients(Err(
                            "Engine needs to open audio device before adding audio track"
                                .to_string(),
                        ))
                        .await;
                    }
                }
            }
            Action::RemoveTrack(ref name) => {
                self.state.lock().tracks.remove(name);
            }
            Action::TrackLevel(ref name, level) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().set_level(level);
                }
            }
            Action::TrackToggleArm(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().arm();
                }
            }
            Action::TrackToggleMute(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().mute();
                }
            }
            Action::TrackToggleSolo(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().solo();
                }
            }
            Action::TrackLoadLv2Plugin {
                ref track_name,
                ref plugin_uri,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().load_lv2_plugin(plugin_uri) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackUnloadLv2Plugin {
                ref track_name,
                ref plugin_uri,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().unload_lv2_plugin(plugin_uri) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackUnloadLv2PluginInstance {
                ref track_name,
                instance_id,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().unload_lv2_plugin_instance(instance_id) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackShowLv2PluginUi {
                ref track_name,
                ref plugin_uri,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().show_lv2_plugin_ui(plugin_uri) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackShowLv2PluginUiInstance {
                ref track_name,
                instance_id,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().show_lv2_plugin_ui_instance(instance_id) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackGetLv2Graph { ref track_name } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        let (plugins, connections) = {
                            let track = track.lock();
                            (track.lv2_graph_plugins(), track.lv2_graph_connections())
                        };
                        self.notify_clients(Ok(Action::TrackLv2Graph {
                            track_name: track_name.clone(),
                            plugins,
                            connections,
                        }))
                        .await;
                        return;
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackLv2Graph { .. } => {}
            Action::TrackConnectLv2Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().connect_lv2_audio(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackConnectLv2Midi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().connect_lv2_midi(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackDisconnectLv2Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().disconnect_lv2_audio(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackDisconnectLv2Midi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().disconnect_lv2_midi(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::ListLv2Plugins => {
                let plugins = {
                    let host = crate::lv2::Lv2Host::new(48_000.0);
                    host.list_plugins()
                };
                self.notify_clients(Ok(Action::Lv2Plugins(plugins))).await;
                return;
            }
            Action::Lv2Plugins(_) => {}
            Action::ClipMove {
                ref kind,
                ref from,
                ref to,
                copy,
            } => {
                if let Some(from_track_handle) = self.state.lock().tracks.get(&from.track_name)
                    && let Some(to_track_handle) = self.state.lock().tracks.get(&to.track_name)
                {
                    let from_track = from_track_handle.lock();
                    let to_track = to_track_handle.lock();
                    match kind {
                        Kind::Audio => {
                            if from.clip_index >= from_track.audio.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    from.clip_index,
                                    from_track.name.clone(),
                                    from_track.audio.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            let clip_copy = from_track.audio.clips[from.clip_index].clone();
                            if !copy {
                                from_track.audio.clips.remove(from.clip_index);
                            }
                            to_track.audio.clips.push(clip_copy);
                        }
                        Kind::MIDI => {
                            if from.clip_index >= from_track.midi.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    from.clip_index,
                                    from_track.name.clone(),
                                    from_track.midi.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            let clip_copy = from_track.midi.clips[from.clip_index].clone();
                            if !copy {
                                from_track.midi.clips.remove(from.clip_index);
                            }
                            to_track.midi.clips.push(clip_copy);
                        }
                    }
                }
            }
            Action::AddClip {
                ref name,
                ref track_name,
                start,
                length,
                kind,
            } => {
                if let Some(track) = self.state.lock().tracks.get(track_name) {
                    match kind {
                        Kind::Audio => {
                            let clip = AudioClip::new(name.clone(), start, length);
                            track.lock().audio.clips.push(clip);
                        }
                        Kind::MIDI => {
                            let clip = MIDIClip::new(name.clone(), start, length);
                            track.lock().midi.clips.push(clip);
                        }
                    }
                }
            }
            Action::RemoveClip(index, ref track_name, kind) => {
                if let Some(track) = self.state.lock().tracks.get(track_name) {
                    match kind {
                        Kind::Audio => {
                            if index >= track.lock().audio.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    index,
                                    track.lock().name.clone(),
                                    track.lock().audio.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            track.lock().audio.clips.remove(index);
                        }
                        Kind::MIDI => {
                            if index >= track.lock().midi.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    index,
                                    track.lock().name.clone(),
                                    track.lock().midi.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            track.lock().midi.clips.remove(index);
                        }
                    }
                }
            }
            Action::Connect {
                ref from_track,
                from_port,
                ref to_track,
                to_port,
                kind,
            } => {
                match kind {
                    Kind::Audio => {
                        let from_audio_io = if from_track == "hw:in" {
                            self.oss_in
                                .as_ref()
                                .and_then(|h| h.lock().channels.get(from_port).cloned())
                        } else {
                            self.state
                                .lock()
                                .tracks
                                .get(from_track)
                                .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                        };
                        let to_audio_io = if to_track == "hw:out" {
                            self.oss_out
                                .as_ref()
                                .and_then(|h| h.lock().channels.get(to_port).cloned())
                        } else {
                            self.state
                                .lock()
                                .tracks
                                .get(to_track)
                                .and_then(|t| t.lock().audio.ins.get(to_port).cloned())
                        };
                        match (from_audio_io, to_audio_io) {
                            (Some(source), Some(target)) => {
                                if from_track != "hw:in"
                                    && to_track != "hw:out"
                                    && self.check_if_leads_to_kind(Kind::Audio, to_track, from_track)
                                {
                                    self.notify_clients(Err("Circular routing is not allowed!".into()))
                                        .await;
                                    return;
                                }
                                crate::audio::io::AudioIO::connect(&source, &target);
                            }
                            (None, _) => {
                                self.notify_clients(Err(format!(
                                    "Source track '{}' not found",
                                    from_track
                                )))
                                .await;
                                return;
                            }
                            (_, None) => {
                                self.notify_clients(Err(format!(
                                    "Destination track '{}' not found",
                                    to_track
                                )))
                                .await;
                                return;
                            }
                        }
                    }
                    Kind::MIDI => {
                        if from_track == "hw:in" || to_track == "hw:out" {
                            self.notify_clients(Err(
                                "Hardware MIDI connections are not supported!".into(),
                            ))
                            .await;
                            return;
                        }
                        if self.check_if_leads_to_kind(Kind::MIDI, to_track, from_track) {
                            self.notify_clients(Err("Circular routing is not allowed!".into()))
                                .await;
                            return;
                        }

                        let state = self.state.lock();
                        let from_track_handle = state.tracks.get(from_track);
                        let to_track_handle = state.tracks.get(to_track);

                        match (from_track_handle, to_track_handle) {
                            (Some(f_t), Some(t_t)) => {
                                let to_in_res = t_t.lock().midi.ins.get(to_port).cloned();
                                if let Some(to_in) = to_in_res {
                                    if let Err(e) = f_t.lock().midi.connect_out(from_port, to_in) {
                                        self.notify_clients(Err(e)).await;
                                    }
                                } else {
                                    self.notify_clients(Err(format!(
                                        "MIDI input port {} not found on track '{}',",
                                        to_port, to_track
                                    )))
                                    .await;
                                }
                            }
                            _ => {
                                self.notify_clients(Err(format!(
                                    "MIDI tracks not found: {} or {}",
                                    from_track, to_track
                                )))
                                .await;
                            }
                        }
                    }
                };
            }
            Action::Disconnect {
                ref from_track,
                from_port,
                ref to_track,
                to_port,
                kind,
            } => {
                let from_audio_io = if from_track == "hw:in" {
                    self.oss_in
                        .as_ref()
                        .and_then(|h| h.lock().channels.get(from_port).cloned())
                } else {
                    let state = self.state.lock();
                    state
                        .tracks
                        .get(from_track)
                        .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                };
                let to_audio_io = if to_track == "hw:out" {
                    self.oss_out
                        .as_ref()
                        .and_then(|h| h.lock().channels.get(to_port).cloned())
                } else {
                    let state = self.state.lock();
                    state
                        .tracks
                        .get(to_track)
                        .and_then(|t| t.lock().audio.ins.get(to_port).cloned())
                };

                if kind == Kind::Audio {
                    match (from_audio_io, to_audio_io) {
                        (Some(source), Some(target)) => {
                            if let Err(e) = crate::audio::io::AudioIO::disconnect(&source, &target)
                            {
                                self.notify_clients(Err(format!("Disconnect failed: {}", e)))
                                    .await;
                                return;
                            }
                        }
                        _ => {
                            self.notify_clients(Err(format!(
                                "Disconnect failed: Port not found ({} -> {})",
                                from_track, to_track
                            )))
                            .await;
                        }
                    }
                } else if kind == Kind::MIDI && from_track != "hw:in" && to_track != "hw:out" {
                    let state = self.state.lock();
                    if let (Some(f_t), Some(t_t)) =
                        (state.tracks.get(from_track), state.tracks.get(to_track))
                        && let Some(to_in) = t_t.lock().midi.ins.get(to_port).cloned()
                    {
                        if let Err(e) = f_t.lock().midi.disconnect_out(from_port, &to_in) {
                            self.notify_clients(Err(e)).await;
                        } else {
                            self.notify_clients(Ok(a.clone())).await;
                        }
                    }
                }
            }

            Action::OpenAudioDevice(ref device) => {
                match oss::Audio::new(device, 48000, 32, true) {
                    Ok(d) => {
                        let channels = d.channels.len();
                        let rate = d.rate as usize;
                        self.oss_in = Some(Arc::new(UnsafeMutex::new(d)));
                        self.notify_clients(Ok(Action::HWInfo {
                            channels,
                            rate,
                            input: true,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e.to_string())).await;
                    }
                }
                match oss::Audio::new(device, 48000, 32, false) {
                    Ok(d) => {
                        let channels = d.channels.len();
                        let rate = d.rate as usize;
                        self.oss_out = Some(Arc::new(UnsafeMutex::new(d)));
                        self.notify_clients(Ok(Action::HWInfo {
                            channels,
                            rate,
                            input: false,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e.to_string())).await;
                    }
                }

                if self.oss_worker.is_none() && self.oss_in.is_some() && self.oss_out.is_some() {
                    let (tx, rx) = channel::<Message>(32);
                    let oss_in = self.oss_in.clone().unwrap();
                    let oss_out = self.oss_out.clone().unwrap();
                    let tx_engine = self.tx.clone();
                    let handler = tokio::spawn(async move {
                        let worker = OssWorker::new(oss_in, oss_out, rx, tx_engine);
                        worker.work().await;
                    });
                    self.oss_worker = Some(WorkerData::new(tx, handler));
                    self.request_hw_cycle().await;
                }
            }
            Action::HWInfo { .. } => {}
        }
        self.notify_clients(Ok(a.clone())).await;
    }

    pub async fn work(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Ready(id) => {
                    self.ready_workers.push(id);
                }
                Message::Finished(workid) => {
                    self.ready_workers.push(workid);
                    let all_finished = self.send_tracks().await;
                    if all_finished {
                        self.request_hw_cycle().await;
                    }
                }
                Message::Channel(s) => {
                    self.clients.push(s);
                }

                Message::Request(a) => match a {
                    Action::OpenAudioDevice(_) | Action::Quit | Action::ListLv2Plugins => {
                        self.handle_request(a).await;
                    }
                    _ => {
                        self.pending_requests.push_back(a);
                        self.request_hw_cycle().await;
                    }
                },
                Message::HWFinished => {
                    if !self.awaiting_hwfinished {
                        continue;
                    }
                    self.awaiting_hwfinished = false;
                    while let Some(a) = self.pending_requests.pop_front() {
                        self.handle_request(a).await;
                    }
                    for track in self.state.lock().tracks.values() {
                        track.lock().setup();
                    }
                    if self.send_tracks().await {
                        self.request_hw_cycle().await;
                    }
                }
                _ => {}
            }
        }
    }
}
