//! The 16-bit mono PCM WAV [`write`] and [`save`] routines.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use rameau_clip::AudioClip;

const BITS_PER_SAMPLE: u16 = 16;
const NUM_CHANNELS: u16 = 1;
const PCM_FORMAT: u16 = 1;
const BLOCK_ALIGN: u16 = NUM_CHANNELS * (BITS_PER_SAMPLE / 8);

/// Writes interleaved 16-bit PCM with `channels` channels to `writer`.
///
/// `samples` holds frames of `channels` interleaved values (`L, R, L, R, …`
/// for stereo).
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] if `channels` is zero, if
/// `samples` does not divide into whole frames, or if the audio is too
/// large for RIFF's 32-bit size fields — all of which would otherwise
/// produce a file whose header disagrees with its contents. Otherwise
/// returns any [`io::Error`] produced while writing to `writer`.
pub fn write_interleaved<W: Write>(
    writer: &mut W,
    samples: &[i16],
    channels: u16,
    sample_rate: u32,
) -> io::Result<()> {
    let invalid = |msg: &'static str| io::Error::new(io::ErrorKind::InvalidInput, msg);
    if channels == 0 {
        return Err(invalid("a WAV stream needs at least one channel"));
    }
    if !samples.len().is_multiple_of(usize::from(channels)) {
        return Err(invalid("the samples do not divide into whole frames"));
    }
    let block_align = channels * (BITS_PER_SAMPLE / 8);
    // RIFF sizes are 32-bit, and the header carries the data length plus the
    // 36 bytes before it; a buffer that overflows either would be written
    // with a truncated size and read back as a different, shorter file.
    let data_len: u32 = samples
        .len()
        .checked_mul(2)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| n.checked_add(36).is_some())
        .ok_or_else(|| invalid("the audio is too large for a 32-bit RIFF size field"))?;
    let byte_rate = sample_rate.saturating_mul(u32::from(block_align));

    writer.write_all(b"RIFF")?;
    writer.write_all(&(36 + data_len).to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&PCM_FORMAT.to_le_bytes())?;
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        writer.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Saves interleaved stereo 16-bit PCM to `path`.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] if `samples` holds an odd number
/// of values (a half frame) or is too large for RIFF, and otherwise any
/// [`io::Error`] from creating or writing the file.
pub fn save_stereo(samples: &[i16], sample_rate: u32, path: impl AsRef<Path>) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    write_interleaved(&mut writer, samples, 2, sample_rate)?;
    writer.flush()
}

/// Writes `clip` as a 16-bit mono PCM WAV stream to `writer`.
///
/// The writer is not internally buffered; wrap it in a [`BufWriter`] (or use
/// [`save`], which does) if it performs syscalls per write.
///
/// # Errors
///
/// Returns any [`io::Error`] produced while writing to `writer`.
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
///
/// # Errors
///
/// Returns any [`io::Error`] from creating `path` or writing to it.
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

#[cfg(test)]
mod interleaved_tests {
    use super::*;

    #[test]
    fn rejects_input_a_header_could_not_describe() {
        let mut out = Vec::new();
        assert_eq!(
            write_interleaved(&mut out, &[0i16, 0], 0, 44_100).unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "zero channels"
        );
        assert_eq!(
            write_interleaved(&mut out, &[0i16, 0, 0], 2, 44_100).unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "a trailing half frame"
        );
        assert!(out.is_empty(), "nothing is written for rejected input");
        assert!(write_interleaved(&mut out, &[0i16; 8], 2, 44_100).is_ok());
    }

    #[test]
    fn header_describes_the_payload() {
        let samples = [1i16, -1, 2, -2, 3, -3];
        let mut out = Vec::new();
        write_interleaved(&mut out, &samples, 2, 48_000).unwrap();
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        let riff_len = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
        let data_len = u32::from_le_bytes([out[40], out[41], out[42], out[43]]);
        assert_eq!(data_len as usize, samples.len() * 2);
        assert_eq!(riff_len, data_len + 36);
        assert_eq!(out.len(), 44 + samples.len() * 2, "header plus payload");
        assert_eq!(u16::from_le_bytes([out[22], out[23]]), 2, "channels");
        assert_eq!(u16::from_le_bytes([out[32], out[33]]), 4, "block align");
        assert_eq!(
            u32::from_le_bytes([out[28], out[29], out[30], out[31]]),
            48_000 * 4,
            "byte rate"
        );
    }
}
