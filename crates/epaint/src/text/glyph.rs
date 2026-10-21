use emath::Vec2;

/// Glyph geometry and its location in font texture storage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GlyphGeometry {
    /// X/Y offset for nice rendering (unit: points).
    pub offset: Vec2,

    /// Screen size (in points) of this glyph.
    /// Note that the height is different from the font height.
    pub size: Vec2,

    /// Bitmap texels or a vector-band record.
    pub location: GlyphLocation,
}

impl GlyphGeometry {
    pub fn is_nothing(&self) -> bool {
        match self.location {
            GlyphLocation::None => true,
            GlyphLocation::Bitmap { min, max } => min == max,
            GlyphLocation::Vector { .. } => false,
        }
    }

    pub fn is_vector(&self) -> bool {
        matches!(self.location, GlyphLocation::Vector { .. })
    }
}

/// The texture data used to draw a glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum GlyphLocation {
    /// No visible geometry, for example a space.
    #[default]
    None,

    /// A rectangle in the bitmap atlas, in texels, with an exclusive maximum.
    Bitmap { min: [u16; 2], max: [u16; 2] },

    /// The origin of a glyph record in the vector band texture, in texels.
    Vector { origin: [u16; 2] },
}

/// The stored geometry and color mode of a glyph.
///
/// The default value is the invisible, zero-size glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphAllocation {
    /// Placement and texture data for drawing.
    pub geometry: GlyphGeometry,

    /// A color glyph (e.g. emoji), stored with its own colors.
    /// Do not tint it with the text color.
    pub is_color: bool,
}

/// An outline glyph positioned for one [`super::font_face::ShapedGlyph`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OutlineGlyph {
    pub allocation: GlyphAllocation,

    /// Left edge of the glyph's pixel grid, in physical pixels.
    ///
    /// Bitmap glyphs use the pixel or subpixel bin selected during rasterization.
    /// Vector glyphs retain fractional placement in [`GlyphGeometry::offset`].
    pub x_px: i32,
}

/// An allocated glyph with an explicit horizontal advance.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphWithAdvance {
    pub allocation: GlyphAllocation,
    pub advance_px: f32,
}
