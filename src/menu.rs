use crate::message::{Message, Show};
use iced::Length;
use iced_aw::{
    menu::{DrawPath, Item, Menu as IcedMenu},
    menu_bar, menu_items,
};
use maolan_engine::message::GlobalMidiLearnTarget;
use std::path::{Path, PathBuf};

pub use maolan_widgets::menu::{
    menu_checkbox_item, menu_dropdown, menu_item, menu_item_maybe, submenu,
};

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
        toolbar_visible: bool,
        log_visible: bool,
        shortcuts_pane_visible: bool,
    ) -> iced::Element<'_, Message> {
        let menu_tpl = |items| IcedMenu::new(items).width(180.0).offset(15.0).spacing(5.0);

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
                    (menu_item(
                        "Delete unused files",
                        Message::DeleteUnusedSessionMediaFiles
                    )),
                    (menu_item("Export", Message::OpenExporter)),
                    (menu_item("Quit", Message::WindowCloseRequested)),
                ))
            }),
            (menu_dropdown("Edit", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("Undo", Message::Undo)),
                    (menu_item("Redo", Message::Redo)),
                    (menu_item("Preferences", Message::Show(Show::Preferences))),
                    (menu_item("Session Diagnostics", Message::SessionDiagnosticsRequest)),
                    (menu_item(
                        "Export Diagnostics Bundle",
                        Message::ExportDiagnosticsBundleRequest
                    )),
                    (menu_item(
                        "Toggle MIDI Mappings Panel",
                        Message::MidiLearnMappingsPanelToggle
                    )),
                    (menu_item(
                        "MIDI Mappings Report",
                        Message::MidiLearnMappingsReportRequest
                    )),
                    (menu_item(
                        "Export MIDI Mappings",
                        Message::MidiLearnMappingsExportRequest
                    )),
                    (menu_item(
                        "Import MIDI Mappings",
                        Message::MidiLearnMappingsImportRequest
                    )),
                    (menu_item(
                        "Clear All MIDI Mappings",
                        Message::MidiLearnMappingsClearAllRequest
                    )),
                    (
                        submenu("MIDI Learn", Message::None),
                        menu_tpl(menu_items!(
                            (menu_item(
                                "MIDI Learn: Play/Pause",
                                Message::GlobalMidiLearnArm {
                                    target: GlobalMidiLearnTarget::PlayPause
                                }
                            )),
                            (menu_item(
                                "MIDI Learn: Stop",
                                Message::GlobalMidiLearnArm {
                                    target: GlobalMidiLearnTarget::Stop
                                }
                            )),
                            (menu_item(
                                "MIDI Learn: Record Toggle",
                                Message::GlobalMidiLearnArm {
                                    target: GlobalMidiLearnTarget::RecordToggle
                                }
                            )),
                            (menu_item(
                                "Clear MIDI Learn: Play/Pause",
                                Message::GlobalMidiLearnClear {
                                    target: GlobalMidiLearnTarget::PlayPause
                                }
                            )),
                            (menu_item(
                                "Clear MIDI Learn: Stop",
                                Message::GlobalMidiLearnClear {
                                    target: GlobalMidiLearnTarget::Stop
                                }
                            )),
                            (menu_item(
                                "Clear MIDI Learn: Record Toggle",
                                Message::GlobalMidiLearnClear {
                                    target: GlobalMidiLearnTarget::RecordToggle
                                }
                            )),
                        ))
                    ),
                ))
            }),
            (menu_dropdown("Track", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("New", Message::Show(Show::AddTrack))),
                ))
            }),
            (menu_dropdown("View", Message::None), {
                menu_tpl(menu_items!(
                    (menu_checkbox_item("Tracks", tracks_visible, Message::ToggleTracksVisibility)),
                    (menu_checkbox_item("Editor", editor_visible, Message::ToggleEditorVisibility)),
                    (menu_checkbox_item("Mixer", mixer_visible, Message::ToggleMixerVisibility)),
                    (menu_checkbox_item(
                        "Toolbar",
                        toolbar_visible,
                        Message::ToggleToolbarVisibility
                    )),
                    (menu_checkbox_item("Log", log_visible, Message::ToggleLogVisibility)),
                    (menu_checkbox_item(
                        "Shortcuts",
                        shortcuts_pane_visible,
                        Message::ToggleShortcutsPane
                    )),
                    (menu_item("About", Message::Show(Show::About))),
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

    #[test]
    fn update_templates_replaces_list() {
        let mut menu = Menu::default();
        menu.update_templates(vec!["Template1".to_string(), "Template2".to_string()]);
        assert_eq!(menu.available_templates.len(), 2);
        assert_eq!(menu.available_templates[0], "Template1");
    }

    #[test]
    fn update_recent_sessions_replaces_list() {
        let mut menu = Menu::default();
        menu.update_recent_sessions(vec!["/path/one".to_string(), "/path/two".to_string()]);
        assert_eq!(menu.recent_session_paths.len(), 2);
    }

    #[test]
    fn recent_session_label_formats_with_filename() {
        let label = Menu::recent_session_label("/home/user/projects/my_session");
        assert!(label.contains("my_session"));
        assert!(label.contains("/home/user/projects/my_session"));
    }

    #[test]
    fn recent_session_label_handles_plain_name() {
        let label = Menu::recent_session_label("just_a_name");
        assert!(label.contains("just_a_name"));
    }

    #[test]
    fn menu_default_is_empty() {
        let menu = Menu::default();
        assert!(menu.available_templates.is_empty());
        assert!(menu.recent_session_paths.is_empty());
    }

    #[test]
    fn menu_can_be_created() {
        let menu = Menu::default();
        assert!(menu.available_templates.is_empty());
        assert!(menu.recent_session_paths.is_empty());
    }
}
