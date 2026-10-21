//! Transform an existing text layout to compare curve and bitmap rendering.

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct FontRenderingDemo {
    scale: f32,
    text: String,
    angle: f32,
}

impl Default for FontRenderingDemo {
    fn default() -> Self {
        Self {
            scale: 4.0,
            angle: 8.0,
            text: "AV fi ffi office\nAg@Q a\u{301} 0123 😀".into(),
        }
    }
}

impl crate::Demo for FontRenderingDemo {
    fn name(&self) -> &'static str {
        "🔠 Font rendering"
    }

    fn show(&mut self, ui: &mut egui::Ui, open: &mut bool) {
        egui::Window::new(self.name())
            .open(open)
            .default_size([650.0, 400.0])
            .show(ui.ctx(), |ui| {
                use crate::View as _;
                self.ui(ui);
            });
    }
}

impl crate::View for FontRenderingDemo {
    fn ui(&mut self, ui: &mut egui::Ui) {
        use egui::epaint::text::FontRendering;
        ui.horizontal(|ui| {
            ui.label("Font rendering:");
            if ui.input(|input| input.vector_fonts) {
                let mut mode = ui.ctx().global_style().visuals.text_options.font_rendering;
                ui.selectable_value(&mut mode, FontRendering::Vector, "Vector");
                ui.selectable_value(&mut mode, FontRendering::Bitmap, "Bitmap");
                ui.ctx()
                    .all_styles_mut(|style| style.visuals.text_options.font_rendering = mode);
            } else {
                ui.label("Bitmap");
            }
        });
        ui.add(
            egui::Slider::new(&mut self.scale, 0.0625..=32.0)
                .logarithmic(true)
                .suffix("×")
                .text("Scale"),
        );
        ui.add(egui::Slider::new(&mut self.angle, -180.0..=180.0).text("Rotation (degrees)"));
        let scale = self.scale;
        ui.label(format!(
            "14 pt layout × {scale:.2}. Scaling and rotation reuse the original layout."
        ));
        ui.label("Zoom in to compare sharp curves against enlarged bitmap pixels.");
        let label = ui.label("Sample text:");
        ui.text_edit_multiline(&mut self.text).labelled_by(label.id);
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        // The layout key stays fixed while the transform controls change.
        let galley = painter.layout_no_wrap(
            self.text.clone(),
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        let mut shape =
            egui::epaint::TextShape::new(egui::Pos2::ZERO, galley, ui.visuals().text_color());
        shape.transform(egui::emath::TSTransform::from_scaling(scale));
        shape.angle = self.angle.to_radians();
        let center = shape.visual_bounding_rect().center();
        shape.pos += rect.center() - center;
        painter.add(shape);
    }
}
