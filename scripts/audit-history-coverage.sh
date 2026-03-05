#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

msg_file="engine/src/message.rs"
hist_file="engine/src/history.rs"

if [[ ! -f "$msg_file" || ! -f "$hist_file" ]]; then
  echo "required files not found"
  exit 1
fi

all_actions="$(
  sed -n '/pub enum Action {/,/^}/p' "$msg_file" \
    | rg '^[[:space:]]+[A-Z][A-Za-z0-9_]*[[:space:]]*(\(|\{|,)' \
    | sed -E 's/^[[:space:]]*([A-Za-z0-9_]+).*/\1/' \
    | sort -u
)"

recorded_actions="$(
  sed -n '/pub fn should_record(action: &Action)/,/^}/p' "$hist_file" \
    | rg -o 'Action::[A-Za-z0-9_]+' \
    | sed 's/^Action:://' \
    | sort -u
)"

mutating_candidates="$(
  printf "%s\n" "$all_actions" | rg -v '^(None|Play|Stop|Undo|Redo|Quit|TrackMeters|PlaybackTick|TransportPosition|BeginHistoryGroup|EndHistoryGroup|BeginSessionRestore|EndSessionRestore|SetSessionPath|RequestSessionDiagnostics|SessionDiagnosticsReport|RequestMidiLearnMappingsReport|MidiLearnMappingsReport|TrackOfflineBounceProgress|TrackOfflineBounceCanceled|OpenAudioDevice|OpenMidiInputDevice|OpenMidiOutputDevice|ListLv2Plugins|TrackGetLv2PluginControls|TrackGetPluginGraph|TrackSnapshotAllClapStates|TrackGetClapState|TrackOpenVst3Editor|TrackSetClapParameterAt|TrackSetVst3ParameterAt)$' || true
)"

echo "== History Coverage Audit =="
echo "Action variants total: $(printf "%s\n" "$all_actions" | sed '/^$/d' | wc -l | tr -d ' ')"
echo "Actions marked recordable: $(printf "%s\n" "$recorded_actions" | sed '/^$/d' | wc -l | tr -d ' ')"
echo

missing="$(
  comm -23 \
    <(printf "%s\n" "$mutating_candidates" | sed '/^$/d' | sort -u) \
    <(printf "%s\n" "$recorded_actions" | sed '/^$/d' | sort -u)
)"

if [[ -z "${missing//$'\n'/}" ]]; then
  echo "No missing mutating candidates found."
  exit 0
fi

echo "Potential mutating actions not recorded in should_record:"
printf "%s\n" "$missing"
echo
echo "Review these manually. Some may be intentionally non-history actions."
