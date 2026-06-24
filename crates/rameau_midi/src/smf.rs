//! Standard MIDI File (SMF) parsing.
//!
//! [`Smf::parse`] turns the raw bytes of a `.mid`/`.midi` file into a
//! structured [`Smf`]: a header (format + time division) and a list of
//! [`Track`]s. Each track is a flat list of [`TrackEvent`]s carrying their
//! original delta-times (in ticks) alongside the decoded
//! [`MidiEvent`]/[`MetaEvent`]/system-exclusive payload.
//!
//! Running status, variable-length quantities, meta events and system
//! exclusive blocks are all handled. Channel messages are mapped onto the
//! crate's [`MidiEvent`] enum.

use crate::error::MidiError;
use crate::event::MidiEvent;
use crate::program::MidiProgram;

/// A parsed Standard MIDI File.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smf {
    /// The arrangement of the tracks (header `format` field).
    pub format: Format,
    /// How delta-times in the tracks should be interpreted.
    pub division: Division,
    /// The track chunks, in file order.
    pub tracks: Vec<Track>,
}

/// SMF format, from the header `format` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A single multi-channel track (format 0).
    Single,
    /// One or more tracks played simultaneously (format 1).
    Parallel,
    /// One or more independent single-track patterns (format 2).
    Sequential,
}

impl Format {
    fn from_u16(value: u16) -> Result<Self, MidiError> {
        match value {
            0 => Ok(Format::Single),
            1 => Ok(Format::Parallel),
            2 => Ok(Format::Sequential),
            other => Err(MidiError::UnsupportedFormat(other)),
        }
    }
}

/// Time division from the header: how a tick relates to wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    /// Metrical timing: ticks per quarter note.
    TicksPerQuarter(u16),
    /// SMPTE timing: frames per second and ticks per frame.
    Smpte {
        /// Frames per second (commonly 24, 25, 29 or 30).
        fps: u8,
        /// Subdivisions of a frame.
        ticks_per_frame: u8,
    },
}

impl Division {
    fn from_raw(raw: u16) -> Self {
        if raw & 0x8000 != 0 {
            // Top bit set: SMPTE. The high byte is a negative frame count.
            let fps = (-((raw >> 8) as i8 as i16)) as u8;
            let ticks_per_frame = (raw & 0x00ff) as u8;
            Division::Smpte {
                fps,
                ticks_per_frame,
            }
        } else {
            Division::TicksPerQuarter(raw)
        }
    }
}

/// A single track: a flat, ordered list of timed events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// The events of the track, in file order.
    pub events: Vec<TrackEvent>,
}

impl Track {
    /// Iterate the track's [`MidiEvent`]s tagged with their absolute tick
    /// (summing delta-times), skipping meta and system-exclusive events.
    ///
    /// This is the shape consumed by `rameau_synthesizer` once ticks are
    /// converted to sample timestamps.
    pub fn midi_events(&self) -> impl Iterator<Item = (u64, MidiEvent)> + '_ {
        let mut tick = 0u64;
        self.events.iter().filter_map(move |ev| {
            tick += u64::from(ev.delta);
            match ev.kind {
                TrackEventKind::Midi(m) => Some((tick, m)),
                _ => None,
            }
        })
    }
}

/// A single event within a track, with its delta-time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackEvent {
    /// Delta-time in ticks since the previous event in this track.
    pub delta: u32,
    /// The decoded event.
    pub kind: TrackEventKind,
}

/// The payload of a [`TrackEvent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackEventKind {
    /// A channel voice/mode message.
    Midi(MidiEvent),
    /// A meta event (tempo, track name, end-of-track, …).
    Meta(MetaEvent),
    /// System-exclusive data, without the leading `F0`/`F7` or length.
    SysEx(Vec<u8>),
}

/// A meta event (`FF` in a track).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaEvent {
    /// Free text (meta type `0x01`).
    Text(String),
    /// Track or sequence name (meta type `0x03`).
    TrackName(String),
    /// Tempo, in microseconds per quarter note (meta type `0x51`).
    Tempo(u32),
    /// Time signature (meta type `0x58`).
    TimeSignature {
        /// Beats per bar.
        numerator: u8,
        /// Note value of one beat, as the actual denominator (e.g. `8`, not 3).
        denominator: u8,
        /// MIDI clocks per metronome click.
        clocks_per_click: u8,
        /// Number of notated 32nd-notes per quarter note.
        thirty_seconds_per_quarter: u8,
    },
    /// Key signature (meta type `0x59`).
    KeySignature {
        /// Number of sharps (positive) or flats (negative).
        sharps: i8,
        /// `true` for a minor key, `false` for major.
        minor: bool,
    },
    /// End of track (meta type `0x2F`).
    EndOfTrack,
    /// Any other meta event: its type byte and raw data.
    Other {
        /// Meta type byte.
        meta_type: u8,
        /// Raw, undecoded payload.
        data: Vec<u8>,
    },
}

