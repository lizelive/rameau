//! Encoding sample audio to the Ogg/Vorbis streams a `.sf3` bank stores.

use std::num::{NonZeroU8, NonZeroU32};

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

use crate::Error;

/// libvorbis accepts VBR quality factors in this range.
const MIN_QUALITY: f32 = -0.2;
const MAX_QUALITY: f32 = 1.0;

/// How many frames are handed to the encoder at a time.
///
/// libvorbis slows down dramatically when given very large blocks; the upstream
/// documentation suggests around 1024 frames, and the largest Vorbis analysis
/// window is 8192.
const BLOCK_FRAMES: usize = 1024;

/// Ogg/Vorbis VBR quality: `-0.2` (smallest) to `1.0` (best).
///
/// Higher values produce larger files. The default, `0.5`, is a reasonable
/// balance for instrument samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality(f32);

impl Quality {
    /// Creates a quality factor, clamping to the range libvorbis accepts.
    pub fn new(quality: f32) -> Self {
        Quality(quality.clamp(MIN_QUALITY, MAX_QUALITY))
    }

    /// The quality factor.
    pub fn get(&self) -> f32 {
        self.0
    }
}

impl Default for Quality {
    fn default() -> Self {
        Quality(0.5)
    }
}

/// Encodes one mono sample as a self-contained Ogg/Vorbis stream.
///
/// The encoder records the exact frame count in the final page's granule
/// position, so a decoder that honours it recovers precisely `pcm.len()`
/// frames. That matters because a sample's loop points are frame offsets: if
/// the length moved, the loop would move with it.
pub(crate) fn encode_sample(
    pcm: &[i16],
    sample_rate: u32,
    quality: Quality,
) -> Result<Vec<u8>, Error> {
    let rate = NonZeroU32::new(sample_rate)
        .ok_or(Error::TooLarge("sample rate of zero cannot be encoded"))?;

    let mut ogg = Vec::new();
    let mut encoder = VorbisEncoderBuilder::new(rate, NonZeroU8::new(1).unwrap(), &mut ogg)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality.get(),
        })
        .build()?;

    // An empty block signals end-of-stream to libvorbis, so a zero-length
    // sample must skip the loop entirely and go straight to `finish`, which
    // still emits a header-only, decodable stream.
    let mut block = Vec::with_capacity(BLOCK_FRAMES);
    for chunk in pcm.chunks(BLOCK_FRAMES) {
        block.clear();
        block.extend(chunk.iter().map(|&s| s as f32 / 32768.0));
        encoder.encode_audio_block([&block[..]])?;
    }

    encoder.finish()?;
    Ok(ogg)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a mono Ogg/Vorbis stream back to PCM, truncating to the granule
    /// position exactly as `rameau_soundfont`'s loader does.
    fn decode(ogg: &[u8]) -> Vec<i16> {
        let mut reader =
            lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg)).unwrap();
        let mut out = Vec::new();
        let mut frames = 0u64;
        while let Some(packet) = reader.read_dec_packet_itl().unwrap() {
            out.extend_from_slice(&packet);
            if let Some(absgp) = reader.get_last_absgp() {
                frames = absgp;
            }
        }
        out.truncate(frames as usize);
        out
    }

    fn tone(n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16)
            .collect()
    }

    /// The whole design rests on Vorbis preserving the exact frame count: loop
    /// points are frame offsets, so a length change would silently move them.
    /// The awkward cases are lengths straddling the 1024-frame block size.
    #[test]
    fn preserves_frame_count_exactly() {
        for &n in &[1usize, 7, 100, 1023, 1024, 1025, 4096, 44_100] {
            for &rate in &[8000u32, 22_050, 44_100] {
                let pcm = tone(n);
                let ogg = encode_sample(&pcm, rate, Quality::default()).unwrap();
                assert_eq!(
                    decode(&ogg).len(),
                    n,
                    "frame count changed for n={n} rate={rate}"
                );
            }
        }
    }

    #[test]
    fn round_trip_is_audibly_close() {
        let pcm = tone(44_100);
        let ogg = encode_sample(&pcm, 44_100, Quality::default()).unwrap();
        let back = decode(&ogg);
        let err: f64 = pcm
            .iter()
            .zip(&back)
            .map(|(&a, &b)| {
                let d = (a as f64 - b as f64) / 32768.0;
                d * d
            })
            .sum::<f64>()
            / pcm.len() as f64;
        assert!(err.sqrt() < 0.05, "rms error too high: {}", err.sqrt());
    }

    /// A zero-length sample must still produce a decodable stream, so that every
    /// `shdr` record points at something valid.
    #[test]
    fn encodes_empty_sample() {
        let ogg = encode_sample(&[], 44_100, Quality::default()).unwrap();
        assert!(!ogg.is_empty(), "expected at least the Vorbis headers");
        assert_eq!(decode(&ogg).len(), 0);
    }

    #[test]
    fn rejects_zero_sample_rate() {
        assert!(matches!(
            encode_sample(&[0, 1, 2], 0, Quality::default()),
            Err(Error::TooLarge(_))
        ));
    }

    #[test]
    fn quality_is_clamped_to_the_libvorbis_range() {
        assert_eq!(Quality::new(5.0).get(), 1.0);
        assert_eq!(Quality::new(-9.0).get(), -0.2);
        assert_eq!(Quality::default().get(), 0.5);
    }
}
