//! CPU framebuffer used by the software renderer.

use std::io::{BufWriter, Write};
use std::path::Path;

/// An RGBA colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// An opaque RGB colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// An RGBA colour.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Scale the colour channels for flat shading, clamping to 0..=255.
    pub(crate) fn shade(self, factor: f64) -> Self {
        let channel = |value: u8| (value as f64 * factor).clamp(0.0, 255.0).round() as u8;
        Self::rgba(channel(self.r), channel(self.g), channel(self.b), self.a)
    }
}

/// A tightly packed RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    /// Allocate an opaque black framebuffer.
    ///
    /// Returns `None` when either dimension is zero, the pixel count overflows
    /// `usize`, or the image would exceed 16 million pixels.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let pixels = pixel_count(width, height)?;
        Some(Self { width, height, pixels: vec![0; pixels * 4] })
    }

    /// Image width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Tightly packed RGBA8 rows, top to bottom.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Read one pixel. Out-of-range coordinates return `None`.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = self.index(x, y);
        Some(Color::rgba(
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ))
    }

    /// Fill the whole image with `colour`.
    pub fn clear(&mut self, colour: Color) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[colour.r, colour.g, colour.b, colour.a]);
        }
    }

    /// Write the image as a binary PPM (P6) file.
    pub fn save_ppm(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "P6")?;
        writeln!(writer, "{} {}", self.width, self.height)?;
        writeln!(writer, "255")?;
        for pixel in self.pixels.chunks_exact(4) {
            writer.write_all(&pixel[..3])?;
        }
        writer.flush()
    }

    pub(crate) fn index(&self, x: u32, y: u32) -> usize {
        ((y as usize) * (self.width as usize) + x as usize) * 4
    }

    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, colour: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = self.index(x, y);
        self.pixels[index..index + 4].copy_from_slice(&[colour.r, colour.g, colour.b, colour.a]);
    }
}

pub(crate) fn pixel_count(width: u32, height: u32) -> Option<usize> {
    const MAX_PIXELS: usize = 16 * 1024 * 1024;
    let count = (width as usize).checked_mul(height as usize)?;
    if count == 0 || count > MAX_PIXELS { None } else { Some(count) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_and_oversized_images() {
        assert!(Framebuffer::new(0, 1).is_none());
        assert!(Framebuffer::new(1, 0).is_none());
        assert!(Framebuffer::new(4096, 4096).is_some());
        assert!(Framebuffer::new(4097, 4097).is_none());
    }

    #[test]
    fn pixels_round_trip_and_out_of_range_reads_return_none() {
        let mut image = Framebuffer::new(2, 2).expect("small framebuffer");
        assert_eq!(image.pixel(0, 0), Some(Color::rgba(0, 0, 0, 0)));
        image.set_pixel(1, 1, Color::rgb(1, 2, 3));
        assert_eq!(image.pixel(1, 1), Some(Color::rgb(1, 2, 3)));
        assert_eq!(image.pixels().len(), 16);
        assert_eq!(image.pixel(2, 1), None);
        assert_eq!(image.pixel(1, 2), None);
    }

    #[test]
    fn save_ppm_writes_a_binary_header_and_rgb_rows() {
        let mut image = Framebuffer::new(1, 1).expect("framebuffer");
        image.clear(Color::rgb(10, 20, 30));
        let path = std::env::temp_dir().join(format!(
            "kaicad-renderer-ppm-{}-{:?}.ppm",
            std::process::id(),
            std::thread::current().id()
        ));
        image.save_ppm(&path).expect("ppm should save");
        let bytes = std::fs::read(&path).expect("ppm should exist");
        let _ = std::fs::remove_file(&path);
        assert!(bytes.starts_with(b"P6\n1 1\n255\n"));
        assert_eq!(&bytes[bytes.len() - 3..], &[10, 20, 30]);
    }

    #[test]
    fn shade_clamps_and_scales_channels() {
        assert_eq!(Color::rgb(100, 200, 50).shade(0.5), Color::rgb(50, 100, 25));
        assert_eq!(Color::rgb(100, 200, 50).shade(10.0), Color::rgb(255, 255, 255));
        assert_eq!(Color::rgb(100, 200, 50).shade(-1.0), Color::rgb(0, 0, 0));
    }
}
