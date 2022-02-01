use core::future::Future;
use std::time::Duration;
use tokio::time::interval;

pub async fn spawn_timer<F, Fut>(time_interval: Duration, f: F)
where
    F: Fn() -> Fut,
    //F: Send + Sync + 'static,
    Fut: Future,
{
    let mut task = interval(time_interval);
    loop {
        task.tick().await;
        f().await;
    }
}
