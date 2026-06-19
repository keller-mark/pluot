use std::{f32::consts::PI, sync::Arc};

use pluot::{
    render, GraphicsFormat, LayerParams, PointLayerParams, PointShapeMode, RenderParams, UnitsMode,
};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, Weak};
use tokio::sync::mpsc::Receiver;

use crate::AppWindow;

pub enum PlotEvents {
    FrequencyChanged(f32),
    NumPointsExpChanged(f32),
    PointRadiusChanged(f32),
    Quit,
}

pub async fn pluot_handler(ui: Weak<AppWindow>, mut rx: Receiver<PlotEvents>) {
    let mut pl = PluotApp::new();

    render_and_set(pl.get_build_params(), ui.clone()).await;

    loop {
        if let Some(msg) = rx.recv().await {
            let update = match msg {
                PlotEvents::FrequencyChanged(val) => pl.set_frequency(val),
                PlotEvents::NumPointsExpChanged(val) => pl.set_num_points_exp(val),
                PlotEvents::PointRadiusChanged(val) => pl.set_point_radius(val),
                PlotEvents::Quit => {
                    slint::quit_event_loop().unwrap();
                    return;
                }
            };
            if update {
                render_and_set(pl.get_build_params(), ui.clone()).await;
            }
        } else {
            eprintln!("Channel is closed.");
            break;
        }
    }
}

struct PluotApp {
    plot_width: u32,
    plot_height: u32,
    frequency: f32,
    /// Exponent for the number of points: actual count = 10^num_points_exp (1.0–7.0 --> 10–10M).
    num_points_exp: f32,
    point_radius: f32,
}

impl PluotApp {
    fn new() -> Self {
        Self {
            plot_width: 800,
            plot_height: 600,
            frequency: 2.0,
            num_points_exp: 2.3,
            point_radius: 6.0,
        }
    }

    fn get_build_params(&self) -> RenderParams {
        let num_points = exp_to_num_points(self.num_points_exp);
        let position_x: Vec<f32> = (0..num_points)
            .map(|i| i as f32 / (num_points - 1).max(1) as f32)
            .collect();
        let position_y: Vec<f32> = position_x
            .iter()
            .map(|&x| 0.5 + 0.4 * (x * self.frequency * 2.0 * PI).sin())
            .collect();

        RenderParams {
            layers: vec![LayerParams::PointLayer(PointLayerParams {
                layer_id: "sine".to_string(),
                bounds: None,
                data_unit_mode_x: UnitsMode::Data,
                data_unit_mode_y: UnitsMode::Data,
                point_radius: self.point_radius,
                point_radius_unit_mode_x: UnitsMode::Pixels,
                point_radius_unit_mode_y: UnitsMode::Pixels,
                point_shape_mode: PointShapeMode::Circle,
                model_matrix: None,
                position_x: Arc::new(position_x),
                position_y: Arc::new(position_y),
                labels_vec: Arc::new(vec![0; num_points]),
                ..Default::default()
            })],
            width: self.plot_width,
            height: self.plot_height,
            format: GraphicsFormat::Raster,
            cache_enabled: false,
            ..Default::default()
        }
    }

    /// Check if frequency value is different from last, and if so, update.
    fn set_frequency(&mut self, val: f32) -> bool {
        if self.frequency == val {
            false
        } else {
            self.frequency = val;
            true
        }
    }

    /// Check if num_points_exp value is different from last and if so, update.
    fn set_num_points_exp(&mut self, val: f32) -> bool {
        if self.num_points_exp == val {
            false
        } else {
            self.num_points_exp = val;
            true
        }
    }

    /// Check if point_radius is different from alst and if so, update.
    fn set_point_radius(&mut self, val: f32) -> bool {
        if self.point_radius == val {
            false
        } else {
            self.point_radius = val;
            true
        }
    }
}

fn exp_to_num_points(exp: f32) -> usize {
    (10f32.powf(exp).round() as usize).max(1)
}

fn pixels_to_image(width: u32, height: u32, pixels: &[u8]) -> Image {
    // The last byte is the `bailed_early` flag
    let pixel_data = &pixels[..pixels.len() - 1];
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pixel_data, width, height);
    Image::from_rgba8(buffer)
}

async fn render_and_set(params: RenderParams, ui: Weak<AppWindow>) {
    let width = params.width;
    let height = params.height;
    let pixels = render(params).await;
    ui.upgrade_in_event_loop(move |ui| {
        let img = pixels_to_image(width, height, &pixels);
        ui.set_figure(img);
    })
    .unwrap();
}
