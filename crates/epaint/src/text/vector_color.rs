//! Collect COLR outline layers before encoding them for the Vector shaders.

use skrifa::{
    GlyphId, MetadataProvider as _, color,
    instance::{LocationRef, Size},
};
use vello_cpu::kurbo::{self, Affine};

use super::{
    font_face::VelloPen,
    vector::{ColorStop, GradientGeometry, Layer, Paint},
};
use crate::Color32;

pub(super) fn layers(
    font: &skrifa::FontRef<'_>,
    glyph: GlyphId,
    location: LocationRef<'_>,
) -> Option<Vec<Layer>> {
    let glyph = font.color_glyphs().get(glyph)?;
    let palettes = font.color_palettes();
    let palette = palettes.get(0)?;
    let mut painter = Painter {
        font,
        location,
        palette: palette.colors(),
        transform: Affine::scale_non_uniform(1.0, -1.0),
        transforms: Vec::new(),
        clips: Vec::new(),
        layers: Vec::new(),
        supported: true,
        group_depth: 0,
    };
    glyph.paint(location, &mut painter).ok()?;
    painter.supported.then_some(painter.layers)
}

struct Painter<'a, 'font> {
    font: &'a skrifa::FontRef<'font>,
    location: LocationRef<'a>,
    palette: &'a [color::Color],
    transform: Affine,
    transforms: Vec<Affine>,
    clips: Vec<kurbo::BezPath>,
    layers: Vec<Layer>,
    supported: bool,
    group_depth: usize,
}

impl color::ColorPainter for Painter<'_, '_> {
    fn push_transform(&mut self, t: color::Transform) {
        self.transforms.push(self.transform);
        self.transform *= Affine::new([
            f64::from(t.xx),
            f64::from(t.yx),
            f64::from(t.xy),
            f64::from(t.yy),
            f64::from(t.dx),
            f64::from(t.dy),
        ]);
    }

    fn pop_transform(&mut self) {
        if let Some(transform) = self.transforms.pop() {
            self.transform = transform;
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let mut path = kurbo::BezPath::new();
        let mut pen = VelloPen {
            path: &mut path,
            x_offset: 0.0,
        };
        let drawn = self
            .font
            .outline_glyphs()
            .get(glyph_id)
            .and_then(|glyph| {
                glyph
                    .draw(
                        skrifa::outline::DrawSettings::unhinted(Size::unscaled(), self.location),
                        &mut pen,
                    )
                    .ok()
            })
            .is_some();
        self.supported &= drawn;
        // VelloPen flips Y; undo that before applying the COLR transform.
        path.apply_affine(self.transform * Affine::scale_non_uniform(1.0, -1.0));
        self.clips.push(path);
    }

    fn push_clip_box(&mut self, clip_box: skrifa::raw::types::BoundingBox<f32>) {
        use kurbo::Shape as _;
        let rect = kurbo::Rect::new(
            f64::from(clip_box.x_min),
            f64::from(clip_box.y_min),
            f64::from(clip_box.x_max),
            f64::from(clip_box.y_max),
        );
        let mut path = rect.to_path(0.0);
        path.apply_affine(self.transform);
        self.clips.push(path);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: color::Brush<'_>) {
        if self.clips.is_empty() {
            self.supported = false;
            return;
        }
        match self.paint(&brush) {
            Some(paint) => self.layers.push(Layer::Fill {
                clips: self.clips.clone(),
                paint,
            }),
            None => self.supported = false,
        }
    }

    fn push_layer(&mut self, mode: color::CompositeMode) {
        self.group_depth += 1;
        self.supported &= self.group_depth <= 16 && mode != color::CompositeMode::Unknown;
        self.layers.push(Layer::Push(mode));
    }

    fn pop_layer(&mut self) {
        self.group_depth -= 1;
        self.layers.push(Layer::Pop);
    }
}

impl Painter<'_, '_> {
    fn paint(&self, brush: &color::Brush<'_>) -> Option<Paint> {
        let (geometry, extend, stops) = match *brush {
            color::Brush::Solid {
                palette_index,
                alpha,
            } => {
                return Some(Paint::Solid {
                    color: self.resolve(palette_index).ok()?,
                    alpha,
                });
            }
            color::Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => (
                GradientGeometry::Linear {
                    start: [p0.x, p0.y],
                    end: [p1.x, p1.y],
                },
                extend,
                color_stops,
            ),
            color::Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => (
                GradientGeometry::Radial {
                    start: [c0.x, c0.y],
                    end: [c1.x, c1.y],
                    start_radius: r0,
                    end_radius: r1,
                },
                extend,
                color_stops,
            ),
            color::Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => (
                GradientGeometry::Sweep {
                    center: [c0.x, c0.y],
                    start_angle,
                    end_angle,
                },
                extend,
                color_stops,
            ),
        };
        if self.transform.determinant().abs() < 1e-20 {
            return None;
        }
        let stops = stops
            .iter()
            .map(|stop| {
                Some(ColorStop {
                    offset: stop.offset,
                    color: self.resolve(stop.palette_index).ok()?,
                    alpha: stop.alpha,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Paint::Gradient {
            geometry,
            extend,
            inverse: self.transform.inverse(),
            stops,
        })
    }

    fn resolve(&self, index: u16) -> Result<Option<Color32>, ()> {
        if index == u16::MAX {
            return Ok(None);
        }
        let c = self.palette.get(usize::from(index)).ok_or(())?;
        Ok(Some(Color32::from_rgba_unmultiplied(
            c.red, c.green, c.blue, c.alpha,
        )))
    }
}
