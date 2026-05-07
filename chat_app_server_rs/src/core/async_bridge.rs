use std::future::Future;

pub fn block_on_result<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
        runtime.block_on(future)
    }
}

pub fn block_on_option<F, T>(future: F) -> Option<T>
where
    F: Future<Output = Option<T>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Runtime::new().ok()?;
        runtime.block_on(future)
    }
}
