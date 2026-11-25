//! Owns one service instance for both its website and MCP adapter.
use super::{
    service::{ServiceConfiguration, ServiceDeployment, ServiceLaunch, ServiceModelMode},
    service_process::ServiceProcess,
};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

type ServiceKey = (String, String);

#[derive(Clone, Debug, Default)]
pub struct ServiceProcessStatus {
    pub state: &'static str,
    pub pid: Option<u32>,
    pub exit_code: Option<u32>,
    pub active_launch: Option<ServiceLaunch>,
    pub instance_id: Option<Uuid>,
    pub service_health: &'static str,
    pub health_error: Option<String>,
    pub task_uses: usize,
    pub page_uses: usize,
}

struct RunningService {
    instance_id: Uuid,
    configuration: ServiceConfiguration,
    process: Option<ServiceProcess>,
    exit_code: Option<u32>,
    stopped: bool,
    health_checked: bool,
    health_error: Option<String>,
    pages: BTreeMap<Uuid, ServicePage>,
    tasks: usize,
    allow_model: bool,
    ended: CancellationToken,
}

struct ServicePage {
    tab_id: String,
    session_id: String,
    origin: url::Origin,
}

impl RunningService {
    fn running(&mut self) -> Result<bool, String> {
        if let Some(process) = &self.process {
            if let Some(code) = process.exit_code()? {
                self.exit_code = Some(code);
                self.process = None;
                self.pages.clear();
                self.ended.cancel();
            }
        }
        Ok(self.process.is_some()
            || (self.configuration.deployment == ServiceDeployment::Cloud && !self.stopped))
    }

    fn health(&self, running: bool) -> &'static str {
        if !running {
            if self.stopped {
                "stopped"
            } else {
                "failed"
            }
        } else if self.health_error.is_some() {
            "degraded"
        } else if self.health_checked {
            "ready"
        } else {
            "starting"
        }
    }
}

#[derive(Default)]
pub struct PluginServiceRuntime {
    pub operations: Mutex<()>,
    pub(crate) model_proxy: Arc<crate::api::plugin_model_proxy::PluginModelProxy>,
    services: Mutex<BTreeMap<ServiceKey, RunningService>>,
    ensure_gates: Mutex<BTreeMap<ServiceKey, Weak<AsyncMutex<()>>>>,
}

pub(crate) struct TaskServiceUse {
    runtime: Arc<PluginServiceRuntime>,
    key: ServiceKey,
    pub instance_id: Uuid,
    pub ended: CancellationToken,
}

impl Drop for TaskServiceUse {
    fn drop(&mut self) {
        if let Some(entry) = self.runtime.services.lock().get_mut(&self.key) {
            if entry.instance_id == self.instance_id {
                entry.tasks -= 1;
            }
        }
    }
}

impl PluginServiceRuntime {
    pub(crate) fn ensure_gate(&self, plugin: &str, service: &str) -> Arc<AsyncMutex<()>> {
        let mut gates = self.ensure_gates.lock();
        gates.retain(|_, gate| gate.strong_count() > 0);
        let key = (plugin.into(), service.into());
        if let Some(gate) = gates.get(&key).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(AsyncMutex::new(()));
        gates.insert(key, Arc::downgrade(&gate));
        gate
    }

    pub fn status(&self, plugin: &str, service: &str) -> Result<ServiceProcessStatus, String> {
        let mut services = self.services.lock();
        let Some(entry) = services.get_mut(&(plugin.into(), service.into())) else {
            return Ok(ServiceProcessStatus {
                state: "stopped",
                service_health: "stopped",
                ..Default::default()
            });
        };
        let running = entry.running()?;
        Ok(ServiceProcessStatus {
            state: if running {
                "running"
            } else if entry.stopped {
                "stopped"
            } else {
                "exited"
            },
            pid: entry.process.as_ref().map(ServiceProcess::pid),
            exit_code: entry.exit_code,
            active_launch: running
                .then(|| entry.configuration.launch.clone())
                .flatten(),
            instance_id: running.then_some(entry.instance_id),
            service_health: entry.health(running),
            health_error: entry.health_error.clone(),
            task_uses: entry.tasks,
            page_uses: entry.pages.len(),
        })
    }

