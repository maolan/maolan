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

- `SetLoopEnabled/SetLoopRange/SetPunchEnabled/SetPunchRange/SetRecordEnabled` remain non-history
  in this pass because they are frequently toggled by transport UX and could create noisy history.
- `TrackAutomationLevel/Balance/Mute` remain non-history because these are often runtime playback
  and write-mode side effects.
- Plugin discovery/query actions (`List*`, `*Parameters`, `*Graph`, `*StateSnapshot`) remain non-history.
- MIDI learn arm actions remain non-history; actual binding changes are history-recorded.

## Follow-up candidates

If stricter undo semantics are needed, revisit:

- `SetLoopEnabled/SetLoopRange/SetPunchEnabled/SetPunchRange`
- `SetRecordEnabled`
- explicit manual-only plugin state restore actions
