//! Host image loading contract (ROADMAP item 7, SPEC §10.3).
//!
//! The renderer never performs I/O. The host asks for an image with
//! `UiView::request_image`, fetches it (or refuses), and delivers bytes with
//! `UiView::deliver_image`. While no result exists the node shows the DESIGN
//! pending box; a refused or invalid result shows the alt text. The cache
//! enforces the renderer-side byte, dimension and pixel caps, and a decode
//! that panics becomes a failed image rather than a crashed frame.

use crate::theme::UiTheme;
use gpui_kit::{
    AnyElement, Image, ImageFormat, ImageSource, IntoElement, ParentElement, Styled, StyledImage,
    img,
};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

/// 8 MiB of encoded bytes per image.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// 4096 px on either side.
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
/// 16 megapixels decoded.
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
/// Bound the number of distinct image sources a single message may cache.
pub const MAX_IMAGE_CACHE_ENTRIES: usize = 256;
/// The host should give up and report `Failed` after this long (contract, not
/// enforced by the renderer, which owns no clock or I/O).
pub const IMAGE_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageError {
    TooLarge,
    TooManyPixels,
    UnsupportedFormat,
    DecodePanicked,
    TooManySources,
}

#[derive(Clone)]
pub enum ImageState {
    Pending,
    Ready(Arc<Image>),
    Failed,
}

impl std::fmt::Debug for ImageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("Pending"),
            Self::Ready(_) => f.write_str("Ready"),
            Self::Failed => f.write_str("Failed"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageLimits {
    pub max_bytes: usize,
    pub max_dimension: u32,
    pub max_pixels: u64,
    pub timeout_ms: u64,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_IMAGE_BYTES,
            max_dimension: MAX_IMAGE_DIMENSION,
            max_pixels: MAX_IMAGE_PIXELS,
            timeout_ms: IMAGE_TIMEOUT_MS,
        }
    }
}

/// The cache keyed by the program's `src` string. It never opens a URL.
#[derive(Default)]
pub struct ImageCache {
    limits: ImageLimits,
    entries: BTreeMap<String, ImageState>,
}

impl ImageCache {
    pub fn with_limits(limits: ImageLimits) -> Self {
        Self {
            limits,
            entries: BTreeMap::new(),
        }
    }

    pub fn limits(&self) -> ImageLimits {
        self.limits
    }

    pub fn state(&self, src: &str) -> ImageState {
        self.entries
            .get(src)
            .cloned()
            .unwrap_or(ImageState::Pending)
    }

    pub fn request(&mut self, src: &str) {
        if self.entries.contains_key(src) {
            return;
        }
        if self.entries.len() >= MAX_IMAGE_CACHE_ENTRIES
            && let Some(first) = self.entries.keys().next().cloned()
        {
            self.entries.remove(&first);
        }
        self.entries.insert(src.to_string(), ImageState::Pending);
    }

    pub fn fail(&mut self, src: &str) {
        self.entries.insert(src.to_string(), ImageState::Failed);
    }

    /// Validates and stores encoded bytes. No network, no file access.
    pub fn deliver(
        &mut self,
        src: &str,
        format: ImageFormat,
        bytes: Vec<u8>,
    ) -> Result<(), ImageError> {
        if bytes.len() > self.limits.max_bytes {
            self.fail(src);
            return Err(ImageError::TooLarge);
        }
        if !self.entries.contains_key(src) && self.entries.len() >= MAX_IMAGE_CACHE_ENTRIES {
            return Err(ImageError::TooManySources);
        }
        if let Some((width, height)) = dimensions(format, &bytes)
            && (width > self.limits.max_dimension
                || height > self.limits.max_dimension
                || u64::from(width).saturating_mul(u64::from(height)) > self.limits.max_pixels)
        {
            self.fail(src);
            return Err(ImageError::TooManyPixels);
        }
        let decoded = catch_unwind(AssertUnwindSafe(|| Image::from_bytes(format, bytes)));
        match decoded {
            Ok(image) => {
                self.entries
                    .insert(src.to_string(), ImageState::Ready(Arc::new(image)));
                Ok(())
            }
            Err(_) => {
                self.fail(src);
                Err(ImageError::DecodePanicked)
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// PNG, GIF and JPEG dimensions from their headers. Unknown formats return
/// `None` and are still byte-capped; the host is expected to deliver a format
/// it has validated.
pub fn dimensions(format: ImageFormat, bytes: &[u8]) -> Option<(u32, u32)> {
    match format {
        ImageFormat::Png => {
            let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
            let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
            Some((width, height))
        }
        ImageFormat::Gif => {
            let width = u32::from(u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?));
            let height = u32::from(u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?));
            Some((width, height))
        }
        ImageFormat::Jpeg => jpeg_dimensions(bytes),
        _ => None,
    }
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut index = 2usize;
    while index + 9 <= bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let height = u32::from(u16::from_be_bytes(
                bytes[index + 5..index + 7].try_into().ok()?,
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes[index + 7..index + 9].try_into().ok()?,
            ));
            return Some((width, height));
        }
        let length = u16::from_be_bytes(bytes.get(index + 2..index + 4)?.try_into().ok()?) as usize;
        index = index.saturating_add(2 + length.max(2));
    }
    None
}

