//! Plugin-owned service descriptions. Values come from the existing settings store.
use super::{manifest::PluginManifest, settings::PluginSetting};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDefinition {
    pub id: String,
    pub title: String,
    pub settings: String,
    #[serde(default)]
    pub executable_key: String,
    #[serde(default)]
    pub working_directory_key: String,
    #[serde(default)]
    pub argument_keys: Vec<String>,
    pub web_url_key: String,
    pub channel_id_key: Option<String>,
    pub channel_provider: Option<String>,
    #[serde(default)]
    pub deployment: ServiceDeployment,
    pub health_path: Option<String>,
    pub mcp_path: Option<String>,
    #[serde(default)]
    pub model_mode: ServiceModelMode,
    #[serde(default)]
    pub cloud_auth: ServiceCloudAuth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeployment {
    #[default]
    Local,
    Cloud,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceModelMode {
    #[default]
    None,
    PlatformChat,
    Independent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCloudAuth {
    #[default]
    None,
    Bearer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceLaunch {
    pub executable: String,
    pub working_directory: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ServiceConfiguration {
    pub package_revision: String,
    pub service_id: String,
    pub deployment: ServiceDeployment,
    pub launch: Option<ServiceLaunch>,
    pub web_url: Option<String>,
    pub health_url: Option<String>,
    pub mcp_url: Option<String>,
    pub model_mode: ServiceModelMode,
    pub cloud_auth: ServiceCloudAuth,
}

impl ServiceConfiguration {
    pub(crate) fn bearer_origin(&self) -> Option<url::Origin> {
        if self.cloud_auth == ServiceCloudAuth::Bearer {
            self.mcp_url
                .as_deref()
                .and_then(|url| url::Url::parse(url).ok())
                .map(|url| url.origin())
        } else {
            None
        }
    }
}

const MANAGED_MCP_SUFFIX: &str = "-plugin-service";

pub(crate) fn managed_mcp_id(plugin: &str, service: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(format!("{plugin}:{service}")));
    format!("{}{MANAGED_MCP_SUFFIX}", &digest[..32])
}

pub(crate) fn is_managed_mcp_id(id: &str) -> bool {
    id.strip_suffix(MANAGED_MCP_SUFFIX)
        .is_some_and(|hash| hash.len() == 32 && hash.bytes().all(|digit| digit.is_ascii_hexdigit()))
}

impl ServiceDefinition {
    pub fn parse(raw: &str, manifest: &PluginManifest) -> Result<Self, String> {
        let service: Self = serde_json::from_str(raw).map_err(|_| "服务描述不是有效 JSON")?;
        super::PluginId::new(&service.id).map_err(|_| "服务 ID 无效")?;
        if service.title.trim().is_empty() || service.argument_keys.len() > 32 {
            return Err("服务名称或参数数量无效".into());
        }
        let settings = service.setting(manifest)?;
        for key in service.setting_keys() {
            if !settings
                .fields
                .iter()
                .any(|f| &f.key == key && f.kind == "text")
            {
                return Err(format!("服务引用的文本设置不存在：{key}"));
            }
        }
        if service.channel_id_key.is_some() != service.channel_provider.is_some() {
            return Err("服务渠道设置不完整".into());
        }
        for path in service.health_path.iter().chain(&service.mcp_path) {
            if !path.starts_with('/') || path.starts_with("//") || path.contains(['\\', '#', '?']) {
                return Err("服务健康和 MCP 地址必须是同源路径".into());
            }
        }
        if service.deployment == ServiceDeployment::Cloud
            && (service.health_path.is_none()
                || service.model_mode == ServiceModelMode::PlatformChat
                || !service.executable_key.is_empty()
                || !service.working_directory_key.is_empty()
                || !service.argument_keys.is_empty())
        {
            return Err("云端服务须声明健康路径并使用独立部署配置".into());
        }
        if service.mcp_path.is_some() && service.health_path.is_none() {
            return Err("MCP 服务须声明独立的健康路径".into());
        }
        if service.cloud_auth == ServiceCloudAuth::Bearer
            && (service.deployment != ServiceDeployment::Cloud || service.mcp_path.is_none())
        {
            return Err("Bearer 认证须用于声明了 MCP 路径的云端服务".into());
        }
        if service.model_mode == ServiceModelMode::PlatformChat && service.mcp_path.is_none() {
            return Err("平台模型调用须关联本地 MCP 适配器".into());
        }
        Ok(service)
    }

    fn setting_keys(&self) -> impl Iterator<Item = &String> {
        let local = (self.deployment == ServiceDeployment::Local)
            .then_some([&self.executable_key, &self.working_directory_key]);
        std::iter::once(&self.web_url_key)
            .chain(local.into_iter().flatten())
            .chain(&self.argument_keys)
            .chain(&self.channel_id_key)
    }

    pub fn setting<'a>(&self, manifest: &'a PluginManifest) -> Result<&'a PluginSetting, String> {
        manifest
            .contributes
            .settings
            .iter()
            .find(|s| s.id == self.settings)
            .ok_or_else(|| "服务设置组不存在".into())
    }

    pub fn text<'a>(&self, values: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
        values.get(key).and_then(Value::as_str).unwrap_or("").trim()
    }

    pub fn launch(&self, values: &BTreeMap<String, Value>) -> Result<ServiceLaunch, String> {
        let executable = self.text(values, &self.executable_key);
        let working_directory = self.text(values, &self.working_directory_key);
        if !Path::new(executable).is_absolute() || !Path::new(executable).is_file() {
            return Err("请配置已存在的可执行文件完整路径".into());
        }
        if !Path::new(working_directory).is_absolute() || !Path::new(working_directory).is_dir() {
            return Err("请配置已存在的工作目录完整路径".into());
        }
        #[cfg(windows)]
        if !executable.to_ascii_lowercase().ends_with(".exe") {
            return Err("服务须直接启动 EXE 程序".into());
        }
        let args = self
            .argument_keys
            .iter()
            .map(|key| self.text(values, key))
            .filter(|value| !value.is_empty())
            .map(String::from)
            .collect::<Vec<_>>();
        if executable.contains('\0')
            || working_directory.contains('\0')
            || args.iter().any(|a| a.contains('\0'))
        {
            return Err("服务路径或参数包含无效字符".into());
        }
        Ok(ServiceLaunch {
            executable: executable.into(),
            working_directory: working_directory.into(),
            args,
        })
    }

    pub fn web_url(&self, values: &BTreeMap<String, Value>) -> Result<Option<String>, String> {
        let raw = self.text(values, &self.web_url_key);
        if raw.is_empty() {
            return Ok(None);
        }
        let url = url::Url::parse(raw).map_err(|_| "管理页地址无效")?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        let allowed = match self.deployment {
            ServiceDeployment::Local => matches!(url.scheme(), "http" | "https") && loopback,
            ServiceDeployment::Cloud => url.scheme() == "https" && url.has_host() && !loopback,
        };
        if !allowed
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(match self.deployment {
                ServiceDeployment::Local => "管理页须使用不含凭据和查询参数的本机 HTTP 地址",
                ServiceDeployment::Cloud => "云端服务须使用不含凭据和查询参数的 HTTPS 地址",
            }
            .into());
        }
        Ok(Some(url.to_string()))
    }

    pub(crate) fn configuration(
        &self,
        values: &BTreeMap<String, Value>,
        package_revision: String,
    ) -> Result<ServiceConfiguration, String> {
        let web = self.web_url(values)?;
        let origin = web.as_deref().and_then(|url| url::Url::parse(url).ok());
        let same_origin = |path: &String| {
            let origin = origin.as_ref().ok_or("请先配置服务地址")?;
            let endpoint = origin.join(path).map_err(|_| "服务路径无效")?;
            if origin.origin() != endpoint.origin() {
                return Err("健康及 MCP 地址须与管理页同源");
            }
            Ok(endpoint.to_string())
        };
        let health_url = self.health_path.as_ref().map(same_origin).transpose()?;
        let mcp_url = self.mcp_path.as_ref().map(same_origin).transpose()?;
        Ok(ServiceConfiguration {
            package_revision,
            service_id: self.id.clone(),
            deployment: self.deployment,
            launch: (self.deployment == ServiceDeployment::Local)
                .then(|| self.launch(values))
                .transpose()?,
            web_url: web,
            health_url,
            mcp_url,
            model_mode: self.model_mode,
            cloud_auth: self.cloud_auth,
        })
    }
}

pub fn validate_files(
    manifest: &PluginManifest,
    files: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for path in &manifest.contributes.services {
        let service =
            ServiceDefinition::parse(files.get(path).ok_or("缺少插件服务描述")?, manifest)?;
        if !ids.insert(service.id) {
            return Err("插件服务 ID 重复".into());
        }
    }
    Ok(())
}
