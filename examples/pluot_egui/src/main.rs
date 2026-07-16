mod app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pluot egui example",
        options,
        Box::new(|cc| Ok(Box::new(app::PluotApp::new(cc)))),
    )
}
