use std::collections::HashMap;

use bluest::AdvertisingDevice;
use eframe::{egui::{self, Ui, RichText}, epaint::Color32};
use egui_plot::{BarChart, Bar, Legend, Plot};
use futures_lite::StreamExt;
use tokio::{
    runtime::Runtime,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

use crate::trainer::{TrainerUpdate, BT};

pub(crate) fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native(
        "Simple Trainer 0.1",
        options,
        Box::new(|_cc| Box::<App>::default()),
    )
}

struct App {
    rt: Runtime,
    bt: BT,
    discover_rx: Option<mpsc::Receiver<AdvertisingDevice>>,
    discover_stop: Option<oneshot::Sender<()>>,
    devices: HashMap<String, AdvertisingDevice>,
    connecting: bool,
    connected_device: Option<Receiver<TrainerUpdate>>,
    connected_rx: Option<oneshot::Receiver<Receiver<TrainerUpdate>>>,
    current_speed: u16,
    current_power: u16,
    historical_speeds: Vec<u16>,
    historical_powers: Vec<u16>,
}

impl Default for App {
    fn default() -> Self {
        let rt = Runtime::new().unwrap();

        let bt = rt.block_on(async { BT::init().await.unwrap() });

        Self {
            rt,
            bt,
            discover_rx: None,
            discover_stop: None,
            devices: HashMap::new(),
            connecting: false,
            connected_device: None,
            connected_rx: None,
            current_speed: 0,
            current_power: 0,
            historical_speeds: vec![],
            historical_powers: vec![],
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ctx.set_pixels_per_point(5.0);
            match &self.connected_device {
                Some(_) => {
                    self.render_connected_screen(ui);
                }
                None => {
                    self.render_setup_screen(ui, ctx);
                }
            };

            self.update_discovery()
        });
    }
}

impl App {
    fn render_connected_screen(&mut self, ui: &mut Ui) {
        ui.heading("Simple Trainer 0.1");

        ui.horizontal(|ui| {
            ui.label("Speed: ");
            ui.label(RichText::new(format!("{} km/h", self.current_speed / 100)).color(Color32::GREEN));
        });

        ui.horizontal(|ui| {
            ui.label("Power: ");
            ui.label(RichText::new(format!("{} watts", self.current_power)).color(Color32::GREEN));
        });

        let bars = self.historical_powers.iter().enumerate().map(|(i, p)| {
            Bar::new(i as f64, *p as f64)
        }).collect();

        let chart = BarChart::new(bars);

        Plot::new("Power")
            .legend(Legend::default())
            .clamp_grid(true)
            .y_axis_width(3)
            .show(ui, |plot_ui| plot_ui.bar_chart(chart))
            .response;
    }

    fn render_setup_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.heading("Simple Trainer 0.1");

        match self.discover_rx {
            Some(_) => {
                if ui.button("Stop Discovery").clicked() {
                    self.stop_discover();
                }
            }
            None => {
                if ui.button("Discover").clicked() {
                    self.start_discover();
                }
            }
        }

        let devices = self.devices.clone();

        if self.connecting {
            ui.horizontal(|ui| {
                ui.spinner();
            });
        } else {
            devices.keys().for_each(|k| {
                ui.horizontal(|ui| {
                    if ui.link(k.clone()).clicked() {
                        self.connect(k.clone(), ctx);
                    }
                });
            });
        }
    }

    fn update_discovery(&mut self) {
        if let Some(ref mut rx) = self.connected_device {
            if let Ok(update) = rx.try_recv() {
                match update {
                    TrainerUpdate::Power { speed, power } => {
                        self.current_speed = speed;
                        self.current_power = power;
                        self.historical_powers.push(power);
                        self.historical_speeds.push(speed);
                    }
                }
            }
        }

        if let Some(ref mut rx) = self.discover_rx {
            if let Ok(device) = rx.try_recv() {
                let name = device.device.name().unwrap_or("UNKNOWN".into());
                self.devices.insert(name, device);
            }
        }

        if let Some(ref mut rx) = self.connected_rx {
            if let Ok(connected) = rx.try_recv() {
                tracing::info!("Updated with connection");
                self.connected_device = Some(connected);
                self.connecting = false;
            }
        }
    }

    fn start_discover(&mut self) {
        let (tx, rx) = mpsc::channel(1024);
        let (tx_stop, mut rx_stop) = oneshot::channel();

        let mut bt = self.bt.clone();

        let _discover_task = self.rt.spawn(async move {
            let mut device_stream = bt.discover_devices().await.unwrap();

            loop {
                tokio::select! {
                    Some(device) = device_stream.next() => {
                        tracing::debug!("{:?}", device);
                        tx.send(device).await.unwrap();
                    }
                    _ = &mut rx_stop => {
                        tracing::info!("Received stop signal. Stopping the task.");
                        break;
                    }
                }
            }
        });

        self.discover_rx = Some(rx);
        self.discover_stop = Some(tx_stop);
    }

    fn stop_discover(&mut self) {
        self.devices.clear();

        let tx = self.discover_stop.take();

        if let Some(tx) = tx {
            tx.send(()).unwrap();
        }
    }

    fn connect(&mut self, device: String, ctx: &egui::Context) {
        tracing::info!("Connecting to {}", device);

        self.connecting = true;

        let (tx, rx) = oneshot::channel();
        let device = self.devices[&device].clone();
        let bt = self.bt.clone();

        self.connected_rx = Some(rx);
        let ctx = ctx.clone();

        self.rt.spawn(async move {
            let trainer = bt.connect(device, ctx).await.unwrap();
            tracing::info!("Connection successful");
            match tx.send(trainer) {
                Ok(_) => {
                    tracing::info!("SENT");
                }
                Err(e) => {
                    tracing::error!("ERROR {:?}", e);
                }
            }
        });
    }
}
