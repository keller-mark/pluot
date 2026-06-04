#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;

use tokio::sync::mpsc;

use crate::app::{pluot_handler, PlotEvents};

mod app;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;

    let (tx, rx) = mpsc::channel(8);
    // Note: Spawning this in local thread means the UI blocks whenever an update comes in.
    slint::spawn_local(async_compat::Compat::new(pluot_handler(ui.as_weak(), rx))).unwrap();

    ui.on_frequency_changed({
        let tx = tx.clone();
        move |f| {
            tx.try_send(PlotEvents::FrequencyChanged(f)).unwrap();
        }
    });

    ui.on_num_points_exp_changed({
        let tx = tx.clone();
        move |f| {
            tx.try_send(PlotEvents::NumPointsExpChanged(f)).unwrap();
        }
    });

    ui.on_point_size_changed({
        let tx = tx.clone();
        move |f| {
            tx.try_send(PlotEvents::PointRadiusChanged(f)).unwrap();
        }
    });

    ui.run()?;

    tx.try_send(PlotEvents::Quit)?;
    slint::run_event_loop_until_quit()?; // wait for task to finish.

    Ok(())
}
