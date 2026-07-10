use std::net::{SocketAddr, UdpSocket};

const OSC_TARGET_ADDR: &str = "127.0.0.1:9000";

#[derive(Debug, Clone)]
struct Args {
    target: String,
    command: Vec<String>,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut target: Option<(String, String)> = None;
        let mut command = Vec::new();
        while let Some(arg) = args.next() {
            if arg == "--host" {
                let host = args.next().ok_or("--host requires a value")?;
                let port = target
                    .as_ref()
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(|| "9000".to_string());
                target = Some((host, port));
            } else if arg == "--port" {
                let port = args.next().ok_or("--port requires a value")?;
                if let Some((host, _)) = target.take() {
                    target = Some((host, port));
                } else {
                    target = Some(("127.0.0.1".to_string(), port));
                }
            } else if arg == "--target" {
                let value = args.next().ok_or("--target requires a value")?;
                target = Some(parse_target_parts(&value)?);
            } else {
                command.push(arg);
            }
        }
        let target = match target {
            Some((host, port)) => format!("{host}:{port}"),
            None => OSC_TARGET_ADDR.to_string(),
        };
        if command.is_empty() {
            return Err("Missing command. See --help.".to_string());
        }
        Ok(Self { target, command })
    }
}

fn parse_target_parts(value: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid target '{value}'. Expected host:port"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse().map_err(|e| format!("{e}\n\n{}", usage()))?;
    if args
        .command
        .first()
        .is_some_and(|c| c == "--help" || c == "-h")
    {
        println!("{}", usage());
        return Ok(());
    }
    let packet = build_packet(&args.command)?;
    send_packet(&args.target, &packet)?;
    Ok(())
}

fn usage() -> &'static str {
    r#"maolan-osc — Send OSC commands to Maolan

Usage: maolan-osc [OPTIONS] <COMMAND>

Options:
  --target <host:port>  OSC target address (default: 127.0.0.1:9000)
  --host <host>         OSC target host
  --port <port>         OSC target port
  -h, --help            Show this help

Transport:
  play                                /transport/play
  stop                                /transport/stop
  pause                               /transport/pause
  start                               /transport/start
  end                                 /transport/end
  position <samples>                  /transport/position
  position_at <sample> <after_frames> /transport/position_at
  session_play                        /transport/session_play
  record <0|1>                        /transport/record
  loop_enable <0|1>                   /transport/loop_enable
  loop_range <start> <end>            /transport/loop_range
  loop_range_clear                    /transport/loop_range/clear
  punch_enable <0|1>                  /transport/punch_enable
  punch_range <start> <end>           /transport/punch_range
  punch_range_clear                   /transport/punch_range/clear
  metronome <0|1>                     /transport/metronome
  clip_playback <0|1>                 /transport/clip_playback
  session_clip_playback <0|1>         /transport/session_clip_playback
  panic                               /transport/panic
  tempo <bpm>                         /transport/tempo
  time_signature <num> <den>          /transport/time_signature
  tempo_map <json>                    /transport/tempo_map
  step_recording <0|1>                /step_recording

Session:
  session launch <track> <scene>      /session/launch
  session stop <track> <scene>        /session/stop
  session scene <scene>               /session/scene
  session stop_scene <scene>          /session/stop_scene
  session stopall                     /session/stopall
  session path <path>                 /session/path

Track management:
  track add <name> [audio_ins] [midi_ins] [audio_outs] [midi_outs] [--folder]
  track remove <name>
  track rename <old> <new>
  track folder <name> <0|1>
  track toggle_folder <name>
  track parent <name> <parent>
  track add_audio_input <name>
  track remove_audio_input <name>
  track add_audio_output <name>
  track remove_audio_output <name>

Track mixing / state:
  track level <name> <db>
  track balance <name> <db>
  track automation_level <name> <db>
  track automation_balance <name> <db>
  track midi_cc <name> <ch> <cc> <value>
  track mute <name> <0|1>
  track solo <name> <0|1>
  track arm <name> <0|1>
  track phase <name> <0|1>
  track master <name> <0|1>
  track color <name> <r> <g> <b> [a]
  track color_clear <name>            /track/color/clear
  track frozen <name> <0|1>
  track midi_lane_channel <name> <lane> <ch|-1>
  track session_slot <name> <scene> <clip_id|"">
  track session_slot_play_enabled <name> <scene> <0|1>
  track toggle_input_monitor <name> <lane>
  track toggle_disk_monitor <name> <lane>
  track toggle_midi_input_monitor <name> <lane>
  track toggle_midi_disk_monitor <name> <lane>
  track clear_default_passthrough <name>
  track clear_plugins <name>
  track connect_audio <track> <from_node> <from_port> <to_node> <to_port>
  track disconnect_audio <track> <from_node> <from_port> <to_node> <to_port>
  track connect_midi <track> <from_node> <from_port> <to_node> <to_port>
  track disconnect_midi <track> <from_node> <from_port> <to_node> <to_port>

