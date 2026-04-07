pub fn hover_radius(base: f32, hovered: bool) -> f32 {
    if hovered { base + 3.0 } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_radius_returns_base_when_not_hovered() {
        assert_eq!(hover_radius(10.0, false), 10.0);
        assert_eq!(hover_radius(5.0, false), 5.0);
    }

    #[test]
    fn hover_radius_increases_when_hovered() {
        assert_eq!(hover_radius(10.0, true), 13.0);
        assert_eq!(hover_radius(5.0, true), 8.0);
    }

    #[test]
    fn hover_radius_handles_zero() {
        assert_eq!(hover_radius(0.0, false), 0.0);
        assert_eq!(hover_radius(0.0, true), 3.0);
    }

    #[test]
    fn hover_radius_handles_negative_base() {
        assert_eq!(hover_radius(-5.0, false), -5.0);
        assert_eq!(hover_radius(-5.0, true), -2.0);
    }
}
