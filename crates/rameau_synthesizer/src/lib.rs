pub trait Synthesizer {
    const MAX_POLYPHONY: u8 = 32;
    const FRAME_SIZE: u16 = 128;
}
