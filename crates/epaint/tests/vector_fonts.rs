//! Font extraction and cache-lifecycle regressions that do not need a GPU.
use epaint::{
    Color32, FontId, Fonts, TextOptions,
    text::{FontData, FontDefinitions, FontFamily, FontRendering, ViewportKey},
};
use std::sync::Arc;

fn fonts(bytes: &'static [u8]) -> Fonts {
    let mut definitions = FontDefinitions::empty();
    definitions
        .font_data
        .insert("test".into(), Arc::new(FontData::from_static(bytes)));
    definitions
        .families
        .insert(FontFamily::Proportional, vec!["test".into()]);
    Fonts::new(TextOptions::default(), definitions)
}

#[test]
fn extracts_quadratic_and_cubic_outlines() {
    for (bytes, text) in [
        (include_bytes!("data/vector.ttf").as_slice(), "AOQ"),
        (include_bytes!("data/vector-cff.otf").as_slice(), "C"),
    ] {
        let mut fonts = fonts(bytes);
        for dpi in [1.0, 1.25, 2.0] {
            let galley = fonts.with_pixels_per_point(dpi).layout_no_wrap(
                text.into(),
                FontId::proportional(32.0),
                Color32::WHITE,
            );
            for glyph in &galley.rows[0].row.glyphs {
                assert!(
                    glyph.geometry.is_vector(),
                    "{text}: failed to extract a curve outline"
                );
                assert!(glyph.geometry.size.x > 0.0 && glyph.geometry.size.y > 0.0);
                assert!(glyph.geometry.offset.is_finite());
            }
            assert!(galley.rows[0].row.visuals.mesh.is_valid());
        }
    }
}

#[test]
fn switching_renderers_invalidates_galleys_and_atlas() {
    let mut fonts = fonts(include_bytes!("data/vector.ttf"));
    for rendering in [
        FontRendering::Vector,
        FontRendering::Bitmap,
        FontRendering::Vector,
    ] {
        fonts.begin_pass(
            TextOptions {
                font_rendering: rendering,
                ..Default::default()
            },
            ViewportKey::default(),
        );
        let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AQ".into(),
            FontId::proportional(32.0),
            Color32::WHITE,
        );
        assert!(
            galley.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| g.geometry.is_vector() == (rendering == FontRendering::Vector))
        );
        assert!(fonts.font_image_delta().is_some());
    }
}

#[test]
fn curve_records_are_shared_across_size_dpi_and_subpixel_phase() {
    for (bytes, text) in [
        (include_bytes!("data/vector.ttf").as_slice(), "AAOQAA"),
        (include_bytes!("data/vector-cff.otf").as_slice(), "CCCC"),
    ] {
        let mut fonts = fonts(bytes);
        let first = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            text.into(),
            FontId::proportional(13.3),
            Color32::WHITE,
        );
        let addresses: Vec<_> = first.rows[0]
            .row
            .glyphs
            .iter()
            .map(|g| g.geometry.location)
            .collect();
        assert_eq!(
            addresses[0], addresses[1],
            "pixel phases must share an outline"
        );
        fonts.font_image_delta();
        fonts.font_textures_delta().unwrap().clear();
        for dpi in [0.75, 1.0, 1.25, 2.0, 3.0] {
            for size in [4.0, 13.3, 72.0, 512.0] {
                let galley = fonts.with_pixels_per_point(dpi).layout_no_wrap(
                    text.into(),
                    FontId::proportional(size),
                    Color32::WHITE,
                );
                let actual: Vec<_> = galley.rows[0]
                    .row
                    .glyphs
                    .iter()
                    .map(|g| g.geometry.location)
                    .collect();
                assert_eq!(addresses, actual, "size={size}, dpi={dpi}");
                assert!(
                    fonts.font_textures_delta().is_none(),
                    "resizing an existing glyph uploaded new curve data"
                );
            }
        }
    }
}

