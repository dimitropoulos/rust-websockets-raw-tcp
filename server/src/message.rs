#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close(Option<CloseFrame<'static>>),
    Frame(Frame),
}