    /// A URL refers to a managed service only when one registered health endpoint matches.
    pub(crate) fn attention_service_for_url(
        &self,
        url: &str,
    ) -> Result<Option<(String, Option<Uuid>, &'static str)>, String> {
        let target = url::Url::parse(url).map_err(|error| error.to_string())?;
        let mut services = self.services.lock();
        let mut matches = services.iter_mut().filter(|(_, entry)| {
            entry
                .configuration
                .health_url
                .as_deref()
                .and_then(|url| url::Url::parse(url).ok())
                .as_ref()
                == Some(&target)
        });
        let Some(((plugin, service), entry)) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Ok(None);
        }
        let running = entry.running()?;
        Ok(Some((
            format!("{plugin}/{service}"),
            running.then_some(entry.instance_id),
            entry.health(running),
        )))
    }

    pub(crate) fn start(
        &self,
        plugin: &str,
        service: &str,
        configuration: ServiceConfiguration,
        allow_model: bool,
        withhold: &[String],
    ) -> Result<Uuid, String> {
        let mut services = self.services.lock();
        let key = (plugin.into(), service.into());
        if let Some(entry) = services.get_mut(&key) {
            if entry.running()? || entry.tasks > 0 {
                return Err("服务已在运行或正在结束活动任务".into());
            }
        }
        let process = configuration
            .launch
            .as_ref()
            .map(|launch| ServiceProcess::spawn(launch, withhold))
            .transpose()?;
        let instance_id = Uuid::new_v4();
        services.insert(
            key,
            RunningService {
                instance_id,
                configuration,
                process,
                exit_code: None,
                stopped: false,
                health_checked: false,
                health_error: None,
                pages: BTreeMap::new(),
                tasks: 0,
                allow_model,
                ended: CancellationToken::new(),
            },
        );
        Ok(instance_id)
    }

    pub(crate) fn configuration_matches(
        &self,
        plugin: &str,
        service: &str,
        instance: Uuid,
        configuration: &ServiceConfiguration,
    ) -> bool {
        self.services
            .lock()
            .get(&(plugin.into(), service.into()))
            .is_some_and(|entry| {
                entry.instance_id == instance && &entry.configuration == configuration
            })
    }

    pub(crate) fn health(
        &self,
        plugin: &str,
        service: &str,
        instance: Uuid,
        health: Result<(), String>,
    ) {
        if let Some(entry) = self
            .services
            .lock()
            .get_mut(&(plugin.into(), service.into()))
        {
            if entry.instance_id == instance {
                entry.health_checked = true;
                entry.health_error = health.err();
            }
        }
    }

    pub(crate) fn acquire_task(
        self: &Arc<Self>,
        plugin: &str,
        service: &str,
        instance: Uuid,
    ) -> Result<TaskServiceUse, String> {
        let key = (plugin.into(), service.into());
        let mut services = self.services.lock();
        let entry = services.get_mut(&key).ok_or("请先通过插件管理页启动服务")?;
        if entry.instance_id != instance || !entry.running()? {
            return Err("服务实例已退出，请重新启动并刷新 MCP 工具".into());
        }
        entry.tasks += 1;
        Ok(TaskServiceUse {
            runtime: Arc::clone(self),
            key,
            instance_id: instance,
            ended: entry.ended.clone(),
        })
    }

    pub(crate) fn acquire_page(
        &self,
        usage: &TaskServiceUse,
        tab: crate::browser::BrowserTab,
        origin: url::Origin,
    ) -> Result<Uuid, String> {
        let mut services = self.services.lock();
        let entry = services.get_mut(&usage.key).ok_or("服务实例已退出")?;
        if entry.instance_id != usage.instance_id || !entry.running()? {
            return Err("服务实例已退出".into());
        }
        let id = Uuid::new_v4();
        entry.pages.insert(
            id,
            ServicePage {
                tab_id: tab.tab_id,
                session_id: tab.session_id,
                origin,
            },
        );
        Ok(id)
    }

    pub(crate) fn reconcile_pages(
        &self,
        plugin: &str,
        service: &str,
        tabs: &[crate::browser::BrowserTab],
    ) {
        if let Some(entry) = self
            .services
            .lock()
            .get_mut(&(plugin.into(), service.into()))
        {
            entry.pages.retain(|_, page| {
                tabs.iter().any(|tab| {
                    tab.tab_id == page.tab_id
                        && tab.session_id == page.session_id
                        && url::Url::parse(&tab.url).is_ok_and(|url| url.origin() == page.origin)
                })
            });
        }
    }

    pub(crate) fn release_page(&self, plugin: &str, service: &str, lease: Uuid) {
        if let Some(entry) = self
            .services
            .lock()
            .get_mut(&(plugin.into(), service.into()))
        {
            entry.pages.remove(&lease);
        }
    }

    pub(crate) fn allows_model(&self, plugin: &str, service: &str, instance: Uuid) -> bool {
        self.model_enabled_for(plugin, service, instance, true)
    }

    pub(crate) fn model_enabled(&self, plugin: &str, service: &str, instance: Uuid) -> bool {
        self.model_enabled_for(plugin, service, instance, false)
    }

    fn model_enabled_for(
        &self,
        plugin: &str,
        service: &str,
        instance: Uuid,
        task_required: bool,
    ) -> bool {
        let mut services = self.services.lock();
        services
            .get_mut(&(plugin.into(), service.into()))
            .is_some_and(|entry| {
                entry.instance_id == instance
                    && entry.allow_model
                    && entry.configuration.model_mode == ServiceModelMode::PlatformChat
                    && (!task_required || entry.tasks > 0)
                    && entry.running().unwrap_or(false)
            })
    }

    pub fn stop(&self, plugin: &str, service: &str) -> Result<(), String> {
        let mut services = self.services.lock();
        if let Some(entry) = services.get_mut(&(plugin.into(), service.into())) {
            if entry.tasks > 0 {
                return Err("服务仍有活动任务，请等待任务结束或取消后再停止".into());
            }
            Self::stop_instance(entry)?;
        }
        Ok(())
    }

    pub fn stop_plugin(&self, plugin: &str) -> Result<(), String> {
        let mut services = self.services.lock();
        if services
            .iter()
            .any(|((owner, _), entry)| owner == plugin && entry.tasks > 0)
        {
            return Err("插件服务有活动任务，请等待任务结束或取消后再继续".into());
        }
        for (_, entry) in services
            .iter_mut()
            .filter(|((owner, _), _)| owner == plugin)
        {
            Self::stop_instance(entry)?;
        }
        Ok(())
    }

    fn stop_instance(entry: &mut RunningService) -> Result<(), String> {
        if let Some(process) = &entry.process {
            process.stop()?;
        }
        entry.ended.cancel();
        entry.process = None;
        entry.exit_code = None;
        entry.stopped = true;
        entry.health_error = None;
        entry.pages.clear();
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        let _operation = self.operations.lock();
        let mut services = self.services.lock();
        for entry in services.values_mut() {
            Self::stop_instance(entry)?;
        }
        services.clear();
        Ok(())
    }
}
