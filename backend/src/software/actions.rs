use super::*;
impl SoftwareRuntime {
    /// Only metadata from active registrations available to this conversation participates in routing.
    pub(crate) fn chat_sessions(
        &self,
        conversation: &str,
    ) -> Result<Vec<(Registration, Session)>, String> {
        let workspace = self.db.conversation_workspace(conversation)?;
        let state = self.state.lock();
        Ok(state
            .sessions
            .values()
            .filter(|s| s.state == "active")
            .filter_map(|s| {
                let r = state.registrations.get(&s.registration_id)?;
                (r.config.allow_observe
                    && (r.config.workspace_id.is_none() || r.config.workspace_id == workspace))
                    .then(|| (r.clone(), s.clone()))
            })
            .collect())
    }

    pub(crate) fn chat_target(
        &self,
        conversation: &str,
        app: Option<&str>,
        target: Option<&SessionTarget>,
    ) -> Result<SessionTarget, String> {
        let mut matches = self
            .chat_sessions(conversation)?
            .into_iter()
            .filter(|(r, s)| {
                target.is_none_or(|t| s.target() == *t)
                    && app.is_none_or(|name| {
                        r.config.name.eq_ignore_ascii_case(name.trim())
                            || std::path::Path::new(&r.config.executable)
                                .file_stem()
                                .and_then(|x| x.to_str())
                                .is_some_and(|x| x.eq_ignore_ascii_case(name.trim()))
                    })
            });
        let (_, session) = matches
            .next()
            .ok_or("当前对话没有匹配且允许观察的软件窗口。请检查软件挂载中的连接和许可。")?;
        if matches.next().is_some() {
            return Err(
                "有多个符合条件的窗口，请先用 software_sessions 查看候选并确认目标。".into(),
            );
        }
        Ok(session.target())
    }

    pub(crate) fn observation_target(&self, id: &str) -> Result<SessionTarget, String> {
        self.observations
            .lock()
            .values()
            .find(|observation| observation.id == id)
            .map(|observation| observation.target.clone())
            .ok_or_else(|| "此观察已过期，请重新观察要操作的窗口".into())
    }

    /// Select from the current conversation's delegated windows without touching the desktop.
    pub(crate) fn wechat_draft_target(&self, conversation: &str) -> Result<SessionTarget, String> {
        self.ready().map_err(|error| error.to_string())?;
        let workspace = self.db.conversation_workspace(conversation)?;
        let state = self.state.lock();
        let mut selected = None;
        let mut needs_permission = false;
        for session in state.sessions.values().filter(|s| s.state == "active") {
            let Some(registration) = state.registrations.get(&session.registration_id) else {
                continue;
            };
            let config = &registration.config;
            if config.adapter != "wechat"
                || (config.workspace_id.is_some() && config.workspace_id != workspace)
            {
                continue;
            }
            if !(config.allow_observe && config.allow_actions) {
                needs_permission = true;
                continue;
            }
            if selected.is_some() {
                return Err(
                    "已连接多个可操作的微信窗口。请在软件挂载中保留要填写草稿的那个窗口，再重试。"
                        .into(),
                );
            }
            selected = Some(session.target());
        }
        selected.ok_or_else(|| if needs_permission {
            "微信已连接，尚未开放草稿操作。请在软件挂载中断开微信，在「登记与许可」开启 AI 观察和控件操作后重新连接。"
        } else {
            "当前对话没有可操作的微信连接。请在「更多 → 软件挂载 → 桌面软件」确认微信的观察和操作许可并连接窗口。"
        }.into())
    }

    pub fn ensure_ai_allowed(
        &self,
        target: &SessionTarget,
        action: &str,
    ) -> Result<(), SoftwareError> {
        self.verify_ai_allowed(target, action).map(|_| ())
    }

