#![expect(unsafe_code)]

use glow::HasContext as _;

pub(crate) struct FontTextures {
    fallback_textures: [glow::Texture; 2],
}

impl FontTextures {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let curves = gl.create_texture()?;
            let bands = match gl.create_texture() {
                Ok(texture) => texture,
                Err(error) => {
                    gl.delete_texture(curves);
                    return Err(error);
                }
            };
            let result = Self {
                fallback_textures: [curves, bands],
            };
            gl.active_texture(glow::TEXTURE0);
            for (texture, (internal, format, kind)) in core::iter::zip(
                result.fallback_textures,
                [
                    (glow::RGBA32F, glow::RGBA, glow::FLOAT),
                    (glow::RGBA32UI, glow::RGBA_INTEGER, glow::UNSIGNED_INT),
                ],
            ) {
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    internal as i32,
                    1,
                    1,
                    0,
                    format,
                    kind,
                    glow::PixelUnpackData::Slice(None),
                );
                for (parameter, value) in [
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
                }
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(result)
        }
    }

    pub unsafe fn bind(
        &self,
        gl: &glow::Context,
        textures: &std::collections::HashMap<egui::TextureId, glow::Texture>,
    ) {
        unsafe {
            for (index, id) in [egui::TextureId::FontCurves, egui::TextureId::FontBands]
                .into_iter()
                .enumerate()
            {
                let unit = [glow::TEXTURE1, glow::TEXTURE2][index];
                let texture = textures
                    .get(&id)
                    .copied()
                    .unwrap_or(self.fallback_textures[index]);
                gl.active_texture(unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            }
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            for texture in self.fallback_textures {
                gl.delete_texture(texture);
            }
        }
    }
}
