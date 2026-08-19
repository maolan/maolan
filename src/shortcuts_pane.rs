use crate::keyboard_shortcuts::{ShortcutAction, ShortcutBindings, binding_for};
use crate::message::Message;
use crate::state::View;
use maolan_widgets::iced::{
    Background, Border, Color, Element, Length,
    widget::{column, container, mouse_area, scrollable, text},
};

pub struct ShortcutsPane;

impl ShortcutsPane {
    pub fn view(
        view: View,
        hint: Option<&str>,
        overrides: &ShortcutBindings,
        editing: Option<ShortcutAction>,
    ) -> Element<'static, Message> {
        let content = match view {
            View::Connections | View::TrackPlugins => connections_shortcuts(hint),
            View::Piano => piano_shortcuts(hint, overrides, editing),
            View::PitchCorrection => pitch_correction_shortcuts(hint, overrides, editing),
            View::Session => session_shortcuts(hint, overrides, editing),
            View::X32 => column![].into(),
            _ => workspace_shortcuts(hint, overrides, editing),
        };

        container(
            column![
                text("Shortcuts").size(16),
                scrollable(content).height(Length::Fill),
            ]
            .spacing(10),
        )
        .style(|_theme| container::Style {
            border: Border {
                color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                width: 1.0,
                ..Border::default()
            },
            ..crate::style::app_background()
        })
        .padding(12)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .into()
    }
}

#[derive(Clone, Copy)]
struct ShortcutRow {
    action: ShortcutAction,
    label: &'static str,
}

enum PaneRow {
    Editable(ShortcutRow),
    Static(&'static str),
}

fn keyboard_row(action: ShortcutAction, label: &'static str) -> PaneRow {
    PaneRow::Editable(ShortcutRow { action, label })
}

fn static_row(label: &'static str) -> PaneRow {
    PaneRow::Static(label)
}

fn section(
    title: impl Into<String>,
    items: Vec<PaneRow>,
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    let title = title.into();
    let mut col = column![text(title).size(13)].spacing(4);
    for item in items {
        col = col.push(row_element(item, hint, overrides, editing));
    }
    col.into()
}

fn row_element(
    item: PaneRow,
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    match item {
        PaneRow::Editable(row) => editable_row(row, hint, overrides, editing),
        PaneRow::Static(label) => {
            styled_row(format!("  * {label}"), highlighted(label, hint), false)
        }
    }
}

fn editable_row(
    row_data: ShortcutRow,
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    let binding = binding_for(overrides, row_data.action).label();
    let is_editing = editing == Some(row_data.action);
    let label = if is_editing {
        format!("  * {binding}: Press new shortcut")
    } else {
        format!("  * {binding}: {}", row_data.label)
    };
    let content = styled_row(
        label,
        highlighted(row_data.label, hint) || is_editing,
        is_editing,
    );
    mouse_area(content)
        .on_double_click(Message::ShortcutEditStart(row_data.action))
        .into()
}

fn styled_row(label: String, is_highlighted: bool, is_editing: bool) -> Element<'static, Message> {
    let color = if is_editing {
        Color::from_rgb(0.72, 0.9, 1.0)
    } else if is_highlighted {
        Color::from_rgb(1.0, 0.95, 0.6)
    } else {
        Color::WHITE
    };

    let row = text(label).size(11).color(color);
    if is_highlighted || is_editing {
        container(row)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.35, 0.4, 0.22, 0.45))),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .padding([2, 4])
            .into()
    } else {
        row.into()
    }
}

fn highlighted(label: &str, hint: Option<&str>) -> bool {
    hint.is_some_and(|h| label.contains(h))
}

fn connections_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    section(
        "Mouse",
        vec![
            static_row("Drag plugin node: Move node"),
            static_row("Drag from port to port: Create connection"),
            static_row("Select connection + Delete: Remove connection"),
            static_row("Select plugin + Delete: Remove plugin"),
        ],
        hint,
        &ShortcutBindings::new(),
        None,
    )
}

