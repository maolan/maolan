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
