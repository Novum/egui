//! Banded quadratic outlines and paint records for GPU vector fonts.

use emath::vec2;
use vello_cpu::kurbo::{self, PathSeg};

use super::{
    face_store::FontFaceKey,
    font_face::{FontFace, ShapedGlyph},
    glyph::{GlyphAllocation, GlyphGeometry, GlyphLocation, GlyphWithAdvance, OutlineGlyph},
    styled_metrics::StyledMetrics,
    vector_texture::Texture,
};
#[cfg(feature = "color_fonts")]
use crate::Color32;
use crate::FontColorTransferFunction;

const BANDS: usize = 16;
const HEADER: usize = 1 + 2 * BANDS;

pub(super) struct Cache {
    atlas: Atlas,
    glyphs: nohash_hasher::IntMap<u64, Option<GlyphAllocation>>,
    transfer: FontColorTransferFunction,
}

impl Cache {
    pub fn new(options: crate::TextOptions) -> Self {
        Self {
            atlas: Atlas::new(options.max_texture_side.min(16 * 1024)),
            glyphs: Default::default(),
            transfer: options.color_transfer_function,
        }
    }

    pub fn fill_ratio(&self) -> f32 {
        self.atlas.fill_ratio()
    }

    pub fn take_delta(&mut self) -> Option<crate::textures::TexturesDelta> {
        self.atlas.take_delta()
    }

    pub fn missing_glyph(&mut self, font_size: f32, pixels_per_point: f32) -> GlyphWithAdvance {
        let mut geometry = self.atlas.missing_glyph(self.transfer).unwrap_or_default();
        geometry.size *= font_size;
        geometry.offset *= font_size;
        GlyphWithAdvance {
            allocation: GlyphAllocation {
                geometry,
                is_color: false,
            },
            advance_px: font_size * pixels_per_point * 0.7,
        }
    }

    pub fn allocate_glyph(
        &mut self,
        face_key: FontFaceKey,
        face: &FontFace,
        metrics: &StyledMetrics,
        shaped: &ShapedGlyph,
    ) -> OutlineGlyph {
        let ShapedGlyph {
            glyph_id, h_pos, ..
        } = *shaped;
        let key = crate::util::hash((face_key, glyph_id, metrics.location_hash));
        let allocation = *self.glyphs.entry(key).or_insert_with(|| {
            #[cfg(feature = "color_fonts")]
            if let Some(layers) = face.color_layers(metrics, glyph_id) {
                return allocate_color(&mut self.atlas, &layers).map(|geometry| GlyphAllocation {
                    geometry,
                    is_color: true,
                });
            }
            // Unsupported color glyphs use the vector missing-glyph marker.
            if face.is_color_glyph(metrics, glyph_id) {
                return None;
            }
            let path = face.unscaled_outline_path(metrics, glyph_id)?;
            allocate(&mut self.atlas, &path, self.transfer).map(|geometry| GlyphAllocation {
                geometry,
                is_color: false,
            })
        });
        if let Some(mut allocation) = allocation {
            let point_scale = metrics.px_scale_factor / metrics.pixels_per_point;
            allocation.geometry.offset *= point_scale;
            allocation.geometry.size *= point_scale;
            let x_px = h_pos.round() as i32;
            // Fractional placement is separate from the cached geometry.
            allocation.geometry.offset.x += (h_pos - x_px as f32) / metrics.pixels_per_point;
            allocation.geometry.offset.y += metrics.y_offset_in_points;
            return OutlineGlyph { allocation, x_px };
        }
        let x_px = h_pos.round() as i32;
        let mut allocation = self
            .missing_glyph(
                metrics.scale / metrics.pixels_per_point,
                metrics.pixels_per_point,
            )
            .allocation;
        allocation.geometry.offset.x += (h_pos - x_px as f32) / metrics.pixels_per_point;
        allocation.geometry.offset.y += metrics.y_offset_in_points;
        OutlineGlyph { allocation, x_px }
    }
}

