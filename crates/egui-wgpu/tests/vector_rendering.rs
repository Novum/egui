//! Opt-in GPU regression test: `cargo test -p egui-wgpu --test vector_rendering -- --ignored`.
//! Set `EGUI_VECTOR_CAPTURE_DIR` to also save the rendered PNGs.

use core::time::Duration;
use std::sync::Arc;

use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use epaint::{
    Color32, FontId, Fonts, Mesh, Rect, TextOptions, TextShape, TextureId, pos2,
    text::{FontData, FontDefinitions, FontFamily, FontRendering},
};

const SIZE: [u32; 2] = [1024, 512];

fn test_fonts() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        "vector-test".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../epaint/tests/data/vector.ttf"
        ))),
    );
    definitions.families.insert(
        FontFamily::Name("vector-test".into()),
        vec!["vector-test".into()],
    );
    definitions.font_data.insert(
        "vector-cff".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../epaint/tests/data/vector-cff.otf"
        ))),
    );
    definitions.families.insert(
        FontFamily::Name("vector-cff".into()),
        vec!["vector-cff".into()],
    );
    definitions.font_data.insert(
        "vector-colrv1".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../epaint/tests/data/vector-colrv1.ttf"
        ))),
    );
    definitions.families.insert(
        FontFamily::Name("vector-colrv1".into()),
        vec!["vector-colrv1".into()],
    );
    definitions
}

fn scene(mode: FontRendering, dpi: f32) -> (Fonts, Vec<TextShape>) {
    let mut fonts = Fonts::new(
        TextOptions {
            font_rendering: mode,
            font_hinting: false,
            color_transfer_function: epaint::FontColorTransferFunction::Off,
            ..Default::default()
        },
        test_fonts(),
    );
    let mut shapes = Vec::new();
    for (i, c) in ['A', 'O', 'Q', '😀', 'F'].iter().enumerate() {
        let galley = fonts.with_pixels_per_point(dpi).layout_no_wrap(
            c.to_string(),
            FontId::new(80.0, FontFamily::Name("vector-test".into())),
            Color32::GREEN,
        );
        assert_eq!(
            galley.rows[0].row.glyphs[0].geometry.is_vector(),
            mode == FontRendering::Vector,
            "glyph rendering mode"
        );
        shapes.push(TextShape::new(
            pos2(20.0 + i as f32 * 90.0, 15.0),
            galley,
            Color32::WHITE,
        ));
    }
    for (i, size) in [11.0, 14.0, 24.0, 48.0, 96.0].iter().enumerate() {
        let galley = fonts.with_pixels_per_point(dpi).layout_no_wrap(
            "Bézier curves: AV fi @gQ 0123".into(),
            FontId::proportional(*size),
            Color32::WHITE,
        );
        shapes.push(TextShape::new(
            pos2(20.0, 120.0 + i as f32 * 60.0),
            galley,
            Color32::WHITE,
        ));
    }
    let rotated = fonts.with_pixels_per_point(dpi).layout_no_wrap(
        "Rotated outlines".into(),
        FontId::proportional(32.0),
        Color32::WHITE,
    );
    shapes.push(TextShape::new(pos2(660.0, 25.0), rotated, Color32::WHITE).with_angle(0.37));
    let cubic = fonts.with_pixels_per_point(dpi).layout_no_wrap(
        "C".into(),
        FontId::new(100.0, FontFamily::Name("vector-cff".into())),
        Color32::WHITE,
    );
    assert_eq!(
        cubic.rows[0].row.glyphs[0].geometry.is_vector(),
        mode == FontRendering::Vector,
        "glyph rendering mode"
    );
    shapes.push(TextShape::new(pos2(900.0, 170.0), cubic, Color32::WHITE));
    let colr = fonts.with_pixels_per_point(dpi).layout_no_wrap(
        "F".into(),
        FontId::new(64.0, FontFamily::Name("vector-colrv1".into())),
        Color32::GREEN,
    );
    assert_eq!(
        colr.rows[0].row.glyphs[0].geometry.is_vector(),
        mode == FontRendering::Vector,
        "glyph rendering mode"
    );
    shapes.push(TextShape::new(pos2(420.0, 110.0), colr, Color32::WHITE));
    (fonts, shapes)
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    fonts: &mut Fonts,
    shapes: &[TextShape],
) -> image::RgbaImage {
    if let Some(delta) = fonts.font_textures_delta() {
        apply_textures(renderer, device, queue, delta);
    }
    if let Some(delta) = fonts.font_image_delta() {
        renderer.update_texture(device, queue, TextureId::default(), &delta);
    }
    let dpi = shapes[0].galley.pixels_per_point;
    let mut tessellator = epaint::Tessellator::new(
        dpi,
        Default::default(),
        fonts.font_image_resolution(),
        vec![],
    );
    let mut mesh = Mesh::default();
    for shape in shapes {
        tessellator.tessellate_text(shape, &mut mesh);
    }
    render_mesh(device, queue, renderer, fonts, mesh, dpi)
}