#[test]
fn variation_instances_have_distinct_curves_but_share_across_sizes() {
    use epaint::text::{LayoutJob, TextFormat, VariationCoords};
    let mut fonts = fonts(include_bytes!("data/vector-variable.ttf"));
    let mut records = Vec::new();
    for size in [16.0, 64.0] {
        for weight in [400.0, 900.0] {
            let job = LayoutJob::single_section(
                "A".into(),
                TextFormat {
                    font_id: FontId::proportional(size),
                    coords: VariationCoords::new([("wght", weight)]),
                    ..Default::default()
                },
            );
            let galley = fonts.with_pixels_per_point(1.0).layout_job(job);
            records.push(galley.rows[0].row.glyphs[0].geometry);
        }
    }
    assert_ne!(
        records[0].location, records[1].location,
        "different variation instances alias"
    );
    assert_eq!(records[0].location, records[2].location);
    assert_eq!(records[1].location, records[3].location);
    assert!(
        records[1].size.x > records[0].size.x * 1.2,
        "variation geometry was not applied"
    );
}

#[test]
fn opentype_kerning_ligatures_context_and_marks_survive_curve_caching() {
    use epaint::text::{LayoutJob, TextFormat};
    for mode in [FontRendering::Vector, FontRendering::Bitmap] {
        let mut fonts = fonts(include_bytes!("data/vector-shaping.ttf"));
        fonts.begin_pass(
            TextOptions {
                font_rendering: mode,
                font_hinting: false,
                ..Default::default()
            },
            ViewportKey::default(),
        );
        let mut shape = |text: &str| {
            fonts
                .with_pixels_per_point(1.0)
                .layout_job(LayoutJob::single_section(
                    text.into(),
                    TextFormat {
                        font_id: FontId::proportional(100.0),
                        ..Default::default()
                    },
                ))
        };
        let a = shape("A");
        let v = shape("V");
        let av = shape("AV");
        assert!(
            (a.size().x + v.size().x - av.size().x - 15.0).abs() < 0.1,
            "GPOS kerning lost in {mode:?}"
        );
        let f = shape("f");
        let i = shape("i");
        let fi = shape("fi");
        let visible: Vec<_> = fi.rows[0]
            .row
            .glyphs
            .iter()
            .filter(|g| !g.geometry.is_nothing())
            .collect();
        assert_eq!(
            visible.len(),
            1,
            "GSUB ligature was split into character glyphs"
        );
        assert_eq!(
            fi.rows[0].row.glyphs.len(),
            2,
            "logical character count changed"
        );
        assert!((fi.size().x - 70.0).abs() < 0.1);
        assert!(fi.size().x < f.size().x + i.size().x);
        assert_ne!(
            visible[0].geometry.location, f.rows[0].row.glyphs[0].geometry.location,
            "cache substituted the character outline for the shaped ligature"
        );
        let ff = shape("ff");
        assert_ne!(
            ff.rows[0].row.glyphs[0].geometry.location, f.rows[0].row.glyphs[0].geometry.location,
            "contextual GSUB substitution lost"
        );
        let base = shape("a");
        let marked = shape("a\u{301}");
        assert!(
            (base.size().x - marked.size().x).abs() < 0.1,
            "combining mark added advance"
        );
        let glyphs = &marked.rows[0].row.glyphs;
        assert_eq!(glyphs.len(), 2);
        let base_top = glyphs[0].pos.y + glyphs[0].geometry.offset.y;
        let mark_top = glyphs[1].pos.y + glyphs[1].geometry.offset.y;
        assert!(
            mark_top < base_top - 10.0,
            "GPOS mark anchor was lost: base={base_top}, mark={mark_top}"
        );
    }
}

#[cfg(feature = "color_fonts")]
#[test]
fn color_curve_records_are_shared_across_sizes() {
    let mut fonts = fonts(include_bytes!("data/vector.ttf"));
    let first = fonts.with_pixels_per_point(1.0).layout_no_wrap(
        "😀F".into(),
        FontId::proportional(16.0),
        Color32::GREEN,
    );
    fonts.font_image_delta();
    fonts.font_textures_delta().unwrap().clear();
    let large = fonts.with_pixels_per_point(2.0).layout_no_wrap(
        "😀F".into(),
        FontId::proportional(160.0),
        Color32::GREEN,
    );
    for (a, b) in core::iter::zip(&first.rows[0].row.glyphs, &large.rows[0].row.glyphs) {
        assert!(a.geometry.is_vector() && b.geometry.is_vector() && a.is_color && b.is_color);
        assert_eq!(a.geometry.location, b.geometry.location);
        assert!((b.geometry.size.x / a.geometry.size.x - 10.0).abs() < 1e-5);
    }
    assert!(fonts.font_image_delta().is_none());
    assert!(fonts.font_textures_delta().is_none());
}