    fn verify_ai_allowed(
        &self,
        target: &SessionTarget,
        action: &str,
    ) -> Result<WindowState, SoftwareError> {
        let (session, registration) = self.session(target)?;
        if session.state != "active" {
            return Err(SoftwareError::new(
                if session.state == "result_unknown" {
                    "result_unknown"
                } else {
                    "software_automation_stopped"
                },
                "Software automation is stopped or awaiting inspection",
            ));
        }
        if !registration.config.allow_observe {
            return Err(SoftwareError::new(
                "software_observation_not_authorized",
                "Explicitly enable observation for this software registration first",
            ));
        }
        if action == "window_ocr" && !registration.config.allow_window_ocr {
            return Err(SoftwareError::new(
                "window_ocr_not_authorized",
                "此软件尚未允许识别整个窗口中的可见文字",
            ));
        }
        if action == "wechat_conversation" && !registration.config.allow_content {
            return Err(SoftwareError::new(
                "software_content_not_authorized",
                "请在微信的登记与许可中开启窗口正文读取",
            ));
        }
        if !matches!(action, "observe" | "window_ocr" | "wechat_conversation")
            && !registration.config.allow_actions
        {
            return Err(SoftwareError::new(
                "software_actions_not_authorized",
                "UI actions need a separate explicit software permission and Gateway approval",
            ));
        }
        self.host()?.verify(&session.window.identity)
    }
    pub fn wechat_conversation(
        &self,
        target: &SessionTarget,
    ) -> Result<WechatConversation, SoftwareError> {
        let (session, registration) = self.session(target)?;
        if registration.config.adapter != "wechat" {
            return Err(SoftwareError::new(
                "wechat_required",
                "请选择已连接的微信窗口",
            ));
        }
        self.verify_ai_allowed(target, "wechat_conversation")?;
        let conversation = self.host()?.wechat_conversation(&session.window.identity)?;
        self.verify_ai_allowed(target, "wechat_conversation")?;
        Ok(conversation)
    }
    pub(super) fn check_window_pixels(&self) -> Result<(), SoftwareError> {
        if !crate::modules::onboarding::existing_window_capture_enabled(&self.db) {
            return Err(SoftwareError::new(
                "screen_capture_disabled",
                "个人权限设置已关闭屏幕采集",
            ));
        }
        Ok(())
    }

