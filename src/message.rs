use maolan_engine::message::Action;

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),

    Request(Action),
    Response(Action),
}
