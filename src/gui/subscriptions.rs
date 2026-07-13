use super::{CLIENT, Maolan};
use crate::{
    consts::gui::{METER_DIRTY_EPSILON_DB, METER_QUANTIZE_STEP_DB},
    message::{Message, Show},
    ui_timing::{
        PLAYHEAD_UPDATE_INTERVAL, RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL,
        RECORDING_PREVIEW_UPDATE_INTERVAL,
    },
};
use iced::futures::{Stream, StreamExt, stream};
use iced::keyboard::Event as KeyEvent;
use iced::{Subscription, event, keyboard, mouse, window};
use maolan_engine::message::{Action as EngineAction, Message as EngineMessage};
use std::collections::HashMap;
use std::time::Duration;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

impl Maolan {
    fn should_drive_playback_ui(&self) -> bool {
        let state = self.state.blocking_read();
        match state.view {
            crate::state::View::Workspace => {
                self.toolbar_visible
                    || self.tracks_visible
                    || self.editor_visible
                    || self.mixer_visible
            }
            crate::state::View::Piano | crate::state::View::PitchCorrection => true,
            _ => false,
        }
    }

    fn should_poll_meters(
        mixer_visible: bool,
        playing: bool,
        paused: bool,
        live_session_playing: bool,
        hw_out_meter_db: &[f32],
        track_meter_dbs: impl IntoIterator<Item = impl AsRef<[f32]>>,
    ) -> bool {
        if !mixer_visible {
            return false;
        }
        if playing || paused || live_session_playing {
            return true;
        }
        hw_out_meter_db.iter().any(|level| *level > -90.0)
            || track_meter_dbs
                .into_iter()
                .any(|meter_db| meter_db.as_ref().iter().any(|level| *level > -90.0))
    }

