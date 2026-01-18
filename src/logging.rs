use std::thread;
use tracing::{info, error, span, Level};
use tracing_subscriber::{FmtSubscriber, layer::SubscriberExt};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

fn main() {
    // Set up the rolling file appender
    let file_appender = RollingFileAppender::new(Rotation::daily(), "./logs", "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    // Set up the subscriber
    let subscriber = FmtSubscriber::builder()
        .with_writer(non_blocking)
        .finish();
    
    // Set the subscriber as the default
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    // Create threads
    let handles: Vec<_> = (0..5).map(|i| {
        thread::spawn(move || {
            let my_span = span!(Level::INFO, "thread_span", thread_id = i);
            let _enter = my_span.enter();

            info!("This is an info message from thread {}", i);
            if i % 2 == 0 {
                error!("This is an error message from thread {}", i);
            }
        })
    }).collect();

    // Join all threads to ensure they complete
    for handle in handles {
        handle.join().unwrap();
    }
}