fn render_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    fonts: &mut Fonts,
    mesh: Mesh,
    dpi: f32,
) -> image::RgbaImage {
    if let Some(delta) = fonts.font_textures_delta() {
        apply_textures(renderer, device, queue, delta);
    }
    if let Some(delta) = fonts.font_image_delta() {
        renderer.update_texture(device, queue, TextureId::default(), &delta);
    }
    let jobs = vec![epaint::ClippedPrimitive {
        clip_rect: Rect::EVERYTHING,
        primitive: epaint::Primitive::Mesh(mesh),
    }];
    let screen = ScreenDescriptor {
        size_in_pixels: SIZE,
        pixels_per_point: dpi,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Vector regression target"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let buffers = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, &jobs, &screen);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * SIZE[1] * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    let submission = queue.submit(core::iter::chain(buffers, [encoder.finish()]));
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("readback receiver");
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .expect("GPU submission completed");
    receiver
        .recv()
        .expect("readback callback")
        .expect("buffer mapping");
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .expect("mapped readback buffer");
    image::RgbaImage::from_raw(SIZE[0], SIZE[1], mapped.to_vec()).expect("RGBA readback dimensions")
}

fn sample(image: &image::RgbaImage, shape: &TextShape, x: f32, y: f32) -> [u8; 4] {
    let glyph = &shape.galley.rows[0].row.glyphs[0];
    let top = shape.pos + glyph.pos.to_vec2() + glyph.geometry.offset;
    let pos = top + glyph.geometry.size * epaint::vec2(x, y);
    image
        .get_pixel(
            (pos.x * shape.galley.pixels_per_point) as u32,
            (pos.y * shape.galley.pixels_per_point) as u32,
        )
        .0
}

#[test]
#[ignore = "requires a GPU adapter"]
fn vector_gpu_rendering() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&Default::default())).expect("GPU adapter");
    log::info!("Vector test adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut renderer = Renderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        RendererOptions::PREDICTABLE,
    );
    mixed_draw_order(&device, &queue, &mut renderer);
    mixed_callback_glyphs(&device, &queue, &mut renderer);
    transformed_existing_text(&device, &queue, &mut renderer);
    color_paints(
        &device,
        &queue,
        &mut renderer,
        include_bytes!("../../epaint/tests/data/vector-paints.ttf"),
        "AQOF",
        "paint-fixture",
    );
    color_paints(
        &device,
        &queue,
        &mut renderer,
        include_bytes!("../../epaint/tests/data/vector-composite.ttf"),
        "AOQF",
        "composite-fixture",
    );
    if let Some(path) = std::env::var_os("EGUI_VECTOR_SYSTEM_FONT") {
        color_paints(
            &device,
            &queue,
            &mut renderer,
            &std::fs::read(path).expect("system font"),
            "\u{1f600}\u{1f520}\u{1f4ca}",
            "emoji",
        );
    }
    for dpi in [1.0, 1.5, 2.0] {
        let (mut fonts, shapes) = scene(FontRendering::Vector, dpi);
        let vector = render(&device, &queue, &mut renderer, &mut fonts, &shapes);
        assert_eq!(sample(&vector, &shapes[0], 0.5, 0.5), [0, 255, 0, 255]);
        assert_eq!(
            sample(&vector, &shapes[1], 0.5, 0.5),
            [0; 4],
            "counter must remain empty"
        );
        assert_eq!(sample(&vector, &shapes[2], 0.5, 0.5), [0, 255, 0, 255]);
        let upper = sample(&vector, &shapes[3], 0.5, 0.25);
        assert!(
            upper[0].abs_diff(127) <= 1 && upper[2].abs_diff(128) <= 1 && upper[3] == 255,
            "color layer: {upper:?}"
        );
        assert_eq!(sample(&vector, &shapes[3], 0.5, 0.75), [255, 0, 0, 255]);
        assert_eq!(
            sample(&vector, &shapes[4], 0.15, 0.5),
            [0, 255, 0, 255],
            "COLR foreground must use the text color"
        );

        let foreground = sample(&vector, shapes.last().unwrap(), 0.15, 0.5);
        assert!(
            foreground[0] == 0
                && foreground[1].abs_diff(128) <= 1
                && foreground[2] == 0
                && foreground[3].abs_diff(128) <= 1,
            "translucent COLRv1 foreground: {foreground:?}"
        );

        // Growing and partially updating the atlas must not invalidate cached galleys.
        fonts.with_pixels_per_point(dpi).layout_no_wrap(
            (0x20..0x300).filter_map(char::from_u32).collect(),
            FontId::proportional(18.0),
            Color32::WHITE,
        );
        let updated = render(&device, &queue, &mut renderer, &mut fonts, &shapes);
        assert_eq!(vector, updated, "atlas updates changed existing glyphs");

        let (mut bitmap_fonts, bitmap_shapes) = scene(FontRendering::Bitmap, dpi);
        let bitmap = render(
            &device,
            &queue,
            &mut renderer,
            &mut bitmap_fonts,
            &bitmap_shapes,
        );
        // Compare real font coverage, including counters and rotated text, to the CPU rasterizer.
        let error: u64 = core::iter::zip(vector.pixels(), bitmap.pixels())
            .map(|(a, b)| u64::from(a[3].abs_diff(b[3])))
            .sum();
        let mass: u64 = bitmap.pixels().map(|p| u64::from(p[3])).sum();
        log::info!(
            "Coverage error / reference ink: {}",
            error as f64 / mass as f64
        );
        assert!(
            (error as f64 / mass as f64) < 0.10,
            "excessive error against CPU coverage"
        );
        if let Some(dir) = std::env::var_os("EGUI_VECTOR_CAPTURE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("capture directory");
            vector.save(dir.join(format!("vector-{dpi}.png"))).unwrap();
            bitmap.save(dir.join(format!("bitmap-{dpi}.png"))).unwrap();
        }
    }
}

