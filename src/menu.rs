use crate::message::{Message, Show};
use engine::message::Action;
use iced::{
    Border, Color, Element, Length, alignment,
    widget::{button, row, text},
};
use iced_aw::{
    menu::{DrawPath, Item, Menu as IcedMenu},
    menu_bar, menu_items,
};
use iced_fonts::lucide::chevron_right;
use maolan_engine as engine;

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

fn menu_button(
    label: &str,
    width: Option<Length>,
    height: Option<Length>,
    msg: Message,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    base_button(
        text(label)
            .height(height.unwrap_or(Length::Shrink))
            .align_y(alignment::Vertical::Center),
        msg,
    )
    .width(width.unwrap_or(Length::Shrink))
    .height(height.unwrap_or(Length::Shrink))
    .into()
}

fn menu_dropdown(
    label: &str,
    message: Message,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Shrink), Some(Length::Shrink), message)
}

fn menu_item(label: &str, message: Message) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    menu_button(label, Some(Length::Fill), Some(Length::Shrink), message)
}

#[allow(dead_code)]
fn submenu(label: &str, msg: Message) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    base_button(
        row![
            text(label)
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
            chevron_right(),
        ]
        .align_y(iced::Alignment::Center),
        msg,
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .into()
}

#[derive(Default)]
pub struct Menu {
    available_templates: Vec<String>,
}

impl Menu {
    pub fn update(&mut self, _message: Message) {}

    pub fn update_templates(&mut self, templates: Vec<String>) {
        self.available_templates = templates;
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let menu_tpl = |items| IcedMenu::new(items).width(180.0).offset(15.0).spacing(5.0);

        // Build the "New" submenu dynamically from stored templates
        let mut new_menu_items: Vec<Item<'_, Message, _, _>> =
            vec![Item::new(menu_item("Empty", Message::NewSession))];
        for template in &self.available_templates {
            new_menu_items.push(Item::new(menu_item(
                template.as_str(),
                Message::NewFromTemplate(template.clone()),
            )));
        }
        let new_submenu = IcedMenu::new(new_menu_items)
            .width(180.0)
            .offset(15.0)
            .spacing(5.0);

        #[rustfmt::skip]
        let mb = menu_bar!(
            (menu_dropdown("File", Message::None), {
                menu_tpl(menu_items!(
                    (submenu("New", Message::None), new_submenu),
                    (menu_item("Open", Message::Show(Show::Open))),
                    (menu_item("Save", Message::Show(Show::Save))),
                    (menu_item("Save as", Message::Show(Show::SaveAs))),
                    (menu_item("Save as template", Message::Show(Show::SaveTemplateAs))),
                    (menu_item("Import", Message::OpenFileImporter)),
                    (menu_item("Export", Message::OpenExporter)),
                    // (submenu("Open Recent", Message::None), menu_tpl(menu_items!(
                    //     (menu_item("First", Message::None)),
                    //     (menu_item("Second", Message::None)),
                    // ))),
                    (menu_item("Quit", Message::Request(Action::Quit))),
                ))
            }),
            (menu_dropdown("Edit", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("Undo (Ctrl+Z)", Message::Undo)),
                    (menu_item("Redo (Ctrl+Shift+Z)", Message::Redo)),
                ))
            }),
            (menu_dropdown("Track", Message::None), {
                menu_tpl(menu_items!(
                    (menu_item("New", Message::Show(Show::AddTrack))),
                ))
            }),
        )
        .draw_path(DrawPath::Backdrop)
        .close_on_item_click_global(true)
        .width(Length::Fill);
        mb.into()
    }
}
