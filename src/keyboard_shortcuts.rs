use crate::message::{Message, Show};
use maolan_widgets::iced::keyboard::{self, Modifiers};
use std::collections::HashMap;
use std::fmt;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    NewSession,
    OpenSession,
    SaveSession,
    SaveSessionAs,
    ImportFiles,
    Export,
    AddTrack,
    SelectAll,
    RecordArmToggle,
    MidiPanic,
    Undo,
    Redo,
    RedoAlternate,
    RemoveSelected,
    RemoveSelectedBackspace,
    Escape,
    ToggleShortcutsPane,
    ToggleModulatorsPane,
    ToggleClipsPane,
    ToggleCutIndicator,
    ToggleSelectedPluginBypass,
    ToggleWorkspaceSession,
    QuantizeSelectedNotes,
    HumanizeSelectedNotes,
    GrooveSelectedNotes,
    ToggleTransport,
    PauseTransport,
    JumpToStart,
    JumpToEnd,
    SessionNavUp,
    SessionNavDown,
    SessionNavLeft,
    SessionNavRight,
    SessionNavLaunch,
    SessionStopAll,
}

impl ShortcutAction {
    pub const ALL: [Self; 35] = [
        Self::NewSession,
        Self::OpenSession,
        Self::SaveSession,
        Self::SaveSessionAs,
        Self::ImportFiles,
        Self::Export,
        Self::AddTrack,
        Self::SelectAll,
        Self::RecordArmToggle,
        Self::MidiPanic,
        Self::Undo,
        Self::Redo,
        Self::RedoAlternate,
        Self::RemoveSelected,
        Self::RemoveSelectedBackspace,
        Self::Escape,
        Self::ToggleShortcutsPane,
        Self::ToggleModulatorsPane,
        Self::ToggleClipsPane,
        Self::ToggleCutIndicator,
        Self::ToggleSelectedPluginBypass,
        Self::ToggleWorkspaceSession,
        Self::QuantizeSelectedNotes,
        Self::HumanizeSelectedNotes,
        Self::GrooveSelectedNotes,
        Self::ToggleTransport,
        Self::PauseTransport,
        Self::JumpToStart,
        Self::JumpToEnd,
        Self::SessionNavUp,
        Self::SessionNavDown,
        Self::SessionNavLeft,
        Self::SessionNavRight,
        Self::SessionNavLaunch,
        Self::SessionStopAll,
    ];