Routing:
  connect <from> <port> <to> <port> <audio|midi>
  disconnect <from> <port> <to> <port> <audio|midi>

Clips:
  clip add <track> <name> <start> <length> <offset> <channel> <muted> <fade_enabled>
           <fade_in> <fade_out> <audio|midi> [source] [preview]
  clip remove <track> <audio|midi> <indices>
  clip move <audio|midi> <from_track> <from_idx> <to_track> <to_offset> <to_channel> <copy>
  clip fade <track> <idx> <audio|midi> <enabled> <fade_in> <fade_out>
  clip bounds <track> <idx> <audio|midi> <start> <length> <offset>
  clip mute <track> <idx> <audio|midi> <0|1>
  clip rename <track> <idx> <audio|midi> <new_name>
  clip source_name <track> <idx> <audio|midi> <name>
  clip plugin_graph_json <track> <idx> <json|"">
  clip pitch_correction <track> <idx> <json|"">
  clip_group add <track> <audio|midi> <audio_json|""> <midi_json|"">

MIDI editing:
  midi insert_notes <track> <clip_idx> <json>
  midi delete_notes <track> <clip_idx> <json>
  midi modify_notes <track> <clip_idx> <json>
  midi insert_controllers <track> <clip_idx> <json>
  midi delete_controllers <track> <clip_idx> <json>
  midi modify_controllers <track> <clip_idx> <json>
  midi sysex <track> <clip_idx> <json>
  midi step_record <device> <channel> <pitch> <velocity>

Plugins:
  plugin load <track> <clap|vst3|lv2> <id>
  plugin unload <track> <format> <id>
  plugin unload_instance <track> <format> <instance>
  plugin bypass <track> <format> <instance> <0|1>
  plugin show_gui <track> <format> <instance>
  plugin snapshot_state <track> <clap|vst3|lv2> <instance>
  plugin snapshot_all_states <track>
  plugin restore_state <track> <clap|vst3|lv2> <instance> <json>
  plugin set_param_at <track> clap <instance> <param_id> <value> <frame>
  plugin begin_param_edit <track> clap <instance> <param_id> <frame>
  plugin end_param_edit <track> clap <instance> <param_id> <frame>
  plugin set_resource_dir <track> <format> <instance> <dir>
  plugin update_file_reference <track> <format> <instance> <index> <path>
  plugin connect_audio <track> <from_node> <from_port> <to_node> <to_port>
  plugin disconnect_audio <track> <from_node> <from_port> <to_node> <to_port>
  plugin connect_midi <track> <from_node> <from_port> <to_node> <to_port>
  plugin disconnect_midi <track> <from_node> <from_port> <to_node> <to_port>
  plugin set_param <track> <clap|vst3|lv2> <instance> <param_id> <value>
  clip_plugin set_param <track> <clap|lv2> <clip_idx> <instance> <param_id> <value>
  clip_plugin snapshot_state <track> <clap|vst3|lv2> <clip_idx> <instance>
  clip_plugin restore_state <track> <clap|lv2|vst3> <clip_idx> <instance> <json>
  clip_plugin set_resource_dir <track> <format> <clip_idx> <instance> <dir>
  clip_plugin update_file_reference <track> <format> <clip_idx> <instance> <index> <path>

VST3 graph:
  vst3 connect_audio <track> <from_node> <from_port> <to_node> <to_port>
  vst3 disconnect_audio <track> <from_node> <from_port> <to_node> <to_port>

Automation:
  automation mode <track> <read|touch|latch|write>
  automation toggle_lane <track> <volume|balance|midi_cc_<ch>_<cc>>
  automation point <track> <target> <sample> <value>
  automation delete_point <track> <target> <sample>
  automation set_lanes <track> <read|touch|latch|write> <json>

MIDI learn:
  midi_learn arm_track <track> <target>
  midi_learn arm_global <target>
  midi_learn arm_session <target>
  midi_learn bind_track <track> <target> <json|"">
  midi_learn bind_global <target> <json|"">
  midi_learn bind_session <target> <json|"">
  midi_learn clear

Modulators / Devices:
  modulators <json>
  device audio_open <json>
  device midi_in_open <device>
  device midi_out_open <device>
  device jack add_audio_in
  device jack remove_audio_in <port>
  device jack add_audio_out
  device jack remove_audio_out <port>

Offline bounce:
  bounce start <track> <output_path> <start> <length> <lanes_json|""> <apply_fader>
  bounce cancel <track>
  bounce cancel_all

Piano key:
  piano_key <track> <note> <velocity> <0|1>

