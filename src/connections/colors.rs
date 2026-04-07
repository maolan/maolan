use iced::Color;

pub fn audio_port_color() -> Color {
    Color::from_rgb(0.2, 0.5, 1.0)
}

pub fn aux_port_color() -> Color {
    Color::from_rgb(1.0, 0.6, 0.0)
}

pub fn midi_port_color() -> Color {
    Color::from_rgb(0.30, 0.82, 0.38)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_port_color_returns_blue() {
        let color = audio_port_color();
        assert!(color.r < 0.5);
        assert!(color.g < 0.6);
        assert!(color.b > 0.9);
    }

    #[test]
    fn aux_port_color_returns_orange() {
        let color = aux_port_color();
        assert!(color.r > 0.9);
        assert!(color.g > 0.5 && color.g < 0.7);
        assert!(color.b < 0.1);
    }

    #[test]
    fn midi_port_color_returns_green() {
        let color = midi_port_color();
        assert!(color.r < 0.4);
        assert!(color.g > 0.8);
        assert!(color.b < 0.4);
    }

    #[test]
    fn colors_are_different() {
        let audio = audio_port_color();
        let aux = aux_port_color();
        let midi = midi_port_color();

        assert_ne!(audio, aux);
        assert_ne!(audio, midi);
        assert_ne!(aux, midi);
    }
}
