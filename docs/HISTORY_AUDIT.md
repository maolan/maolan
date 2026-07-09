# History Coverage Audit

Date: 2026-03-05

Note:

This repository currently includes the audit notes below, but not the helper script that originally generated them.
To refresh the audit, compare `engine::message::Action` against `history::should_record` manually or add a new local audit script.

## Scope

This audit checks `engine::message::Action` variants against `history::should_record`.
It highlights potential mutating actions that are not currently history-recorded.

## Current approach

- Keep interactive editing/mixing actions undoable.
- Keep transport/runtime query/report actions non-history.
- Keep high-frequency automation playback actions non-history to avoid history spam.

## Decisions

- `SetLoopEnabled/SetLoopRange/SetPunchEnabled/SetPunchRange/SetRecordEnabled` are history-recorded
  (the previous audit left them as follow-up candidates, but they are now recorded).
- `TrackAutomationLevel/Balance/Mute` remain non-history because these are often runtime playback
  and write-mode side effects.
- Plugin discovery/query actions (`List*`, `*Parameters`, `*Graph`, `*StateSnapshot`) remain non-history.
- MIDI learn arm actions remain non-history; actual binding changes are history-recorded.
- Tempo / time signature edits are history-recorded via `SetTempo`, `SetTimeSignature`, and `SetTempoMap`.
- Modulator changes are history-recorded via `SetModulators`.
- Track hierarchy changes are history-recorded via `TrackSetFolder`, `TrackSetParent`, `TrackToggleFolder`,
  and `TrackToggleMaster`.
- Clip plugin graph changes are history-recorded via `SetClipPluginGraphJson`.
- Automation lane point edits are history-recorded via `SetTrackAutomationLanes` (lane-level granularity).
- Pitch correction edits are history-recorded via `SetClipPitchCorrection`.

## Follow-up candidates

If stricter undo semantics are needed, revisit:

- Granular per-point undo for automation lanes (currently the whole lane snapshot is recorded).
- Explicit manual-only plugin state restore actions