#[cfg(feature = "color_fonts")]
#[test]
fn colr_layers_are_curves_and_keep_foreground_tint() {
    let mut fonts = fonts(include_bytes!("data/vector.ttf"));
    let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
        "😀F".into(),
        FontId::proportional(32.0),
        Color32::GREEN,
    );
    for glyph in &galley.rows[0].row.glyphs {
        assert!(glyph.is_color && glyph.geometry.is_vector());
    }
    let mut mesh = epaint::Mesh::default();
    let shape = epaint::TextShape::new(epaint::Pos2::ZERO, galley, Color32::GREEN);
    epaint::Tessellator::new(
        1.0,
        Default::default(),
        fonts.font_image_resolution(),
        vec![],
    )
    .tessellate_text(&shape, &mut mesh);
    assert!(
        mesh.vertices
            .iter()
            .all(|v| v.color == Color32::GREEN && v.glyph_origin[0] != 0)
    );
}

#[cfg(feature = "color_fonts")]
#[test]
fn colrv1_transformed_foreground_is_a_curve() {
    let mut fonts = fonts(include_bytes!("data/vector-colrv1.ttf"));
    let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
        "F".into(),
        FontId::proportional(40.0),
        Color32::GREEN,
    );
    let glyph = &galley.rows[0].row.glyphs[0];
    assert!(glyph.is_color && glyph.geometry.is_vector());
}

#[cfg(feature = "color_fonts")]
#[test]
#[ignore = "requires EGUI_VECTOR_SYSTEM_FONT pointing to a COLRv1 emoji font"]
fn system_emoji_stays_vector_across_sizes() {
    let bytes = std::fs::read(
        std::env::var_os("EGUI_VECTOR_SYSTEM_FONT").expect("EGUI_VECTOR_SYSTEM_FONT"),
    )
    .expect("emoji font");
    let mut definitions = FontDefinitions::empty();
    definitions
        .font_data
        .insert("emoji".into(), Arc::new(FontData::from_owned(bytes)));
    definitions
        .families
        .insert(FontFamily::Proportional, vec!["emoji".into()]);
    let mut fonts = Fonts::new(TextOptions::default(), definitions);
    let mut address = None;
    for size in [14.0, 64.0, 512.0] {
        let g = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "\u{1f4ca}\u{1f600}\u{1f520}".into(),
            FontId::proportional(size),
            Color32::WHITE,
        );
        let glyph = &g.rows[0].row.glyphs[0];
        assert!(
            glyph.is_color && glyph.geometry.is_vector(),
            "emoji fell back to bitmap at {size}"
        );
        if let Some(a) = address {
            assert_eq!(glyph.geometry.location, a);
        }
        address = Some(glyph.geometry.location);
    }
}

#[cfg(feature = "color_fonts")]
#[test]
fn clipped_gradients_stay_vector() {
    let mut fonts = fonts(include_bytes!("data/vector-paints.ttf"));
    for size in [14.0, 256.0] {
        let g = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AQOF".into(),
            FontId::proportional(size),
            Color32::WHITE,
        );
        assert!(
            g.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| g.is_color && g.geometry.is_vector())
        );
    }
}