fn mixed_callback_glyphs(device: &wgpu::Device, queue: &wgpu::Queue, renderer: &mut Renderer) {
    use epaint::text::{
        FontPriority, GlyphBitmap, GlyphRasterizer, GlyphRasterizerRequest, RasterizedGlyph,
    };

    let mut fonts = Fonts::new(TextOptions::default(), test_fonts());
    for color in [Color32::RED, Color32::BLUE] {
        fonts.set_glyph_rasterizers(vec![
            GlyphRasterizer::new("colored", move |request: &GlyphRasterizerRequest<'_>| {
                (request.cluster == "A").then(|| {
                    let size = request.font_size_px as usize;
                    RasterizedGlyph {
                        bitmap: GlyphBitmap {
                            image: epaint::ColorImage::filled([size, size], color),
                            offset_px: epaint::vec2(0.0, -(size as f32)),
                            is_color: true,
                        },
                        advance_px: size as f32,
                    }
                })
            })
            .with_priority(FontPriority::Highest),
            GlyphRasterizer::new("monochrome", |_: &GlyphRasterizerRequest<'_>| {
                Some(RasterizedGlyph {
                    bitmap: GlyphBitmap {
                        image: epaint::ColorImage::filled([40, 40], Color32::WHITE),
                        offset_px: epaint::vec2(0.0, -40.0),
                        is_color: false,
                    },
                    advance_px: 40.0,
                })
            }),
        ]);
        let shapes: Vec<_> = ["A", "Q", "\u{e000}"]
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
                    text.into(),
                    FontId::new(80.0, FontFamily::Name("vector-test".into())),
                    Color32::GREEN,
                );
                assert_eq!(
                    galley.rows[0].row.glyphs[0].geometry.is_vector(),
                    i == 1,
                    "only the font outline should use vector geometry"
                );
                TextShape::new(pos2(20.0 + i as f32 * 100.0, 20.0), galley, Color32::WHITE)
            })
            .collect();
        let image = render(device, queue, renderer, &mut fonts, &shapes);
        assert_eq!(
            sample(&image, &shapes[0], 0.5, 0.5),
            color.to_array(),
            "color callback must retain its color"
        );
        assert_eq!(
            sample(&image, &shapes[1], 0.5, 0.5),
            Color32::GREEN.to_array(),
            "font outline must retain the text color"
        );
        assert_eq!(
            sample(&image, &shapes[2], 0.5, 0.5),
            Color32::GREEN.to_array(),
            "monochrome callback must use the text color"
        );

        let before = fonts.font_image_resolution();
        fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "A".into(),
            FontId::proportional(400.0),
            Color32::WHITE,
        );
        if color == Color32::RED {
            assert_ne!(
                fonts.font_image_resolution(),
                before,
                "larger callback must grow the image atlas"
            );
        }
        assert_eq!(
            render(device, queue, renderer, &mut fonts, &shapes),
            image,
            "callback atlas growth changed cached glyphs"
        );
    }
}