Queries:
  query tracks
  query transport
  query meters
  query plugins <track>
  query plugin_parameters <track> <clap|vst3|lv2> <instance>
  query clip_plugin_parameters <track> <lv2> <clip_idx> <instance>
  query clap_plugins
  query clap_plugins_with_capabilities
  query vst3_plugins
  query lv2_plugins
  query clap_note_names <track>
  query lv2_midnam <track>              /query/lv2_midnam
  query vst3_graph <track>
  query diagnostics
  query midi_learn_report
"#
}

fn build_packet(command: &[String]) -> Result<Vec<u8>, String> {
    if command.is_empty() {
        return Err("Empty command".to_string());
    }
    match command[0].as_str() {
        "play" => no_args("/transport/play", command),
        "stop" => no_args("/transport/stop", command),
        "pause" => no_args("/transport/pause", command),
        "start" => no_args("/transport/start", command),
        "end" => no_args("/transport/end", command),
        "position" => one_int("/transport/position", command),
        "position_at" => two_ints("/transport/position_at", command),
        "session_play" => no_args("/transport/session_play", command),
        "record" => one_bool("/transport/record", command),
        "loop_enable" => one_bool("/transport/loop_enable", command),
        "loop_range" => two_ints("/transport/loop_range", command),
        "loop_range_clear" => no_args("/transport/loop_range/clear", command),
        "punch_enable" => one_bool("/transport/punch_enable", command),
        "punch_range" => two_ints("/transport/punch_range", command),
        "punch_range_clear" => no_args("/transport/punch_range/clear", command),
        "metronome" => one_bool("/transport/metronome", command),
        "clip_playback" => one_bool("/transport/clip_playback", command),
        "session_clip_playback" => one_bool("/transport/session_clip_playback", command),
        "panic" => no_args("/transport/panic", command),
        "tempo" => one_float("/transport/tempo", command),
        "time_signature" => two_ints("/transport/time_signature", command),
        "tempo_map" => one_string("/transport/tempo_map", command),
        "step_recording" => one_bool("/step_recording", command),
        "session" => build_session_packet(command),
        "track" => build_track_packet(command),
        "clip" => build_clip_packet(command),
        "clip_group" => build_clip_group_packet(command),
        "midi" => build_midi_packet(command),
        "connect" => build_connect_packet(command, true),
        "disconnect" => build_connect_packet(command, false),
        "plugin" => build_plugin_packet(command),
        "clip_plugin" => build_clip_plugin_packet(command),
        "vst3" => build_vst3_packet(command),
        "automation" => build_automation_packet(command),
        "midi_learn" => build_midi_learn_packet(command),
        "modulators" => one_string("/modulators", command),
        "device" => build_device_packet(command),
        "bounce" => build_bounce_packet(command),
        "piano_key" => build_piano_key_packet(command),
        "query" => build_query_packet(command),
        _ => Err(format!("Unknown command '{}'. See --help.", command[0])),
    }
}

fn build_session_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "launch" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/session/launch",
                "si",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                ],
            ))
        }
        "stop" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/session/stop",
                "si",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                ],
            ))
        }
        "scene" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/session/scene",
                "i",
                &[OscArg::Int(parse_int(&command[2])?)],
            ))
        }
        "stop_scene" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/session/stop_scene",
                "i",
                &[OscArg::Int(parse_int(&command[2])?)],
            ))
        }
        "stopall" => no_args("/session/stopall", command),
        "path" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/session/path",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        _ => Err(format!("Unknown session subcommand '{}'", command[1])),
    }
}

fn build_clip_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "add" => {
            require_len(command, 12)?;
            let kind = parse_kind(&command[11])?;
            let source = command.get(12).cloned().unwrap_or_default();
            let preview = command.get(13).cloned().unwrap_or_default();
            Ok(osc_packet_with_args(
                "/clip/add",
                "sssiiiiiiiiss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::Int(parse_int(&command[7])?),
                    OscArg::Int(parse_bool(&command[8])?),
                    OscArg::Int(parse_bool(&command[9])?),
                    OscArg::Int(parse_int(&command[10])?),
                    OscArg::Int(parse_int(&command[11])?),
                    OscArg::String(kind.to_string()),
                    OscArg::String(source),
                    OscArg::String(preview),
                ],
            ))
        }
        "remove" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/clip/remove",
                "sss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(parse_kind(&command[3])?.to_string()),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "move" => {
            require_len(command, 9)?;
            Ok(osc_packet_with_args(
                "/clip/move",
                "sisisiii",
                &[
                    OscArg::String(parse_kind(&command[2])?.to_string()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::Int(parse_int(&command[7])?),
                    OscArg::Int(parse_bool(&command[8])?),
                ],
            ))
        }
        "fade" => {
            require_len(command, 8)?;
            Ok(osc_packet_with_args(
                "/clip/fade",
                "sissiii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(parse_kind(&command[4])?.to_string()),
                    OscArg::Int(parse_bool(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::Int(parse_int(&command[7])?),
                ],
            ))
        }
        "bounds" => {
            require_len(command, 9)?;
            Ok(osc_packet_with_args(
                "/clip/bounds",
                "sissiii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(parse_kind(&command[4])?.to_string()),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::Int(parse_int(&command[7])?),
                ],
            ))
        }
        "mute" => string_int_kind_bool("/clip/mute", command),
        "rename" => string_int_kind_string("/clip/rename", command),
        "source_name" => string_int_kind_string("/clip/source_name", command),
        "plugin_graph_json" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/clip/plugin_graph_json",
                "sis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "pitch_correction" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/clip/pitch_correction",
                "sis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        _ => Err(format!("Unknown clip subcommand '{}'", command[1])),
    }
}

