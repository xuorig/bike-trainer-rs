use bluest::{
    btuuid::{self, characteristics::INDOOR_BIKE_DATA, services::FITNESS_MACHINE},
    Adapter, AdvertisingDevice,
};
use eframe::egui;
use futures_lite::{Stream, StreamExt};
use tokio::sync::mpsc::{self, Receiver};
use tracing::error;

#[derive(Clone)]
pub(crate) struct BT {
    adapter: Adapter,
}

impl BT {
    pub async fn init() -> Result<Self, bluest::Error> {
        let adapter = Adapter::default()
            .await
            .ok_or("Bluetooth adapter not found")
            .unwrap();
        adapter.wait_available().await?;
        Ok(Self { adapter })
    }

    pub async fn discover_devices<'a>(
        &'a mut self,
    ) -> Result<impl Stream<Item = AdvertisingDevice> + 'a, bluest::Error> {
        let services = &[btuuid::services::FITNESS_MACHINE];
        self.adapter.scan(services).await
    }

    pub async fn connect(
        &self,
        device: AdvertisingDevice,
        ctx: egui::Context,
    ) -> Result<Receiver<TrainerUpdate>, bluest::Error> {
        self.adapter.connect_device(&device.device).await?;

        let (tx, rx) = mpsc::channel(1024);

        tokio::spawn(async move {
            let services = device.device.services().await.unwrap();
            let ftms = services
                .iter()
                .find(|s| s.uuid() == FITNESS_MACHINE)
                .unwrap();

            let characteristics = ftms.characteristics().await.unwrap();

            let bike_data = characteristics
                .iter()
                .find(|c| c.uuid() == INDOOR_BIKE_DATA)
                .unwrap();

            let mut stream = bike_data.notify().await.unwrap();

            while let Some(update) = stream.next().await {
                if let Ok(update) = update {
                    let speed = u16::from_le_bytes([update[2], update[3]]);
                    let power = u16::from_le_bytes([update[4], update[5]]);

                    if let Err(_) = tx.send(TrainerUpdate::Power { speed, power }).await {
                        // Handle the error if the receiver is closed.
                        error!("Channel closed");
                        break;
                    }

                    ctx.request_repaint();
                }
            }
        });

        Ok(rx)
    }
}

#[derive(Debug)]
pub(crate) enum TrainerUpdate {
    Power { speed: u16, power: u16 },
}
