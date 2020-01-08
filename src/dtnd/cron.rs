use std::time::Duration;
use tokio::time::interval;

pub async fn spawn_timer<F>(millis: u64, f: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let mut task = interval(Duration::from_millis(millis));
    loop {
        task.tick().await;
        f();
    }
}
