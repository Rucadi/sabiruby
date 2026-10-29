//! Media decoders shared by the RGSS core and its hosts.
//!
//! The core owns decoding and timing, while a host decides how frames are
//! presented.  This keeps WGPU, software and WASM frontends on the same
//! asset semantics without making the VM depend on a window or audio stack.

pub mod gif;
pub mod movie;

/// A decoded RGBA8 frame owned by the media subsystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "media frame dimensions overflow".to_owned())?;
        if pixels.len() != expected {
            return Err(format!(
                "media frame has {} bytes, expected {expected}",
                pixels.len()
            ));
        }
        Ok(Self {
            width: width.max(1),
            height: height.max(1),
            pixels,
        })
    }
}