fn build_clip_group_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "add" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/clip_group/add",
                "ssss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(parse_kind(&command[3])?.to_string()),
                    OscArg::String(command[4].clone()),
                    OscArg::String(command[5].clone()),
                ],
            ))
        }
        _ => Err(format!("Unknown clip_group subcommand '{}'", command[1])),
    }
}

fn build_midi_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "insert_notes" | "delete_notes" | "modify_notes" | "insert_controllers"
        | "delete_controllers" | "modify_controllers" | "sysex" => {
            require_len(command, 5)?;
            let address = format!("/midi/{}", command[1]);
            Ok(osc_packet_with_args(
                &address,
                "sis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "step_record" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/midi/step_record",
                "siiii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                ],
            ))
        }
        _ => Err(format!("Unknown midi subcommand '{}'", command[1])),
    }
}

fn build_track_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "add" => {
            if command.len() < 3 {
                return Err("track add requires <name>".to_string());
            }
            let name = command[2].clone();
            let audio_ins = parse_int(command.get(3).unwrap_or(&"2".to_string()))?;
            let midi_ins = parse_int(command.get(4).unwrap_or(&"0".to_string()))?;
            let audio_outs = parse_int(command.get(5).unwrap_or(&"2".to_string()))?;
            let midi_outs = parse_int(command.get(6).unwrap_or(&"0".to_string()))?;
            let folder = command.len() > 7 && command[7] == "--folder";
            Ok(osc_packet_with_args(
                "/track/add",
                "siiiii",
                &[
                    OscArg::String(name),
                    OscArg::Int(audio_ins),
                    OscArg::Int(midi_ins),
                    OscArg::Int(audio_outs),
                    OscArg::Int(midi_outs),
                    OscArg::Int(if folder { 1 } else { 0 }),
                ],
            ))
        }
        "remove" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/track/remove",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "rename" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/track/rename",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "folder" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/track/set_folder",
                "si",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_bool(&command[3])?),
                ],
            ))
        }
        "toggle_folder" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/track/toggle_folder",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "parent" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/track/set_parent",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "level" => string_float("/track/level", command),
        "balance" => string_float("/track/balance", command),
        "automation_level" => string_float("/track/automation_level", command),
        "automation_balance" => string_float("/track/automation_balance", command),
        "midi_cc" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/track/midi_cc",
                "siii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                ],
            ))
        }
        "mute" => string_bool("/track/mute", command),
        "solo" => string_bool("/track/solo", command),
        "arm" => string_bool("/track/arm", command),
        "phase" => string_bool("/track/phase", command),
        "master" => string_bool("/track/master", command),
        "add_audio_input" => string_only("/track/add_audio_input", command),
        "remove_audio_input" => string_only("/track/remove_audio_input", command),
        "add_audio_output" => string_only("/track/add_audio_output", command),
        "remove_audio_output" => string_only("/track/remove_audio_output", command),
        "toggle_input_monitor" => string_int("/track/toggle_input_monitor", command),
        "toggle_disk_monitor" => string_int("/track/toggle_disk_monitor", command),
        "toggle_midi_input_monitor" => string_int("/track/toggle_midi_input_monitor", command),
        "toggle_midi_disk_monitor" => string_int("/track/toggle_midi_disk_monitor", command),
        "midi_lane_channel" => {
            require_len(command, 5)?;
            let channel = parse_int(&command[4])?;
            Ok(osc_packet_with_args(
                "/track/midi_lane_channel",
                "sii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::Int(channel),
                ],
            ))
        }
        "frozen" => string_bool("/track/frozen", command),
        "color" => {
            require_len(command, 6)?;
            let a = parse_float(command.get(6).unwrap_or(&"1.0".to_string()))?;
            Ok(osc_packet_with_args(
                "/track/color",
                "sffff",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Float(parse_float(&command[3])?),
                    OscArg::Float(parse_float(&command[4])?),
                    OscArg::Float(parse_float(&command[5])?),
                    OscArg::Float(a),
                ],
            ))
        }
        "color_clear" => string_only("/track/color/clear", command),
        "session_slot" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/track/session_slot",
                "sis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "session_slot_play_enabled" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/track/session_slot_play_enabled",
                "sii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::Int(parse_int(&command[3])?),
                    OscArg::Int(parse_bool(&command[4])?),
                ],
            ))
        }
        "clear_default_passthrough" => string_only("/track/clear_default_passthrough", command),
        "clear_plugins" => string_only("/track/clear_plugins", command),
        "connect_audio" | "disconnect_audio" | "connect_midi" | "disconnect_midi" => {
            require_len(command, 7)?;
            let address = format!("/track/{}", command[1]);
            Ok(osc_packet_with_args(
                &address,
                "ssisi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                    OscArg::Int(parse_int(&command[6])?),
                ],
            ))
        }
        _ => Err(format!("Unknown track subcommand '{}'", command[1])),
    }
}

