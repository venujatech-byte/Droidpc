use std::io::{self, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelLayout {
    Rgb,
    Rgba,
}

pub struct Frame {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
    pub layout: PixelLayout,
}

// The fourth word is a snapshot color-space ID (0..=2) or an RGB row stride (>=3).
pub fn read_any_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut prefix = [0_u8; 16];
    reader.read_exact(&mut prefix)?;
    let snapshot = u32::from_le_bytes(prefix[12..16].try_into().unwrap()) <= 2;
    let mut prefixed = io::Cursor::new(prefix).chain(reader);
    if snapshot {
        read_frame(&mut prefixed)
    } else {
        read_display_frame(&mut prefixed)
    }
}

// Android 13 raw capture: width, height, pixel format, color space, packed rows.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut header = [0_u8; 16];
    reader.read_exact(&mut header)?;
    let word = |offset| {
        u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap())
    };
    let (width, height, format, color_space) =
        (word(0) as usize, word(4) as usize, word(8), word(12));
    let bpp = match format {
        1 | 2 | 5 => 4,
        3 => 3,
        4 => 2,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported Android pixel format {format}"),
            ));
        }
    };
    let pixels = checked_pixels(width, height)?;
    // shortcut: the desktop texture is sRGB; color-managed P3 conversion is a future renderer change.
    if color_space > 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported raw capture header",
        ));
    }
    let mut raw = vec![0; pixels * bpp];
    reader.read_exact(&mut raw)?;
    let rgba = if format == 1 {
        raw
    } else {
        let mut rgba = Vec::with_capacity(pixels * 4);
        for p in raw.chunks_exact(bpp) {
            let color = match format {
                2 => [p[0], p[1], p[2], 255],
                3 => [p[0], p[1], p[2], 255],
                5 => [p[2], p[1], p[0], p[3]],
                4 => {
                    let v = u16::from_le_bytes([p[0], p[1]]);
                    [
                        (((v >> 11) & 31) * 255 / 31) as u8,
                        (((v >> 5) & 63) * 255 / 63) as u8,
                        ((v & 31) * 255 / 31) as u8,
                        255,
                    ]
                }
                _ => unreachable!(),
            };
            rgba.extend_from_slice(&color);
        }
        rgba
    };
    Ok(Frame {
        size: [width, height],
        pixels: rgba,
        layout: PixelLayout::Rgba,
    })
}

fn checked_pixels(width: usize, height: usize) -> io::Result<usize> {
    width
        .checked_mul(height)
        .filter(|&n| {
            n > 0 && n <= 16_777_216 && width <= 8192 && height <= 8192
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Android frame dimensions",
            )
        })
}

// Android FrameOutput packets contain length, width, height, stride and RGB888.
pub fn read_display_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut header = [0_u8; 20];
    reader.read_exact(&mut header)?;
    let word = |offset| {
        u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap())
            as usize
    };
    let (length, width, height, stride, format) =
        (word(0), word(4), word(8), word(12), word(16));
    let bytes = checked_pixels(width, height)? * 3;
    if format != 3 || stride != width * 3 || length != 16 + bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Android display-stream packet",
        ));
    }
    let mut pixels = vec![0; bytes];
    reader.read_exact(&mut pixels)?;
    Ok(Frame {
        size: [width, height],
        pixels,
        layout: PixelLayout::Rgb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn capture(w: u32, h: u32, format: u32, data: &[u8]) -> Vec<u8> {
        [
            w.to_le_bytes(),
            h.to_le_bytes(),
            format.to_le_bytes(),
            1_u32.to_le_bytes(),
        ]
        .concat()
        .into_iter()
        .chain(data.iter().copied())
        .collect()
    }

    #[test]
    fn reads_consecutive_frames_and_converts_pixel_formats() {
        let mut stream = Cursor::new(
            [
                capture(1, 1, 1, &[10, 20, 30, 255]),
                capture(1, 1, 5, &[3, 2, 1, 255]),
                capture(1, 1, 4, &[0, 248]),
            ]
            .concat(),
        );
        assert_eq!(read_frame(&mut stream).unwrap().pixels, [10, 20, 30, 255]);
        assert_eq!(read_frame(&mut stream).unwrap().pixels, [1, 2, 3, 255]);
        assert_eq!(read_frame(&mut stream).unwrap().pixels, [255, 0, 0, 255]);
    }

    #[test]
    fn rejects_oversized_unknown_and_truncated_frames() {
        for data in [
            capture(u32::MAX, 2, 1, &[]),
            capture(0, 1, 1, &[]),
            capture(1, 1, 100, &[]),
            capture(1, 1, 1, &[0]),
        ] {
            assert!(read_frame(&mut Cursor::new(data)).is_err());
        }
    }

    fn display_packet(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        [
            (16 + data.len() as u32).to_le_bytes(),
            width.to_le_bytes(),
            height.to_le_bytes(),
            (width * 3).to_le_bytes(),
            3_u32.to_le_bytes(),
        ]
        .concat()
        .into_iter()
        .chain(data.iter().copied())
        .collect()
    }

    #[test]
    fn display_packets_keep_rgb_bytes_without_cpu_expansion() {
        let packet = display_packet(1, 1, &[10, 20, 30]);
        let mut stream = Cursor::new([packet.clone(), packet].concat());
        for _ in 0..2 {
            let frame = read_display_frame(&mut stream).unwrap();
            assert_eq!(frame.size, [1, 1]);
            assert_eq!(frame.layout, PixelLayout::Rgb);
            assert_eq!(frame.pixels, [10, 20, 30]);
        }
    }

    #[test]
    fn display_packets_reject_invalid_lengths_strides_and_truncation() {
        let valid = display_packet(1, 1, &[10, 20, 30]);
        for offset in [0, 4, 8, 12, 16] {
            let mut invalid = valid.clone();
            invalid[offset..offset + 4]
                .copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(read_display_frame(&mut Cursor::new(invalid)).is_err());
        }
        assert!(read_display_frame(&mut Cursor::new(&valid[..21])).is_err());
    }
    #[test]
    fn transport_can_fall_back_between_complete_packets() {
        let data = [
            display_packet(1, 1, &[1, 2, 3]),
            capture(1, 1, 1, &[4, 5, 6, 255]),
        ]
        .concat();
        let mut stream = Cursor::new(data);
        assert_eq!(
            read_any_frame(&mut stream).unwrap().layout,
            PixelLayout::Rgb
        );
        assert_eq!(read_any_frame(&mut stream).unwrap().pixels, [4, 5, 6, 255]);
    }
}
