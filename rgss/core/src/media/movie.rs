//! Pure-Rust Ogg/Theora playback for RGSS movies.
//!
//! The decoder is intentionally host-neutral.  `Movie::frame_at` advances by
//! presentation time and returns an RGBA8 [`super::Frame`]; a windowed host
//! can upload that frame while a headless or WASM host can consume it through
//! the same API.  Ogg pages may contain several video packets, so packet
//! timestamps are assigned from the page granule position instead of treating
//! every page as one frame.

use super::Frame;
use ogg::reading::PacketReader;
use std::{
    collections::VecDeque,
    io::{BufReader, Cursor},
};
use theora_rs::{
    DecoderContext, HeaderParser, PixelFmt, YCbCrBuffer, th_decode_packetin, th_decode_ycbcr_out,
    th_granule_time,
};

pub struct Movie {
    reader: PacketReader<BufReader<Cursor<Vec<u8>>>>,
    parser: HeaderParser,
    decoder: Option<DecoderContext>,
    serial: Option<u32>,
    packet_index: i64,
    packets: VecDeque<(theora_rs::OggPacket, f64)>,
    next_time: f64,
    pending: Option<(f64, Frame)>,
    current: Option<Frame>,
    revision: u64,
    finished: bool,
}

impl Movie {
    /// Open an Ogg/Theora stream and parse its headers.
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let mut movie = Self {
            reader: PacketReader::new(BufReader::new(Cursor::new(bytes))),
            parser: HeaderParser::new(),
            decoder: None,
            serial: None,
            packet_index: 0,
            packets: VecDeque::new(),
            next_time: 0.0,
            pending: None,
            current: None,
            revision: 0,
            finished: false,
        };
        loop {
            let Some(packet) = movie.next_theora_packet()? else {
                return Err("movie ended before its Theora headers".to_owned());
            };
            movie
                .parser
                .push(&packet)
                .map_err(|error| error.to_string())?;
            if movie.parser.is_ready() {
                break;
            }
        }
        movie.decoder = Some(movie.parser.decoder().map_err(|error| error.to_string())?);
        Ok(movie)
    }

    pub fn width(&self) -> u32 {
        self.parser.info.pic_width.max(1)
    }

    pub fn height(&self) -> u32 {
        self.parser.info.pic_height.max(1)
    }

    /// True once the stream has ended and no decoded frame remains pending.
    pub fn finished(&self) -> bool {
        self.finished && self.packets.is_empty() && self.pending.is_none()
    }

    /// Monotonically increasing frame revision.  Hosts can use it to avoid
    /// uploading the same decoded frame more than once.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the frame visible at `elapsed` seconds, decoding ahead only as
    /// far as necessary.  `None` means that no displayable frame was decoded.
    pub fn frame_at(&mut self, elapsed: f64) -> Result<Option<&Frame>, String> {
        loop {
            if self.pending.is_none() {
                while self.pending.is_none() && (!self.finished || !self.packets.is_empty()) {
                    if !self.decode_next()? {
                        break;
                    }
                }
                if self.pending.is_none() {
                    break;
                }
            }
            let due = self.current.is_none()
                || matches!(&self.pending, Some((time, _)) if *time <= elapsed);
            if !due {
                break;
            }
            if let Some((_, frame)) = self.pending.take() {
                self.current = Some(frame);
                self.revision = self.revision.saturating_add(1);
            }
        }
        Ok(self.current.as_ref())
    }

    fn next_theora_packet(&mut self) -> Result<Option<theora_rs::OggPacket>, String> {
        loop {
            let packet = match self.reader.read_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    self.finished = true;
                    return Ok(None);
                }
                Err(error) => return Err(error.to_string()),
            };
            if self.serial.is_none() {
                let data = &packet.data;
                let is_theora = packet.first_in_stream()
                    && data.len() >= 7
                    && data[0] == 0x80
                    && &data[1..7] == b"theora";
                if !is_theora {
                    continue;
                }
                self.serial = Some(packet.stream_serial());
            }
            if Some(packet.stream_serial()) != self.serial {
                continue;
            }
            let packet = theora_rs::OggPacket {
                packet: packet.data.clone(),
                b_o_s: packet.first_in_stream(),
                e_o_s: packet.last_in_stream(),
                // A page granule belongs to its final packet only.
                granulepos: if packet.last_in_page() {
                    packet.absgp_page() as i64
                } else {
                    -1
                },
                packetno: self.packet_index,
            };
            self.packet_index += 1;
            return Ok(Some(packet));
        }
    }

    fn decode_next(&mut self) -> Result<bool, String> {
        if self.packets.is_empty() {
            let mut page = Vec::new();
            let mut end_time = None;
            while let Some(packet) = self.next_theora_packet()? {
                let granule = packet.granulepos;
                page.push(packet);
                if granule >= 0 {
                    end_time = Some(th_granule_time(&self.parser.info, granule));
                    break;
                }
            }
            let info = &self.parser.info;
            let duration = info.fps_denominator as f64 / info.fps_numerator.max(1) as f64;
            // The page granule marks the end of its final frame.  Work
            // backwards so packets on one page retain distinct timestamps.
            let start = end_time.map_or(self.next_time, |end| end - page.len() as f64 * duration);
            for (index, packet) in page.into_iter().enumerate() {
                let time = (start + index as f64 * duration).max(0.0);
                self.packets.push_back((packet, time));
                self.next_time = time + duration;
            }
        }
        let Some((packet, time)) = self.packets.pop_front() else {
            return Ok(false);
        };
        let Some(decoder) = self.decoder.as_mut() else {
            return Ok(false);
        };
        th_decode_packetin(decoder, &packet).map_err(|error| error.to_string())?;
        let frame = match th_decode_ycbcr_out(decoder) {
            Ok(frame) => frame,
            // Header/control packets do not necessarily produce a picture.
            Err(_) => return Ok(true),
        };
        self.pending = Some((time, ycbcr_to_frame(&frame, &self.parser.info)?));
        Ok(true)
    }
}

