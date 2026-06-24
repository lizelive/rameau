//! The 16-bit mono PCM WAV [`write`] and [`save`] routines.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use rameau_clip::AudioClip;

const BITS_PER_SAMPLE: u16 = 16;
const NUM_CHANNELS: u16 = 1;
const PCM_FORMAT: u16 = 1;
const BLOCK_ALIGN: u16 = NUM_CHANNELS * (BITS_PER_SAMPLE / 8);

/// Writes `clip` as a 16-bit mono PCM WAV stream to `writer`.
///
/// The writer is not internally buffered; wrap it in a [`BufWriter`] (or use
/// [`save`], which does) if it performs syscalls per write.
pub fn write<W, C>(writer: &mut W, clip: &C) -> io::Result<()>
where
    W: Write,
    C: AudioClip<Value = i16> + ?Sized,
{
    let samples = clip.data();
    let sample_rate = clip.sample_rate();

    let data_len = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * u32::from(BLOCK_ALIGN);
    // RIFF chunk size covers everything after the first 8 bytes:
    // "WAVE" (4) + fmt chunk (8 + 16) + data chunk header (8) + data.
    let riff_len = 4 + (8 + 16) + 8 + data_len;

    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_len.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&PCM_FORMAT.to_le_bytes())?;
    writer.write_all(&NUM_CHANNELS.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&BLOCK_ALIGN.to_le_bytes())?;
    writer.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;

    writer.write_all(b"data")?;
    writer.write_all(&data_len.to_le_bytes())?;
    for &sample in samples {
        writer.write_all(&sample.to_le_bytes())?;
    }

    Ok(())
}

/// Saves `clip` to `path` as a 16-bit mono PCM WAV file.
pub fn save<C>(clip: &C, path: impl AsRef<Path>) -> io::Result<()>
where
    C: AudioClip<Value = i16> + ?Sized,
{
    let mut writer = BufWriter::new(File::create(path)?);
    write(&mut writer, clip)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_clip::Clip;

    #[test]
    fn writes_canonical_header() {
        let clip = Clip::new(vec![0i16, 1, -1], 8_000);
        let mut buf = Vec::new();
        write(&mut buf, &clip).unwrap();

        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
        assert_eq!(&buf[12..16], b"fmt ");
        // 44-byte header + 3 samples * 2 bytes.
        assert_eq!(buf.len(), 44 + 6);

        // data chunk size.
        let data_len = u32::from_le_bytes(buf[40..44].try_into().unwrap());
        assert_eq!(data_len, 6);
        // sample rate.
        let rate = u32::from_le_bytes(buf[24..28].try_into().unwrap());
        assert_eq!(rate, 8_000);
        // RIFF size = total length - 8.
        let riff = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        assert_eq!(riff as usize, buf.len() - 8);
    }

    #[test]
    fn round_trips_sample_bytes() {
        let clip = Clip::new(vec![-1i16, 256], 44_100);
        let mut buf = Vec::new();
        write(&mut buf, &clip).unwrap();
        assert_eq!(&buf[44..46], &(-1i16).to_le_bytes());
        assert_eq!(&buf[46..48], &256i16.to_le_bytes());
    }
}
