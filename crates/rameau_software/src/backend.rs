//! The [`Software`] sample-mixing [`AudioPlayback`] backend.

use std::sync::Arc;

use rameau_clip::{AudioClip, Clip};
use rameau_playback::{AudioPlayback, LoopRegion, PlaybackError, Timestamp, VoiceParams};

use crate::voice::Voice;

/// Default attack time applied when a voice starts, in seconds.
const DEFAULT_ATTACK: f32 = 0.002;
/// Default release time applied when a voice is stopped, in seconds.
const DEFAULT_RELEASE: f32 = 0.08;

/// A handle to one voice started on a [`Software`] backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    id: u64,
}

/// A software sample-mixing [`AudioPlayback`] backend.
pub struct Software {
    sample_rate: u32,
    attack: f32,
    release: f32,
    /// Absolute frame clock, advanced by each [`render`](Self::render).
    clock: u64,
    next_id: u64,
    voices: Vec<Voice>,
}

impl Software {
    /// Creates a backend that renders interleaved stereo at `sample_rate` Hz.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            attack: DEFAULT_ATTACK,
            release: DEFAULT_RELEASE,
            clock: 0,
            next_id: 0,
            voices: Vec::new(),
        }
    }

    /// Overrides the default attack/release times, in seconds.
    pub fn with_envelope(mut self, attack: f32, release: f32) -> Self {
        self.attack = attack;
        self.release = release;
        self
    }

    /// The render sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of voices currently allocated (sounding or pending).
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Resolves a [`Timestamp`] to an absolute frame on the backend clock.
    fn frame_of(&self, when: Timestamp) -> u64 {
        match when {
            Timestamp::Now => self.clock,
            Timestamp::AtSeconds(s) => (s.max(0.0) * self.sample_rate as f64) as u64,
        }
    }

    fn voice_mut(&mut self, handle: &Handle) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| v.id == handle.id)
    }
}

impl AudioPlayback for Software {
    type Clip = Arc<Clip<i16>>;
    type Playback = Handle;

    fn clip_from_pcm(
        &mut self,
        samples: &[i16],
        sample_rate: u32,
    ) -> Result<Self::Clip, PlaybackError> {
        Ok(Arc::new(Clip::new(samples.to_vec(), sample_rate)))
    }

    fn clip_from_vorbis(&mut self, _ogg: &[u8]) -> Result<Self::Clip, PlaybackError> {
        // No Vorbis decoder here; callers (e.g. the SoundFont loader) fall back
        // to decoding to PCM themselves.
        Err(PlaybackError::Unsupported("Software::clip_from_vorbis"))
    }

    fn start(
        &mut self,
        when: Timestamp,
        clip: &Self::Clip,
        params: VoiceParams,
        loop_region: Option<LoopRegion>,
    ) -> Result<Self::Playback, PlaybackError> {
        let id = self.next_id;
        self.next_id += 1;
        let start_frame = self.frame_of(when);
        let pan = params.position.x.clamp(-1.0, 1.0);
        let voice = Voice::new(
            id,
            Arc::clone(clip),
            self.sample_rate,
            start_frame,
            params.pitch,
            params.volume,
            pan,
            loop_region,
            self.attack,
            self.release,
        );
        self.voices.push(voice);
        Ok(Handle { id })
    }

    fn update(
        &mut self,
        _when: Timestamp,
        playback: &mut Self::Playback,
        params: VoiceParams,
    ) -> Result<(), PlaybackError> {
        // Parameter changes apply immediately (scheduling is best-effort here).
        if let Some(v) = self.voice_mut(playback) {
            v.update(params.pitch, params.volume, params.position.x.clamp(-1.0, 1.0));
        }
        Ok(())
    }

    fn stop(
        &mut self,
        when: Timestamp,
        playback: &mut Self::Playback,
    ) -> Result<(), PlaybackError> {
        let frame = self.frame_of(when);
        if let Some(v) = self.voice_mut(playback) {
            v.schedule_release(frame);
        }
        Ok(())
    }

    fn render(&mut self, clip: &mut dyn AudioClip<Value = f32>) -> Result<(), PlaybackError> {
        let buf = clip.data_mut();
        buf.fill(0.0);
        let frames = buf.len() / 2;
        if frames == 0 {
            return Ok(());
        }
        let block_start = self.clock;
        for voice in &mut self.voices {
            voice.render(buf, block_start, 1.0, 1.0);
        }
        self.clock += frames as u64;
        self.voices.retain(|v| !v.is_finished());
        Ok(())
    }
}

impl Default for Software {
    fn default() -> Self {
        Self::new(44_100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rameau_playback::Vec3;

    fn sine_clip(rate: u32, n: usize) -> Arc<Clip<i16>> {
        let data: Vec<i16> = (0..n)
            .map(|i| ((i as f32 * 0.2).sin() * 10_000.0) as i16)
            .collect();
        Arc::new(Clip::new(data, rate))
    }

    fn rms(buf: &[f32]) -> f32 {
        let sum: f32 = buf.iter().map(|s| s * s).sum();
        (sum / buf.len() as f32).sqrt()
    }

    #[test]
    fn silent_without_voices() {
        let mut sw = Software::new(44_100);
        let mut buf = Clip::new(vec![0.0f32; 256 * 2], 44_100);
        sw.render(&mut buf).unwrap();
        assert_eq!(rms(&buf.data), 0.0);
    }

    #[test]
    fn started_voice_produces_sound() {
        let mut sw = Software::new(44_100);
        let clip = sine_clip(44_100, 200);
        let params = VoiceParams {
            pitch: 0.0,
            volume: 1.0,
            position: Vec3::pan(0.0),
            velocity: Vec3::default(),
        };
        let loops = Some(10u32..190);
        let _h = sw.start(Timestamp::Now, &clip, params, loops).unwrap();
        let mut buf = Clip::new(vec![0.0f32; 512 * 2], 44_100);
        sw.render(&mut buf).unwrap();
        assert!(rms(&buf.data) > 0.0, "a held looping voice should sound");
    }

    #[test]
    fn stop_releases_the_voice() {
        let mut sw = Software::new(8_000).with_envelope(0.0, 0.005);
        let clip = sine_clip(8_000, 200);
        let mut h = sw
            .start(Timestamp::Now, &clip, VoiceParams::default(), Some(10u32..190))
            .unwrap();
        let mut buf = Clip::new(vec![0.0f32; 64 * 2], 8_000);
        sw.render(&mut buf).unwrap();
        assert_eq!(sw.active_voices(), 1);
        sw.stop(Timestamp::Now, &mut h).unwrap();
        for _ in 0..50 {
            sw.render(&mut buf).unwrap();
        }
        assert_eq!(sw.active_voices(), 0, "voice should free after release");
    }

    #[test]
    fn scheduled_start_delays_onset() {
        let mut sw = Software::new(1_000);
        let clip = sine_clip(1_000, 4_000);
        // Start one second in.
        let _h = sw
            .start(
                Timestamp::AtSeconds(1.0),
                &clip,
                VoiceParams::default(),
                None,
            )
            .unwrap();
        let mut buf = Clip::new(vec![0.0f32; 1_000 * 2], 1_000);
        // First second: silent (voice not audible yet).
        sw.render(&mut buf).unwrap();
        assert_eq!(rms(&buf.data), 0.0, "no audio before the scheduled start");
        // Second second: audible.
        sw.render(&mut buf).unwrap();
        assert!(rms(&buf.data) > 0.0, "audio after the scheduled start");
    }
}
