use serde::Deserialize;
use std::time::Duration;

#[derive(PartialEq, Eq)]
pub struct ScreenPermission {
    pub scope: String,
    pub revision: u64,
    pub allowed: bool,
}

/// Read the permission from the active profile through the authenticated
/// loopback API. The desktop does not retain a database from a prior profile.
pub async fn read_screen_permission(port: u16, token: &str) -> Result<ScreenPermission, String> {
    #[derive(Deserialize)]
    struct ProfileResponse {
        scope: String,
        revision: u64,
        profile: Profile,
    }
    #[derive(Deserialize)]
    struct Profile {
        permissions: Permissions,
    }
    #[derive(Deserialize)]
    struct Permissions {
        #[serde(default)]
        screen: bool,
    }
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("http://127.0.0.1:{port}/api/v1/profile"))
        .header(crate::safety::CONTROL_SESSION_HEADER, token)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<ProfileResponse>()
        .await
        .map_err(|error| error.to_string())?;
    Ok(ScreenPermission {
        scope: response.scope,
        revision: response.revision,
        allowed: response.profile.permissions.screen,
    })
}
