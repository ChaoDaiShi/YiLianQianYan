import { useEffect, useState } from "react";
import { Activity, ExternalLink, Settings2, ShieldCheck, Trash2 } from "lucide-react";
import Button from "../../components/ui/Button";
import Drawer from "../../components/ui/Drawer";
import {
  getRobotPolicy, updateRobotPolicy, type RobotPolicy, type RobotPolicyInput,
} from "../../api/channelRobot";
import type { ChannelCheckItem, ChannelProvider, ExternalChannel } from "../../api/channels";
import { parseAllowedSenders, replyModeDescription, replyModeLabel, channelStatusLabel, connectionModeLabel } from "./channelsModel";

const problem = (value: unknown) => value instanceof Error ? value.message : "设置保存失败";

export interface ChannelSettingsDrawerProps {
  channel: ExternalChannel;
  provider: ChannelProvider | undefined;
  open: boolean;
  onClose: () => void;
  onPolicySaved: () => void;
  onEdit: () => void;
  onCheck: () => void;
  checkBusy: boolean;
  checks: ChannelCheckItem[] | undefined;
  onBeginLogin: () => void;
  loginBusy: boolean;
  onToggleChannel: () => void;
  channelBusy: boolean;
  replyBusy: boolean;
  onChangeReplyMode: () => void;
  onDelete: () => void;
}

