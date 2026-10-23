import { useEffect, useRef, useState } from "react";
import { CheckCircle2, RefreshCw } from "lucide-react";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import { cancelChannelLogin, getChannelLogin, startChannelLogin, verifyChannelLogin, type ChannelLogin, type ExternalChannel } from "../../api/channels";
import { channelLoginStatusLabel, isChannelLoginComplete } from "./channelsModel";

const problem = (value: unknown) => value instanceof Error ? value.message : "微信登录操作失败";
const terminalLoginStatuses = new Set(["expired", "cancelled", "confirmed", "connected", "logged_in", "error", "binded_redirect", "verify_code_blocked"]);

export default function ChannelLoginDialog({ channel, initialLogin, onClose, onUpdated }: {
  channel: ExternalChannel;
  initialLogin: ChannelLogin;
  onClose: () => void;
  onUpdated: () => void;
}) {
  const [login, setLogin] = useState<ChannelLogin>(initialLogin);
  const [code, setCode] = useState("");
  const [loading, setLoading] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState("");
  const inFlight = useRef(false);
  const cancelInFlight = useRef(false);
  const alive = useRef(true);
  const onUpdatedRef = useRef(onUpdated);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  useEffect(() => { onUpdatedRef.current = onUpdated; }, [onUpdated]);

  useEffect(() => {
    if (!login || terminalLoginStatuses.has(login.status)) return;
    let active = true;
    const poll = async () => {
      if (!active || document.visibilityState !== "visible" || inFlight.current) return;
      inFlight.current = true;
      try {
        const result = await getChannelLogin(channel.id);
        if (active) {
          setLogin(result.login);
          setError("");
          onUpdatedRef.current();
        }
      } catch (reason) {
        if (active) setError(problem(reason));
      } finally {
        inFlight.current = false;
      }
    };
    const timer = window.setInterval(() => void poll(), 1800);
    return () => { active = false; window.clearInterval(timer); };
  }, [channel.id, login.status, login.qr_code_data_url]);

  async function verify() {
    if (!/^\d+$/.test(code) || verifying) return;
    setVerifying(true);
    setError("");
    try {
      const result = await verifyChannelLogin(channel.id, code);
      if (!alive.current) return;
      setLogin(result.login);
      setCode("");
      onUpdated();
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setVerifying(false);
    }
  }

  async function restartLogin() {
    setLoading(true);
    setError("");
    try {
      const result = await startChannelLogin(channel.id);
      if (!alive.current) return;
      setLogin(result.login);
      setCode("");
      onUpdated();
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setLoading(false);
    }
  }

  async function cancel() {
    if (cancelInFlight.current) return;
    cancelInFlight.current = true;
    setCancelling(true);
    setError("");
    try {
      await cancelChannelLogin(channel.id);
      if (alive.current) {
        onUpdated();
        onClose();
      }
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      cancelInFlight.current = false;
      if (alive.current) setCancelling(false);
    }
  }

  async function closeDialog() {
    if (loading || verifying || cancelling) {
      setError("微信登录请求正在处理中，请完成后再关闭。");
      return;
    }
    if (connected || terminalLoginStatuses.has(login.status)) {
      onClose();
      return;
    }
    await cancel();
  }

  const connected = isChannelLoginComplete(login.status);
  const needsVerifyCode = login.status === "need_verifycode";
  const expired = login.status === "expired";
  const scanned = login.status === "scaned" || login.status === "scanned" || login.status === "scaned_but_redirect";

  return (
    <Modal
      open
      onClose={() => void closeDialog()}
      title="微信扫码登录"
      size="md"
      footer={(
        <>
          {connected ? <Button onClick={onClose}>完成</Button> : <Button variant="ghost" disabled={cancelling || loading || verifying} loading={cancelling} onClick={() => void cancel()}>取消本次扫码</Button>}
        </>
      )}
    >
      <div className="channel-login-dialog">
        <p>渠道：<strong>{channel.name}</strong></p>
        {loading && <p role="status" className="channels-muted">正在向微信 iLink 请求登录二维码…</p>}
        {<div className="channel-login-state" data-status={login.status}>
          {connected ? <CheckCircle2 size={18} aria-hidden="true" /> : <span className="channel-login-state-dot" aria-hidden="true" />}
          <span>登录状态：{channelLoginStatusLabel(login.status)}</span>
        </div>}
        {login.qr_code_data_url && !connected && <img className="channel-login-qr" src={login.qr_code_data_url} alt="微信 iLink 登录二维码" />}
        {!login.qr_code_data_url && !connected && !expired && !scanned && !needsVerifyCode && !login.error && <p className="channels-muted">正在等待平台更新登录状态。</p>}
        {login.expires_at != null && <p className="channels-muted">二维码有效至 {new Date(login.expires_at).toLocaleString()}</p>}
        {login.bot_id && <p className="channels-muted">已登录账号 ID：{login.bot_id}</p>}
        {needsVerifyCode && <div className="channel-verify-code">
          <p>微信已在手机上显示数字验证码，请在下方输入并继续登录。</p>
          <label htmlFor="wechat-verify-code">手机显示的数字验证码</label>
          <div className="channel-verify-code-row">
            <input id="wechat-verify-code" maxLength={12} disabled={verifying} inputMode="numeric" autoComplete="one-time-code" value={code} onChange={event => setCode(event.target.value.replace(/\D/g, ""))} />
            <Button disabled={verifying || !/^\d+$/.test(code)} loading={verifying} onClick={() => void verify()}>验证并继续</Button>
          </div>
        </div>}
        {expired && <div className="channel-login-expired"><p>二维码已过期。关闭此窗口后再次开始扫码，会向平台重新请求二维码。</p><RefreshCw size={16} aria-hidden="true" /></div>}
        {login.error && <p className="channels-error" role="alert">{login.error}</p>}
        {expired && <Button variant="secondary" disabled={loading} loading={loading} onClick={() => void restartLogin()}><RefreshCw size={14} />重新请求二维码</Button>}
        {error && <p className="channels-error" role="alert">{error}</p>}
        {!connected && <p className="channels-muted">仅在此桌面端保存平台返回的登录凭据。登录成功表示授权完成，不表示已经收到消息。</p>}
      </div>
    </Modal>
  );
}
