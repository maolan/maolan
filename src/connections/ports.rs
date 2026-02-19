pub fn hover_radius(base: f32, hovered: bool) -> f32 {
    if hovered {
        base + 3.0
    } else {
        base
    }
}
