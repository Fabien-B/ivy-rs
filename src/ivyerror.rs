use std::io;

#[derive(Debug)]
pub enum IvyError {
    BadDomain,
    IoError(io::Error),
    ParseFail,
}


impl From<io::Error> for IvyError {
    fn from(e: io::Error) -> Self {
        IvyError::IoError(e)
    }
}
