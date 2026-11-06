use crate::frame::{Frame, PixelLayout};
use eframe::{
    egui,
    glow::{self, HasContext},
};

pub struct Texture {
    pub id: egui::TextureId,
    native: glow::NativeTexture,
    size: [usize; 2],
    layout: PixelLayout,
}

pub fn upload(
    texture: Option<Texture>,
    image: &Frame,
    frame: &mut eframe::Frame,
) -> Result<Texture, String> {
    let gl = frame.gl().cloned().ok_or("OpenGL context unavailable")?;
    let mut texture = match texture {
        Some(texture) => texture,
        None => {
            // App::update runs with this window's GL context current.
            let native = unsafe { gl.create_texture()? };
            let id = frame.register_native_glow_texture(native);
            Texture {
                id,
                native,
                size: [0, 0],
                layout: image.layout,
            }
        }
    };
    let (internal, format) = match image.layout {
        PixelLayout::Rgb => (glow::SRGB8, glow::RGB),
        PixelLayout::Rgba => (glow::SRGB8_ALPHA8, glow::RGBA),
    };
    // Validated packed pixels are uploaded directly; egui owns and frees the texture.
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(texture.native));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        if texture.size != image.size || texture.layout != image.layout {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal as i32,
                image.size[0] as i32,
                image.size[1] as i32,
                0,
                format,
                glow::UNSIGNED_BYTE,
                None,
            );
            texture.size = image.size;
            texture.layout = image.layout;
        }
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            image.size[0] as i32,
            image.size[1] as i32,
            format,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(&image.pixels),
        );
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.bind_texture(glow::TEXTURE_2D, None);
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(format!(
                "Android GPU texture upload failed: GL {error:#x}"
            ));
        }
    }
    Ok(texture)
}