fn build_connect_packet(command: &[String], connect: bool) -> Result<Vec<u8>, String> {
    require_len(command, 6)?;
    let address = if connect { "/connect" } else { "/disconnect" };
    Ok(osc_packet_with_args(
        address,
        "sisis",
        &[
            OscArg::String(command[1].clone()),
            OscArg::Int(parse_int(&command[2])?),
            OscArg::String(command[3].clone()),
            OscArg::Int(parse_int(&command[4])?),
            OscArg::String(command[5].clone()),
        ],
    ))
}

fn build_plugin_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "load" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/plugin/load",
                "sss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "unload" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/plugin/unload",
                "sss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "unload_instance" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/plugin/unload_instance",
                "ssi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                ],
            ))
        }
        "bypass" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/plugin/bypass",
                "ssii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_bool(&command[5])?),
                ],
            ))
        }
        "show_gui" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/plugin/show_gui",
                "ssi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                ],
            ))
        }
        "snapshot_state" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/plugin/snapshot_state",
                "ssi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                ],
            ))
        }
        "restore_state" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/plugin/restore_state",
                "ssis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                ],
            ))
        }
        "set_resource_dir" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/plugin/set_resource_dir",
                "ssis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                ],
            ))
        }
        "update_file_reference" => {
            require_len(command, 7)?;
            Ok(osc_packet_with_args(
                "/plugin/update_file_reference",
                "ssiis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::String(command[6].clone()),
                ],
            ))
        }
        "snapshot_all_states" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/plugin/snapshot_all_states",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "set_param_at" => {
            require_len(command, 8)?;
            Ok(osc_packet_with_args(
                "/plugin/set_param_at",
                "ssiifi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Float(parse_float(&command[6])?),
                    OscArg::Int(parse_int(&command[7])?),
                ],
            ))
        }
        "begin_param_edit" | "end_param_edit" => {
            require_len(command, 7)?;
            let address = format!("/plugin/{}", command[1]);
            Ok(osc_packet_with_args(
                &address,
                "ssiii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                ],
            ))
        }
        "connect_audio" | "disconnect_audio" | "connect_midi" | "disconnect_midi" => {
            require_len(command, 7)?;
            let address = format!("/plugin/{}", command[1]);
            Ok(osc_packet_with_args(
                &address,
                "ssisisi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                    OscArg::Int(parse_int(&command[6])?),
                ],
            ))
        }
        "set_param" => {
            require_len(command, 7)?;
            Ok(osc_packet_with_args(
                "/plugin/set_param",
                "ssiif",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Float(parse_float(&command[6])?),
                ],
            ))
        }
        _ => Err(format!("Unknown plugin subcommand '{}'", command[1])),
    }
}

fn build_clip_plugin_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "set_param" => {
            require_len(command, 8)?;
            Ok(osc_packet_with_args(
                "/clip_plugin/set_param",
                "ssiiif",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::Float(parse_float(&command[7])?),
                ],
            ))
        }
        "snapshot_state" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/clip_plugin/snapshot_state",
                "ssii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                ],
            ))
        }
        "restore_state" => {
            require_len(command, 7)?;
            Ok(osc_packet_with_args(
                "/clip_plugin/restore_state",
                "ssiis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::String(command[6].clone()),
                ],
            ))
        }
        "set_resource_dir" => {
            require_len(command, 7)?;
            Ok(osc_packet_with_args(
                "/clip_plugin/set_resource_dir",
                "ssiis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::String(command[6].clone()),
                ],
            ))
        }
        "update_file_reference" => {
            require_len(command, 8)?;
            Ok(osc_packet_with_args(
                "/clip_plugin/update_file_reference",
                "ssiiis",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::Int(parse_int(&command[6])?),
                    OscArg::String(command[7].clone()),
                ],
            ))
        }
        _ => Err(format!("Unknown clip_plugin subcommand '{}'", command[1])),
    }
}