fn chroma_decimation(format: PixelFmt) -> (usize, usize) {
    match format {
        PixelFmt::Pf444 => (0, 0),
        PixelFmt::Pf422 => (1, 0),
        PixelFmt::Pf420 | PixelFmt::Reserved => (1, 1),
    }
}

fn sample_plane(plane: &theora_rs::ImgPlane, x: usize, y: usize) -> u8 {
    let width = plane.width.max(1) as usize;
    if x >= width {
        return 0;
    }
    let index = plane.data_offset as isize + y as isize * plane.stride as isize + x as isize;
    if index < 0 {
        return 0;
    }
    plane.data.get(index as usize).copied().unwrap_or(0)
}

fn clamp_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

/// Convert the visible Theora YCbCr picture to tightly packed RGBA8.
pub fn ycbcr_to_frame(frame: &YCbCrBuffer, info: &theora_rs::Info) -> Result<Frame, String> {
    let width = info.pic_width.max(1);
    let height = info.pic_height.max(1);
    let (horizontal_decimation, vertical_decimation) = chroma_decimation(info.pixel_fmt);
    let crop_x = info.pic_x as usize;
    let crop_y = info.pic_y as usize;
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Theora picture dimensions overflow".to_owned())?;
    let mut pixels = vec![0; pixel_count];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let yy = i32::from(sample_plane(&frame[0], crop_x + x, crop_y + y));
            let cb = i32::from(sample_plane(
                &frame[1],
                (crop_x + x) >> horizontal_decimation,
                (crop_y + y) >> vertical_decimation,
            ));
            let cr = i32::from(sample_plane(
                &frame[2],
                (crop_x + x) >> horizontal_decimation,
                (crop_y + y) >> vertical_decimation,
            ));
            let red = clamp_channel((1_904_000 * yy + 2_609_823 * cr - 363_703_744) / 1_635_200);
            let green = clamp_channel(
                (3_827_562 * yy - 1_287_801 * cb - 2_672_387 * cr + 447_306_710) / 3_287_200,
            );
            let blue = clamp_channel((952_000 * yy + 1_649_289 * cb - 225_932_192) / 817_600);
            let offset = (y * width as usize + x) * 4;
            pixels[offset..offset + 4].copy_from_slice(&[red, green, blue, 255]);
        }
    }
    Frame::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ogg_theora_bytes_without_panicking() {
        assert!(Movie::open(vec![0, 1, 2, 3]).is_err());
    }
}