#[test]
fn switching_modes_releases_font_data() {
    use epaint::TextureId;
    let mut fonts = fonts(include_bytes!("data/vector.ttf"));
    let primitive_pixels = fonts.image();
    for mode in [
        FontRendering::Vector,
        FontRendering::Bitmap,
        FontRendering::Vector,
    ] {
        fonts.begin_pass(
            TextOptions {
                font_rendering: mode,
                ..Default::default()
            },
            ViewportKey::default(),
        );
        let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AOQ".into(),
            FontId::proportional(80.0),
            Color32::WHITE,
        );
        let mut delta = fonts.font_textures_delta().expect("mode update");
        match mode {
            FontRendering::Vector => {
                assert!(delta.set.contains_key(&TextureId::FontCurves));
                assert!(delta.set.contains_key(&TextureId::FontBands));
                assert!(delta.free.is_empty());
                assert_eq!(
                    fonts.image(),
                    primitive_pixels,
                    "vector mode allocated bitmap glyphs"
                );
                assert!(
                    galley.rows[0]
                        .row
                        .glyphs
                        .iter()
                        .all(|g| g.geometry.is_vector())
                );
            }
            FontRendering::Bitmap => {
                assert!(delta.set.is_empty());
                assert!(delta.free.contains(&TextureId::FontCurves));
                assert!(delta.free.contains(&TextureId::FontBands));
                assert_ne!(
                    fonts.image(),
                    primitive_pixels,
                    "bitmap mode did not rasterize glyphs"
                );
                assert!(
                    galley.rows[0]
                        .row
                        .glyphs
                        .iter()
                        .all(|g| !g.geometry.is_vector())
                );
            }
        }
        delta.clear();
        assert!(fonts.font_textures_delta().is_none());
    }
}

fn callback_bitmap(color: Color32, size: usize) -> epaint::text::RasterizedGlyph {
    epaint::text::RasterizedGlyph {
        bitmap: epaint::text::GlyphBitmap {
            image: epaint::ColorImage::filled([size, size], color),
            offset_px: epaint::vec2(0.0, -(size as f32)),
            is_color: true,
        },
        advance_px: size as f32,
    }
}

#[test]
fn callbacks_share_the_atlas_with_the_selected_outline_renderer() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use epaint::text::{FontPriority, GlyphLocation, GlyphRasterizer, GlyphRasterizerRequest};

    for (priority, cluster) in [
        (FontPriority::Highest, "A"),
        (FontPriority::Lowest, "\u{e000}"),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let mut fonts = fonts(include_bytes!("data/vector.ttf")).with_glyph_rasterizer(
            GlyphRasterizer::new("callback", move |request: &GlyphRasterizerRequest<'_>| {
                (request.cluster == cluster).then(|| {
                    counter.fetch_add(1, Ordering::Relaxed);
                    callback_bitmap(Color32::RED, request.font_size_px as usize)
                })
            })
            .with_priority(priority),
        );
        let primitive_pixels = fonts.image();
        for mode in [
            FontRendering::Vector,
            FontRendering::Bitmap,
            FontRendering::Vector,
        ] {
            fonts.begin_pass(
                TextOptions {
                    font_rendering: mode,
                    ..Default::default()
                },
                ViewportKey::default(),
            );
            assert_eq!(
                fonts.image(),
                primitive_pixels,
                "mode switch retained bitmap glyphs"
            );
            let before = calls.load(Ordering::Relaxed);
            let mut outline_location = None;
            for (dpi, size) in [(1.0, 20.0), (1.0, 30.0), (2.0, 20.0)] {
                let galley = fonts.with_pixels_per_point(dpi).layout_no_wrap(
                    format!("{cluster}O{cluster}"),
                    FontId::proportional(size),
                    Color32::GREEN,
                );
                let glyphs = &galley.rows[0].row.glyphs;
                let GlyphLocation::Bitmap { min: [x, y], .. } = glyphs[0].geometry.location else {
                    panic!("callback did not allocate a bitmap in {mode:?}");
                };
                assert_eq!(fonts.image()[(x as usize, y as usize)], Color32::RED);
                assert_eq!(glyphs[0].geometry, glyphs[2].geometry);
                assert_eq!(glyphs[0].advance_width, size);
                assert_eq!(
                    glyphs[1].geometry.is_vector(),
                    mode == FontRendering::Vector
                );
                if mode == FontRendering::Vector {
                    if let Some(location) = outline_location {
                        assert_eq!(glyphs[1].geometry.location, location);
                        assert!(fonts.font_textures_delta().is_none());
                    } else {
                        outline_location = Some(glyphs[1].geometry.location);
                        fonts
                            .font_textures_delta()
                            .expect("new glyph texture data")
                            .clear();
                    }
                }
                assert!(fonts.font_image_delta().is_some());
            }
            assert_eq!(
                calls.load(Ordering::Relaxed) - before,
                3,
                "callback cache missed"
            );
        }
    }
}

