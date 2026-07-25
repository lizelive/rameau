/// Ogg/Vorbis VBR quality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality(f32);

impl Default for Quality {
    fn default() -> Self {
        Quality(0.5)
    }
}
