//! GIF87a/GIF89a decoding with RGSS-compatible frame compositing.
//!
//! GIF frames are often partial rectangles.  A renderer that uploads each
//! rectangle as a complete bitmap will leave stale pixels behind, so this
//! module composites every frame onto the logical screen and honours the
//! disposal method, including `Previous`.  Delays are returned in
//! milliseconds; zero-centisecond delays are clamped to one centisecond so a
//! frame is still observable.

use super::Frame;
use gif::{ColorOutput, DecodeOptions, DisposalMethod};
use std::io::Cursor;

/// A fully composited GIF animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Frame>,
    pub delays_ms: Vec<u64>,
}

/// Returns whether `bytes` begin with a GIF87a/GIF89a signature.
pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

/// Decode and composite all frames in a GIF.
pub fn decode(bytes: &[u8]) -> Result<Animation, String> {
    if !is_gif(bytes) {
        return Err("media is not a GIF".to_owned());
    }
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|error| error.to_string())?;
    let width = u32::from(decoder.width()).max(1);
    let height = u32::from(decoder.height()).max(1);
    let canvas_len = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "GIF dimensions overflow".to_owned())?;
    let (w, h) = (width as usize, height as usize);
    let mut canvas = vec![0; canvas_len];
    let mut frames = Vec::new();
    let mut delays_ms = Vec::new();
    let mut previous = None;

    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|error| error.to_string())?
    {
        let disposal = frame.dispose;
        if disposal == DisposalMethod::Previous {
            previous = Some(canvas.clone());
        }
        let frame_width = usize::from(frame.width);
        let frame_height = usize::from(frame.height);
        let left = usize::from(frame.left);
        let top = usize::from(frame.top);
        let buffer = frame.buffer.as_ref();
        for y in 0..frame_height {
            let Some(destination_y) = top.checked_add(y) else {
                break;
            };
            if destination_y >= h {
                break;
            }
            for x in 0..frame_width {
                let Some(destination_x) = left.checked_add(x) else {
                    break;
                };
                if destination_x >= w {
                    break;
                }
                let source = (y * frame_width + x) * 4;
                let alpha = buffer.get(source + 3).copied().unwrap_or(0);
                if alpha == 0 {
                    continue;
                }
                let destination = (destination_y * w + destination_x) * 4;
                canvas[destination..destination + 4].copy_from_slice(&buffer[source..source + 4]);
            }
        }
        frames.push(Frame::new(width, height, canvas.clone())?);
        delays_ms.push(u64::from(frame.delay.max(1)) * 10);

        match disposal {
            DisposalMethod::Background => {
                for y in 0..frame_height {
                    let Some(destination_y) = top.checked_add(y) else {
                        break;
                    };
                    if destination_y >= h {
                        break;
                    }
                    for x in 0..frame_width {
                        let Some(destination_x) = left.checked_add(x) else {
                            break;
                        };
                        if destination_x >= w {
                            break;
                        }
                        let destination = (destination_y * w + destination_x) * 4;
                        canvas[destination..destination + 4].fill(0);
                    }
                }
            }
            DisposalMethod::Previous => {
                if let Some(restored) = previous.take() {
                    canvas = restored;
                }
            }
            _ => {}
        }
    }

    if frames.is_empty() {
        return Err("GIF contains no frames".to_owned());
    }
    Ok(Animation {
        width,
        height,
        frames,
        delays_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_gif_bytes_without_panicking() {
        assert!(decode(b"not a gif").is_err());
    }

    #[test]
    fn decodes_a_minimal_gif_to_rgba() {
        let bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        let animation = decode(bytes).expect("bundled GIF should decode");
        assert!(animation.width > 0);
        assert!(animation.height > 0);
        assert!(!animation.frames.is_empty());
        assert_eq!(animation.frames.len(), animation.delays_ms.len());
        assert!(animation
            .frames
            .iter()
            .all(|frame| frame.pixels.len() == frame.width as usize * frame.height as usize * 4));
    }
}
