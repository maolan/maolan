use super::{CLIENT, Maolan};
use crate::{
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

const METER_DIRTY_EPSILON_DB: f32 = 0.2;

impl Maolan {
    pub fn subscription(&self) -> Subscription<Message> {
        fn meter_changed(prev: &[f32], next: &[f32]) -> bool {
            if prev.len() != next.len() {
                return true;
            }
            prev.iter()
                .zip(next.iter())
                .any(|(a, b)| (a - b).abs() >= METER_DIRTY_EPSILON_DB)
        }

        fn listener() -> impl Stream<Item = Message> {
            stream::once(CLIENT.subscribe()).flat_map(|receiver| {
                #[cfg(all(unix, not(target_os = "macos")))]
                let initial = stream::once(async { Message::RefreshLv2Plugins });
                #[cfg(all(unix, not(target_os = "macos")))]
                let initial = initial.chain(stream::once(async { Message::RefreshVst3Plugins }));
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                let initial = stream::once(async { Message::RefreshVst3Plugins });
                let initial = initial.chain(stream::once(async { Message::RefreshClapPlugins }));
                initial.chain(stream::unfold(
                    (receiver, HashMap::<String, Vec<f32>>::new()),
                    |(mut rx, mut last_meters)| async move {
                        loop {
                            match rx.recv().await {
                                Some(EngineMessage::Response(r)) => {
                                    if let Ok(EngineAction::TrackMeters {
                                        track_name,
                                        output_db,
                                    }) = &r
                                    {
                                        let should_forward = match last_meters.get(track_name) {
                                            Some(prev) => meter_changed(prev, output_db),
                                            None => true,
                                        };
                                        if !should_forward {
                                            continue;
                                        }
                                        last_meters.insert(track_name.clone(), output_db.clone());
                                    }
                                    return Some((Message::Response(r), (rx, last_meters)));
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

        let keyboard_sub = keyboard::listen().map(|event| match event {
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
                    if s == "s" {
                        if modifiers.shift() {
                            return Message::Show(Show::SaveAs);
                        }
                        return Message::Show(Show::Save);
                    }
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::Space) if modifiers.shift() => {
                        Message::TransportPause
                    }
                    keyboard::Key::Named(keyboard::key::Named::Space) => Message::ToggleTransport,
                    keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                    keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                    keyboard::Key::Named(keyboard::key::Named::Delete)
                    | keyboard::Key::Named(keyboard::key::Named::Backspace) => Message::Remove,
                    _ => Message::None,
                }
            }
            KeyEvent::KeyReleased { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftReleased,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlReleased,
                _ => Message::None,
            },
            _ => Message::None,
        });

        let event_sub = event::listen().map(|event| match event {
            event::Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::ButtonPressed(button) => Message::MousePressed(button),
                mouse::Event::CursorMoved { .. } => Message::MouseMoved(mouse_event),
                mouse::Event::ButtonReleased(_) => Message::MouseReleased,
                _ => Message::None,
            },
            event::Event::Window(window::Event::Resized(size)) => Message::WindowResized(size),
            _ => Message::None,
        });

        let playback_sub = if self.playing && !self.paused {
            iced::time::every(PLAYHEAD_UPDATE_INTERVAL).map(|_| Message::PlaybackTick)
        } else {
            Subscription::none()
        };
        let recording_preview_sub =
            if self.playing
                && !self.paused
                && self.record_armed
                && self.recording_preview_start_sample.is_some()
            {
                iced::time::every(RECORDING_PREVIEW_UPDATE_INTERVAL)
                    .map(|_| Message::RecordingPreviewTick)
            } else {
                Subscription::none()
            };
        let recording_preview_peaks_sub =
            if self.playing
                && !self.paused
                && self.record_armed
                && self.recording_preview_start_sample.is_some()
            {
                iced::time::every(RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL)
                    .map(|_| Message::RecordingPreviewPeaksTick)
            } else {
                Subscription::none()
            };
        Subscription::batch(vec![
            engine_sub,
            keyboard_sub,
            event_sub,
            playback_sub,
            recording_preview_sub,
            recording_preview_peaks_sub,
        ])
    }
}