    pub const fn message(self, current_view: crate::state::View) -> Message {
        match self {
            Self::NewSession => Message::NewSession,
            Self::OpenSession => Message::Show(Show::Open),
            Self::SaveSession => Message::Show(Show::Save),
            Self::SaveSessionAs => Message::Show(Show::SaveAs),
            Self::ImportFiles => Message::OpenFileImporter,
            Self::Export => Message::OpenExporter,
            Self::AddTrack => Message::Show(Show::AddTrack),
            Self::SelectAll => Message::SelectAll,
            Self::RecordArmToggle => Message::TransportRecordToggle,
            Self::MidiPanic => Message::TransportPanic,
            Self::Undo => Message::Undo,
            Self::Redo | Self::RedoAlternate => Message::Redo,
            Self::RemoveSelected | Self::RemoveSelectedBackspace => Message::Remove,
            Self::Escape => Message::EscapePressed,
            Self::ToggleShortcutsPane => Message::ToggleShortcutsPane,
            Self::ToggleModulatorsPane => Message::ToggleModulatorsPane,
            Self::ToggleClipsPane => Message::ToggleClipsPane,
            Self::ToggleCutIndicator => Message::ToggleCutIndicator,
            Self::ToggleSelectedPluginBypass => Message::ToggleSelectedPluginBypass,
            Self::ToggleWorkspaceSession => {
                if matches!(current_view, crate::state::View::Session) {
                    Message::Workspace
                } else {
                    Message::Session
                }
            }
            Self::QuantizeSelectedNotes => Message::PianoQuantizeSelectedNotes,
            Self::HumanizeSelectedNotes => Message::PianoHumanizeSelectedNotes,
            Self::GrooveSelectedNotes => Message::PianoGrooveSelectedNotes,
            Self::ToggleTransport => Message::ToggleTransport,
            Self::PauseTransport => Message::TransportPause,
            Self::JumpToStart => Message::JumpToStart,
            Self::JumpToEnd => Message::JumpToEnd,
            Self::SessionNavUp => Message::SessionNavMove {
                delta_x: 0,
                delta_y: -1,
            },
            Self::SessionNavDown => Message::SessionNavMove {
                delta_x: 0,
                delta_y: 1,
            },
            Self::SessionNavLeft => Message::SessionNavMove {
                delta_x: -1,
                delta_y: 0,
            },
            Self::SessionNavRight => Message::SessionNavMove {
                delta_x: 1,
                delta_y: 0,
            },
            Self::SessionNavLaunch => Message::SessionNavLaunch,
            Self::SessionStopAll => Message::SessionNavStopAll,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ShortcutBinding {
    pub key: ShortcutKey,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
}

impl ShortcutBinding {
    pub const fn new(key: ShortcutKey, ctrl: bool, shift: bool) -> Self {
        Self { key, ctrl, shift }
    }

    pub fn from_iced_key(key: &keyboard::Key, modifiers: Modifiers) -> Option<Self> {
        let key = ShortcutKey::from_iced_key(key)?;
        Some(Self {
            key,
            ctrl: modifiers.control(),
            shift: modifiers.shift(),
        })
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(self.key.to_string());
        parts.join("+")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutKey {
    Character(String),
    Space,
    Tab,
    Return,
    Home,
    End,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Delete,
    Backspace,
    Escape,
}

impl ShortcutKey {
    fn from_iced_key(key: &keyboard::Key) -> Option<Self> {
        match key {
            keyboard::Key::Character(ch) => {
                let normalized = ch.trim().to_ascii_lowercase();
                (!normalized.is_empty()).then_some(Self::Character(normalized))
            }
            keyboard::Key::Named(keyboard::key::Named::Space) => Some(Self::Space),
            keyboard::Key::Named(keyboard::key::Named::Tab) => Some(Self::Tab),
            keyboard::Key::Named(keyboard::key::Named::Enter) => Some(Self::Return),
            keyboard::Key::Named(keyboard::key::Named::Home) => Some(Self::Home),
            keyboard::Key::Named(keyboard::key::Named::End) => Some(Self::End),
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(Self::ArrowUp),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Some(Self::ArrowDown),
            keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => Some(Self::ArrowLeft),
            keyboard::Key::Named(keyboard::key::Named::ArrowRight) => Some(Self::ArrowRight),
            keyboard::Key::Named(keyboard::key::Named::Delete) => Some(Self::Delete),
            keyboard::Key::Named(keyboard::key::Named::Backspace) => Some(Self::Backspace),
            keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Self::Escape),
            _ => None,
        }
    }
}

impl fmt::Display for ShortcutKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(ch) => write!(f, "{}", ch.to_ascii_uppercase()),
            Self::Space => write!(f, "Space"),
            Self::Tab => write!(f, "Tab"),
            Self::Return => write!(f, "Return"),
            Self::Home => write!(f, "Home"),
            Self::End => write!(f, "End"),
            Self::ArrowUp => write!(f, "ArrowUp"),
            Self::ArrowDown => write!(f, "ArrowDown"),
            Self::ArrowLeft => write!(f, "ArrowLeft"),
            Self::ArrowRight => write!(f, "ArrowRight"),
            Self::Delete => write!(f, "Delete"),
            Self::Backspace => write!(f, "Backspace"),
            Self::Escape => write!(f, "Escape"),
        }
    }
}

pub type ShortcutBindings = HashMap<ShortcutAction, ShortcutBinding>;

pub fn default_binding(action: ShortcutAction) -> ShortcutBinding {
    use ShortcutAction as Action;
    use ShortcutKey as Key;

    match action {
        Action::NewSession => ShortcutBinding::new(Key::Character("n".into()), true, false),
        Action::OpenSession => ShortcutBinding::new(Key::Character("o".into()), true, false),
        Action::SaveSession => ShortcutBinding::new(Key::Character("s".into()), true, false),
        Action::SaveSessionAs => ShortcutBinding::new(Key::Character("s".into()), true, true),
        Action::ImportFiles => ShortcutBinding::new(Key::Character("i".into()), true, false),
        Action::Export => ShortcutBinding::new(Key::Character("e".into()), true, false),
        Action::AddTrack => ShortcutBinding::new(Key::Character("t".into()), true, false),
        Action::SelectAll => ShortcutBinding::new(Key::Character("a".into()), true, false),
        Action::RecordArmToggle => ShortcutBinding::new(Key::Character("r".into()), true, false),
        Action::MidiPanic => ShortcutBinding::new(Key::Character("l".into()), true, false),
        Action::Undo => ShortcutBinding::new(Key::Character("z".into()), true, false),
        Action::Redo => ShortcutBinding::new(Key::Character("z".into()), true, true),
        Action::RedoAlternate => ShortcutBinding::new(Key::Character("y".into()), true, false),
        Action::RemoveSelected => ShortcutBinding::new(Key::Delete, false, false),
        Action::RemoveSelectedBackspace => ShortcutBinding::new(Key::Backspace, false, false),
        Action::Escape => ShortcutBinding::new(Key::Escape, false, false),
        Action::ToggleShortcutsPane => {
            ShortcutBinding::new(Key::Character("s".into()), false, false)
        }
        Action::ToggleModulatorsPane => {
            ShortcutBinding::new(Key::Character("m".into()), false, false)
        }
        Action::ToggleClipsPane => ShortcutBinding::new(Key::Character("c".into()), false, false),
        Action::ToggleCutIndicator => {
            ShortcutBinding::new(Key::Character("x".into()), false, false)
        }
        Action::ToggleSelectedPluginBypass => {
            ShortcutBinding::new(Key::Character("b".into()), false, false)
        }
        Action::ToggleWorkspaceSession => ShortcutBinding::new(Key::Tab, false, false),
        Action::QuantizeSelectedNotes => {
            ShortcutBinding::new(Key::Character("q".into()), false, false)
        }
        Action::HumanizeSelectedNotes => {
            ShortcutBinding::new(Key::Character("h".into()), false, false)
        }
        Action::GrooveSelectedNotes => {
            ShortcutBinding::new(Key::Character("g".into()), false, false)
        }
        Action::ToggleTransport => ShortcutBinding::new(Key::Space, false, false),
        Action::PauseTransport => ShortcutBinding::new(Key::Space, false, true),
        Action::JumpToStart => ShortcutBinding::new(Key::Home, false, false),
        Action::JumpToEnd => ShortcutBinding::new(Key::End, false, false),
        Action::SessionNavUp => ShortcutBinding::new(Key::ArrowUp, false, false),
        Action::SessionNavDown => ShortcutBinding::new(Key::ArrowDown, false, false),
        Action::SessionNavLeft => ShortcutBinding::new(Key::ArrowLeft, false, false),
        Action::SessionNavRight => ShortcutBinding::new(Key::ArrowRight, false, false),
        Action::SessionNavLaunch => ShortcutBinding::new(Key::Return, false, false),
        Action::SessionStopAll => ShortcutBinding::new(Key::Space, false, true),
    }
}

pub fn binding_for(overrides: &ShortcutBindings, action: ShortcutAction) -> ShortcutBinding {
    overrides
        .get(&action)
        .cloned()
        .unwrap_or_else(|| default_binding(action))
}

pub fn action_for_binding(
    binding: &ShortcutBinding,
    overrides: &ShortcutBindings,
    current_view: crate::state::View,
) -> Option<ShortcutAction> {
    shortcut_priority(current_view)
        .into_iter()
        .find(|action| &binding_for(overrides, *action) == binding)
}

fn shortcut_priority(current_view: crate::state::View) -> Vec<ShortcutAction> {
    use ShortcutAction as Action;

    let mut actions = Vec::new();
    if matches!(current_view, crate::state::View::Session) {
        actions.extend([
            Action::SessionStopAll,
            Action::SessionNavUp,
            Action::SessionNavDown,
            Action::SessionNavLeft,
            Action::SessionNavRight,
            Action::SessionNavLaunch,
        ]);
    } else {
        actions.push(Action::PauseTransport);
    }

    actions.extend([
        Action::NewSession,
        Action::OpenSession,
        Action::SaveSessionAs,
        Action::SaveSession,
        Action::ImportFiles,
        Action::Export,
        Action::AddTrack,
        Action::SelectAll,
        Action::RecordArmToggle,
        Action::MidiPanic,
        Action::Redo,
        Action::RedoAlternate,
        Action::Undo,
        Action::RemoveSelected,
        Action::RemoveSelectedBackspace,
        Action::Escape,
        Action::ToggleShortcutsPane,
        Action::ToggleModulatorsPane,
        Action::ToggleClipsPane,
        Action::ToggleCutIndicator,
        Action::ToggleSelectedPluginBypass,
        Action::ToggleWorkspaceSession,
        Action::QuantizeSelectedNotes,
        Action::HumanizeSelectedNotes,
        Action::GrooveSelectedNotes,
        Action::ToggleTransport,
        Action::JumpToStart,
        Action::JumpToEnd,
    ]);
    actions
}