    /// One frame for the user-initiated, trusted workbench selection dialog.
    pub fn wechat_frame(&self, target: SessionTarget) -> Result<Vec<u8>, SoftwareError> {
        let (session, registration) = self.session(&target)?;
        if registration.config.adapter != "wechat" {
            return Err(SoftwareError::new(
                "wechat_required",
                "请选择已连接的微信窗口",
            ));
        }
        let window = self.verify_ai_allowed(&target, "observe")?;
        if !window.visible || window.minimized {
            return Err(SoftwareError::new(
                "wechat_not_visible",
                "请先显示微信并打开目标群聊",
            ));
        }
        self.check_window_pixels()?;
        let frame = self.host()?.wechat_frame_png(&session.window.identity)?;
        self.check_window_pixels()?;
        self.verify_ai_allowed(&target, "observe")?;
        Ok(frame)
    }
    pub fn write_wechat_draft(
        &self,
        target: SessionTarget,
        group: Option<&str>,
        text: &str,
        replace: bool,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        if text.trim().is_empty()
            || text.len() > 1024
            || text.chars().any(char::is_control)
            || group.is_some_and(|g| g.trim().is_empty() || g.len() > 255)
        {
            return Err(SoftwareError::new(
                "invalid_software_value",
                "需要单行草稿文字及有效的会话名称",
            ));
        }
        let (session, registration) = self.session(&target)?;
        if registration.config.adapter != "wechat" {
            return Err(SoftwareError::new(
                "wechat_required",
                "请选择已连接的微信窗口",
            ));
        }
        self.run_operation(
            operation_id,
            &registration.id,
            Some(&target.session_id),
            "wechat_draft",
            serde_json::json!([target, group, text, replace]),
            |token| {
                self.ensure_ai_allowed(&target, "control")?;
                Self::check_cancel(token)?;
                let host = self.host()?;
                host.write_wechat_draft(&session.window.identity, group, text, replace, token)?;
                let window = host.verify(&session.window.identity)?;
                self.replace_window(&target, window, None, false, token)
            },
        )
    }
    pub(crate) fn recognize_window(
        &self,
        target: &SessionTarget,
    ) -> Result<(String, bool), SoftwareError> {
        let window = self.verify_ai_allowed(target, "window_ocr")?;
        if window.minimized || !window.visible {
            return Err(SoftwareError::new(
                "window_not_visible",
                "请先显示要识别的窗口",
            ));
        }
        self.check_window_pixels()?;
        let text = self.host()?.recognize_window(&window.identity)?;
        self.check_window_pixels()?;
        self.verify_ai_allowed(target, "window_ocr")?;
        let mut chars = text.trim().chars();
        let excerpt: String = chars.by_ref().take(4096).collect();
        if excerpt.is_empty() {
            return Err(SoftwareError::new(
                "window_text_unavailable",
                "当前窗口没有识别到文字",
            ));
        }
        Ok((excerpt, chars.next().is_some()))
    }
    pub fn observe(&self, target: SessionTarget) -> Result<Observation, SoftwareError> {
        let _gate = self.gate.lock();
        self.ensure_ai_allowed(&target, "observe")?;
        let (session, registration) = self.session(&target)?;
        let observation_id = uuid::Uuid::new_v4().to_string();
        let mut controls = self.host()?.observe(
            &session.window.identity,
            &observation_id,
            registration.config.allow_content,
        )?;
        // Native UIA traversal is intentionally bounded and skips sensitive or
        // unavailable providers. A short list is never proof of the full tree.
        let truncated = true;
        controls.truncate(64);
        controls.retain(|c| !c.is_password);
        for control in &mut controls {
            if !registration.config.allow_content {
                control.visible_text = None;
            }
            control.name = control.name.chars().take(120).collect();
            control.automation_id = control.automation_id.chars().take(120).collect();
            control.role = control.role.chars().take(60).collect();
        }
        // A concurrent stop/revocation invalidates this read before it is exposed.
        let verified_window = self.verify_ai_allowed(&target, "observe")?;
        let observation = Observation {
            id: observation_id,
            target: target.clone(),
            controls,
            truncated,
            untrusted_application_data: true,
            window_evidence: Some(WindowEvidence::capture(target.generation, &verified_window)),
        };
        self.observations
            .lock()
            .insert(target.session_id, observation.clone());
        Ok(observation)
    }
    pub fn control(
        &self,
        target: SessionTarget,
        observation_id: &str,
        element_id: &str,
        action: &str,
        value: Option<String>,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        self.control_with_outcome(
            target,
            observation_id,
            element_id,
            action,
            value,
            None,
            operation_id,
        )
    }

