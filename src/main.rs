use std::error::Error;

use tracing_subscriber;

mod app;
mod trainer;

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    app::run()?;
    Ok(())
}