fn build_vst3_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "connect_audio" | "disconnect_audio" => {
            require_len(command, 7)?;
            let address = format!("/vst3/{}", command[1]);
            Ok(osc_packet_with_args(
                &address,
                "ssisi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::String(command[5].clone()),
                    OscArg::Int(parse_int(&command[6])?),
                ],
            ))
        }
        _ => Err(format!("Unknown vst3 subcommand '{}'", command[1])),
    }
}

fn build_automation_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "mode" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/automation/mode",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "toggle_lane" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/automation/toggle_lane",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "point" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/automation/insert_point",
                "ssif",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Float(parse_float(&command[5])?),
                ],
            ))
        }
        "delete_point" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/automation/delete_point",
                "ssi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                ],
            ))
        }
        "set_lanes" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/automation/set_lanes",
                "sss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        _ => Err(format!("Unknown automation subcommand '{}'", command[1])),
    }
}

fn build_midi_learn_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "arm_track" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/midi_learn/arm_track",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "arm_global" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/midi_learn/arm_global",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "arm_session" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/midi_learn/arm_session",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "bind_track" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/midi_learn/bind_track",
                "sss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::String(command[4].clone()),
                ],
            ))
        }
        "bind_global" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/midi_learn/bind_global",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "bind_session" => {
            require_len(command, 4)?;
            Ok(osc_packet_with_args(
                "/midi_learn/bind_session",
                "ss",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                ],
            ))
        }
        "clear" => no_args("/midi_learn/clear", command),
        _ => Err(format!("Unknown midi_learn subcommand '{}'", command[1])),
    }
}

fn build_device_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "audio_open" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/device/audio_open",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "midi_in_open" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/device/midi_in_open",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "midi_out_open" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/device/midi_out_open",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "jack" => {
            require_len(command, 3)?;
            match command[2].as_str() {
                "add_audio_in" => no_args("/device/jack/add_audio_in", command),
                "add_audio_out" => no_args("/device/jack/add_audio_out", command),
                "remove_audio_in" => {
                    require_len(command, 4)?;
                    Ok(osc_packet_with_args(
                        "/device/jack/remove_audio_in",
                        "i",
                        &[OscArg::Int(parse_int(&command[3])?)],
                    ))
                }
                "remove_audio_out" => {
                    require_len(command, 4)?;
                    Ok(osc_packet_with_args(
                        "/device/jack/remove_audio_out",
                        "i",
                        &[OscArg::Int(parse_int(&command[3])?)],
                    ))
                }
                _ => Err(format!("Unknown jack subcommand '{}'", command[2])),
            }
        }
        _ => Err(format!("Unknown device subcommand '{}'", command[1])),
    }
}

fn build_bounce_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "start" => {
            require_len(command, 8)?;
            Ok(osc_packet_with_args(
                "/bounce/start",
                "sssiisi",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                    OscArg::String(command[6].clone()),
                    OscArg::Int(parse_bool(&command[7])?),
                ],
            ))
        }
        "cancel" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/bounce/cancel",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "cancel_all" => no_args("/bounce/cancel_all", command),
        _ => Err(format!("Unknown bounce subcommand '{}'", command[1])),
    }
}

fn build_piano_key_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 5)?;
    Ok(osc_packet_with_args(
        "/piano_key",
        "siii",
        &[
            OscArg::String(command[1].clone()),
            OscArg::Int(parse_int(&command[2])?),
            OscArg::Int(parse_int(&command[3])?),
            OscArg::Int(parse_bool(&command[4])?),
        ],
    ))
}

fn build_query_packet(command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    match command[1].as_str() {
        "tracks" => no_args("/query/tracks", command),
        "transport" => no_args("/query/transport", command),
        "meters" => no_args("/query/meters", command),
        "plugins" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/query/plugins",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "plugin_parameters" => {
            require_len(command, 5)?;
            Ok(osc_packet_with_args(
                "/query/plugin_parameters",
                "ssii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                ],
            ))
        }
        "clip_plugin_parameters" => {
            require_len(command, 6)?;
            Ok(osc_packet_with_args(
                "/query/clip_plugin_parameters",
                "ssiii",
                &[
                    OscArg::String(command[2].clone()),
                    OscArg::String(command[3].clone()),
                    OscArg::Int(parse_int(&command[4])?),
                    OscArg::Int(parse_int(&command[5])?),
                ],
            ))
        }
        "clap_plugins" => no_args("/query/clap_plugins", command),
        "clap_plugins_with_capabilities" => {
            no_args("/query/clap_plugins_with_capabilities", command)
        }
        "vst3_plugins" => no_args("/query/vst3_plugins", command),
        "lv2_plugins" => no_args("/query/lv2_plugins", command),
        "clap_note_names" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/query/clap_note_names",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "lv2_midnam" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/query/lv2_midnam",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "vst3_graph" => {
            require_len(command, 3)?;
            Ok(osc_packet_with_args(
                "/query/vst3_graph",
                "s",
                &[OscArg::String(command[2].clone())],
            ))
        }
        "diagnostics" => no_args("/query/diagnostics", command),
        "midi_learn_report" => no_args("/query/midi_learn_report", command),
        _ => Err(format!("Unknown query subcommand '{}'", command[1])),
    }
}

