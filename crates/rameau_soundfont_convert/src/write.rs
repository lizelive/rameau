use std::io::Write;
use std::path::Path;

use rameau_soundfont::SoundFont;

use crate::Error;

pub fn write_sf3<W: Write>(_sf: &SoundFont, _quality: crate::Quality, _out: W) -> Result<(), Error> {
    todo!()
}

pub fn save_sf3(
    _sf: &SoundFont,
    _quality: crate::Quality,
    _path: impl AsRef<Path>,
) -> Result<(), Error> {
    todo!()
}

pub fn convert_file(
    _input: impl AsRef<Path>,
    _output: impl AsRef<Path>,
    _quality: crate::Quality,
) -> Result<(), Error> {
    todo!()
}
