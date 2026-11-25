//! Windows service ownership, separate from the agent's restricted shell runner.
//! An installed service is started by the desktop user, never via agent tool bypass.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::ServiceProcess;

#[cfg(not(windows))]
pub struct ServiceProcess;
#[cfg(not(windows))]
impl ServiceProcess {
    pub fn spawn(_: &super::service::ServiceLaunch, _withhold: &[String]) -> Result<Self, String> {
        Err("外部插件服务目前仅支持 Windows 桌面".into())
    }
    pub fn pid(&self) -> u32 {
        0
    }
    pub fn exit_code(&self) -> Result<Option<u32>, String> {
        Ok(Some(0))
    }
    pub fn stop(&self) -> Result<(), String> {
        Ok(())
    }
}
