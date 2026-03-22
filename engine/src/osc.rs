use crate::message::{Action, Message};
use std::{
    net::{ToSocketAddrs, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::mpsc::Sender;
use tracing::{error, info, warn};

#[cfg(test)]
use std::net::SocketAddr;

const OSC_LISTEN_ADDR: &str = "0.0.0.0:9000";

pub struct OscServer {
    stop: Arc<AtomicBool>,
    #[cfg(test)]
    listen_addr: SocketAddr,
    handle: Option<thread::JoinHandle<()>>,
}

impl OscServer {
    pub fn start(tx: Sender<Message>) -> Result<Self, String> {
        Self::start_on_addr(tx, OSC_LISTEN_ADDR)
    }

    pub fn start_on_addr<A: ToSocketAddrs>(tx: Sender<Message>, addr: A) -> Result<Self, String> {
        let bind_addr = addr
            .to_socket_addrs()
            .map_err(|e| format!("Failed to resolve OSC socket address: {e}"))?
            .next()
            .ok_or_else(|| "Failed to resolve OSC socket address".to_string())?;
        let socket = UdpSocket::bind(bind_addr)
            .map_err(|e| format!("Failed to bind OSC socket on {bind_addr}: {e}"))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .map_err(|e| format!("Failed to configure OSC socket timeout: {e}"))?;
        let listen_addr = socket
            .local_addr()
            .map_err(|e| format!("Failed to read OSC socket address: {e}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = thread::spawn(move || {
            let mut buf = [0_u8; 2048];
            info!("OSC server listening on {listen_addr}");
            while !stop_thread.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buf) {
                    Ok((len, _)) => {
                        if let Some(action) = parse_osc_action(&buf[..len]) {
                            if let Err(err) = tx.blocking_send(Message::Request(action)) {
                                error!("Failed to forward OSC action to engine: {err}");
                                break;
                            }
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(err) => {
                        error!("OSC receive error: {err}");
                        break;
                    }
                }
            }
            info!("OSC server stopped");
        });

        Ok(Self {
            stop,
            #[cfg(test)]
            listen_addr,
            handle: Some(handle),
        })
    }

    #[cfg(test)]
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take()
            && let Err(err) = handle.join()
        {
            warn!("Failed to join OSC thread: {:?}", err);
        }
    }
}

impl Drop for OscServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn parse_osc_action(packet: &[u8]) -> Option<Action> {
    let (address, next) = parse_osc_string(packet, 0)?;
    let (type_tags, _) = parse_osc_string(packet, next)?;
    if type_tags != "," {
        return None;
    }

    match address.as_str() {
        "/start" | "/transport/start" | "/transport/play" => Some(Action::Play),
        "/stop" | "/transport/stop" => Some(Action::Stop),
        "/pause" | "/transport/pause" => Some(Action::Pause),
        "/jump_to_start" | "/transport/jump_to_start" | "/transport/start_of_session" => {
            Some(Action::TransportPosition(0))
        }
        "/jump_to_end" | "/transport/jump_to_end" | "/transport/end_of_session" => {
            Some(Action::JumpToEnd)
        }
        _ => None,
    }
}

fn parse_osc_string(packet: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset >= packet.len() {
        return None;
    }
    let end = packet[offset..].iter().position(|byte| *byte == 0)? + offset;
    let value = std::str::from_utf8(&packet[offset..end]).ok()?.to_string();
    let next = (end + 4) & !3;
    if next > packet.len() {
        return None;
    }
    Some((value, next))
}

#[cfg(test)]
mod tests {
    use super::parse_osc_action;
    use crate::message::Action;

    fn osc_packet(address: &str) -> Vec<u8> {
        fn push_padded_string(buf: &mut Vec<u8>, value: &str) {
            buf.extend_from_slice(value.as_bytes());
            buf.push(0);
            while buf.len() % 4 != 0 {
                buf.push(0);
            }
        }

        let mut buf = Vec::new();
        push_padded_string(&mut buf, address);
        push_padded_string(&mut buf, ",");
        buf
    }

    #[test]
    fn parses_basic_transport_messages() {
        assert!(matches!(
            parse_osc_action(&osc_packet("/start")),
            Some(Action::Play)
        ));
        assert!(matches!(
            parse_osc_action(&osc_packet("/pause")),
            Some(Action::Pause)
        ));
        assert!(matches!(
            parse_osc_action(&osc_packet("/stop")),
            Some(Action::Stop)
        ));
        assert!(matches!(
            parse_osc_action(&osc_packet("/jump_to_start")),
            Some(Action::TransportPosition(0))
        ));
        assert!(matches!(
            parse_osc_action(&osc_packet("/jump_to_end")),
            Some(Action::JumpToEnd)
        ));
    }
}