    pub fn subscription(&self) -> Subscription<Message> {
        fn quantize_meter_db(level_db: f32) -> f32 {
            let step = METER_QUANTIZE_STEP_DB;
            ((level_db / step).round() * step).clamp(-90.0, 20.0)
        }

        fn meter_changed(prev: &[f32], next: &[f32]) -> bool {
            if prev.len() != next.len() {
                return true;
            }
            prev.iter().zip(next.iter()).any(|(a, b)| {
                (quantize_meter_db(*a) - quantize_meter_db(*b)).abs() >= METER_DIRTY_EPSILON_DB
            })
        }

        fn listener() -> impl Stream<Item = Message> {
            stream::once(CLIENT.subscribe()).flat_map(|receiver| {
                #[cfg(all(unix, not(target_os = "macos")))]
                let initial_messages = vec![
                    Message::RefreshLv2Plugins,
                    Message::RefreshVst3Plugins,
                    Message::RefreshClapPlugins,
                ];
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                let initial_messages =
                    vec![Message::RefreshVst3Plugins, Message::RefreshClapPlugins];
                let initial = stream::iter(initial_messages);
                initial.chain(stream::unfold(
                    (
                        receiver,
                        Vec::<f32>::new(),
                        HashMap::<String, Vec<f32>>::new(),
                    ),
                    |(mut rx, mut last_hw_out, mut last_meters)| async move {
                        loop {
                            match rx.recv().await {
                                Some(EngineMessage::Response(r)) => {
                                    if let Ok(action) = &r {
                                        match action {
                                            EngineAction::TrackMeters {
                                                track_name,
                                                output_db,
                                            } => {
                                                let should_forward = match last_meters
                                                    .get(track_name)
                                                {
                                                    Some(prev) => meter_changed(prev, output_db),
                                                    None => true,
                                                };
                                                if !should_forward {
                                                    continue;
                                                }
                                                last_meters
                                                    .insert(track_name.clone(), output_db.clone());
                                            }
                                            EngineAction::MeterSnapshot {
                                                hw_out_db,
                                                track_meters,
                                            } => {
                                                // Always forward meter snapshots so the GUI can
                                                // keep smoothing levels down even when the engine
                                                // has already reached silence (for example after
                                                // playback stops).
                                                last_hw_out.clear();
                                                last_hw_out.extend_from_slice(hw_out_db);
                                                last_meters.clear();
                                                last_meters.reserve(track_meters.len());
                                                for (track_name, output_db) in track_meters.iter() {
                                                    last_meters.insert(
                                                        track_name.clone(),
                                                        output_db.clone(),
                                                    );
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    return Some((
                                        Message::Response(r),
                                        (rx, last_hw_out, last_meters),
                                    ));
                                }
                                Some(_) => {}
                                None => return None,
                            }
                        }
                    },
                ))
            })
        }
        let engine_sub = Subscription::run(listener);

        let current_view = self.state.blocking_read().view.clone();
        let keyboard_sub = keyboard::listen()
            .with(current_view)
            .map(|(current_view, event)| Self::keyboard_message(event, current_view));

        let event_sub = event::listen().map(|event| match event {
            event::Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::ButtonPressed(button) => Message::MousePressed(button),
                mouse::Event::CursorMoved { .. } => Message::MouseMoved(mouse_event),
                mouse::Event::ButtonReleased(_) => Message::MouseReleased,
                _ => Message::None,
            },
            event::Event::Window(window::Event::Resized(size)) => Message::WindowResized(size),
            event::Event::Window(window::Event::CloseRequested) => Message::WindowCloseRequested,
            _ => Message::None,
        });

        let shutdown_sub = Subscription::run(|| {
            stream::unfold((), |_| async move {
                #[cfg(unix)]
                {
                    let mut sigint = signal(SignalKind::interrupt()).ok()?;
                    let mut sigquit = signal(SignalKind::quit()).ok()?;
                    let mut sigterm = signal(SignalKind::terminate()).ok()?;
                    tokio::select! {
                        _ = sigint.recv() => Some((Message::WindowCloseRequested, ())),
                        _ = sigquit.recv() => Some((Message::WindowCloseRequested, ())),
                        _ = sigterm.recv() => Some((Message::WindowCloseRequested, ())),
                    }
                }
                #[cfg(not(unix))]
                {
                    tokio::signal::ctrl_c().await.ok()?;
                    Some((Message::WindowCloseRequested, ()))
                }
            })
        });

        let playback_sub = if self.playing && !self.paused && self.should_drive_playback_ui() {
            iced::time::every(PLAYHEAD_UPDATE_INTERVAL).map(|_| Message::PlaybackTick)
        } else {
            Subscription::none()
        };
        let should_poll_meters = if self.mixer_visible {
            let state = self.state.blocking_read();
            Self::should_poll_meters(
                self.mixer_visible,
                self.playing,
                self.paused,
                self.live_session_playing,
                &state.hw_out_meter_db,
                state
                    .tracks
                    .iter()
                    .map(|track| track.meter_out_db.as_slice()),
            )
        } else {
            false
        };
        let meter_poll_sub = if should_poll_meters {
            iced::time::every(Duration::from_millis(40)).map(|_| Message::MeterPollTick)
        } else {
            Subscription::none()
        };
        let recording_preview_sub = if self.playing
            && !self.paused
            && self.record_armed
            && self.recording_preview_start_sample.is_some()
        {
            iced::time::every(RECORDING_PREVIEW_UPDATE_INTERVAL)
                .map(|_| Message::RecordingPreviewTick)
        } else {
            Subscription::none()
        };
        let recording_preview_peaks_sub = if self.playing
            && !self.paused
            && self.record_armed
            && self.recording_preview_start_sample.is_some()
        {
            iced::time::every(RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL)
                .map(|_| Message::RecordingPreviewPeaksTick)
        } else {
            Subscription::none()
        };
        let autosave_sub =
            iced::time::every(Duration::from_secs(15)).map(|_| Message::AutosaveSnapshotTick);
        let peak_rebuild_sub = if !self.pending_peak_rebuilds.is_empty() {
            iced::time::every(Duration::from_millis(16)).map(|_| Message::DrainAudioPeakUpdates)
        } else {
            Subscription::none()
        };
        let hw_mixer_sub = {
            let state = self.state.blocking_read();
            if matches!(state.view, crate::state::View::X32) {
                mixosc::app::subscription(&self.hw_mixer).map(Message::HwMixer)
            } else {
                Subscription::none()
            }
        };
        Subscription::batch(vec![
            engine_sub,
            keyboard_sub,
            event_sub,
            shutdown_sub,
            playback_sub,
            meter_poll_sub,
            autosave_sub,
            peak_rebuild_sub,
            recording_preview_sub,
            recording_preview_peaks_sub,
            hw_mixer_sub,
        ])
    }

    fn keyboard_message(event: KeyEvent, current_view: crate::state::View) -> Message {
        match event {
            KeyEvent::KeyPressed { key, modifiers, .. } => {
                if modifiers.control()
                    && let keyboard::Key::Character(ch) = &key
                {
                    let s = ch.to_ascii_lowercase();
                    if s == "n" {
                        return Message::NewSession;
                    }
                    if s == "o" {
                        return Message::Show(Show::Open);
                    }
                    if s == "i" {
                        return Message::OpenFileImporter;
                    }
                    if s == "e" {
                        return Message::OpenExporter;
                    }
                    if s == "s" {
                        if modifiers.shift() {
                            return Message::Show(Show::SaveAs);
                        }
                        return Message::Show(Show::Save);
                    }
                    if s == "t" {
                        return Message::Show(Show::AddTrack);
                    }
                    if s == "a" {
                        return Message::SelectAll;
                    }
                    if s == "r" {
                        return Message::TransportRecordToggle;
                    }
                    if s == "l" {
                        return Message::TransportPanic;
                    }
                    if s == "z" {
                        if modifiers.shift() {
                            return Message::Redo;
                        }
                        return Message::Undo;
                    }
                    if s == "y" {
                        return Message::Redo;
                    }
                }
                match key {
                    keyboard::Key::Character(ch) if !modifiers.control() => {
                        let s = ch.to_ascii_lowercase();
                        if s == "q" {
                            Message::PianoQuantizeSelectedNotes
                        } else if s == "h" {
                            Message::PianoHumanizeSelectedNotes
                        } else if s == "g" {
                            Message::PianoGrooveSelectedNotes
                        } else if s == "n" {
                            Message::ToggleShortcutsPane
                        } else if s == "m" {
                            Message::ToggleModulatorsPane
                        } else if s == "c" {
                            Message::ToggleCutIndicator
                        } else if s == "b" {
                            Message::ToggleSelectedPluginBypass
                        } else {
                            Message::None
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::Tab) => {
                        if matches!(current_view, crate::state::View::Session) {
                            Message::Workspace
                        } else {
                            Message::Session
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::Space) if modifiers.shift() => {
                        if matches!(current_view, crate::state::View::Session) {
                            Message::SessionNavStopAll
                        } else {
                            Message::TransportPause
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::Space) => Message::ToggleTransport,
                    keyboard::Key::Named(keyboard::key::Named::Home) => Message::JumpToStart,
                    keyboard::Key::Named(keyboard::key::Named::End) => Message::JumpToEnd,
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp)
                        if matches!(current_view, crate::state::View::Session) =>
                    {
                        Message::SessionNavMove {
                            delta_x: 0,
                            delta_y: -1,
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown)
                        if matches!(current_view, crate::state::View::Session) =>
                    {
                        Message::SessionNavMove {
                            delta_x: 0,
                            delta_y: 1,
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
                        if matches!(current_view, crate::state::View::Session) =>
                    {
                        Message::SessionNavMove {
                            delta_x: -1,
                            delta_y: 0,
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowRight)
                        if matches!(current_view, crate::state::View::Session) =>
                    {
                        Message::SessionNavMove {
                            delta_x: 1,
                            delta_y: 0,
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                        if matches!(current_view, crate::state::View::Session) =>
                    {
                        Message::SessionNavLaunch
                    }
                    keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                    keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                    keyboard::Key::Named(keyboard::key::Named::Delete)
                    | keyboard::Key::Named(keyboard::key::Named::Backspace) => Message::Remove,
                    keyboard::Key::Named(keyboard::key::Named::Escape) => Message::EscapePressed,
                    _ => Message::None,
                }
            }
            KeyEvent::KeyReleased { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftReleased,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlReleased,
                _ => Message::None,
            },
            _ => Message::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Maolan;
    use crate::message::{Message, Show};
    use iced::keyboard::{self, Event as KeyEvent, Key, Modifiers, key::Named};

    #[test]
    fn polls_meters_while_playing() {
        assert!(Maolan::should_poll_meters(
            true,
            true,
            false,
            false,
            &[-90.0],
            [&[-90.0][..]]
        ));
    }

    #[test]
    fn polls_meters_while_paused() {
        assert!(Maolan::should_poll_meters(
            true,
            false,
            true,
            false,
            &[-90.0],
            [&[-90.0][..]]
        ));
    }

    #[test]
    fn polls_meters_during_live_session_play() {
        assert!(Maolan::should_poll_meters(
            true,
            false,
            false,
            true,
            &[-90.0],
            [&[-90.0][..]]
        ));
    }

    #[test]
    fn polls_meters_after_stop_when_hw_out_is_still_active() {
        assert!(Maolan::should_poll_meters(
            true,
            false,
            false,
            false,
            &[-24.0],
            [&[-90.0][..]]
        ));
    }

    #[test]
    fn polls_meters_after_stop_when_track_is_still_active() {
        assert!(Maolan::should_poll_meters(
            true,
            false,
            false,
            false,
            &[-90.0],
            [&[-18.0][..]]
        ));
    }

    #[test]
    fn stops_polling_meters_when_transport_is_stopped_and_all_meters_are_silent() {
        assert!(!Maolan::should_poll_meters(
            true,
            false,
            false,
            false,
            &[-90.0, -90.0],
            [&[-90.0, -90.0][..], &[-90.0][..]],
        ));
    }

    #[test]
    fn does_not_poll_meters_when_mixer_is_hidden() {
        assert!(!Maolan::should_poll_meters(
            false,
            true,
            false,
            false,
            &[0.0],
            [&[0.0][..]],
        ));
    }

    #[test]
    fn keyboard_shortcuts_map_to_transport_actions() {
        let view = crate::state::View::Workspace;
        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Character("r".into()),
                    modified_key: Key::Character("r".into()),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyR),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::CTRL,
                    text: Some("r".into()),
                    repeat: false,
                },
                view.clone()
            ),
            Message::TransportRecordToggle
        ));

        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Character("l".into()),
                    modified_key: Key::Character("l".into()),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyL),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::CTRL,
                    text: Some("l".into()),
                    repeat: false,
                },
                view.clone()
            ),
            Message::TransportPanic
        ));

        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Named(Named::Home),
                    modified_key: Key::Named(Named::Home),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Home),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                },
                view.clone()
            ),
            Message::JumpToStart
        ));

        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Named(Named::End),
                    modified_key: Key::Named(Named::End),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::End),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                },
                view
            ),
            Message::JumpToEnd
        ));
    }

    #[test]
    fn ctrl_t_in_workspace_opens_add_track_modal() {
        let view = crate::state::View::Workspace;
        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Character("t".into()),
                    modified_key: Key::Character("t".into()),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyT),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::CTRL,
                    text: Some("t".into()),
                    repeat: false,
                },
                view
            ),
            Message::Show(crate::message::Show::AddTrack)
        ));
    }

    #[test]
    fn ctrl_t_in_session_opens_add_track_modal() {
        let view = crate::state::View::Session;
        assert!(matches!(
            Maolan::keyboard_message(
                KeyEvent::KeyPressed {
                    key: Key::Character("t".into()),
                    modified_key: Key::Character("t".into()),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyT),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::CTRL,
                    text: Some("t".into()),
                    repeat: false,
                },
                view
            ),
            Message::Show(Show::AddTrack)
        ));
    }
}