fn glyph_mesh(fonts: &Fonts, shape: &TextShape) -> Mesh {
    let mut mesh = Mesh::default();
    epaint::Tessellator::new(
        shape.galley.pixels_per_point,
        epaint::TessellationOptions {
            round_text_to_pixels: false,
            ..Default::default()
        },
        fonts.font_image_resolution(),
        vec![],
    )
    .tessellate_text(shape, &mut mesh);
    mesh
}

fn transformed_existing_text(device: &wgpu::Device, queue: &wgpu::Queue, renderer: &mut Renderer) {
    let mut fonts = Fonts::new(
        TextOptions {
            font_hinting: false,
            color_transfer_function: epaint::FontColorTransferFunction::Off,
            ..Default::default()
        },
        test_fonts(),
    );
    let font = FontId::new(8.0, FontFamily::Name("vector-test".into()));
    let galley =
        fonts
            .with_pixels_per_point(1.0)
            .layout_no_wrap("AOQ".into(), font, Color32::WHITE);
    let original = TextShape::new(epaint::Pos2::ZERO, Arc::clone(&galley), Color32::WHITE);
    let original_mesh = glyph_mesh(&fonts, &original);
    if let Some(delta) = fonts.font_textures_delta() {
        apply_textures(renderer, device, queue, delta);
    }
    if let Some(delta) = fonts.font_image_delta() {
        renderer.update_texture(device, queue, TextureId::default(), &delta);
    }

    for scale in [0.0625, 0.125, 0.5, 1.0, 4.0, 16.0, 32.0] {
        for angle in [0.0_f32, 0.37, 1.57, 2.6] {
            let mut cached = original_mesh.clone();
            cached.transform(epaint::emath::TSTransform::from_scaling(scale));
            cached.rotate(epaint::emath::Rot2::from_angle(angle), epaint::Pos2::ZERO);
            let offset = epaint::vec2(20.75, 20.75) - cached.calc_bounds().min.to_vec2();
            cached.translate(offset);

            // A fresh large layout is an independent reference for the transformed
            // small layout; neither may upload another copy of the outline.
            let fresh = fonts.with_pixels_per_point(1.0).layout_no_wrap(
                "AOQ".into(),
                FontId::new(8.0 * scale, FontFamily::Name("vector-test".into())),
                Color32::WHITE,
            );
            let mut fresh_mesh = glyph_mesh(
                &fonts,
                &TextShape::new(epaint::Pos2::ZERO, fresh, Color32::WHITE).with_angle(angle),
            );
            fresh_mesh
                .translate(epaint::vec2(20.75, 20.75) - fresh_mesh.calc_bounds().min.to_vec2());
            assert!(
                fonts.font_image_delta().is_none(),
                "scale {scale} created new curve data"
            );
            let actual = render_mesh(device, queue, renderer, &mut fonts, cached, 1.0);
            let expected = render_mesh(device, queue, renderer, &mut fonts, fresh_mesh, 1.0);
            let mass: u64 = expected.pixels().map(|p| u64::from(p[3])).sum();
            let error: u64 = core::iter::zip(expected.pixels(), actual.pixels())
                .map(|(a, b)| u64::from(a[3].abs_diff(b[3])))
                .sum();
            assert!(mass > 0, "minified glyphs disappeared");
            assert!(
                error as f64 / (mass as f64) < 0.02,
                "scale={scale}, angle={angle}, error={error}, mass={mass}"
            );

            // Exercise TextShape's separate transform path with the same cached galley.
            let mut transformed = original.clone();
            transformed.transform(epaint::emath::TSTransform::from_scaling(scale));
            transformed.angle = angle;
            let mut shape_mesh = glyph_mesh(&fonts, &transformed);
            shape_mesh
                .translate(epaint::vec2(20.75, 20.75) - shape_mesh.calc_bounds().min.to_vec2());
            let via_shape = render_mesh(device, queue, renderer, &mut fonts, shape_mesh, 1.0);
            assert_eq!(
                actual, via_shape,
                "mesh and text transforms differ at scale={scale}, angle={angle}"
            );

            // Scene zoom applies its transform after tessellation, separately
            // from TextShape::transform. It must update the vertex basis too.
            let mut jobs = Vec::new();
            epaint::Tessellator::new(
                1.0,
                epaint::TessellationOptions {
                    round_text_to_pixels: false,
                    ..Default::default()
                },
                fonts.font_image_resolution(),
                vec![],
            )
            .tessellate_clipped_shape(
                epaint::ClippedShape {
                    clip_rect: Rect::EVERYTHING,
                    shape: original.clone().with_angle(angle).into(),
                    transform_after_tessellation: epaint::emath::TSTransform::from_scaling(scale),
                },
                &mut jobs,
            );
            let epaint::Primitive::Mesh(mut scene_mesh) = jobs.remove(0).primitive else {
                unreachable!()
            };
            scene_mesh
                .translate(epaint::vec2(20.75, 20.75) - scene_mesh.calc_bounds().min.to_vec2());
            let via_scene = render_mesh(device, queue, renderer, &mut fonts, scene_mesh, 1.0);
            let difference: u64 = core::iter::zip(actual.pixels(), via_scene.pixels())
                .map(|(a, b)| u64::from(a[3].abs_diff(b[3])))
                .sum();
            assert!(
                difference as f64 / (mass as f64) < 0.02,
                "Scene transform failed: scale={scale}, angle={angle}"
            );
        }
    }

    // At a fractional edge, coverage must reach beyond the undilated geometry.
    // Removing vertex dilation leaves this pixel uncovered even at large sizes.
    let mut edge = original_mesh.clone();
    edge.translate(epaint::vec2(20.75, 20.75) - edge.calc_bounds().min.to_vec2());
    let image = render_mesh(device, queue, renderer, &mut fonts, edge, 1.0);
    let alpha = image.get_pixel(20, 24)[3];
    assert!(
        alpha.abs_diff(64) <= 3,
        "dynamic dilation lost outside-edge coverage: {alpha}"
    );

    if let Some(dir) = std::env::var_os("EGUI_VECTOR_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("capture directory");
        for mode in [FontRendering::Vector, FontRendering::Bitmap] {
            let mut fonts = Fonts::new(
                TextOptions {
                    font_rendering: mode,
                    font_hinting: false,
                    ..Default::default()
                },
                FontDefinitions::default(),
            );
            let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
                "Ag@Q Bézier".into(),
                FontId::proportional(14.0),
                Color32::WHITE,
            );
            let shape = TextShape::new(epaint::Pos2::ZERO, galley, Color32::WHITE);
            let mut mesh = glyph_mesh(&fonts, &shape);
            mesh.transform(epaint::emath::TSTransform::from_scaling(9.0));
            mesh.rotate(epaint::emath::Rot2::from_angle(0.2), epaint::Pos2::ZERO);
            mesh.translate(epaint::vec2(30.0, 30.0) - mesh.calc_bounds().min.to_vec2());
            let image = render_mesh(device, queue, renderer, &mut fonts, mesh, 1.0);
            image
                .save(dir.join(format!("transformed-{mode:?}.png")))
                .expect("write transformed capture");
        }
    }
}

