use iced::advanced::text::Renderer as TextRenderer;
use iced::widget::{Text, text};
use iced::{Font, Theme};

pub const LUCIDE_METRONOME_FONT_BYTES: &[u8] =
    include_bytes!("../../assets/fonts/lucide-metronome.ttf");
pub const LUCIDE_METRONOME_FONT: Font = Font::with_name("lucide-metronome");

#[must_use]
pub fn metronome<'a, Renderer>() -> Text<'a, Theme, Renderer>
where
    Renderer: TextRenderer<Font = Font>,
{
    text('\u{E6CD}')
        .font(LUCIDE_METRONOME_FONT)
        .shaping(text::Shaping::Basic)
}
