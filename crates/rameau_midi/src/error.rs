use thiserror::Error;

#[derive(Error, Debug)]
pub enum MidiError {
    #[error("bad value")]
    BadValue,
}
