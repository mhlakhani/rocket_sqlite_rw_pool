/// A wrapper around [`spawn_blocking`] that propagates panics to the calling code.
pub async fn run_blocking<F, R>(job: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    match tokio::task::spawn_blocking(job).await {
        Ok(ret) => ret,
        Err(e) => e.try_into_panic().map_or_else(
            |_| unreachable!("spawn_blocking tasks are never cancelled"),
            |panic| std::panic::resume_unwind(panic),
        ),
    }
}
