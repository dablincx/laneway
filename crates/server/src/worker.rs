//! Background worker. Will run the job queue (`laneway-jobs`) once it exists.

use anyhow::Result;
use tokio::sync::watch;

pub async fn run(mut stop: watch::Receiver<bool>) -> Result<()> {
    tracing::info!("worker started (no jobs yet)");
    let _ = stop.wait_for(|stopped| *stopped).await;
    tracing::info!("worker stopped");
    Ok(())
}