fn no_args(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 1)?;
    Ok(osc_packet(address))
}

fn one_int(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    Ok(osc_packet_with_args(
        address,
        "i",
        &[OscArg::Int(parse_int(&command[1])?)],
    ))
}

fn one_bool(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    Ok(osc_packet_with_args(
        address,
        "i",
        &[OscArg::Int(parse_bool(&command[1])?)],
    ))
}

fn one_float(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    Ok(osc_packet_with_args(
        address,
        "f",
        &[OscArg::Float(parse_float(&command[1])?)],
    ))
}

fn two_ints(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 3)?;
    Ok(osc_packet_with_args(
        address,
        "ii",
        &[
            OscArg::Int(parse_int(&command[1])?),
            OscArg::Int(parse_int(&command[2])?),
        ],
    ))
}

fn string_float(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 4)?;
    Ok(osc_packet_with_args(
        address,
        "sf",
        &[
            OscArg::String(command[2].clone()),
            OscArg::Float(parse_float(&command[3])?),
        ],
    ))
}

fn string_bool(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 4)?;
    Ok(osc_packet_with_args(
        address,
        "si",
        &[
            OscArg::String(command[2].clone()),
            OscArg::Int(parse_bool(&command[3])?),
        ],
    ))
}

fn one_string(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 2)?;
    Ok(osc_packet_with_args(
        address,
        "s",
        &[OscArg::String(command[1].clone())],
    ))
}

fn string_only(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 3)?;
    Ok(osc_packet_with_args(
        address,
        "s",
        &[OscArg::String(command[2].clone())],
    ))
}

fn string_int(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 4)?;
    Ok(osc_packet_with_args(
        address,
        "sii",
        &[
            OscArg::String(command[2].clone()),
            OscArg::Int(parse_int(&command[3])?),
        ],
    ))
}

fn string_int_kind_bool(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 6)?;
    Ok(osc_packet_with_args(
        address,
        "sissi",
        &[
            OscArg::String(command[2].clone()),
            OscArg::Int(parse_int(&command[3])?),
            OscArg::String(parse_kind(&command[4])?.to_string()),
            OscArg::Int(parse_bool(&command[5])?),
        ],
    ))
}

fn string_int_kind_string(address: &str, command: &[String]) -> Result<Vec<u8>, String> {
    require_len(command, 6)?;
    Ok(osc_packet_with_args(
        address,
        "sissi",
        &[
            OscArg::String(command[2].clone()),
            OscArg::Int(parse_int(&command[3])?),
            OscArg::String(parse_kind(&command[4])?.to_string()),
            OscArg::String(command[5].clone()),
        ],
    ))
}

fn parse_kind(value: &str) -> Result<&'static str, String> {
    match value {
        "audio" => Ok("audio"),
        "midi" => Ok("midi"),
        _ => Err(format!("Expected 'audio' or 'midi', got '{value}'")),
    }
}

fn require_len(command: &[String], len: usize) -> Result<(), String> {
    if command.len() < len {
        return Err(format!(
            "Command '{}' requires at least {} argument(s)",
            command.first().map(|s| s.as_str()).unwrap_or(""),
            len.saturating_sub(1)
        ));
    }
    Ok(())
}

fn parse_int(value: &str) -> Result<i32, String> {
    value
        .parse()
        .map_err(|_| format!("Expected integer, got '{value}'"))
}

fn parse_float(value: &str) -> Result<f32, String> {
    value
        .parse()
        .map_err(|_| format!("Expected number, got '{value}'"))
}

fn parse_bool(value: &str) -> Result<i32, String> {
    match value {
        "0" | "false" | "off" => Ok(0),
        "1" | "true" | "on" => Ok(1),
        _ => Err(format!(
            "Expected boolean (0/1/false/true/off/on), got '{value}'"
        )),
    }
}

fn send_packet(target: &str, packet: &[u8]) -> Result<(), String> {
    let addr: SocketAddr = target
        .parse()
        .map_err(|_| format!("Invalid target address '{target}'"))?;
    let socket = UdpSocket::bind("127.0.0.1:0")
        .map_err(|err| format!("Failed to open UDP socket: {err}"))?;
    socket
        .send_to(packet, addr)
        .map_err(|err| format!("Failed to send OSC packet to {target}: {err}"))?;
    Ok(())
}

fn osc_packet(address: &str) -> Vec<u8> {
    let mut packet = Vec::new();
    push_padded_osc_string(&mut packet, address);
    push_padded_osc_string(&mut packet, ",");
    packet
}

