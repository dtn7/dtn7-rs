use std::time::Duration;
use tokio::time::interval;

pub async fn spawn_timer<F>(time_interval: Duration, f: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let mut task = interval(time_interval);
    loop {
        task.tick().await;
        f();
    }
}
