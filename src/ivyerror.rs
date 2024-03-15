use std::{io, num::ParseIntError};

#[derive(Debug)]
pub enum IvyError {
    BadInit,
    BadDomain,
    IoError(io::Error),
    ParseError(ParseIntError),
    ParseAnnounceError,
    ParseFail,
    PingTimeout,
    DeadLock,
}


impl From<io::Error> for IvyError {
    fn from(e: io::Error) -> Self {
        IvyError::IoError(e)
    }
}


impl From<ParseIntError> for IvyError {
    fn from(e: ParseIntError) -> Self {
        IvyError::ParseError(e)
    }
}