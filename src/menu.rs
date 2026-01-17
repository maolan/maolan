use crate::message::Message;
use iced::widget::{button, container, row, text, tooltip};
use iced::{Border, Color, Element, Length, alignment};
use iced_aw::menu::{DrawPath, Menu};
use iced_aw::{menu_bar, menu_items};

use iced_aw::iced_aw_font;

fn base_button<'a>(
    content: impl Into<Element<'a, Message>>,
    msg: Message,
) -> button::Button<'a, Message> {
    button(content)
        .padding([4, 8])
        .style(|theme, status| {
            use button::{Status, Style};

            let palette = theme.extended_palette();
            let base = Style {
                text_color: palette.background.base.text,
                border: Border::default().rounded(6.0),
                ..Style::default()
            };
            match status {
                Status::Active => base.with_background(Color::TRANSPARENT),
                Status::Hovered => base.with_background(Color::from_rgb(
                    palette.primary.weak.color.r * 1.2,
                    palette.primary.weak.color.g * 1.2,
                    palette.primary.weak.color.b * 1.2,
                )),
                Status::Disabled => base.with_background(Color::from_rgb(0.5, 0.5, 0.5)),
                Status::Pressed => base.with_background(palette.primary.weak.color),
            }
        })
        .on_press(msg)
}

fn build_tooltip<'a>(
    _label: String,
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    tooltip(
        content,
        container(text("").color(Color::TRANSPARENT))
            .style(|theme| container::bordered_box(theme).background(Color::TRANSPARENT)),
        tooltip::Position::Bottom,
    )
    .into()
}

fn tooltip_button<'a>(
    label: String,
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    width: Option<Length>,
    height: Option<Length>,
    msg: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    build_tooltip(
        label,
        base_button(content, msg)
            .width(width.unwrap_or(Length::Shrink))
            .height(height.unwrap_or(Length::Shrink)),
    )
}

fn menu_button(
    label: &str,
    width: Option<Length>,
    height: Option<Length>,
    message: Message,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    tooltip_button(
        label.to_string(),
        text(label)
            .height(height.unwrap_or(Length::Shrink))
            .align_y(alignment::Vertical::Center),
        width,
        height,
        message,
    )
}

fn menu_button_s(label: &str, message: Message) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Shrink), Some(Length::Shrink), message)
}

fn menu_button_f(label: &str, message: Message) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Fill), Some(Length::Shrink), message)
}

fn submenu_button(label: &str, message: Message) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    tooltip_button(
        label.to_string(),
        row![
            text(label)
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
            iced_aw_font::right_open()
                .width(Length::Shrink)
                .align_y(alignment::Vertical::Center),
        ]
        .align_y(iced::Alignment::Center),
        Some(Length::Fill),
        None,
        message,
    )
}

#[derive(Default)]
pub struct MaolanMenu {}

impl MaolanMenu {
    pub fn update(&mut self, message: Message) {
        match message {
            _ => {},
        }
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let menu_tpl = |items| Menu::new(items).width(180.0).offset(15.0).spacing(5.0);

        #[rustfmt::skip]
        let mb = menu_bar!(
            (menu_button_s("File", Message::Debug("File".to_string())), {
                menu_tpl(menu_items!(
                    (menu_button_f("New", Message::Echo("New".to_string()))),
                    (menu_button_f("Open", Message::Debug("Open".to_string()))),
                    (submenu_button("Open Recent", Message::Debug("Open Recent".to_string())), menu_tpl(menu_items!(
                        (menu_button_f("First", Message::Debug("First".to_string()))),
                        (menu_button_f("Second", Message::Debug("Second".to_string()))),
                    ))),
                    (menu_button_f("Close", Message::Debug("Close".to_string()))),
                    (menu_button_f("Quit", Message::Debug("Quit".to_string()))),
                ))
            }),
        )
        .draw_path(DrawPath::Backdrop)
        .close_on_item_click_global(true)
        .width(Length::Fill);

        mb.into()
    }
}
