use crate::message::Message;
use crate::state::View;
use iced::{
    Background, Border, Color, Element, Length,
    widget::{column, container, scrollable, text},
};

pub struct ShortcutsPane;

impl ShortcutsPane {
    pub fn view(view: View, hint: Option<&str>) -> Element<'static, Message> {
        let content = match view {
            View::Connections | View::TrackPlugins => connections_shortcuts(hint),
            View::Piano => piano_shortcuts(hint),
            View::PitchCorrection => pitch_correction_shortcuts(hint),
            View::Session => session_shortcuts(hint),
            View::X32 => column![].into(),
            _ => workspace_shortcuts(hint),
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

fn section(
    title: impl Into<String>,
    items: &[impl AsRef<str>],
    hint: Option<&str>,
) -> Element<'static, Message> {
    let title = title.into();
    let mut col = column![text(title).size(13)].spacing(4);
    for item in items {
        let item_str = item.as_ref();
        let is_highlighted = hint.is_some_and(|h| item_str.contains(h));
        let item_element: Element<'static, Message> = if is_highlighted {
            container(
                text(format!("  • {}", item_str))
                    .size(11)
                    .color(Color::from_rgb(1.0, 0.95, 0.6)),
            )
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
            text(format!("  • {}", item_str)).size(11).into()
        };
        col = col.push(item_element);
    }
    col.into()
}

fn connections_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    column![section(
        "Mouse",
        &[
            "Drag plugin node: Move node",
            "Drag from port to port: Create connection",
            "Select connection + Delete: Remove connection",
            "Select plugin + Delete: Remove plugin",
        ],
        hint
    ),]
    .spacing(16)
    .into()
}

fn piano_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            &[
                "Q: Quantize selected notes",
                "H: Humanize selected notes",
                "G: Groove selected notes",
                "Space: Toggle play/stop",
                "Shift+Space: Pause",
                "Home: Rewind to start",
                "End: Rewind to end",
            ],
            hint
        ),
        section(
            "Mouse",
            &[
                "Click/drag notes: Select and move",
                "Drag note edge: Resize note",
                "Left drag empty area: Box-select notes",
                "Right drag empty area: Create notes",
                "Middle click note: Delete note",
                "Mouse wheel over note: Adjust velocity",
            ],
            hint
        ),
        section(
            "Controller Lanes",
            &[
                "Left drag: Adjust point/value",
                "Middle click/drag: Erase",
                "Right drag: Draw",
                "Mouse wheel over event: Adjust value",
            ],
            hint
        ),
        section(
            "SysEx Lane",
            &[
                "Left drag: Move SysEx event",
                "Double click: Open SysEx editor",
            ],
            hint
        ),
    ]
    .spacing(16)
    .into()
}

fn pitch_correction_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            &[
                "Ctrl+A: Select all segments",
                "Ctrl+Z: Undo local edits",
                "Ctrl+Shift+Z / Ctrl+Y: Redo local edits",
            ],
            hint
        ),
        section(
            "Mouse",
            &[
                "Left click segment: Select",
                "Shift+Left click: Add/remove from selection",
                "Left drag selected: Retarget vertically",
                "Left drag empty: Box-select",
                "Shift+Left drag empty: Add to selection",
                "Double click: Snap to nearest semitone",
            ],
            hint
        ),
    ]
    .spacing(16)
    .into()
}

fn session_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    column![
        section(
            "Keyboard",
            &[
                "Tab: Toggle Workspace/Session view",
                "Return: Launch/stop selected slot",
                "Shift+Space: Stop all session clips",
                "Arrow keys: Navigate slots",
                "Space: Play/stop arrangement transport",
            ],
            hint
        ),
        section(
            "Mouse",
            &[
                "Click slot: Launch/stop clip",
                "Click scene: Launch all clips in scene",
                "Right click slot: Context menu",
                "Double click slot: Open referenced clip",
            ],
            hint
        ),
    ]
    .spacing(16)
    .into()
}

fn workspace_shortcuts(hint: Option<&str>) -> Element<'static, Message> {
    column![
        section(
            "Session",
            &[
                "Ctrl+N: New session",
                "Ctrl+O: Open session",
                "Ctrl+S: Save session",
                "Ctrl+Shift+S: Save as",
                "Ctrl+I: Import files",
                "Ctrl+E: Export",
                "Ctrl+T: Add track",
                "Ctrl+A: Select all",
                "Ctrl+R: Record arm toggle",
                "Ctrl+L: MIDI panic",
                "Ctrl+Z: Undo",
                "Ctrl+Shift+Z / Ctrl+Y: Redo",
                "Delete / Backspace: Remove selected",
                "Escape: Cancel / clear",
                "S: Toggle shortcuts pane",
                "M: Toggle modulators pane",
                "C: Toggle clips pane",
                "X: Toggle cut indicator",
            ],
            hint
        ),
        section(
            "Transport",
            &[
                "Space: Play/stop",
                "Shift+Space: Pause",
                "Home: Rewind to start",
                "End: Rewind to end",
            ],
            hint
        ),
        section(
            "Tracks",
            &[
                "Left click: Select track",
                "Ctrl+Left click: Add to selection",
                "Double click: Open plugin graph",
                "Right click: Context menu",
                "Drag track: Reorder",
                "Drag bottom edge: Resize height",
            ],
            hint
        ),
        section(
            "Timeline Clips",
            &[
                "Left click: Select clip",
                "Left click empty: Deselect",
                "Left drag: Move clip",
                "Ctrl+drag: Copy clip",
                "Drag edge: Resize bounds",
                "Shift+drag edge: Stretch audio",
                "Drag fade handles: Resize fade",
                "Middle click clip: Split at cursor",
                "Double click MIDI clip: Open piano roll",
                "Right click clip: Context menu",
            ],
            hint
        ),
        section(
            "Markers",
            &[
                "Right click empty header: Create marker",
                "Left drag marker: Move",
                "Right click marker: Rename",
                "Middle click marker: Delete",
            ],
            hint
        ),
        section(
            "Selection",
            &[
                "Left drag empty editor: Marquee select",
                "Right drag MIDI lane: Create empty MIDI clip",
            ],
            hint
        ),
        section(
            "Automation Lanes",
            &[
                "Left click empty area: Insert automation point",
                "Right click point: Delete automation point",
            ],
            hint
        ),
        section(
            "Ruler",
            &[
                "Left click: Move playhead",
                "Left drag: Set loop range",
                "Middle drag inside loop: Move loop range",
                "Middle drag loop edge: Adjust loop start/end",
                "Right click: Clear loop range",
            ],
            hint
        ),
    ]
    .spacing(16)
    .into()
}
