use std::{
    collections::{HashMap, VecDeque},
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
    oss_in: Option<oss::Audio>,
    oss_out: Option<oss::Audio>,
    sorted_tracks: Vec<String>,
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
            sorted_tracks: vec![],
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

    pub fn sort_tracks(&mut self) {
        let state = self.state.lock();
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        for name in state.tracks.keys() {
            in_degree.insert(name.clone(), 0);
            adjacency_list.insert(name.clone(), vec![]);
        }

        for (from_name, from_track_handle) in &state.tracks {
            let from_track = from_track_handle.lock();
            for out_port in &from_track.audio.outs {
                let conns = out_port.connections.lock();
                for connection in conns.iter() {
                    for (to_name, to_track_handle) in &state.tracks {
                        if from_name == to_name {
                            continue;
                        }
                        let target_track = to_track_handle.lock();
                        if target_track
                            .audio
                            .ins
                            .iter()
                            .any(|ins| Arc::ptr_eq(&ins.buffer, &connection.buffer))
                        {
                            adjacency_list
                                .get_mut(from_name)
                                .unwrap()
                                .push(to_name.clone());
                            *in_degree.get_mut(to_name).unwrap() += 1;
                            break;
                        }
                    }
                }
            }
        }

        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|&(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        self.sorted_tracks.clear();
        while let Some(u) = queue.pop_front() {
            self.sorted_tracks.push(u.clone());
            if let Some(neighbors) = adjacency_list.get(&u) {
                for v in neighbors {
                    let degree = in_degree.get_mut(v).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(v.clone());
                    }
                }
            }
        }
    }

    pub fn check_if_leads_to(&self, current_track_name: &str, target_track_name: &str) -> bool {
        let neighbors: Vec<String> = {
            let state = self.state.lock();
            let mut found_neighbors = Vec::new();

            if let Some(current_track_handle) = state.tracks.get(current_track_name) {
                let current_track = current_track_handle.lock();

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
            found_neighbors
        };

        for neighbor in neighbors {
            if neighbor == target_track_name {
                return true;
            }

            if self.check_if_leads_to(&neighbor, target_track_name) {
                return true;
            }
        }

        false
    }

    pub async fn work(&mut self) {
        let mut ready_workers: Vec<usize> = vec![];
        while let Some(message) = self.rx.recv().await {
            match message {
                // Message::Play => {
                //     let track;
                //     {
                //         track = self.state.lock().audio.tracks[""].clone();
                //     }
                //     match self.workers[0].tx.send(Message::ProcessAudio(track)) {
                //         Ok(_) => {}
                //         Err(e) => {
                //             println!("Error occured while sending PLAY: {e}")
                //         }
                //     }
                // }
                Message::Ready(id) => {
                    ready_workers.push(id);
                }
                Message::Finished(_workid, _trackid) => {}
                Message::Channel(s) => {
                    self.clients.push(s);
                }

                Message::Request(a) => {
                    match a {
                        Action::Play => {}
                        Action::Quit => {
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
                                    tracks.insert(
                                        name.clone(),
                                        Arc::new(UnsafeMutex::new(Box::new(Track::new(
                                            name.clone(),
                                            audio_ins,
                                            audio_outs,
                                            midi_ins,
                                            midi_outs,
                                            oss.chsamples,
                                        )))),
                                    );
                                }
                                None => {
                                    self.notify_clients(
                                        Err("Engine needs to open audio device before adding audio track".to_string())
                                    ).await;
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
                        Action::ClipMove {
                            ref kind,
                            ref from,
                            ref to,
                            copy,
                        } => {
                            if let Some(from_track_handle) =
                                self.state.lock().tracks.get(&from.track_name)
                                && let Some(to_track_handle) =
                                    self.state.lock().tracks.get(&to.track_name)
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
                                            ))).await;
                                            return;
                                        }
                                        let clip_copy =
                                            from_track.audio.clips[from.clip_index].clone();
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
                                            ))).await;
                                            return;
                                        }
                                        let clip_copy =
                                            from_track.midi.clips[from.clip_index].clone();
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
                                            ))).await;
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
                                            ))).await;
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
                            let state = self.state.lock();
                            let from_track_handle = state.tracks.get(from_track);
                            let to_track_handle = state.tracks.get(to_track);

                            if from_track_handle.is_none() {
                                self.notify_clients(Err(format!(
                                    "Source track '{}' not found",
                                    from_track
                                )))
                                .await;
                                return;
                            }

                            if to_track_handle.is_none() {
                                self.notify_clients(Err(format!(
                                    "Destination track '{}' not found",
                                    to_track
                                )))
                                .await;
                                return;
                            }

                            let from_track_ref = from_track_handle.unwrap();
                            let to_track_ref = to_track_handle.unwrap();

                            match kind {
                                Kind::Audio => {
                                    let to_in_arc = {
                                        let to_track = to_track_ref.lock();
                                        to_track.audio.ins.get(to_port).cloned()
                                    };
                                    match to_in_arc {
                                        Some(to_in) => {
                                            if self.check_if_leads_to(to_track, from_track) {
                                                self.notify_clients(Err("Circular routing is not allowed in this engine!".to_string())).await;
                                                return;
                                            }
                                            if let Err(e) = from_track_ref
                                                .lock()
                                                .audio
                                                .connect_out(from_port, to_in.clone())
                                            {
                                                self.notify_clients(Err(e)).await;
                                            }
                                        }
                                        None => {
                                            self.notify_clients(Err(format!(
                                                "Audio input port {} not found on track '{}'",
                                                to_port, to_track
                                            )))
                                            .await;
                                        }
                                    }
                                }
                                Kind::MIDI => {
                                    if let Some(to_in) = to_track_ref.lock().midi.ins.get(to_port) {
                                        if let Err(e) = from_track_ref
                                            .lock()
                                            .midi
                                            .connect_out(from_port, to_in.clone())
                                        {
                                            self.notify_clients(Err(e)).await;
                                        }
                                    } else {
                                        self.notify_clients(Err(format!(
                                            "MIDI input port {} not found on track '{}'",
                                            to_port, to_track
                                        )))
                                        .await;
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
                            let state = self.state.lock();
                            let from_track_handle = state.tracks.get(from_track);
                            let to_track_handle = state.tracks.get(to_track);

                            if from_track_handle.is_none() {
                                self.notify_clients(Err(format!(
                                    "Source track '{}' not found",
                                    from_track
                                )))
                                .await;
                                return;
                            }

                            if to_track_handle.is_none() {
                                self.notify_clients(Err(format!(
                                    "Destination track '{}' not found",
                                    to_track
                                )))
                                .await;
                                return;
                            }

                            let from_track_ref = from_track_handle.unwrap();
                            let to_track_ref = to_track_handle.unwrap();

                            let result = match kind {
                                Kind::Audio => {
                                    if let Some(to_in) = to_track_ref.lock().audio.ins.get(to_port)
                                    {
                                        from_track_ref.lock().audio.disconnect_out(from_port, to_in)
                                    } else {
                                        Err(format!(
                                            "Audio input port {} not found on track '{}'",
                                            to_port, to_track
                                        ))
                                    }
                                }
                                Kind::MIDI => {
                                    if let Some(to_in) = to_track_ref.lock().midi.ins.get(to_port) {
                                        from_track_ref.lock().midi.disconnect_out(from_port, to_in)
                                    } else {
                                        Err(format!(
                                            "MIDI input port {} not found on track '{}'",
                                            to_port, to_track
                                        ))
                                    }
                                }
                            };

                            match result {
                                Ok(_) => {
                                    self.notify_clients(Ok(a.clone())).await;
                                    return;
                                }
                                Err(err) => {
                                    self.notify_clients(Err(err)).await;
                                    return;
                                }
                            }
                        }
                        Action::OpenAudioDevice(ref device) => {
                            match oss::Audio::new(device, 48000, 32, true) {
                                Ok(d) => {
                                    self.oss_in = Some(d);
                                }
                                Err(e) => {
                                    self.notify_clients(Err(e.to_string())).await;
                                }
                            }
                            match oss::Audio::new(device, 48000, 32, false) {
                                Ok(d) => {
                                    self.oss_out = Some(d);
                                }
                                Err(e) => {
                                    self.notify_clients(Err(e.to_string())).await;
                                }
                            }
                        }
                    }
                    self.notify_clients(Ok(a.clone())).await;
                }
                _ => {}
            }
        }
    }
}
