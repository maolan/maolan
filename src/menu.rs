use crate::message::{Message, Show};
use iced::{
    Border, Color, Element, Length, alignment,
    widget::{button, row, text},
};
use iced_aw::{
    menu::{DrawPath, Item, Menu as IcedMenu},
    menu_bar, menu_items,
};
use iced_fonts::lucide::chevron_right;
use maolan_engine::message::GlobalMidiLearnTarget;
use std::path::{Path, PathBuf};

pub(crate) fn base_button<'a>(
    content: impl Into<Element<'a, Message>>,
    msg: Message,
) -> button::Button<'a, Message> {
    button(content)
        .padding([4, 8])
        .style(|theme, status| {
            use button::{Status, Style};

            let palette = theme.extended_palette();
            let base = Style {
                text_color: palette.background.base.text,
                border: Border::default().rounded(6.0),
                ..Style::default()
            };
            match status {
                Status::Active => base.with_background(Color::TRANSPARENT),
                Status::Hovered => base.with_background(Color::from_rgb(
                    palette.primary.weak.color.r * 1.2,
                    palette.primary.weak.color.g * 1.2,
                    palette.primary.weak.color.b * 1.2,
                )),
                Status::Disabled => base.with_background(Color::from_rgb(0.5, 0.5, 0.5)),
                Status::Pressed => base.with_background(palette.primary.weak.color),
            }
        })
        .on_press(msg)
}

pub(crate) fn menu_button(
    label: impl Into<String>,
    width: Option<Length>,
    height: Option<Length>,
    msg: Message,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let label = label.into();
    base_button(
        text(label)
            .height(height.unwrap_or(Length::Shrink))
            .align_y(alignment::Vertical::Center),
        msg,
    )
    .width(width.unwrap_or(Length::Shrink))
    .height(height.unwrap_or(Length::Shrink))
    .into()
}

pub(crate) fn menu_dropdown(
    label: impl Into<String>,
    message: Message,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Shrink), Some(Length::Shrink), message)
}

pub(crate) fn menu_item(
    label: impl Into<String>,
    message: Message,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Fill), Some(Length::Shrink), message)
}

pub(crate) fn menu_item_maybe(
    label: impl Into<String>,
    message: Option<Message>,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let label = label.into();
    let button = button(
        text(label)
            .height(Length::Shrink)
            .align_y(alignment::Vertical::Center),
    )
    .padding([4, 8])
    .style(|theme: &iced::Theme, status| {
        use button::{Status, Style};

        let palette = theme.extended_palette();
        let base = Style {
            text_color: palette.background.base.text,
            border: Border::default().rounded(6.0),
            ..Style::default()
        };
        match status {
            Status::Active => base.with_background(Color::TRANSPARENT),
            Status::Hovered => base.with_background(Color::from_rgb(
                palette.primary.weak.color.r * 1.2,
                palette.primary.weak.color.g * 1.2,
                palette.primary.weak.color.b * 1.2,
            )),
            Status::Disabled => base.with_background(Color::from_rgb(0.5, 0.5, 0.5)),
            Status::Pressed => base.with_background(palette.primary.weak.color),
        }
    })
    .width(Length::Fill)
    .height(Length::Shrink);
    if let Some(message) = message {
        button.on_press(message).into()
    } else {
        button.into()
    }
}