/// Renders an image in the pending, failed or ready state (DESIGN §7).
pub fn element(
    theme: &UiTheme,
    alt: &str,
    height: gpui_kit::Pixels,
    width: gpui_kit::Pixels,
    fit: &str,
    state: &ImageState,
    visual: crate::layout::VisualState,
) -> AnyElement {
    let width = gpui_kit::px(width.as_f32().max(0.0));
    match state {
        ImageState::Ready(image) => {
            let fit = if fit == "contain" {
                gpui_kit::ObjectFit::Contain
            } else {
                gpui_kit::ObjectFit::Cover
            };
            img(ImageSource::Image(image.clone()))
                .object_fit(fit)
                .w(width)
                .h(height)
                .rounded(theme.radius(crate::theme::Radius::Lg))
                .into_any_element()
        }
        ImageState::Pending => {
            let _ = visual;
            boxed(theme, width, height, None)
        }
        ImageState::Failed => boxed(theme, width, height, Some(alt)),
    }
}

fn boxed(
    theme: &UiTheme,
    width: gpui_kit::Pixels,
    height: gpui_kit::Pixels,
    alt: Option<&str>,
) -> AnyElement {
    let caption = theme.text(crate::theme::TextRole::Caption);
    let mut element = gpui_kit::div()
        .flex()
        .items_center()
        .justify_center()
        .w(width)
        .h(height)
        .bg(theme.surface_subtle())
        .border_1()
        .border_color(theme.border())
        .rounded(theme.radius(crate::theme::Radius::Lg));
    if let Some(alt) = alt {
        element = element.child(
            gpui_kit::div()
                .text_size(caption.size)
                .line_height(caption.line_height)
                .text_color(theme.muted_foreground())
                .child(crate::props::sanitize_text(alt)),
        );
    }
    element.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 33];
        bytes[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn png_and_jpeg_dimensions_are_read_from_headers() {
        assert_eq!(
            dimensions(ImageFormat::Png, &png_header(640, 480)),
            Some((640, 480))
        );
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01,
            0xE0, 0x02, 0x80,
        ];
        assert_eq!(dimensions(ImageFormat::Jpeg, &jpeg), Some((640, 480)));
        assert_eq!(dimensions(ImageFormat::Png, &[0u8; 4]), None);
    }

    #[test]
    fn cache_enforces_byte_and_pixel_caps_without_panicking() {
        let mut cache = ImageCache::default();
        let too_big = vec![0u8; MAX_IMAGE_BYTES + 1];
        assert_eq!(
            cache.deliver("a", ImageFormat::Png, too_big),
            Err(ImageError::TooLarge)
        );
        assert!(matches!(cache.state("a"), ImageState::Failed));

        let huge = png_header(5000, 5000);
        assert_eq!(
            cache.deliver("b", ImageFormat::Png, huge),
            Err(ImageError::TooManyPixels)
        );
        assert!(matches!(cache.state("b"), ImageState::Failed));

        let ok = png_header(320, 200);
        assert_eq!(cache.deliver("c", ImageFormat::Png, ok), Ok(()));
        assert!(matches!(cache.state("c"), ImageState::Ready(_)));
    }

    #[test]
    fn the_source_cache_is_bounded() {
        let mut cache = ImageCache::default();
        for index in 0..MAX_IMAGE_CACHE_ENTRIES + 10 {
            cache.request(&format!("src-{index}"));
        }
        assert_eq!(cache.len(), MAX_IMAGE_CACHE_ENTRIES);
        assert_eq!(
            cache.deliver("extra", ImageFormat::Png, png_header(1, 1)),
            Err(ImageError::TooManySources)
        );
    }

    #[test]
    fn unknown_and_missing_sources_are_pending_not_fetched() {
        let cache = ImageCache::default();
        assert!(matches!(
            cache.state("https://example.com/x.png"),
            ImageState::Pending
        ));
        assert_eq!(cache.limits().max_bytes, MAX_IMAGE_BYTES);
        assert_eq!(cache.limits().timeout_ms, IMAGE_TIMEOUT_MS);
    }
}
