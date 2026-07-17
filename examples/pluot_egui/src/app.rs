use std::f32::consts::PI;
use std::sync::Arc;

use eframe::egui;
use pluot::{
    render, GraphicsFormat, LayerParams, PointLayerParams, PointShapeMode, RenderParams, UnitsMode,
};

pub struct PluotApp {
    texture: egui::TextureHandle,
    plot_width: u32,
    plot_height: u32,
    rt: tokio::runtime::Runtime,
    frequency: f32,
    /// Exponent for the number of points: actual count = 10^num_points_exp (1.0–7.0 --> 10–10M).
    num_points_exp: f32,
    point_radius: f32,
}

impl PluotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let plot_width: u32 = 800;
        let plot_height: u32 = 600;
        let frequency = 2.0f32;
        let num_points_exp = 2.3f32; // ~200 points
        let point_radius = 6.0f32;

        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let pixels = rt.block_on(render(build_params(
            plot_width, plot_height, frequency, num_points_exp, point_radius,
        )));
        let texture = cc.egui_ctx.load_texture(
            "pluot-plot",
            pixels_to_image(plot_width, plot_height, &pixels),
            egui::TextureOptions::LINEAR,
        );

        Self {
            texture,
            plot_width,
            plot_height,
            rt,
            frequency,
            num_points_exp,
            point_radius,
        }
    }

    fn rerender(&mut self) {
        let pixels = self.rt.block_on(render(build_params(
            self.plot_width,
            self.plot_height,
            self.frequency,
            self.num_points_exp,
            self.point_radius,
        )));
        self.texture.set(
            pixels_to_image(self.plot_width, self.plot_height, &pixels),
            egui::TextureOptions::LINEAR,
        );
    }
}

impl eframe::App for PluotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Sine wave");

            let freq_changed = ui
                .add(
                    egui::Slider::new(&mut self.frequency, 0.5..=8.0)
                        .text("Frequency (periods)")
                        .step_by(0.1),
                )
                .changed();

            let num_points = exp_to_num_points(self.num_points_exp);
            let points_changed = ui
                .add(
                    egui::Slider::new(&mut self.num_points_exp, 1.0..=5.0)
                        .text(format!("Points (10^exp = {num_points})"))
                        .step_by(0.01),
                )
                .changed();

            let radius_changed = ui
                .add(
                    egui::Slider::new(&mut self.point_radius, 1.0..=20.0)
                        .text("Point size (px)")
                        .step_by(0.5),
                )
                .changed();

            if freq_changed || points_changed || radius_changed {
                self.rerender();
            }

            ui.add(egui::Image::new(&self.texture).max_size(egui::vec2(
                self.plot_width as f32,
                self.plot_height as f32,
            )));
        });
    }
}

fn exp_to_num_points(exp: f32) -> usize {
    (10f32.powf(exp).round() as usize).max(1)
}

fn build_params(
    plot_width: u32,
    plot_height: u32,
    frequency: f32,
    num_points_exp: f32,
    point_radius: f32,
) -> RenderParams {
    let num_points = exp_to_num_points(num_points_exp);
    let position_x: Vec<f32> = (0..num_points)
        .map(|i| i as f32 / (num_points - 1).max(1) as f32)
        .collect();
    let position_y: Vec<f32> = position_x
        .iter()
        .map(|&x| 0.5 + 0.4 * (x * frequency * 2.0 * PI).sin())
        .collect();

    RenderParams {
        layers: vec![LayerParams::PointLayer(PointLayerParams {
            layer_id: "sine".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            point_radius,
            point_radius_unit_mode_x: UnitsMode::Pixels,
            point_radius_unit_mode_y: UnitsMode::Pixels,
            point_shape_mode: PointShapeMode::Circle,
            model_matrix: None,
            position_x: Arc::new(position_x),
            position_y: Arc::new(position_y),
            labels_vec: Arc::new(vec![0; num_points]),
            ..Default::default()
        })],
        width: plot_width,
        height: plot_height,
        format: GraphicsFormat::Raster,
        cache_enabled: false,
        ..Default::default()
    }
}

fn pixels_to_image(width: u32, height: u32, pixels: &[u8]) -> egui::ColorImage {
    // The last byte is the `bailed_early` flag. Strip it before loading the texture.
    let pixel_data = &pixels[..pixels.len() - 1];
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], pixel_data)
}