fn piano_shortcuts(
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            vec![
                keyboard_row(
                    ShortcutAction::QuantizeSelectedNotes,
                    "Quantize selected notes"
                ),
                keyboard_row(
                    ShortcutAction::HumanizeSelectedNotes,
                    "Humanize selected notes"
                ),
                keyboard_row(ShortcutAction::GrooveSelectedNotes, "Groove selected notes"),
                keyboard_row(ShortcutAction::ToggleTransport, "Toggle play/stop"),
                keyboard_row(ShortcutAction::PauseTransport, "Pause"),
                keyboard_row(ShortcutAction::JumpToStart, "Rewind to start"),
                keyboard_row(ShortcutAction::JumpToEnd, "Rewind to end"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Mouse",
            vec![
                static_row("Click/drag notes: Select and move"),
                static_row("Drag note edge: Resize note"),
                static_row("Left drag empty area: Box-select notes"),
                static_row("Right drag empty area: Create notes"),
                static_row("Middle click note: Delete note"),
                static_row("Mouse wheel over note: Adjust velocity"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Controller Lanes",
            vec![
                static_row("Left drag: Adjust point/value"),
                static_row("Middle click/drag: Erase"),
                static_row("Right drag: Draw"),
                static_row("Mouse wheel over event: Adjust value"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "SysEx Lane",
            vec![
                static_row("Left drag: Move SysEx event"),
                static_row("Double click: Open SysEx editor"),
            ],
            hint,
            overrides,
            editing,
        ),
    ]
    .spacing(16)
    .into()
}

fn pitch_correction_shortcuts(
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            vec![
                keyboard_row(ShortcutAction::SelectAll, "Select all segments"),
                keyboard_row(ShortcutAction::Undo, "Undo local edits"),
                keyboard_row(ShortcutAction::Redo, "Redo local edits"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Mouse",
            vec![
                static_row("Left click segment: Select"),
                static_row("Shift+Left click: Add/remove from selection"),
                static_row("Left drag selected: Retarget vertically"),
                static_row("Left drag empty: Box-select"),
                static_row("Shift+Left drag empty: Add to selection"),
                static_row("Double click: Snap to nearest semitone"),
            ],
            hint,
            overrides,
            editing,
        ),
    ]
    .spacing(16)
    .into()
}

fn session_shortcuts(
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            vec![
                keyboard_row(
                    ShortcutAction::ToggleWorkspaceSession,
                    "Toggle Workspace/Session view"
                ),
                keyboard_row(
                    ShortcutAction::SessionNavLaunch,
                    "Launch/stop selected slot"
                ),
                keyboard_row(ShortcutAction::SessionStopAll, "Stop all session clips"),
                keyboard_row(ShortcutAction::SessionNavUp, "Navigate slots up"),
                keyboard_row(ShortcutAction::SessionNavDown, "Navigate slots down"),
                keyboard_row(ShortcutAction::SessionNavLeft, "Navigate slots left"),
                keyboard_row(ShortcutAction::SessionNavRight, "Navigate slots right"),
                keyboard_row(
                    ShortcutAction::ToggleTransport,
                    "Play/stop arrangement transport"
                ),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Mouse",
            vec![
                static_row("Click slot: Launch/stop clip"),
                static_row("Click scene: Launch all clips in scene"),
                static_row("Right click slot: Context menu"),
                static_row("Double click slot: Open referenced clip"),
            ],
            hint,
            overrides,
            editing,
        ),
    ]
    .spacing(16)
    .into()
}

fn workspace_shortcuts(
    hint: Option<&str>,
    overrides: &ShortcutBindings,
    editing: Option<ShortcutAction>,
) -> Element<'static, Message> {
    column![
        section(
            "Session",
            vec![
                keyboard_row(ShortcutAction::NewSession, "New session"),
                keyboard_row(ShortcutAction::OpenSession, "Open session"),
                keyboard_row(ShortcutAction::SaveSession, "Save session"),
                keyboard_row(ShortcutAction::SaveSessionAs, "Save as"),
                keyboard_row(ShortcutAction::ImportFiles, "Import files"),
                keyboard_row(ShortcutAction::Export, "Export"),
                keyboard_row(ShortcutAction::AddTrack, "Add track"),
                keyboard_row(ShortcutAction::SelectAll, "Select all"),
                keyboard_row(ShortcutAction::RecordArmToggle, "Record arm toggle"),
                keyboard_row(ShortcutAction::MidiPanic, "MIDI panic"),
                keyboard_row(ShortcutAction::Undo, "Undo"),
                keyboard_row(ShortcutAction::Redo, "Redo"),
                keyboard_row(ShortcutAction::RemoveSelected, "Remove selected"),
                keyboard_row(ShortcutAction::Escape, "Cancel / clear"),
                keyboard_row(ShortcutAction::ToggleShortcutsPane, "Toggle shortcuts pane"),
                keyboard_row(
                    ShortcutAction::ToggleModulatorsPane,
                    "Toggle modulators pane"
                ),
                keyboard_row(ShortcutAction::ToggleClipsPane, "Toggle clips pane"),
                keyboard_row(ShortcutAction::ToggleCutIndicator, "Toggle cut indicator"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Transport",
            vec![
                keyboard_row(ShortcutAction::ToggleTransport, "Play/stop"),
                keyboard_row(ShortcutAction::PauseTransport, "Pause"),
                keyboard_row(ShortcutAction::JumpToStart, "Rewind to start"),
                keyboard_row(ShortcutAction::JumpToEnd, "Rewind to end"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Tracks",
            vec![
                static_row("Left click: Select track"),
                static_row("Ctrl+Left click: Add to selection"),
                static_row("Double click: Open plugin graph"),
                static_row("Right click: Context menu"),
                static_row("Drag track: Reorder"),
                static_row("Drag bottom edge: Resize height"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Timeline Clips",
            vec![
                static_row("Left click: Select clip"),
                static_row("Left click empty: Deselect"),
                static_row("Left drag: Move clip"),
                static_row("Ctrl+drag: Copy clip"),
                static_row("Drag edge: Resize bounds"),
                static_row("Shift+drag edge: Stretch audio"),
                static_row("Drag fade handles: Resize fade"),
                static_row("Middle click clip: Split at cursor"),
                static_row("Double click MIDI clip: Open piano roll"),
                static_row("Right click clip: Context menu"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Markers",
            vec![
                static_row("Right click empty header: Create marker"),
                static_row("Left drag marker: Move"),
                static_row("Right click marker: Rename"),
                static_row("Middle click marker: Delete"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Selection",
            vec![
                static_row("Left drag empty editor: Marquee select"),
                static_row("Right drag MIDI lane: Create empty MIDI clip"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Automation Lanes",
            vec![
                static_row("Left click empty area: Insert automation point"),
                static_row("Right click point: Delete automation point"),
            ],
            hint,
            overrides,
            editing,
        ),
        section(
            "Ruler",
            vec![
                static_row("Left click: Move playhead"),
                static_row("Left drag: Set loop range"),
                static_row("Middle drag inside loop: Move loop range"),
                static_row("Middle drag loop edge: Adjust loop start/end"),
                static_row("Right click: Clear loop range"),
            ],
            hint,
            overrides,
            editing,
        ),
    ]
    .spacing(16)
    .into()
}