export default function ChannelSettingsDrawer(props: ChannelSettingsDrawerProps) {
  const { channel, provider, open, onClose, onPolicySaved, onEdit, onCheck, checkBusy, checks, onBeginLogin, loginBusy, onToggleChannel, channelBusy, replyBusy, onChangeReplyMode, onDelete } = props;
  const [policy, setPolicy] = useState<RobotPolicy | null>(null);
  const [owners, setOwners] = useState("");
  const [privateIds, setPrivateIds] = useState("");
  const [groupIds, setGroupIds] = useState("");
  const [loadingPolicy, setLoadingPolicy] = useState(false);
  const [savingPolicy, setSavingPolicy] = useState(false);
  const [policyError, setPolicyError] = useState("");
  const [policyNotice, setPolicyNotice] = useState("");

  useEffect(() => {
    if (!open || channel.provider !== "onebot11") return;
    let active = true;
    setLoadingPolicy(true);
    setPolicyError("");
    void getRobotPolicy(channel.id).then(result => {
      if (!active) return;
      setPolicy(result.policy);
      setOwners(result.policy.owner_ids.join("\n"));
      setPrivateIds(result.policy.send_private_ids.join("\n"));
      setGroupIds(result.policy.send_group_ids.join("\n"));
    }).catch(reason => { if (active) setPolicyError(problem(reason)); })
      .finally(() => { if (active) setLoadingPolicy(false); });
    return () => { active = false; };
  }, [channel.id, channel.provider, open]);

  function setPolicyField<K extends keyof RobotPolicy>(key: K, value: RobotPolicy[K]) {
    setPolicy(current => current ? { ...current, [key]: value } : current);
    setPolicyNotice("");
  }

  async function savePolicy() {
    if (!policy || savingPolicy) return;
    setSavingPolicy(true);
    setPolicyError("");
    setPolicyNotice("");
    const input: RobotPolicyInput = {
      enabled: policy.enabled,
      owner_ids: parseAllowedSenders(owners),
      read_friends: policy.read_friends,
      read_groups: policy.read_groups,
      send_mode: policy.send_mode,
      send_private_ids: parseAllowedSenders(privateIds),
      send_group_ids: parseAllowedSenders(groupIds),
      send_all_private: policy.send_all_private,
      send_all_groups: policy.send_all_groups,
    };
    try {
      const result = await updateRobotPolicy(channel.id, input);
      setPolicy(result.policy);
      setOwners(result.policy.owner_ids.join("\n"));
      setPrivateIds(result.policy.send_private_ids.join("\n"));
      setGroupIds(result.policy.send_group_ids.join("\n"));
      setPolicyNotice("机器人权限已保存");
      onPolicySaved();
    } catch (reason) { setPolicyError(problem(reason)); }
    finally { setSavingPolicy(false); }
  }

  return <Drawer open={open} side="right" title={channel.provider === "onebot11" ? "QQ 机器人设置" : channel.name + " 设置"} onClose={onClose} width={540} className="channel-settings-panel">
    <div className="channel-settings-drawer">
      <section className="channel-settings-account">
        <div className="channel-settings-account-title"><div><span className="channel-step-label">账号与连接</span><h3>{channel.name}</h3><p>{provider?.label ?? channel.provider} · {connectionModeLabel(channel.connection_mode)}</p></div><span className="channel-status-pill" data-status={channel.status}>{channelStatusLabel(channel.status)}</span></div>
        <dl>
          {channel.self_id && <div><dt>当前操作账号</dt><dd>{channel.self_id}</dd></div>}
          <div><dt>账号凭据</dt><dd>{channel.credentials_configured ? "本机凭据库已配置" : "尚未完整配置"}</dd></div>
          <div><dt>允许接收私聊</dt><dd>{channel.allowed_senders.length} 个用户</dd></div>
          <div><dt>允许接收群</dt><dd>{channel.allowed_groups.length} 个群</dd></div>
          {channel.provider !== "wechat_onebot" && <div><dt>回复方式</dt><dd>{replyModeLabel(channel.reply_mode)}</dd></div>}
        </dl>
        <div className="channel-settings-actions">
          <Button variant="secondary" size="sm" onClick={onEdit}><Settings2 size={14} />编辑接入配置</Button>
          <Button variant="secondary" size="sm" disabled={checkBusy} loading={checkBusy} onClick={onCheck}><ShieldCheck size={14} />检查连接</Button>
          {channel.provider === "wechat_ilink" && <Button variant="secondary" size="sm" disabled={loginBusy} loading={loginBusy} onClick={onBeginLogin}>开始扫码</Button>}
          <Button variant="secondary" size="sm" disabled={replyBusy} loading={replyBusy} onClick={onChangeReplyMode}>{channel.reply_mode === "automatic" ? "改为审阅发送" : "开启自动回复"}</Button>
          <Button variant="ghost" size="sm" disabled={channelBusy} loading={channelBusy} onClick={onToggleChannel}>{channel.enabled ? "停用渠道" : "启用渠道"}</Button>
          <Button variant="ghost" size="sm" onClick={onDelete}><Trash2 size={14} />删除渠道</Button>
        </div>
        {channel.provider !== "wechat_onebot" && <p className="channel-settings-note">{replyModeDescription(channel.reply_mode)}</p>}
        {channel.provider === "wechat_onebot" && <p className="channel-settings-note">Akasha-WeChat 是单独的 reverse_websocket provider。当前应用没有微信扫码登录，也不会把 QQ 通讯录／历史工具显示为微信能力。</p>}
        {channel.last_error && <p className="channels-error" role="alert">最近错误：{channel.last_error}</p>}
        {provider?.limitations.map((item, index) => <p className="channel-settings-note" key={index}>{item}</p>)}
        {provider && <a className="channel-provider-doc-link" href={provider.docs_url} target="_blank" rel="noreferrer">平台资料 <ExternalLink size={13} /></a>}
      </section>

      {checks && <section className="channel-settings-checks" aria-label="连接检查结果"><h3><Activity size={14} />连接检查</h3>{checks.map((item, index) => <div key={item.label + index} data-ok={item.ok}><strong>{item.ok ? "通过" : "未通过"} · {item.label}</strong><p>{item.detail}</p></div>)}</section>}

      {channel.provider === "onebot11" ? <section className="channel-robot-policy-editor" aria-label="QQ 机器人权限设置">
        <div className="channel-section-heading"><span><ShieldCheck size={13} /></span><div><h3>机器人自主模式</h3><p>主人身份、读取范围与自动发送范围分别保存。</p></div></div>
        {loadingPolicy && <p role="status" className="channel-workspace-muted">正在读取已保存权限…</p>}
        {policyError && <p className="channels-error" role="alert">{policyError}</p>}
        {policy && <>
          <label className="channel-check-label"><input type="checkbox" checked={policy.enabled} onChange={event => setPolicyField("enabled", event.target.checked)} />启用机器人自主模式</label>
          <div className="channel-policy-account"><span>当前渠道账号</span><strong>{channel.self_id || "尚未配置"}</strong><p>{policy.account_id ? "当前授权范围绑定到账号 " + policy.account_id : "首次保存权限时，将绑定当前渠道账号。"}</p>{policy.account_id && policy.account_id !== channel.self_id && <p className="channels-error" role="alert">当前渠道账号已变化。保存新的权限范围前，不会对新账号执行自主操作。</p>}</div>
          <label>主人 QQ 号<textarea aria-label="主人 QQ 号" rows={2} value={owners} onChange={event => { setOwners(event.target.value); setPolicyNotice(""); }} placeholder="每行一个 QQ 号" /></label>
          <p className="channel-settings-note">主人身份按已验证的发送者 ID 判断，不由聊天正文决定。普通联系人不会因此获得跨会话控制权。</p>
          <fieldset className="channel-policy-mode"><legend>自动发送策略</legend><label><input type="radio" name="robot-send-mode" value="automatic" checked={policy.send_mode === "automatic"} onChange={() => setPolicyField("send_mode", "automatic")} />范围内自动发送</label><label><input type="radio" name="robot-send-mode" value="manual" checked={policy.send_mode === "manual"} onChange={() => setPolicyField("send_mode", "manual")} />每条消息先审阅</label></fieldset>
          <div className="channel-policy-scope">
            <h4>读取范围</h4>
            <label className="channel-check-label"><input type="checkbox" checked={policy.read_friends} onChange={event => setPolicyField("read_friends", event.target.checked)} />允许读取好友</label>
            <label className="channel-check-label"><input type="checkbox" checked={policy.read_groups} onChange={event => setPolicyField("read_groups", event.target.checked)} />允许读取群</label>
          </div>
          <div className="channel-policy-scope">
            <h4>私聊自动发送</h4>
            <label className="channel-check-label"><input type="checkbox" checked={policy.send_all_private} onChange={event => setPolicyField("send_all_private", event.target.checked)} />向全部好友自动发送</label>
            <label>允许自动发送的好友 ID<textarea aria-label="允许自动发送的好友 ID" rows={3} disabled={policy.send_all_private} value={privateIds} onChange={event => { setPrivateIds(event.target.value); setPolicyNotice(""); }} placeholder="每行一个 QQ 好友号" /></label>
          </div>
          <div className="channel-policy-scope">
            <h4>群聊自动发送</h4>
            <label className="channel-check-label"><input type="checkbox" checked={policy.send_all_groups} onChange={event => setPolicyField("send_all_groups", event.target.checked)} />向全部群自动发送</label>
            <label>允许自动发送的群 ID<textarea aria-label="允许自动发送的群 ID" rows={3} disabled={policy.send_all_groups} value={groupIds} onChange={event => { setGroupIds(event.target.value); setPolicyNotice(""); }} placeholder="每行一个 QQ 群号" /></label>
          </div>
          {policyNotice && <p className="channel-robot-notice" role="status">{policyNotice}</p>}
          <Button disabled={loadingPolicy || savingPolicy} loading={savingPolicy} onClick={() => void savePolicy()}>保存机器人权限</Button>
        </>}
      </section> : channel.provider === "wechat_onebot" ? <section className="channel-akasha-limits" aria-label="Akasha 微信支持范围"><h3>Akasha-WeChat 接入范围</h3><p>私聊与允许群内的文本收发由实际桥接连接决定。此处不显示 QQ 主人、目录、历史或 AgentTurn 工具。</p></section> : <section className="channel-akasha-limits"><h3>当前平台功能</h3><p>配置、连接检查和收件能力按该渠道的真实接口状态展示。</p></section>}
    </div>
  </Drawer>;
}
