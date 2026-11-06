use std::io::{self, Read};

pub struct Frame {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
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
    let pixels = width
        .checked_mul(height)
        .filter(|&n| n > 0 && n <= 16_777_216);
    let Some(pixels) = pixels.filter(|_| width <= 8192 && height <= 8192)
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Android frame dimensions",
        ));
    };
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
        rgba,
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
        assert_eq!(read_frame(&mut stream).unwrap().rgba, [10, 20, 30, 255]);
        assert_eq!(read_frame(&mut stream).unwrap().rgba, [1, 2, 3, 255]);
        assert_eq!(read_frame(&mut stream).unwrap().rgba, [255, 0, 0, 255]);
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
}
