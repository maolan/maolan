use iced::widget::{Column, button, column, text};

#[derive(Debug, Clone, Copy)]
enum Message {
    Increment,
    Decrement,
}

#[derive(Default)]
struct Maolan {
    value: i64,
}

impl Maolan {
    fn update(&mut self, message: Message) {
        match message {
            Message::Increment => {
                self.value += 1;
                println!("{}", self.value);
            }
            Message::Decrement => {
                self.value -= 1;
                println!("{}", self.value);
            }
        }
    }

    fn view(&self) -> Column<'_, Message> {
        column![
            button("+").on_press(Message::Increment),
            text(self.value),
            button("-").on_press(Message::Decrement),
        ]
    }
}

fn main() -> iced::Result {
    iced::run("Maolan", Maolan::update, Maolan::view)
}
