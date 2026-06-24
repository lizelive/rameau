//! A real-time [`AudioPlayback`] backend built on [`kira`].
//!
//! [`Kira`] owns a [`kira::AudioManager`] and plays each voice as a
//! [`StaticSoundData`]: kira does the resampling, pitch-shifting, mixing and
//! device output. Starting a voice returns a [`StaticSoundHandle`] that
//! [`update`](Kira::update) and [`stop`](Kira::stop) drive with tweens.
//!
//! # Capability mapping
//!
//! - `pitch` → playback rate via [`Semitones`].
//! - `volume` (linear) → [`Decibels`].
//! - `position` → stereo [`Panning`] (`position.x`); full 3D spatialization and
//!   `velocity`/Doppler are **degraded** away, not errored.
//! - offline [`render`](Kira::render) is real-time only → [`PlaybackError::Unsupported`].

use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kira::backend::DefaultBackend;
use kira::sound::PlaybackPosition;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::track::MainTrackBuilder;
use kira::{
    AudioManager, AudioManagerSettings, Decibels, Frame, Panning, Semitones, StartTime, Tween,
};

use rameau_clip::AudioClip;
use rameau_playback::{AudioPlayback, LoopRegion, PlaybackError, Timestamp, VoiceParams};

/// A real-time sample-playback backend using `kira`.
pub struct Kira {
    manager: AudioManager<DefaultBackend>,
    /// When this backend's timeline began, for [`Timestamp::AtSeconds`].
    started: Instant,
    /// Default tween used for parameter updates.
    update_tween: Tween,
    /// Fade-out applied when a voice is stopped.
    release_tween: Tween,
}

impl Kira {
    /// Opens the default audio device and returns a ready backend.
    ///
    /// The main track is given a generous voice capacity so that dense MIDI
    /// (many overlapping notes, plus release tails) does not overflow it.
    pub fn new() -> Result<Self, PlaybackError> {
        let settings = AudioManagerSettings {
            main_track_builder: MainTrackBuilder::new().sound_capacity(1024),
            ..Default::default()
        };
        let manager = AudioManager::<DefaultBackend>::new(settings)
            .map_err(|e| PlaybackError::Backend(e.to_string()))?;
        Ok(Self {
            manager,
            started: Instant::now(),
            update_tween: Tween {
                duration: Duration::from_millis(5),
                ..Default::default()
            },
            release_tween: Tween {
                duration: Duration::from_millis(80),
                ..Default::default()
            },
        })
    }

    /// Sets the fade-out applied to stopped voices.
    pub fn with_release(mut self, release: Duration) -> Self {
        self.release_tween.duration = release;
        self
    }

    /// Direct access to the underlying `kira` manager.
    pub fn manager(&mut self) -> &mut AudioManager<DefaultBackend> {
        &mut self.manager
    }

    fn start_time(&self, when: Timestamp) -> StartTime {
        match when {
            Timestamp::Now => StartTime::Immediate,
            Timestamp::AtSeconds(s) => {
                let elapsed = self.started.elapsed().as_secs_f64();
                let delay = (s - elapsed).max(0.0);
                if delay == 0.0 {
                    StartTime::Immediate
                } else {
                    StartTime::Delayed(Duration::from_secs_f64(delay))
                }
            }
        }
    }
}

/// Linear amplitude (`0.0..`) to decibels, with a silence floor.
fn linear_to_db(amplitude: f32) -> Decibels {
    if amplitude <= 1e-4 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amplitude.log10())
    }
}

impl AudioPlayback for Kira {
    type Clip = StaticSoundData;
    type Playback = StaticSoundHandle;

    fn clip_from_pcm(
        &mut self,
        samples: &[i16],
        sample_rate: u32,
    ) -> Result<Self::Clip, PlaybackError> {
        let frames: Arc<[Frame]> = samples
            .iter()
            .map(|&s| Frame::from_mono(s as f32 / 32768.0))
            .collect();
        Ok(StaticSoundData {
            sample_rate,
            frames,
            settings: StaticSoundSettings::default(),
            slice: None,
        })
    }

    fn clip_from_vorbis(&mut self, ogg: &[u8]) -> Result<Self::Clip, PlaybackError> {
        StaticSoundData::from_cursor(Cursor::new(ogg.to_vec()))
            .map_err(|e| PlaybackError::Decode(e.to_string()))
    }

    fn start(
        &mut self,
        when: Timestamp,
        clip: &Self::Clip,
        params: VoiceParams,
        loop_region: Option<LoopRegion>,
    ) -> Result<Self::Playback, PlaybackError> {
        let pan = params.position.x.clamp(-1.0, 1.0);
        let mut sound = clip
            .volume(linear_to_db(params.volume))
            .playback_rate(Semitones(params.pitch as f64))
            .panning(Panning(pan))
            .start_time(self.start_time(when));
        if let Some(r) = loop_region {
            sound = sound.loop_region(
                PlaybackPosition::Samples(r.start as usize)
                    ..PlaybackPosition::Samples(r.end as usize),
            );
        }
        self.manager
            .play(sound)
            .map_err(|e| PlaybackError::Backend(e.to_string()))
    }

    fn update(
        &mut self,
        _when: Timestamp,
        playback: &mut Self::Playback,
        params: VoiceParams,
    ) -> Result<(), PlaybackError> {
        let pan = params.position.x.clamp(-1.0, 1.0);
        playback.set_volume(linear_to_db(params.volume), self.update_tween);
        playback.set_playback_rate(Semitones(params.pitch as f64), self.update_tween);
        playback.set_panning(Panning(pan), self.update_tween);
        Ok(())
    }

    fn stop(
        &mut self,
        _when: Timestamp,
        playback: &mut Self::Playback,
    ) -> Result<(), PlaybackError> {
        playback.stop(self.release_tween);
        Ok(())
    }

    fn render(&mut self, _clip: &mut dyn AudioClip<Value = f32>) -> Result<(), PlaybackError> {
        Err(PlaybackError::Unsupported(
            "Kira is real-time only; use a software backend for offline render",
        ))
    }
}
