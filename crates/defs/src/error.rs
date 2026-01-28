use std::io;

use crate::{Dimension, PointId};
#[derive(Debug, PartialEq, Eq)]
pub enum DbError {
    ParseError,
    StorageError(String),
    SerializationError(String),
    DeserializationError,
    IndexError(String),
    LockError,
    IndexInitError, //TODO: Change this
    UnsupportedSimilarity,
    DimensionMismatch,
    SnapshotError(String),
    StorageInitializationError,
    StorageCheckpointError(String),
    InvalidMagicBytes(String),
    VectorNotFound(uuid::Uuid),
    SnapshotRegistryError(String),
    StorageEngineError(String),
    InvalidDimension { expected: Dimension, got: Dimension },
    PointAlreadyExists { id: PointId },
    PointNotFound { id: PointId },
}

#[derive(Debug)]
pub enum ServerError {
    Bind(io::Error),
    Serve(io::Error),
}

#[derive(Debug)]
pub enum AppError {
    ServerError(ServerError),
}

// Error type for server
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
