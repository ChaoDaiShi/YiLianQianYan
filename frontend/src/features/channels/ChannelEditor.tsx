import { useEffect, useMemo, useRef, useState } from "react";
import Button from "../../components/ui/Button";
import Drawer from "../../components/ui/Drawer";
import Modal from "../../components/ui/Modal";
import {
  createChannel,
  updateChannel,
  type ChannelGroupTrigger,
  type ChannelInput,
  type ChannelPreset,
  type ChannelProvider,
  type ExternalChannel,
} from "../../api/channels";
import { connectionModesForProvider, parseAllowedSenders, replyModeDescription, type ChannelInputMode } from "./channelsModel";

const problem = (value: unknown) => value instanceof Error ? value.message : "渠道操作失败";

export interface ChannelEditorProps {
  channel: ExternalChannel | null;
  initialProvider?: string;
  providers: ChannelProvider[];
  presets: ChannelPreset[];
  presentation?: "modal" | "drawer";
  onClose: () => void;
  onSaved: (channelId: string) => void;
}

export default function ChannelEditor({ channel, initialProvider: initialProviderProp, providers, presets, presentation = "modal", onClose, onSaved }: ChannelEditorProps) {
  const initialProvider = channel?.provider ?? initialProviderProp ?? providers[0]?.id ?? "";
  const [provider, setProvider] = useState(initialProvider);
  const [connectionMode, setConnectionMode] = useState<ChannelInputMode>(
    channel?.connection_mode ?? connectionModesForProvider(initialProvider)[0]?.value ?? "http",
  );
  const [name, setName] = useState(channel?.name ?? "");
  const [appId, setAppId] = useState(channel?.app_id ?? "");
  const [base, setBase] = useState(channel?.api_base_url ?? "");
  const [selfId, setSelfId] = useState(channel?.self_id ?? "");
  const [allowed, setAllowed] = useState(channel?.allowed_senders.join("\n") ?? "");
  const [allowedGroups, setAllowedGroups] = useState(channel?.allowed_groups.join("\n") ?? "");
  const [groupTrigger, setGroupTrigger] = useState<ChannelGroupTrigger>(channel?.group_trigger ?? "mention");
  const [presetId, setPresetId] = useState(channel?.preset_id ?? "");
  const [replyMode, setReplyMode] = useState<ChannelInput["reply_mode"]>(channel ? channel.reply_mode : "automatic");
  const [credential, setCredential] = useState("");
  const [eventSecret, setEventSecret] = useState("");
  const [enabled, setEnabled] = useState(channel?.enabled ?? false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const alive = useRef(true);
  const modeOptions = useMemo(() => connectionModesForProvider(provider), [provider]);
  const selectedProvider = providers.find(item => item.id === provider);
  const isFeishu = provider === "feishu";
  const isOneBot = provider === "onebot11";
  const isWeChat = provider === "wechat_ilink";
  const isAkasha = provider === "wechat_onebot";

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  function changeProvider(nextProvider: string) {
    setProvider(nextProvider);
    setConnectionMode(connectionModesForProvider(nextProvider)[0]?.value ?? "http");
    setAppId("");
    setBase("");
    setSelfId("");
    setAllowed("");
    setAllowedGroups("");
    setGroupTrigger("mention");
    setCredential("");
    setEventSecret("");
  }

  async function save() {
    setBusy(true);
    setError("");
    const allowedSenders = parseAllowedSenders(allowed);
    const secrets = isFeishu
      ? { app_secret: credential || undefined, verification_token: eventSecret || undefined }
      : isOneBot || isAkasha
        ? { access_token: credential || undefined, event_secret: eventSecret || undefined }
        : undefined;
    const input: ChannelInput = {
      name: name.trim(),
      provider,
      connection_mode: connectionMode,
      preset_id: presetId || null,
      reply_mode: replyMode,
      enabled,
      app_id: isFeishu ? appId.trim() || null : null,
      api_base_url: isOneBot && connectionMode === "http" ? base.trim() || null : null,
      self_id: isOneBot || isAkasha ? selfId.trim() || null : null,
      allowed_senders: allowedSenders,
      allowed_groups: parseAllowedSenders(allowedGroups),
      group_trigger: groupTrigger,
      ...(secrets ? { secrets } : {}),
    };
    try {
      const result = channel ? await updateChannel(channel.id, input) : await createChannel(input);
      if (alive.current) onSaved(result.channel.id);
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  const footer = <>
    <Button variant="secondary" disabled={busy} onClick={onClose}>取消</Button>
    <Button form="channel-editor-form" type="submit" disabled={busy || !provider} loading={busy} loadingLabel="保存中…">保存配置</Button>
  </>;
  const form = <form id="channel-editor-form" className="channel-wizard channel-editor" onSubmit={event => { event.preventDefault(); void save(); }}>
        <div className="channel-wizard-intro">
          <span className="channel-step-label">渠道接入 · 配置向导</span>
          <h3>把消息入口接到忆涟千言</h3>
          <p>填写平台信息、连接方式和允许联系的用户。保存配置后，连接检查会分别报告真实可用的能力。</p>
        </div>

        <section className="channel-form-section" aria-labelledby="channel-platform-heading">
          <div className="channel-section-heading"><span>01</span><div><h4 id="channel-platform-heading">平台与连接方式</h4><p>为此渠道选择明确的平台协议。</p></div></div>
          <div className="channel-form-grid">
            <label>渠道名称
              <input aria-label="渠道名称" required maxLength={80} value={name} onChange={event => setName(event.target.value)} placeholder="例如：工作飞书" />
            </label>
            <label>平台模板
              <select aria-label="平台模板" value={provider} disabled={Boolean(channel)} onChange={event => changeProvider(event.target.value)}>
                {providers.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
              </select>
            </label>
            <label className="channel-field-wide">连接方式
              <select aria-label="连接方式" value={connectionMode} onChange={event => setConnectionMode(event.target.value as ChannelInputMode)}>
                {modeOptions.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}
              </select>
            </label>
          </div>
          {selectedProvider && <p className="channel-form-note">{selectedProvider.limitations.join(" · ")}</p>}
        </section>

        <section className="channel-form-section" aria-labelledby="channel-account-heading">
          <div className="channel-section-heading"><span>02</span><div><h4 id="channel-account-heading">账号与鉴权</h4><p>凭据只写入本机凭据库，不会显示在收件箱。</p></div></div>
          <div className="channel-form-grid">
            {isFeishu && <label>App ID
              <input aria-label="App ID" required autoComplete="off" value={appId} onChange={event => setAppId(event.target.value)} placeholder="飞书应用的 App ID" />
            </label>}
            {isOneBot && <>
              {connectionMode === "http" && <label className="channel-field-wide">OneBot HTTP API 地址
                <input aria-label="OneBot HTTP API 地址" required value={base} onChange={event => setBase(event.target.value)} placeholder="http://127.0.0.1:5700" />
              </label>}
              <label>{isAkasha ? "Akasha 映射机器人 ID" : "机器人 self_id"}
                <input aria-label={isAkasha ? "Akasha 映射机器人 ID" : "机器人 self_id"} required inputMode="numeric" value={selfId} onChange={event => setSelfId(event.target.value)} placeholder="OneBot 账号数字 ID" />
              </label>
            </>}
            {(isFeishu || isOneBot || isAkasha) && <label>{isAkasha ? "Akasha OneBot Token" : "访问凭据"}
              <input aria-label={isFeishu ? "App Secret" : isAkasha ? "Akasha OneBot Token" : "Access Token"} type="password" value={credential} onChange={event => setCredential(event.target.value)} autoComplete="new-password" placeholder={channel ? "留空保持已有凭据" : "写入本机凭据库"} />
            </label>}
            {isFeishu && connectionMode === "http" && <label>Verification Token
              <input aria-label="Verification Token" type="password" value={eventSecret} onChange={event => setEventSecret(event.target.value)} autoComplete="new-password" placeholder={channel ? "留空保持已有凭据" : "与飞书事件订阅配置一致"} />
            </label>}
            {isOneBot && connectionMode === "http" && <label>HTTP 上报签名 secret
              <input aria-label="HTTP 上报签名 secret" type="password" value={eventSecret} onChange={event => setEventSecret(event.target.value)} autoComplete="new-password" placeholder={channel ? "留空保持已有凭据" : "与 OneBot 服务配置一致"} />
            </label>}
            {isWeChat && <div className="channel-wechat-login-note channel-field-wide">
              微信 iLink 使用官方扫码授权。保存后，在渠道卡片中选择“开始扫码”并按提示在手机上确认。
            </div>}
            {isAkasha && <div className="channel-wechat-login-note channel-field-wide">
              Akasha-WeChat 使用独立反向 WebSocket 桥接。此处填写 Akasha 映射账号与 OneBot Token；本渠道不提供微信扫码登录。
            </div>}
          </div>
        </section>

        <section className="channel-form-section" aria-labelledby="channel-allowlist-heading">
          <div className="channel-section-heading"><span>03</span><div><h4 id="channel-allowlist-heading">允许联系的用户</h4><p>只为名单内私聊生成回复草稿。</p></div></div>
          <label className="channel-allowlist-field">用户 ID
            <textarea aria-label="允许联系的用户 ID" required={isFeishu} rows={3} value={allowed} onChange={event => setAllowed(event.target.value)} placeholder={isFeishu ? "飞书 open_id，每行一个" : isOneBot ? "OneBot 用户数字 ID，每行一个" : isAkasha ? "Akasha 映射用户 ID，每行一个" : "留空后由扫码绑定联系人"} />
          </label>
          {isWeChat && <p className="channel-form-note">如果留空，首次扫码成功后会将扫码绑定的微信用户 ID 作为唯一允许联系人，不会开放其他联系人。</p>}
          {(isOneBot || isAkasha) && <>
            <label className="channel-allowlist-field">允许接收群 ID
              <textarea aria-label="允许接收群 ID" rows={3} value={allowedGroups} onChange={event => setAllowedGroups(event.target.value)} placeholder="每行一个群号" />
            </label>
            {isOneBot && <label className="channel-field-wide">群消息触发方式
              <select aria-label="群消息触发方式" value={groupTrigger} onChange={event => setGroupTrigger(event.target.value as ChannelGroupTrigger)}>
                <option value="mention">只响应提及机器人</option>
                <option value="all">接收允许群内的全部消息</option>
              </select>
            </label>}
            {isOneBot && <p className="channel-form-note">群接收范围独立于私聊用户范围；群内成员发言仍是该群会话内容，不会取得主人跨会话控制权。</p>}
            {isAkasha && <p className="channel-form-note">群聊触发方式在 Akasha 中设置；此处的群 ID 列表只定义应用允许接收的群范围。</p>}
          </>}
        </section>

        <section className="channel-form-section" aria-labelledby="channel-reply-heading">
          <div className="channel-section-heading"><span>04</span><div><h4 id="channel-reply-heading">回复预设</h4><p>预设只影响之后开始生成的草稿。</p></div></div>
          <label className="channel-field-wide">选择回复预设
            <select aria-label="渠道回复预设" value={presetId} onChange={event => setPresetId(event.target.value)}>
              <option value="">使用全局默认聊天配置</option>
              {presets.map(item => <option key={item.id} value={item.id}>{item.name}{item.builtin ? " · 内置" : ""}</option>)}
            </select>
          </label>
          <label className="channel-enabled"><input type="checkbox" checked={enabled} onChange={event => setEnabled(event.target.checked)} /> 保存后启用并接收消息</label>
        </section>

        <section className="channel-form-section" aria-labelledby="channel-reply-mode-heading">
          <div className="channel-section-heading"><span>05</span><div><h4 id="channel-reply-mode-heading">回复发送方式</h4><p>此设置只影响之后生成的回复；已有草稿保持原状。</p></div></div>
          <label className="channel-field-wide">新回复的处理方式
            <select aria-label="回复发送方式" value={replyMode} onChange={event => setReplyMode(event.target.value as ChannelInput["reply_mode"])}>
              <option value="automatic">自动回复 · 生成后直接发送</option>
              <option value="manual">人工审阅 · 批准后发送</option>
            </select>
          </label>
          <p className="channel-form-note">{replyModeDescription(replyMode)}</p>
        </section>

        {error && <p className="channels-error" role="alert">{error}</p>}
      </form>;

  if (presentation === "drawer") return <Drawer open side="right" title={channel ? "编辑消息渠道" : "添加消息渠道"} onClose={onClose} width={620} className="channel-editor-drawer">
    {form}<div className="channel-editor-drawer-footer">{footer}</div>
  </Drawer>;

  return <Modal open onClose={onClose} title={channel ? "编辑消息渠道" : "添加消息渠道"} size="lg" footer={footer}>{form}</Modal>;
}
