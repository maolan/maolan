## ADDED Requirements
### Requirement: GUI-side LV2 UI Hosting
The system SHALL host LV2 plugin editor windows from the GUI application layer (`src`) instead of the audio engine layer.

#### Scenario: User opens LV2 plugin UI from plugin graph
- **WHEN** the user requests to open an LV2 plugin editor
- **THEN** the GUI-side LV2 host creates and runs the LV2 UI window lifecycle
- **AND** the engine is not responsible for creating or running LV2 editor event loops

### Requirement: Engine/UI Responsibility Boundary
The system SHALL keep plugin UI window management out of engine action handling.

#### Scenario: Engine receives UI-related action
- **WHEN** an action would only affect LV2 editor window lifecycle
- **THEN** the action is handled in GUI-side code paths
- **AND** engine actions remain limited to processing, routing, and state/parameter updates

### Requirement: LV2 UI Close Behavior
The system SHALL allow LV2 editor windows to close without leaving stuck worker loops.

#### Scenario: User closes an LV2 plugin editor window
- **WHEN** the host receives UI close/hide notifications
- **THEN** the GUI-side LV2 host exits its UI loop and releases associated resources
- **AND** the main application remains responsive