struct Atlas {
    width: usize,
    curves: Texture<f32>,
    bands: Texture<u32>,
    max_side: usize,
    dirty: bool,
    missing: Option<GlyphGeometry>,
}

impl Atlas {
    pub fn new(max_side: usize) -> Self {
        Self {
            width: max_side.min(1024),
            curves: Texture::new(max_side.min(1024), max_side),
            bands: Texture::new(max_side.min(1024), max_side),
            max_side,
            dirty: true,
            missing: None,
        }
    }

    pub fn missing_glyph(&mut self, transfer: FontColorTransferFunction) -> Option<GlyphGeometry> {
        if let Some(uv) = self.missing {
            return Some(uv);
        }
        let mut path = kurbo::BezPath::new();
        for points in [
            [(0.1, -0.8), (0.6, -0.8), (0.6, 0.0), (0.1, 0.0)],
            [(0.16, -0.74), (0.16, -0.06), (0.54, -0.06), (0.54, -0.74)],
        ] {
            path.move_to(points[0]);
            for point in &points[1..] {
                path.line_to(*point);
            }
            path.close_path();
        }
        self.missing = allocate(self, &path, transfer);
        self.missing
    }

    pub fn take_delta(&mut self) -> Option<crate::textures::TexturesDelta> {
        core::mem::take(&mut self.dirty).then(|| {
            let mut delta = crate::textures::TexturesDelta::default();
            self.curves.take_delta(
                &mut delta,
                crate::TextureId::FontCurves,
                |resolution, pixels| crate::ImageData::Rgba32Float { resolution, pixels },
            );
            self.bands.take_delta(
                &mut delta,
                crate::TextureId::FontBands,
                |resolution, pixels| crate::ImageData::Rgba32Uint { resolution, pixels },
            );
            delta
        })
    }

    pub fn fill_ratio(&self) -> f32 {
        self.curves.data.len().max(self.bands.data.len()) as f32
            / (self.width * self.max_side) as f32
    }

    fn allocate(&mut self, record: Record, bounds: kurbo::Rect) -> Option<GlyphGeometry> {
        let capacity = self.width * self.max_side;
        if self.curves.data.len() + record.curves.len() > capacity
            || self.bands.data.len() + record.bands.len() > capacity
        {
            return None;
        }
        let start = self.bands.data.len();
        self.curves.data.extend(record.curves);
        self.bands.data.extend(record.bands);
        self.dirty = true;
        let x = start % self.width;
        let y = start / self.width;
        Some(GlyphGeometry {
            offset: vec2(bounds.x0 as f32, bounds.y0 as f32),
            size: vec2(bounds.width() as f32, bounds.height() as f32),
            location: GlyphLocation::Vector {
                origin: [x as u16, y as u16],
            },
        })
    }
}

struct Record {
    curves: Vec<[f32; 4]>,
    bands: Vec<[u32; 4]>,
    curve_offset: u32,
}

impl Record {
    fn new(atlas: &Atlas) -> Self {
        Self {
            curves: Vec::new(),
            bands: Vec::new(),
            curve_offset: atlas.curves.data.len() as u32,
        }
    }

    fn curve(&mut self, value: [f32; 4]) -> u32 {
        let address = self.curve_offset + self.curves.len() as u32;
        self.curves.push(value);
        address
    }
}

/// A source-over fill intersected with its active clips, in font units.
#[cfg(feature = "color_fonts")]
pub(super) enum Layer {
    Fill {
        clips: Vec<kurbo::BezPath>,
        paint: Paint,
    },
    Push(skrifa::color::CompositeMode),
    Pop,
}

#[cfg(feature = "color_fonts")]
pub(super) enum Paint {
    Solid {
        color: Option<Color32>,
        alpha: f32,
    },
    Gradient {
        geometry: GradientGeometry,
        extend: skrifa::color::Extend,
        inverse: kurbo::Affine,
        stops: Vec<ColorStop>,
    },
}

#[cfg(feature = "color_fonts")]
pub(super) enum GradientGeometry {
    Linear {
        start: [f32; 2],
        end: [f32; 2],
    },
    Radial {
        start: [f32; 2],
        end: [f32; 2],
        start_radius: f32,
        end_radius: f32,
    },
    Sweep {
        center: [f32; 2],
        start_angle: f32,
        end_angle: f32,
    },
}

