use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MidiError {
    #[error("bad value")]
    BadValue,
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid standard midi file header")]
    InvalidHeader,
    #[error("malformed variable-length quantity")]
    InvalidVarLen,
    #[error("data byte encountered before any status byte")]
    RunningStatus,
    #[error("unsupported standard midi file format {0}")]
    UnsupportedFormat(u16),
}
