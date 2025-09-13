use iced::widget::{Column, button, column, text};
use maolan_engine::init;

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
    let (_client, _handle) = init();
    let _c = _client.clone();
    let result = iced::run("Maolan", Maolan::update, Maolan::view);
    _c.quit();
    let _ = _handle.join();
    result
}