#[test]
fn replacing_callbacks_invalidates_bitmaps_and_preserves_curves() {
    use epaint::text::{FontPriority, GlyphRasterizer, GlyphRasterizerRequest};
    let mut fonts = fonts(include_bytes!("data/vector.ttf"));
    let rasterizer = |key, color: Option<Color32>| {
        GlyphRasterizer::new(key, move |request: &GlyphRasterizerRequest<'_>| {
            if request.cluster == "A" {
                color.map(|color| callback_bitmap(color, 20))
            } else {
                None
            }
        })
        .with_priority(FontPriority::Highest)
    };
    fonts.add_glyph_rasterizer(rasterizer("miss", None));
    let layout = |fonts: &mut Fonts| {
        fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AO".into(),
            FontId::proportional(20.0),
            Color32::WHITE,
        )
    };
    let original = layout(&mut fonts);
    assert!(original.rows[0].row.glyphs[0].geometry.is_vector());
    fonts.font_textures_delta().unwrap().clear();
    for (key, color) in [("added", Color32::RED), ("replaced", Color32::BLUE)] {
        if key == "added" {
            fonts.add_glyph_rasterizer(rasterizer(key, Some(color)));
        } else {
            fonts.set_glyph_rasterizers(vec![rasterizer(key, Some(color))]);
        }
        let galley = layout(&mut fonts);
        let epaint::text::GlyphLocation::Bitmap { min: [x, y], .. } =
            galley.rows[0].row.glyphs[0].geometry.location
        else {
            panic!("stale callback result");
        };
        assert_eq!(fonts.image()[(x as usize, y as usize)], color);
        assert_eq!(
            galley.rows[0].row.glyphs[1].geometry,
            original.rows[0].row.glyphs[1].geometry
        );
        assert!(fonts.font_textures_delta().is_none());
    }
    fonts.set_glyph_rasterizers(vec![]);
    assert_eq!(
        layout(&mut fonts).rows[0].row.glyphs[0].geometry,
        original.rows[0].row.glyphs[0].geometry
    );
    assert!(fonts.font_textures_delta().is_none());
}

#[test]
fn vector_mode_reclaims_full_callback_atlas() {
    use epaint::text::{GlyphRasterizer, GlyphRasterizerRequest};
    let options = TextOptions {
        max_texture_side: 1024,
        ..Default::default()
    };
    let mut fonts = Fonts::new(options, FontDefinitions::empty()).with_glyph_rasterizer(
        GlyphRasterizer::new("large", |_: &GlyphRasterizerRequest<'_>| {
            Some(callback_bitmap(Color32::RED, 900))
        }),
    );
    let primitive_pixels = fonts.image();
    fonts.with_pixels_per_point(1.0).layout_no_wrap(
        "x".into(),
        FontId::proportional(900.0),
        Color32::WHITE,
    );
    assert!(fonts.font_atlas_fill_ratio() > 0.8);
    fonts.begin_pass(options, ViewportKey::default());
    assert_eq!(fonts.image(), primitive_pixels);
    assert!(fonts.font_atlas_fill_ratio() < 0.8);
}

#[test]
#[cfg(feature = "monochrome_emoji_fonts")]
fn emoji_variation_selectors_have_no_visible_geometry() {
    for mode in [FontRendering::Bitmap, FontRendering::Vector] {
        let mut fonts = Fonts::new(
            TextOptions {
                font_rendering: mode,
                ..Default::default()
            },
            FontDefinitions::default(),
        );
        for text in ["⌨️", "🖥️"] {
            let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
                text.into(),
                FontId::proportional(18.0),
                Color32::WHITE,
            );
            assert_eq!(
                galley.rows[0]
                    .row
                    .glyphs
                    .iter()
                    .filter(|g| !g.geometry.is_nothing())
                    .count(),
                1,
                "{text} in {mode:?}: {:?}",
                galley.rows[0].row.glyphs
            );
        }
    }
}
