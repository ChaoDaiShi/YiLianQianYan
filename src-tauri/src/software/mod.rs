//! Native software integration. No title harvesting, process termination or input simulation.
#[cfg(windows)]
mod automation;
#[cfg(windows)]
pub(crate) mod chat_capture;
#[cfg(windows)]
mod chat_draft;
#[cfg(windows)]
mod dock;
#[cfg(windows)]
mod geometry;
#[cfg(windows)]
mod identity;
#[cfg(windows)]
mod monitor;
#[cfg(windows)]
mod mount;
#[cfg(windows)]
mod notifications;
#[cfg(windows)]
pub use mount::NativeSoftwareHost;

#[cfg(all(windows, debug_assertions, feature = "native-software-fixture"))]
pub fn fixture_candidate(
    hwnd: usize,
) -> Result<yilian_backend::software::WindowCandidate, yilian_backend::software::SoftwareError> {
    identity::candidate(windows::Win32::Foundation::HWND(hwnd as *mut _))
}

#[cfg(not(windows))]
pub struct NativeSoftwareHost;
#[cfg(not(windows))]
impl NativeSoftwareHost {
    pub fn new() -> Self {
        Self
    }
    pub fn set_parent(&self, _: u64) -> Result<(), String> {
        Err("Software mounting requires Windows".into())
    }
    pub fn restore_embedded(&self) -> Result<(), String> {
        Ok(())
    }
    pub fn suspend_embedding(&self) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(not(windows))]
impl yilian_backend::software::SoftwareHost for NativeSoftwareHost {
    fn available(&self) -> bool {
        false
    }
    fn discover(
        &self,
        _: &yilian_backend::software::Registration,
    ) -> Result<
        Vec<yilian_backend::software::WindowCandidate>,
        yilian_backend::software::SoftwareError,
    > {
        Err(unavailable())
    }
    fn launch(
        &self,
        _: &yilian_backend::software::Registration,
    ) -> Result<yilian_backend::software::WindowCandidate, yilian_backend::software::SoftwareError>
    {
        Err(unavailable())
    }
    fn verify(
        &self,
        _: &yilian_backend::software::WindowIdentity,
    ) -> Result<yilian_backend::software::WindowState, yilian_backend::software::SoftwareError>
    {
        Err(unavailable())
    }
    fn present(
        &self,
        _: &yilian_backend::software::WindowIdentity,
        _: &str,
        _: Option<yilian_backend::software::Viewport>,
    ) -> Result<yilian_backend::software::WindowState, yilian_backend::software::SoftwareError>
    {
        Err(unavailable())
    }
    fn release(
        &self,
        _: &yilian_backend::software::WindowIdentity,
    ) -> Result<(), yilian_backend::software::SoftwareError> {
        Ok(())
    }
    fn observe(
        &self,
        _: &yilian_backend::software::WindowIdentity,
        _: &str,
        _: bool,
    ) -> Result<
        Vec<yilian_backend::software::SoftwareControl>,
        yilian_backend::software::SoftwareError,
    > {
        Err(unavailable())
    }
    fn control(
        &self,
        _: &yilian_backend::software::WindowIdentity,
        _: &yilian_backend::software::SoftwareControl,
        _: &str,
        _: Option<&str>,
        _: &std::sync::atomic::AtomicBool,
    ) -> Result<(), yilian_backend::software::SoftwareError> {
        Err(unavailable())
    }
    fn shutdown(&self) -> Result<(), yilian_backend::software::SoftwareError> {
        Ok(())
    }
}
#[cfg(not(windows))]
fn unavailable() -> yilian_backend::software::SoftwareError {
    yilian_backend::software::SoftwareError::new(
        "platform_unavailable",
        "Native software integration currently requires Windows",
    )
}
