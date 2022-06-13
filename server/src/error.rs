//! Error handling

use std::{result, str, string};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("UTF-8 encoding error")]
    Utf8,
}

pub type Result<T, E = Error> = result::Result<T, E>;

impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Self {
        Error::Utf8
    }
}

impl From<string::FromUtf8Error> for Error {
    fn from(_: string::FromUtf8Error) -> Self {
        Error::Utf8
    }
}

impl From<http::header::ToStrError> for Error {
    fn from(_: http::header::ToStrError) -> Self {
        Error::Utf8
    }
}