impl Smf {
    /// Parse the bytes of a Standard MIDI File.
    pub fn parse(bytes: &[u8]) -> Result<Smf, MidiError> {
        let mut r = Reader::new(bytes);

        // --- Header chunk (MThd) ---
        if &r.tag()? != b"MThd" {
            return Err(MidiError::InvalidHeader);
        }
        let header_len = r.u32()?;
        if header_len < 6 {
            return Err(MidiError::InvalidHeader);
        }
        let format = Format::from_u16(r.u16()?)?;
        let ntracks = r.u16()?;
        let division = Division::from_raw(r.u16()?);
        // Skip any vendor-specific trailing header bytes.
        if header_len > 6 {
            r.bytes(header_len as usize - 6)?;
        }

        // --- Track chunks (MTrk) ---
        let mut tracks = Vec::with_capacity(ntracks as usize);
        for _ in 0..ntracks {
            let tag = r.tag()?;
            let len = r.u32()? as usize;
            let chunk = r.bytes(len)?;
            // Skip any non-MTrk chunk (the spec allows alien chunk types).
            if &tag == b"MTrk" {
                tracks.push(parse_track(chunk)?);
            }
        }

        Ok(Smf {
            format,
            division,
            tracks,
        })
    }
}

/// Parse one MTrk chunk body into a [`Track`].
fn parse_track(data: &[u8]) -> Result<Track, MidiError> {
    let mut r = Reader::new(data);
    let mut events = Vec::new();
    let mut running_status: Option<u8> = None;

    while !r.is_empty() {
        let delta = r.vlq()?;
        let byte = r.u8()?;

        let kind = match byte {
            0xFF => {
                // Meta event. Clears running status.
                running_status = None;
                TrackEventKind::Meta(parse_meta(&mut r)?)
            }
            0xF0 | 0xF7 => {
                // System exclusive. Clears running status.
                running_status = None;
                let len = r.vlq()? as usize;
                TrackEventKind::SysEx(r.bytes(len)?.to_vec())
            }
            status if status & 0x80 != 0 => {
                // A fresh status byte for a channel message.
                running_status = Some(status);
                TrackEventKind::Midi(parse_channel(&mut r, status, None)?)
            }
            data => {
                // No status byte: reuse the running status, `data` is the
                // first data byte of the message.
                let status = running_status.ok_or(MidiError::RunningStatus)?;
                TrackEventKind::Midi(parse_channel(&mut r, status, Some(data))?)
            }
        };

        events.push(TrackEvent { delta, kind });
    }

    Ok(Track { events })
}

/// Decode a channel message whose `status` byte is known. `first` is the
/// already-consumed first data byte when running status is in effect.
fn parse_channel(
    r: &mut Reader,
    status: u8,
    first: Option<u8>,
) -> Result<MidiEvent, MidiError> {
    let channel = status & 0x0f;
    // Read the first data byte (or take the one already consumed).
    let d1 = match first {
        Some(b) => b,
        None => r.u8()?,
    };

    let event = match status & 0xf0 {
        0x80 => {
            let _vel = r.u8()?; // off-velocity is not represented
            MidiEvent::NoteOff { channel, key: d1 }
        }
        0x90 => {
            let vel = r.u8()?;
            MidiEvent::NoteOn {
                channel,
                key: d1,
                vel,
            }
        }
        0xA0 => {
            let value = r.u8()?;
            MidiEvent::PolyphonicKeyPressure {
                channel,
                key: d1,
                value,
            }
        }
        0xB0 => {
            let value = r.u8()?;
            // Channel-mode messages share the control-change status.
            match d1 {
                120 => MidiEvent::AllSoundOff { channel },
                123 => MidiEvent::AllNotesOff { channel },
                ctrl => MidiEvent::ControlChange {
                    channel,
                    ctrl,
                    value,
                },
            }
        }
        0xC0 => MidiEvent::ProgramChange {
            channel,
            program: MidiProgram::from(d1),
        },
        0xD0 => MidiEvent::ChannelPressure { channel, value: d1 },
        0xE0 => {
            let msb = r.u8()?;
            MidiEvent::PitchBend {
                channel,
                value: u16::from(d1) | (u16::from(msb) << 7),
            }
        }
        _ => return Err(MidiError::BadValue),
    };

    Ok(event)
}

