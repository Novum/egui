//! Hidden-window OpenGL regression test on Windows.
#![cfg(all(target_os = "windows", feature = "winit"))]
#![expect(unsafe_code, clippy::undocumented_unsafe_blocks)]

use core::num::NonZeroU32;
use egui::{Color32, FontFamily, FontId, epaint};
use egui_glow::{Painter, ShaderVersion, glow};
use glow::HasContext as _;
use glutin::{display::GetGlDisplay as _, prelude::*};
use std::{ffi::CString, sync::Arc};
use winit::{
    platform::windows::EventLoopBuilderExtWindows as _, raw_window_handle::HasWindowHandle as _,
};

#[test]
#[ignore = "requires an OpenGL driver and desktop session"]
fn vector_glow_rendering() {
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let (window, config) = glutin_winit::DisplayBuilder::new()
        .with_window_attributes(Some(
            winit::window::WindowAttributes::default().with_visible(false),
        ))
        .build(
            &event_loop,
            glutin::config::ConfigTemplateBuilder::new(),
            |mut configs| configs.next().unwrap(),
        )
        .unwrap();
    let window = window.unwrap();
    let raw = window.window_handle().unwrap().as_raw();
    let display = config.display();
    let attrs = glutin::context::ContextAttributesBuilder::new()
        .with_profile(glutin::context::GlProfile::Compatibility)
        .with_context_api(glutin::context::ContextApi::OpenGl(Some(
            glutin::context::Version::new(3, 3),
        )))
        .build(Some(raw));
    let context = unsafe { display.create_context(&config, &attrs).unwrap() };
    let attrs = glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
        .build(
            raw,
            NonZeroU32::new(640).unwrap(),
            NonZeroU32::new(128).unwrap(),
        );
    let surface = unsafe { display.create_window_surface(&config, &attrs).unwrap() };
    let _context = context.make_current(&surface).unwrap();
    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function(|name| {
            display.get_proc_address(&CString::new(name).unwrap())
        })
    });
    unsafe {
        let target = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(target));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            640,
            128,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        let framebuffer = gl.create_framebuffer().unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(target),
            0,
        );
        assert_eq!(
            gl.check_framebuffer_status(glow::FRAMEBUFFER),
            glow::FRAMEBUFFER_COMPLETE
        );
        let mut definitions = epaint::text::FontDefinitions::empty();
        definitions.font_data.insert(
            "test".into(),
            Arc::new(epaint::text::FontData::from_static(include_bytes!(
                "../../epaint/tests/data/vector.ttf"
            ))),
        );
        definitions
            .families
            .insert(FontFamily::Proportional, vec!["test".into()]);
        definitions.font_data.insert(
            "paints".into(),
            Arc::new(epaint::text::FontData::from_static(include_bytes!(
                "../../epaint/tests/data/vector-paints.ttf"
            ))),
        );
        definitions
            .families
            .insert(FontFamily::Name("paints".into()), vec!["paints".into()]);
        definitions.font_data.insert(
            "composite".into(),
            Arc::new(epaint::text::FontData::from_static(include_bytes!(
                "../../epaint/tests/data/vector-composite.ttf"
            ))),
        );
        definitions.families.insert(
            FontFamily::Name("composite".into()),
            vec!["composite".into()],
        );
        let mut fonts = epaint::Fonts::new(
            epaint::TextOptions {
                font_hinting: false,
                color_transfer_function: epaint::FontColorTransferFunction::Off,
                ..Default::default()
            },
            definitions,
        );
        let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AOQ😀F".into(),
            FontId::proportional(80.0),
            Color32::GREEN,
        );
        assert!(
            galley.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| g.geometry.is_vector())
        );
        let shape = epaint::TextShape::new(epaint::pos2(20.0, 15.0), galley, Color32::WHITE);
        let mut mesh = epaint::Mesh::default();
        epaint::Tessellator::new(
            1.0,
            Default::default(),
            fonts.font_image_resolution(),
            vec![],
        )
        .tessellate_text(&shape, &mut mesh);
        let gradient_galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "AQO".into(),
            FontId::new(60.0, FontFamily::Name("paints".into())),
            Color32::WHITE,
        );
        assert!(
            gradient_galley.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| g.geometry.is_vector())
        );
        let gradient_shape =
            epaint::TextShape::new(epaint::pos2(430.0, 15.0), gradient_galley, Color32::WHITE);
        epaint::Tessellator::new(
            1.0,
            Default::default(),
            fonts.font_image_resolution(),
            vec![],
        )
        .tessellate_text(&gradient_shape, &mut mesh);
        let jobs = [epaint::ClippedPrimitive {
            clip_rect: epaint::Rect::EVERYTHING,
            primitive: epaint::Primitive::Mesh(mesh),
        }];
        let delta = fonts.font_image_delta().unwrap();
        let mut font_delta = fonts.font_textures_delta().unwrap();
        for version in [ShaderVersion::Gl140] {
            let mut painter = Painter::new(Arc::clone(&gl), "", Some(version), false).unwrap();
            apply_textures(&mut painter, font_delta.clone());
            painter.set_texture(egui::TextureId::default(), &delta);
            let mut mixed = epaint::Mesh::default();
            mixed.add_colored_rect(
                epaint::Rect::from_min_max(epaint::pos2(10.0, 5.0), epaint::pos2(110.0, 100.0)),
                Color32::RED,
            );
            let epaint::Primitive::Mesh(glyphs) = &jobs[0].primitive else {
                unreachable!()
            };
            mixed.append(glyphs.clone());
            mixed.add_colored_rect(
                epaint::Rect::from_min_max(epaint::pos2(40.0, 30.0), epaint::pos2(60.0, 60.0)),
                Color32::BLUE,
            );
            painter.paint_primitives(
                [640, 128],
                1.0,
                &[epaint::ClippedPrimitive {
                    clip_rect: epaint::Rect::EVERYTHING,
                    primitive: epaint::Primitive::Mesh(mixed),
                }],
            );
            for (x, y, expected) in [
                (15, 10, [255, 0, 0, 255]),
                (35, 40, [0, 255, 0, 255]),
                (50, 40, [0, 0, 255, 255]),
            ] {
                let mut pixel = [0; 4];
                gl.read_pixels(
                    x,
                    127 - y,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixel)),
                );
                assert_eq!(pixel, expected, "{version:?}: mixed draw order at {x},{y}");
            }
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            painter.paint_primitives([640, 128], 1.0, &jobs);
            let mut pixels = vec![0; 640 * 128 * 4];
            gl.read_pixels(
                0,
                0,
                640,
                128,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            let sample = |index: usize, x: f32, y: f32| {
                let glyph = &shape.galley.rows[0].row.glyphs[index];
                let top = shape.pos + glyph.pos.to_vec2() + glyph.geometry.offset;
                let p = top + glyph.geometry.size * epaint::vec2(x, y);
                let offset = ((127 - p.y as usize) * 640 + p.x as usize) * 4;
                <[u8; 4]>::try_from(&pixels[offset..offset + 4]).unwrap()
            };
            assert_eq!(sample(0, 0.5, 0.5), [0, 255, 0, 255]);
            assert_eq!(sample(1, 0.5, 0.5), [0; 4]);
            assert_eq!(sample(2, 0.5, 0.5), [0, 255, 0, 255]);
            assert_eq!(sample(3, 0.5, 0.75), [255, 0, 0, 255]);
            let upper = sample(3, 0.5, 0.25);
            assert!(
                upper[0].abs_diff(127) <= 1 && upper[2].abs_diff(128) <= 1 && upper[3] == 255,
                "{version:?}: {upper:?}"
            );
            assert_eq!(sample(4, 0.15, 0.5), [0, 255, 0, 255]);
            let gradient_sample = |index: usize, x: f32, y: f32| {
                let glyph = &gradient_shape.galley.rows[0].row.glyphs[index];
                let top = gradient_shape.pos + glyph.pos.to_vec2() + glyph.geometry.offset;
                let p = top + glyph.geometry.size * epaint::vec2(x, y);
                let offset = ((127 - p.y as usize) * 640 + p.x as usize) * 4;
                <[u8; 4]>::try_from(&pixels[offset..offset + 4]).unwrap()
            };
            assert_eq!(
                gradient_sample(0, 0.05, 0.5),
                [0; 4],
                "clip box must cut the outline"
            );
            let linear = gradient_sample(0, 0.25, 0.5);
            assert!(
                linear[0].abs_diff(191) < 8 && linear[2].abs_diff(32) < 8,
                "linear gradient: {linear:?}"
            );
            let radial = gradient_sample(1, 0.5, 0.5);
            assert!(
                radial[0] > 240 && radial[2] < 8,
                "radial gradient: {radial:?}"
            );
            let sweep = gradient_sample(2, 0.75, 0.75);
            assert!(sweep[2] > sweep[0], "sweep gradient direction: {sweep:?}");
            let epaint::Primitive::Mesh(original_mesh) = &jobs[0].primitive else {
                unreachable!()
            };
            for scale in [0.125, 1.0, 2.0] {
                let mut mesh = original_mesh.clone();
                mesh.transform(epaint::emath::TSTransform::from_scaling(scale));
                mesh.translate(epaint::vec2(20.75, 20.75) - mesh.calc_bounds().min.to_vec2());
                let transformed = [epaint::ClippedPrimitive {
                    clip_rect: epaint::Rect::EVERYTHING,
                    primitive: epaint::Primitive::Mesh(mesh),
                }];
                gl.clear(glow::COLOR_BUFFER_BIT);
                painter.paint_primitives([640, 128], 1.0, &transformed);
                let mut pixel = [0; 4];
                gl.read_pixels(
                    20,
                    127 - 24,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixel)),
                );
                assert!(
                    pixel[3].abs_diff(64) <= 3,
                    "{version:?}, scale={scale}: outside-edge coverage {pixel:?}"
                );
            }
            let composite = fonts.with_pixels_per_point(1.0).layout_no_wrap(
                "A".into(),
                FontId::new(80.0, FontFamily::Name("composite".into())),
                Color32::WHITE,
            );
            let glyph = &composite.rows[0].row.glyphs[0];
            assert!(glyph.is_color && glyph.geometry.is_vector());
            let sample = epaint::pos2(20.0, 15.0)
                + glyph.pos.to_vec2()
                + glyph.geometry.offset
                + glyph.geometry.size * 0.5;
            let shape = epaint::TextShape::new(epaint::pos2(20.0, 15.0), composite, Color32::WHITE);
            let mut mesh = epaint::Mesh::default();
            epaint::Tessellator::new(
                1.0,
                Default::default(),
                fonts.font_image_resolution(),
                vec![],
            )
            .tessellate_text(&shape, &mut mesh);
            apply_textures(&mut painter, fonts.font_textures_delta().unwrap());
            gl.clear(glow::COLOR_BUFFER_BIT);
            painter.paint_primitives(
                [640, 128],
                1.0,
                &[epaint::ClippedPrimitive {
                    clip_rect: epaint::Rect::EVERYTHING,
                    primitive: epaint::Primitive::Mesh(mesh),
                }],
            );
            let mut pixel = [0; 4];
            gl.read_pixels(
                sample.x as i32,
                127 - sample.y as i32,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixel)),
            );
            assert!(
                pixel[0].abs_diff(48) <= 1
                    && pixel[3].abs_diff(48) <= 1
                    && pixel[1] == 0
                    && pixel[2] == 0,
                "source-in group: {pixel:?}"
            );
            painter.free_texture(epaint::TextureId::FontCurves);
            painter.free_texture(epaint::TextureId::FontBands);
            painter.paint_primitives([640, 128], 1.0, &[]);
            for unit in [glow::TEXTURE1, glow::TEXTURE2] {
                gl.active_texture(unit);
                assert_eq!(
                    gl.get_tex_level_parameter_i32(glow::TEXTURE_2D, 0, glow::TEXTURE_WIDTH),
                    1
                );
                assert_eq!(
                    gl.get_tex_level_parameter_i32(glow::TEXTURE_2D, 0, glow::TEXTURE_HEIGHT),
                    1
                );
            }
            gl.active_texture(glow::TEXTURE0);
            apply_textures(&mut painter, font_delta.clone());
            gl.clear(glow::COLOR_BUFFER_BIT);
            painter.paint_primitives([640, 128], 1.0, &jobs);
            let mut pixel = [0; 4];
            gl.read_pixels(
                35,
                127 - 40,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixel)),
            );
            assert_eq!(
                pixel,
                [0, 255, 0, 255],
                "vector resources restored after bitmap mode"
            );
            painter.destroy();
        }
        font_delta.clear();
        fonts.begin_pass(
            epaint::TextOptions {
                font_rendering: epaint::text::FontRendering::Bitmap,
                ..Default::default()
            },
            epaint::text::ViewportKey::default(),
        );
        let galley = fonts.with_pixels_per_point(1.0).layout_no_wrap(
            "A".into(),
            FontId::proportional(80.0),
            Color32::GREEN,
        );
        assert!(
            galley.rows[0]
                .row
                .glyphs
                .iter()
                .all(|g| !g.geometry.is_vector())
        );
        let shape = epaint::TextShape::new(epaint::pos2(20.0, 15.0), galley, Color32::WHITE);
        let mut mesh = epaint::Mesh::default();
        epaint::Tessellator::new(
            1.0,
            Default::default(),
            fonts.font_image_resolution(),
            vec![],
        )
        .tessellate_text(&shape, &mut mesh);
        let mut painter =
            Painter::new(Arc::clone(&gl), "", Some(ShaderVersion::Gl120), false).unwrap();
        assert!(!painter.supports_vector_fonts());
        apply_textures(&mut painter, fonts.font_textures_delta().unwrap());
        painter.set_texture(
            egui::TextureId::default(),
            &fonts.font_image_delta().unwrap(),
        );
        gl.clear(glow::COLOR_BUFFER_BIT);
        painter.paint_primitives(
            [640, 128],
            1.0,
            &[epaint::ClippedPrimitive {
                clip_rect: epaint::Rect::EVERYTHING,
                primitive: epaint::Primitive::Mesh(mesh),
            }],
        );
        let mut pixel = [0; 4];
        gl.read_pixels(
            35,
            127 - 40,
            1,
            1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixel)),
        );
        assert_eq!(
            pixel,
            [0, 255, 0, 255],
            "legacy shader renders bitmap glyphs"
        );
        assert_eq!(gl.get_error(), glow::NO_ERROR);
        painter.destroy();
        gl.delete_framebuffer(framebuffer);
        gl.delete_texture(target);
    }
}

#[expect(clippy::iter_over_hash_type)] // Texture IDs are independent.
fn apply_textures(
    painter: &mut egui_glow::Painter,
    mut delta: egui::epaint::textures::TexturesDelta,
) {
    for (id, updates) in delta.set.drain() {
        for update in updates {
            painter.set_texture(id, &update);
        }
    }
    for id in delta.free.drain() {
        painter.free_texture(id);
    }
}