pub(crate) fn submenu(
    label: impl Into<String>,
    msg: Message,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let label = label.into();
    base_button(
        row![
            text(label)
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
            chevron_right(),
        ]
        .align_y(iced::Alignment::Center),
        msg,
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .into()
}

#[derive(Default)]
pub struct Menu {
    available_templates: Vec<String>,
    recent_session_paths: Vec<String>,
}

impl Menu {
    pub fn update(&mut self, _message: &Message) {}

    pub fn update_templates(&mut self, templates: Vec<String>) {
        self.available_templates = templates;
    }

    pub fn update_recent_sessions(&mut self, recent_session_paths: Vec<String>) {
        self.recent_session_paths = recent_session_paths;
    }

    fn recent_session_label(path: &str) -> String {
        let base = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        format!("{base}  ({path})")
    }

    pub fn view(
        &self,
        tracks_visible: bool,
        editor_visible: bool,
        mixer_visible: bool,
    ) -> iced::Element<'_, Message> {
        let menu_tpl = |items| IcedMenu::new(items).width(180.0).offset(15.0).spacing(5.0);

        // Build the "New" submenu dynamically from stored templates
        let mut new_menu_items: Vec<Item<'_, Message, _, _>> =
            vec![Item::new(menu_item("Empty", Message::NewSession))];
        for template in &self.available_templates {
            new_menu_items.push(Item::new(menu_item(
                template.as_str(),
                Message::NewFromTemplate(template.clone()),
            )));
        }
        let new_submenu = IcedMenu::new(new_menu_items)
            .width(180.0)
            .offset(15.0)
            .spacing(5.0);
        let mut recent_menu_items: Vec<Item<'_, Message, _, _>> = Vec::new();
        if self.recent_session_paths.is_empty() {
            recent_menu_items.push(Item::new(menu_item("No Recent Sessions", Message::None)));
        } else {
            for path in &self.recent_session_paths {
                recent_menu_items.push(Item::new(menu_item(
                    Self::recent_session_label(path),
                    Message::OpenFolderSelected(Some(PathBuf::from(path))),
                )));
            }
        }
        let recent_submenu = IcedMenu::new(recent_menu_items)
            .width(420.0)
            .offset(15.0)
            .spacing(5.0);

        #[rustfmt::skip]
        let mb = menu_bar!(
            (menu_dropdown("File", Message::None), {
                menu_tpl(menu_items!(
                    (submenu("New", Message::None), new_submenu),
                    (menu_item("Open", Message::Show(Show::Open))),
                    (submenu("Recent", Message::None), recent_submenu),
                    (menu_item("Save", Message::Show(Show::Save))),
                    (menu_item("Save as", Message::Show(Show::SaveAs))),
                    (menu_item("Session metadata", Message::Show(Show::SessionMetadata))),
                    (menu_item("Save as template", Message::Show(Show::SaveTemplateAs))),
                    (menu_item("Import", Message::OpenFileImporter)),
                    (menu_item("Generate audio", Message::Show(Show::GenerateAudio))),
                    (menu_item("Delete unused files", Message::DeleteUnusedSessionMediaFiles)),
                    (menu_item("Export", Message::OpenExporter)),
                    (menu_item("Quit", Message::WindowCloseRequested)),
                ))
            }),
            (menu_dropdown("Edit", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("Undo (Ctrl+Z)", Message::Undo)),
                    (menu_item("Redo (Ctrl+Shift+Z)", Message::Redo)),
                    (menu_item("Preferences", Message::Show(Show::Preferences))),
                    (menu_item("Session Diagnostics", Message::SessionDiagnosticsRequest)),
                    (menu_item("Export Diagnostics Bundle", Message::ExportDiagnosticsBundleRequest)),
                    (menu_item("Toggle MIDI Mappings Panel", Message::MidiLearnMappingsPanelToggle)),
                    (menu_item("MIDI Mappings Report", Message::MidiLearnMappingsReportRequest)),
                    (menu_item("Export MIDI Mappings", Message::MidiLearnMappingsExportRequest)),
                    (menu_item("Import MIDI Mappings", Message::MidiLearnMappingsImportRequest)),
                    (menu_item("Clear All MIDI Mappings", Message::MidiLearnMappingsClearAllRequest)),
                    (submenu("MIDI Learn", Message::None), menu_tpl(menu_items!(
                        (menu_item("MIDI Learn: Play/Pause", Message::GlobalMidiLearnArm { target: GlobalMidiLearnTarget::PlayPause })),
                        (menu_item("MIDI Learn: Stop", Message::GlobalMidiLearnArm { target: GlobalMidiLearnTarget::Stop })),
                        (menu_item("MIDI Learn: Record Toggle", Message::GlobalMidiLearnArm { target: GlobalMidiLearnTarget::RecordToggle })),
                        (menu_item("Clear MIDI Learn: Play/Pause", Message::GlobalMidiLearnClear { target: GlobalMidiLearnTarget::PlayPause })),
                        (menu_item("Clear MIDI Learn: Stop", Message::GlobalMidiLearnClear { target: GlobalMidiLearnTarget::Stop })),
                        (menu_item("Clear MIDI Learn: Record Toggle", Message::GlobalMidiLearnClear { target: GlobalMidiLearnTarget::RecordToggle })),
                    ))),
                ))
            }),
            (menu_dropdown("Track", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("New", Message::Show(Show::AddTrack))),
                ))
            }),
            (menu_dropdown("View", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item(
                        if tracks_visible { "Tracks [x]" } else { "Tracks [ ]" },
                        Message::ToggleTracksVisibility
                    )),
                    (menu_item(
                        if editor_visible { "Editor [x]" } else { "Editor [ ]" },
                        Message::ToggleEditorVisibility
                    )),
                    (menu_item(
                        if mixer_visible { "Mixer [x]" } else { "Mixer [ ]" },
                        Message::ToggleMixerVisibility
                    )),
                ))
            }),
        )
        .draw_path(DrawPath::Backdrop)
        .close_on_item_click_global(true)
        .width(Length::Fill);
        mb.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_is_a_no_op() {
        let mut menu = Menu::default();
        menu.update_templates(vec!["A".to_string()]);
        menu.update_recent_sessions(vec!["/tmp/session".to_string()]);

        menu.update(&Message::Cancel);

        assert_eq!(menu.available_templates, vec!["A".to_string()]);
        assert_eq!(menu.recent_session_paths, vec!["/tmp/session".to_string()]);
    }
}
