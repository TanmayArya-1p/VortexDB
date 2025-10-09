#[derive(Debug)]
pub enum DbError {
    ParseError,
    StorageError(String),
    SerializationError(String),
    DeserializationError,
    IndexError(String),
    LockError,
}
