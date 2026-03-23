use std::net::UdpSocket;

const OSC_TARGET_ADDR: &str = "127.0.0.1:9000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Play,
    Stop,
    Pause,
    Start,
    End,
}

impl Command {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "play" => Ok(Self::Play),
            "stop" => Ok(Self::Stop),
            "pause" => Ok(Self::Pause),
            "start" => Ok(Self::Start),
            "end" => Ok(Self::End),
            _ => Err(format!(
                "Invalid command '{value}'. Expected one of: play, stop, pause, start, end"
            )),
        }
    }

    fn osc_address(self) -> &'static str {
        match self {
            Self::Play => "/transport/play",
            Self::Stop => "/transport/stop",
            Self::Pause => "/transport/pause",
            Self::Start => "/transport/start",
            Self::End => "/transport/end",
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let command = match (args.next(), args.next()) {
        (Some(value), None) => Command::parse(&value)?,
        _ => {
            return Err("Usage: maolan-osc <play|stop|pause|start|end>".into());
        }
    };

    send_osc_command(command)?;
    Ok(())
}

fn send_osc_command(command: Command) -> Result<(), String> {
    let socket = UdpSocket::bind("127.0.0.1:0")
        .map_err(|err| format!("Failed to open UDP socket: {err}"))?;
    let packet = osc_packet(command.osc_address());
    socket
        .send_to(&packet, OSC_TARGET_ADDR)
        .map_err(|err| format!("Failed to send OSC packet to {OSC_TARGET_ADDR}: {err}"))?;
    Ok(())
}

fn osc_packet(address: &str) -> Vec<u8> {
    let mut packet = Vec::new();
    push_padded_osc_string(&mut packet, address);
    push_padded_osc_string(&mut packet, ",");
    packet
}

fn push_padded_osc_string(packet: &mut Vec<u8>, value: &str) {
    packet.extend_from_slice(value.as_bytes());
    packet.push(0);
    while !packet.len().is_multiple_of(4) {
        packet.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, osc_packet};

    #[test]
    fn parses_only_supported_commands() {
        assert_eq!(Command::parse("play").expect("play"), Command::Play);
        assert_eq!(Command::parse("stop").expect("stop"), Command::Stop);
        assert_eq!(Command::parse("pause").expect("pause"), Command::Pause);
        assert_eq!(Command::parse("start").expect("start"), Command::Start);
        assert_eq!(Command::parse("end").expect("end"), Command::End);
        assert!(Command::parse("panic").is_err());
    }

    #[test]
    fn maps_commands_to_expected_osc_paths() {
        assert_eq!(Command::Play.osc_address(), "/transport/play");
        assert_eq!(Command::Stop.osc_address(), "/transport/stop");
        assert_eq!(Command::Pause.osc_address(), "/transport/pause");
        assert_eq!(Command::Start.osc_address(), "/transport/start");
        assert_eq!(Command::End.osc_address(), "/transport/end");
    }

    #[test]
    fn builds_valid_empty_osc_packet() {
        let packet = osc_packet("/transport/play");
        assert_eq!(&packet[..16], b"/transport/play\0");
        assert_eq!(&packet[16..20], b",\0\0\0");
    }
}