fn color_paints(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    bytes: &[u8],
    text: &str,
    label: &str,
) {
    let mut images = Vec::new();
    for mode in [FontRendering::Vector, FontRendering::Bitmap] {
        let mut definitions = FontDefinitions::empty();
        definitions.font_data.insert(
            "emoji".into(),
            Arc::new(FontData::from_owned(bytes.to_vec())),
        );
        definitions
            .families
            .insert(FontFamily::Proportional, vec!["emoji".into()]);
        let mut fonts = Fonts::new(
            TextOptions {
                font_rendering: mode,
                font_hinting: false,
                color_transfer_function: epaint::FontColorTransferFunction::Off,
                ..Default::default()
            },
            definitions,
        );
        let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            text.into(),
            FontId::proportional(256.0),
            Color32::WHITE,
        );
        assert!(
            galley.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| g.geometry.is_vector() == (mode == FontRendering::Vector)),
            "{label}: glyph rendering mode"
        );
        let shape = TextShape::new(pos2(20.0, 100.0), galley, Color32::WHITE);
        let image = render(device, queue, renderer, &mut fonts, &[shape]);
        if let Some(dir) = std::env::var_os("EGUI_VECTOR_CAPTURE_DIR") {
            image
                .save(std::path::PathBuf::from(dir).join(format!("{label}-{mode:?}.png")))
                .expect("write color capture");
        }
        images.push(image);
    }
    let error: u64 = core::iter::zip(images[0].pixels(), images[1].pixels())
        .map(|(a, b)| {
            core::iter::zip(&a.0, b.0)
                .map(|(x, y)| u64::from(x.abs_diff(y)))
                .sum::<u64>()
        })
        .sum();
    let mass: u64 = images[1]
        .pixels()
        .map(|p| p.0.iter().map(|v| u64::from(*v)).sum::<u64>())
        .sum();
    log::info!(
        "{label} RGBA error / reference: {}",
        error as f64 / mass as f64
    );
    assert!(
        (error as f64 / mass as f64) < 0.03,
        "emoji color or clipping differs from CPU reference"
    );
}