/// Decode a meta event body (the bytes after the `FF` status).
fn parse_meta(r: &mut Reader) -> Result<MetaEvent, MidiError> {
    let meta_type = r.u8()?;
    let len = r.vlq()? as usize;
    let data = r.bytes(len)?;

    let meta = match meta_type {
        0x01 => MetaEvent::Text(String::from_utf8_lossy(data).into_owned()),
        0x03 => MetaEvent::TrackName(String::from_utf8_lossy(data).into_owned()),
        0x2F => MetaEvent::EndOfTrack,
        0x51 if data.len() == 3 => {
            MetaEvent::Tempo(u32::from_be_bytes([0, data[0], data[1], data[2]]))
        }
        0x58 if data.len() == 4 => MetaEvent::TimeSignature {
            numerator: data[0],
            denominator: 1u8.checked_shl(u32::from(data[1])).unwrap_or(0),
            clocks_per_click: data[2],
            thirty_seconds_per_quarter: data[3],
        },
        0x59 if data.len() == 2 => MetaEvent::KeySignature {
            sharps: data[0] as i8,
            minor: data[1] != 0,
        },
        _ => MetaEvent::Other {
            meta_type,
            data: data.to_vec(),
        },
    };

    Ok(meta)
}

/// A forward-only byte cursor over a slice, with big-endian and
/// variable-length-quantity helpers.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn u8(&mut self) -> Result<u8, MidiError> {
        let b = *self.data.get(self.pos).ok_or(MidiError::UnexpectedEof)?;
        self.pos += 1;
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, MidiError> {
        Ok(u16::from_be_bytes([self.u8()?, self.u8()?]))
    }

    fn u32(&mut self) -> Result<u32, MidiError> {
        Ok(u32::from_be_bytes([
            self.u8()?,
            self.u8()?,
            self.u8()?,
            self.u8()?,
        ]))
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], MidiError> {
        if self.remaining() < n {
            return Err(MidiError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn tag(&mut self) -> Result<[u8; 4], MidiError> {
        let b = self.bytes(4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    /// Read a MIDI variable-length quantity (7 bits per byte, MSB continues).
    fn vlq(&mut self) -> Result<u32, MidiError> {
        let mut value = 0u32;
        // A VLQ is at most 4 bytes in a valid SMF.
        for _ in 0..4 {
            let b = self.u8()?;
            value = (value << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(MidiError::InvalidVarLen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_format0_file() {
        // MThd: format 0, 1 track, 96 ticks/quarter.
        let mut bytes = vec![
            b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 0, 0, 1, 0, 96,
        ];
        // One track: note on, note off (running status), end of track.
        let track = vec![
            0x00, 0x90, 0x3c, 0x40, // dt 0: note on C4 vel 64
            0x60, 0x3c, 0x00, // dt 96: running status -> note on C4 vel 0
            0x00, 0xFF, 0x2F, 0x00, // dt 0: end of track
        ];
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&track);

        let smf = Smf::parse(&bytes).unwrap();
        assert_eq!(smf.format, Format::Single);
        assert_eq!(smf.division, Division::TicksPerQuarter(96));
        assert_eq!(smf.tracks.len(), 1);

        let events: Vec<_> = smf.tracks[0].midi_events().collect();
        assert_eq!(
            events,
            vec![
                (0, MidiEvent::NoteOn { channel: 0, key: 60, vel: 64 }),
                (96, MidiEvent::NoteOn { channel: 0, key: 60, vel: 0 }),
            ]
        );
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(
            Smf::parse(b"not a midi file at all"),
            Err(MidiError::InvalidHeader)
        ));
    }

    #[test]
    fn vlq_decodes_multibyte() {
        // 0x81 0x00 == 128
        let mut r = Reader::new(&[0x81, 0x00]);
        assert_eq!(r.vlq().unwrap(), 128);
    }
}
