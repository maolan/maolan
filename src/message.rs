#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),
    Echo(String),
    Error(String),
}