    pub(crate) fn control_with_outcome(
        &self,
        target: SessionTarget,
        observation_id: &str,
        element_id: &str,
        action: &str,
        value: Option<String>,
        expected_label: Option<&str>,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        if expected_label
            .is_some_and(|s| action != "invoke" || s.trim().is_empty() || s.len() > 240)
        {
            return Err(SoftwareError::new(
                "invalid_arguments",
                "点击后核对的可见文字须为 1 至 240 字节",
            ));
        }
        if !matches!(
            action,
            "focus"
                | "focus_control"
                | "invoke"
                | "set_value"
                | "select"
                | "check"
                | "uncheck"
                | "expand"
                | "collapse"
        ) {
            return Err(SoftwareError::new(
                "unsupported_software_action",
                "Choose an action supported by this UI Automation control",
            ));
        }
        if action == "set_value"
            && value
                .as_ref()
                .is_none_or(|v| v.len() > 4096 || v.contains('\0') || v.contains(['\r', '\n']))
        {
            return Err(SoftwareError::new("invalid_software_value","A single-line bounded value is required; Enter/newline submission is not supported"));
        }
        if action != "set_value" && value.is_some() {
            return Err(SoftwareError::new(
                "invalid_software_value",
                "Only set_value accepts text",
            ));
        }
        let registration_id = self
            .state
            .lock()
            .sessions
            .get(&target.session_id)
            .map(|s| s.registration_id.clone())
            .ok_or_else(|| {
                SoftwareError::new(
                    "software_session_not_found",
                    "Software session does not exist",
                )
            })?;
        let mut arguments = serde_json::json!({"target":target,"observation_id":observation_id,"element_id":element_id,"action":action,"value":value});
        if let Some(label) = expected_label {
            arguments["expected_label"] = serde_json::json!(label);
        }
        self.run_operation(
            operation_id,
            &registration_id,
            Some(&target.session_id),
            "control",
            arguments,
            |token| {
                self.ensure_ai_allowed(&target, "control")?;
                let observation = self
                    .observations
                    .lock()
                    .get(&target.session_id)
                    .cloned()
                    .ok_or_else(|| {
                        SoftwareError::new(
                            "stale_software_observation",
                            "Observe the current window before acting",
                        )
                    })?;
                if observation.id != observation_id || observation.target != target {
                    return Err(SoftwareError::new(
                        "stale_software_observation",
                        "Observation no longer belongs to the current session generation",
                    ));
                }
                let control = observation
                    .controls
                    .iter()
                    .find(|c| c.id == element_id)
                    .ok_or_else(|| {
                        SoftwareError::new(
                            "software_element_not_found",
                            "Element is not part of the current observation",
                        )
                    })?;
                if control.is_password || !control.enabled {
                    return Err(SoftwareError::new(
                        "software_element_not_actionable",
                        "Sensitive or disabled elements cannot be automated",
                    ));
                }
                if !control.supports(action) {
                    return Err(SoftwareError::new(
                        "software_pattern_unavailable",
                        "The selected element does not expose the requested UIA pattern",
                    ));
                }
                let matches_label = |c: &SoftwareControl, label: &str| {
                    c.enabled
                        && !c.is_password
                        && (c.name == label || c.visible_text.as_deref() == Some(label))
                };
                if expected_label.is_some_and(|label| {
                    observation.controls.iter().any(|c| matches_label(c, label))
                }) {
                    return Err(SoftwareError::new(
                        "postcondition_already_visible",
                        "目标文字在操作前已可见，请选择能区分操作前后状态的结果",
                    ));
                }
                let (session, registration) = self.session(&target)?;
                let host = self.host()?;
                Self::check_cancel(token)?;
                self.ensure_ai_allowed(&target, "control")?;
                self.observations.lock().remove(&target.session_id);
                let native_action = if expected_label.is_some() {
                    "invoke_then_observe"
                } else {
                    action
                };
                host.control(
                    &session.window.identity,
                    control,
                    native_action,
                    value.as_deref(),
                    token,
                )?;
                if let Some(label) = expected_label {
                    let mut seen = false;
                    for attempt in 0..2 {
                        if attempt > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(180));
                        }
                        Self::check_cancel(token)
                            .map_err(|_| SoftwareError::unknown("点击后已停止核对"))?;
                        self.ensure_ai_allowed(&target, "observe")
                            .map_err(|_| SoftwareError::unknown("点击后无法核对原窗口"))?;
                        let controls = host
                            .observe(
                                &session.window.identity,
                                observation_id,
                                registration.config.allow_content,
                            )
                            .map_err(|_| SoftwareError::unknown("已点击，无法读取操作后的窗口"))?;
                        if controls.iter().any(|c| matches_label(c, label)) {
                            seen = true;
                            break;
                        }
                    }
                    if !seen {
                        return Err(SoftwareError::unknown(
                            "已点击，窗口中未观察到预期结果，请核对后再继续",
                        ));
                    }
                }
                let window = host
                    .verify(&session.window.identity)
                    .map_err(|_| SoftwareError::unknown("操作后无法核对原窗口"))?;
                self.replace_window(&target, window, None, false, token)
            },
        )
    }
}