fn mixed_draw_order(device: &wgpu::Device, queue: &wgpu::Queue, renderer: &mut Renderer) {
    let mut fonts = Fonts::new(TextOptions::default(), test_fonts());
    let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
        "A".into(),
        FontId::new(80.0, FontFamily::Name("vector-test".into())),
        Color32::GREEN,
    );
    let shape = TextShape::new(pos2(40.0, 40.0), galley, Color32::WHITE);
    let mut mesh = Mesh::default();
    mesh.add_colored_rect(
        Rect::from_min_max(pos2(30.0, 30.0), pos2(140.0, 140.0)),
        Color32::RED,
    );
    mesh.append(glyph_mesh(&fonts, &shape));
    mesh.add_colored_rect(
        Rect::from_min_max(pos2(60.0, 50.0), pos2(80.0, 80.0)),
        Color32::BLUE,
    );
    let image = render_mesh(device, queue, renderer, &mut fonts, mesh, 1.0);
    assert_eq!(image.get_pixel(35, 35).0, [255, 0, 0, 255], "background");
    assert_eq!(
        image.get_pixel(55, 60).0,
        [0, 255, 0, 255],
        "curve above background"
    );
    assert_eq!(
        image.get_pixel(70, 60).0,
        [0, 0, 255, 255],
        "overlay above curve"
    );
}

#[expect(clippy::iter_over_hash_type)] // Texture IDs are independent.
fn apply_textures(
    renderer: &mut egui_wgpu::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut delta: epaint::textures::TexturesDelta,
) {
    for (id, updates) in delta.set.drain() {
        for update in updates {
            renderer.update_texture(device, queue, id, &update);
        }
    }
    for id in delta.free.drain() {
        renderer.free_texture(&id);
    }
}
