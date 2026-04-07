use maolan_engine::kind::Kind;

pub fn can_connect_kinds(from: Kind, to: Kind) -> bool {
    from == to
}

pub fn should_highlight_port(is_hovered: bool, active_kind: Option<Kind>, port_kind: Kind) -> bool {
    match active_kind {
        Some(kind) => is_hovered && can_connect_kinds(kind, port_kind),
        None => is_hovered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_connect_same_kinds() {
        assert!(can_connect_kinds(Kind::Audio, Kind::Audio));
        assert!(can_connect_kinds(Kind::MIDI, Kind::MIDI));
    }

    #[test]
    fn cannot_connect_different_kinds() {
        assert!(!can_connect_kinds(Kind::Audio, Kind::MIDI));
        assert!(!can_connect_kinds(Kind::MIDI, Kind::Audio));
    }

    #[test]
    fn should_highlight_when_hovered_and_no_active_kind() {
        assert!(should_highlight_port(true, None, Kind::Audio));
        assert!(should_highlight_port(true, None, Kind::MIDI));
    }

    #[test]
    fn should_not_highlight_when_not_hovered() {
        assert!(!should_highlight_port(false, None, Kind::Audio));
        assert!(!should_highlight_port(
            false,
            Some(Kind::Audio),
            Kind::Audio
        ));
    }

    #[test]
    fn should_highlight_when_kinds_match() {
        assert!(should_highlight_port(true, Some(Kind::Audio), Kind::Audio));
        assert!(should_highlight_port(true, Some(Kind::MIDI), Kind::MIDI));
    }

    #[test]
    fn should_not_highlight_when_kinds_differ() {
        assert!(!should_highlight_port(true, Some(Kind::Audio), Kind::MIDI));
        assert!(!should_highlight_port(true, Some(Kind::MIDI), Kind::Audio));
    }
}