#[cfg(feature = "color_fonts")]
pub(super) struct ColorStop {
    pub offset: f32,

    /// `None` selects the text foreground color.
    pub color: Option<Color32>,
    pub alpha: f32,
}

fn allocate(
    atlas: &mut Atlas,
    path: &kurbo::BezPath,
    transfer: FontColorTransferFunction,
) -> Option<GlyphGeometry> {
    let bounds = path.control_box();
    if path.is_empty() || (bounds.is_finite() && (bounds.width() == 0.0 || bounds.height() == 0.0))
    {
        return Some(GlyphGeometry::default());
    }
    if !valid_bounds(bounds) {
        return None;
    }
    let mut record = Record::new(atlas);
    encode(&mut record, path, bounds, transfer);
    atlas.allocate(record, bounds)
}

fn encode(
    record: &mut Record,
    path: &kurbo::BezPath,
    bounds: kurbo::Rect,
    transfer: FontColorTransferFunction,
) -> u32 {
    let base = record.bands.len();
    record.bands.resize(base + HEADER, [0; 4]);
    let (mode, gamma) = match transfer {
        FontColorTransferFunction::Off => (0, 1.0),
        FontColorTransferFunction::Gamma(gamma) => (1, gamma),
        FontColorTransferFunction::TwoCoverageMinusCoverageSq => (2, 1.0),
    };
    let gamma = record.curve([gamma, 0.0, 0.0, 0.0]);
    record.bands[base] = [mode, gamma, 0, 0];
    let mut curves = Vec::new();
    for segment in path.segments() {
        match segment {
            PathSeg::Line(line) => curves.push(kurbo::QuadBez::new(
                line.p0,
                line.p0.midpoint(line.p1),
                line.p1,
            )),
            PathSeg::Quad(quad) => curves.push(quad),
            PathSeg::Cubic(cubic) => curves.extend(
                cubic
                    .to_quads(bounds.width().max(bounds.height()) * 1e-6)
                    .map(|(_, _, quad)| quad),
            ),
        }
    }
    let mut bands: [Vec<(u32, f32)>; 2 * BANDS] = core::array::from_fn(|_| Vec::new());
    for curve in curves {
        let points = [curve.p0, curve.p1, curve.p2].map(|p| {
            [
                ((p.x - bounds.x0) / bounds.width()) as f32,
                ((p.y - bounds.y0) / bounds.height()) as f32,
            ]
        });
        let address = record.curve([points[0][0], points[0][1], points[1][0], points[1][1]]);
        record.curve([points[2][0], points[2][1], 0.0, 0.0]);
        for axis in 0..2 {
            let coord = 1 - axis;
            let min = points.iter().map(|p| p[coord]).fold(1.0_f32, f32::min);
            let max = points.iter().map(|p| p[coord]).fold(0.0_f32, f32::max);
            let first = ((min * BANDS as f32).floor() as usize).min(BANDS - 1);
            let last = ((max * BANDS as f32).floor() as usize).min(BANDS - 1);
            let bound = points
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max);
            for band in first..=last {
                bands[axis * BANDS + band].push((address, bound));
            }
        }
    }
    for (i, band) in bands.iter_mut().enumerate() {
        band.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        record.bands[base + 1 + i] = [record.bands.len() as u32, band.len() as u32, 0, 0];
        record.bands.extend(band.chunks(4).map(|chunk| {
            let mut addresses = [0; 4];
            for (out, &(address, _)) in core::iter::zip(&mut addresses, chunk) {
                *out = address;
            }
            addresses
        }));
    }
    base as u32
}

fn valid_bounds(bounds: kurbo::Rect) -> bool {
    bounds.is_finite() && bounds.width() > 0.0 && bounds.height() > 0.0
}

