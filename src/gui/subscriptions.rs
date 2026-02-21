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
use maolan_engine::message::Message as EngineMessage;

impl Maolan {
    pub fn subscription(&self) -> Subscription<Message> {
        fn listener() -> impl Stream<Item = Message> {
            stream::once(CLIENT.subscribe()).flat_map(|receiver| {
                stream::once(async { Message::RefreshLv2Plugins }).chain(stream::unfold(
                    receiver,
                    |mut rx| async move {
                        match rx.recv().await {
                            Some(m) => match m {
                                EngineMessage::Response(r) => {
                                    let result = Message::Response(r);
                                    Some((result, rx))
                                }
                                _ => Some((Message::None, rx)),
                            },
                            None => None,
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
                    if s == "s" {
                        return Message::Show(Show::Save);
                    }
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::Space) => Message::ToggleTransport,
                    keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                    keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                    keyboard::Key::Named(keyboard::key::Named::Delete) => Message::Remove,
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
                mouse::Event::CursorMoved { .. } => Message::MouseMoved(mouse_event),
                mouse::Event::ButtonReleased(_) => Message::MouseReleased,
                _ => Message::None,
            },
            event::Event::Window(window::Event::Resized(size)) => Message::WindowResized(size),
            _ => Message::None,
        });

        let playback_sub = if self.playing {
            iced::time::every(PLAYHEAD_UPDATE_INTERVAL).map(|_| Message::PlaybackTick)
        } else {
            Subscription::none()
        };
        let recording_preview_sub =
            if self.playing && self.record_armed && self.recording_preview_start_sample.is_some() {
                iced::time::every(RECORDING_PREVIEW_UPDATE_INTERVAL)
                    .map(|_| Message::RecordingPreviewTick)
            } else {
                Subscription::none()
            };
        let recording_preview_peaks_sub =
            if self.playing && self.record_armed && self.recording_preview_start_sample.is_some() {
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