fn osc_packet_with_args(address: &str, type_tags: &str, args: &[OscArg]) -> Vec<u8> {
    let mut packet = Vec::new();
    push_padded_osc_string(&mut packet, address);
    push_padded_osc_string(&mut packet, &format!(",{type_tags}"));
    for arg in args {
        match arg {
            OscArg::String(s) => push_padded_osc_string(&mut packet, s),
            OscArg::Int(i) => packet.extend_from_slice(&i.to_be_bytes()),
            OscArg::Float(f) => packet.extend_from_slice(&f.to_be_bytes()),
        }
    }
    packet
}

fn push_padded_osc_string(packet: &mut Vec<u8>, value: &str) {
    packet.extend_from_slice(value.as_bytes());
    packet.push(0);
    while !packet.len().is_multiple_of(4) {
        packet.push(0);
    }
}

#[derive(Debug, Clone)]
enum OscArg {
    String(String),
    Int(i32),
    Float(f32),
}

#[cfg(test)]
mod tests {
    use super::{build_packet, parse_bool, parse_float, parse_int};

    fn packet_starts_with_address(packet: &[u8], address: &str) -> bool {
        packet.starts_with(address.as_bytes()) && packet[address.len()] == 0
    }

    #[test]
    fn parses_integers() {
        assert_eq!(parse_int("42").unwrap(), 42);
        assert_eq!(parse_int("-3").unwrap(), -3);
        assert!(parse_int("x").is_err());
    }

    #[test]
    fn parses_floats() {
        assert!((parse_float("-6.5").unwrap() - -6.5).abs() < f32::EPSILON);
        assert!(parse_float("abc").is_err());
    }

    #[test]
    fn parses_booleans() {
        assert_eq!(parse_bool("0").unwrap(), 0);
        assert_eq!(parse_bool("false").unwrap(), 0);
        assert_eq!(parse_bool("off").unwrap(), 0);
        assert_eq!(parse_bool("1").unwrap(), 1);
        assert_eq!(parse_bool("true").unwrap(), 1);
        assert_eq!(parse_bool("on").unwrap(), 1);
        assert!(parse_bool("maybe").is_err());
    }

    #[test]
    fn builds_new_osc_packets() {
        let packet = build_packet(&["step_recording".to_string(), "1".to_string()]).unwrap();
        assert!(packet_starts_with_address(&packet, "/step_recording"));

        let packet = build_packet(&[
            "session".to_string(),
            "stop_scene".to_string(),
            "2".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/session/stop_scene"));

        let packet = build_packet(&[
            "track".to_string(),
            "automation_level".to_string(),
            "drums".to_string(),
            "-6.0".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(
            &packet,
            "/track/automation_level"
        ));

        let packet = build_packet(&[
            "track".to_string(),
            "midi_cc".to_string(),
            "drums".to_string(),
            "1".to_string(),
            "7".to_string(),
            "64".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/track/midi_cc"));

        let packet = build_packet(&[
            "plugin".to_string(),
            "show_gui".to_string(),
            "drums".to_string(),
            "clap".to_string(),
            "0".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/plugin/show_gui"));

        let packet = build_packet(&[
            "plugin".to_string(),
            "restore_state".to_string(),
            "drums".to_string(),
            "clap".to_string(),
            "0".to_string(),
            r#"{"bytes":[1,2,3]}"#.to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/plugin/restore_state"));

        let packet = build_packet(&[
            "clip_plugin".to_string(),
            "update_file_reference".to_string(),
            "drums".to_string(),
            "clap".to_string(),
            "1".to_string(),
            "0".to_string(),
            "0".to_string(),
            "/tmp/sample.wav".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(
            &packet,
            "/clip_plugin/update_file_reference"
        ));

        let packet = build_packet(&["query".to_string(), "diagnostics".to_string()]).unwrap();
        assert!(packet_starts_with_address(&packet, "/query/diagnostics"));

        let packet = build_packet(&[
            "automation".to_string(),
            "set_lanes".to_string(),
            "drums".to_string(),
            "latch".to_string(),
            "[]".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/automation/set_lanes"));

        let packet = build_packet(&[
            "plugin".to_string(),
            "snapshot_all_states".to_string(),
            "drums".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(
            &packet,
            "/plugin/snapshot_all_states"
        ));

        let packet = build_packet(&[
            "plugin".to_string(),
            "begin_param_edit".to_string(),
            "drums".to_string(),
            "clap".to_string(),
            "0".to_string(),
            "7".to_string(),
            "64".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(
            &packet,
            "/plugin/begin_param_edit"
        ));

        let packet = build_packet(&[
            "vst3".to_string(),
            "connect_audio".to_string(),
            "drums".to_string(),
            "track_input".to_string(),
            "0".to_string(),
            "vst3_1".to_string(),
            "0".to_string(),
        ])
        .unwrap();
        assert!(packet_starts_with_address(&packet, "/vst3/connect_audio"));
    }
}