#[cfg(feature = "color_fonts")]
fn allocate_color(atlas: &mut Atlas, layers: &[Layer]) -> Option<GlyphGeometry> {
    if layers.is_empty() {
        return None;
    }
    let bounds = layers
        .iter()
        .flat_map(|layer| match layer {
            Layer::Fill { clips, .. } => clips.as_slice(),
            Layer::Push(_) | Layer::Pop => &[],
        })
        .filter(|path| !path.is_empty())
        .map(|path| path.control_box())
        .reduce(|a, b| a.union(b))?;
    if !valid_bounds(bounds) {
        return None;
    }
    let mut record = Record::new(atlas);
    record.bands.resize(1 + layers.len(), [0; 4]);
    record.bands[0] = [3, layers.len() as u32, 0, 0];
    for (i, layer) in layers.iter().enumerate() {
        let base = record.bands.len();
        record.bands[1 + i][0] = base as u32;
        match layer {
            Layer::Fill { clips, paint } => {
                record.bands.resize(base + 1 + clips.len(), [0; 4]);
                record.bands[base][0] = clips.len() as u32;
                for (j, path) in clips.iter().enumerate() {
                    let clip = encode(&mut record, path, bounds, FontColorTransferFunction::Off);
                    record.bands[base + 1 + j][0] = clip;
                }
                record.bands[base][1] = encode_paint(&mut record, paint, bounds)?;
            }
            Layer::Push(mode) => record.bands.push([u32::MAX, *mode as u32, 0, 0]),
            Layer::Pop => record.bands.push([u32::MAX - 1, 0, 0, 0]),
        }
    }
    atlas.allocate(record, bounds)
}

#[cfg(feature = "color_fonts")]
fn encode_paint(record: &mut Record, paint: &Paint, bounds: kurbo::Rect) -> Option<u32> {
    let base = record.bands.len() as u32;
    let color = |color: Option<Color32>, alpha| {
        color
            .unwrap_or(Color32::WHITE)
            .gamma_multiply(alpha)
            .to_normalized_gamma_f32()
    };
    match paint {
        Paint::Solid {
            color: value,
            alpha,
        } => {
            let address = record.curve(color(*value, *alpha));
            record
                .bands
                .push([0, u32::from(value.is_none()), 0, address]);
        }
        Paint::Gradient {
            geometry,
            extend,
            inverse,
            stops,
        } => {
            if stops.is_empty() {
                return None;
            }
            let (kind, parameters) = match *geometry {
                GradientGeometry::Linear { start, end } => (
                    1,
                    [[start[0], start[1], 0.0, 0.0], [end[0], end[1], 0.0, 0.0]],
                ),
                GradientGeometry::Radial {
                    start,
                    end,
                    start_radius,
                    end_radius,
                } => (
                    2,
                    [
                        [start[0], start[1], start_radius, 0.0],
                        [end[0], end[1], end_radius, 0.0],
                    ],
                ),
                GradientGeometry::Sweep {
                    center,
                    start_angle,
                    end_angle,
                } => (
                    3,
                    [
                        [center[0], center[1], 0.0, 0.0],
                        [start_angle, end_angle, 0.0, 0.0],
                    ],
                ),
            };
            let extend = match extend {
                skrifa::color::Extend::Pad => 0,
                skrifa::color::Extend::Repeat => 1,
                skrifa::color::Extend::Reflect => 2,
                _ => return None,
            };
            let inverse = *inverse
                * kurbo::Affine::new([
                    bounds.width(),
                    0.0,
                    0.0,
                    bounds.height(),
                    bounds.x0,
                    bounds.y0,
                ]);
            let c = inverse.as_coeffs().map(|v| v as f32);
            let address = record.curve([c[0], c[2], c[4], 0.0]);
            record.curve([c[1], c[3], c[5], 0.0]);
            record.curves.extend(parameters);
            for stop in stops {
                record.curve([
                    stop.offset,
                    f32::from(u8::from(stop.color.is_none())),
                    0.0,
                    0.0,
                ]);
                record.curve(color(stop.color, stop.alpha));
            }
            record
                .bands
                .push([kind, extend, stops.len() as u32, address]);
        }
    }
    Some(base)
}
