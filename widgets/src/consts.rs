use std::time::Duration;

pub const DOUBLE_CLICK: Duration = Duration::from_millis(350);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_duration_is_350ms() {
        assert_eq!(DOUBLE_CLICK, Duration::from_millis(350));
    }

    #[test]
    fn double_click_is_reasonable_duration() {
        // Double click should be between 200ms and 1000ms
        assert!(DOUBLE_CLICK >= Duration::from_millis(200));
        assert!(DOUBLE_CLICK <= Duration::from_millis(1000));
    }
}
