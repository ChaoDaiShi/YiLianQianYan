import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  getCloudProfile,
  type CloudIdentity,
  type LogoutReport,
  type ProfileView,
} from "../../api/cloud";
import { revocationPresentation } from "./profileSwitch";
import { Badge, Button } from "../../components/ui";

interface AccountProfilePanelProps {
  identity?: CloudIdentity;
  lastLogoutReport?: LogoutReport | null;
  onRequestLogin?: () => void;
  initialProfileView?: ProfileView | null;
  guestMode?: boolean;
}

export function AccountProfilePanel({
  identity,
  lastLogoutReport = null,
  onRequestLogin,
  initialProfileView = null,
  guestMode = false,
}: AccountProfilePanelProps) {
  const [profileView, setProfileView] = useState(initialProfileView);
  const [loading, setLoading] = useState(!initialProfileView);
  const [error, setError] = useState("");

  const fetchProfile = async () => {
    setLoading(true);
    setError("");
    const result = await getCloudProfile();
    if (result.ok) {
      setProfileView(result.data);
    } else {
      setError(result.error);
    }
    setLoading(false);
  };

  useEffect(() => {
    void fetchProfile();
  }, []);

  const revocationMeta = lastLogoutReport
    ? revocationPresentation(lastLogoutReport.remote_revocation)
    : null;
  const currentProfile = profileView?.profiles.find((record) => record.profile_id === profileView.active_profile_id);
  const pendingRevocations = profileView?.pending_revocations ?? [];

  return (
    <div className="cloud-card account-profile-panel">
      {!guestMode ? (
        <div className="cloud-card-title">
          <div>
            <h3>切换账号</h3>
            <p>登录目标账号后，应用会使用该账号的本机工作区。</p>
          </div>
          {onRequestLogin ? <Button variant="secondary" size="sm" onClick={onRequestLogin}>登录目标账号</Button> : null}
        </div>
      ) : null}
      {currentProfile?.reauthentication_required ? <p className="cloud-inline-error" role="status">当前账号需重新登录。</p> : null}

      {lastLogoutReport ? (
        <div className="cloud-profile-notice">
          <div className="cloud-actions">
            <Badge tone={lastLogoutReport.local_cleanup === "cleared" ? "success" : "danger"}>
              {lastLogoutReport.local_cleanup === "cleared" ? "本地已退出" : "本地退出未完成"}
            </Badge>
            {revocationMeta ? <Badge tone={revocationMeta.tone}>{revocationMeta.label}</Badge> : null}
          </div>
          <p>
            {lastLogoutReport.local_cleanup === "cleared" ? "本地凭据已清除。" : "本地仍有未清理凭据，请重试。"}
            {lastLogoutReport.remote_revocation === "pending" ? "联网后会重试撤销远端会话。" : null}
          </p>
          {lastLogoutReport.error_codes?.length ? (
            <details className="cloud-disclosure">
              <summary>退出登录诊断</summary>
              <p>错误代码：{lastLogoutReport.error_codes.join("、")}</p>
            </details>
          ) : null}
        </div>
      ) : null}

      {pendingRevocations.length ? (
        <div className="cloud-inline-error" role="status">
          <strong>远端撤销待处理</strong>
          <ul>
            {pendingRevocations.map((item) => (
              <li key={`${item.profile_id}:${item.session_id}`}>
                {item.remote_revocation === "unknown" ? "远端会话状态未知" : "正在重试撤销远端会话"}
                {item.last_error_code ? ` · ${item.last_error_code}` : ""}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <details className="cloud-disclosure account-profile-details">
        <summary>本机身份与数据详情</summary>
        <Button variant="secondary" size="sm" onClick={() => void fetchProfile()} disabled={loading}>
          <RefreshCw />刷新本机身份
        </Button>
        {loading ? <p role="status">正在读取本机身份…</p> : null}
        {error ? <p className="cloud-inline-error" role="alert">{error}</p> : null}
        <dl className="cloud-data-list">
          {identity ? (
            <>
              <div><dt>本机身份</dt><dd>{identity.local_identity_id}</dd></div>
              <div><dt>当前设备</dt><dd>{identity.device_id}</dd></div>
              {identity.account_id ? <div><dt>云端账号</dt><dd>{identity.account_id}</dd></div> : null}
            </>
          ) : null}
          {profileView && !guestMode ? (
            <>
              <div><dt>本机工作区</dt><dd>{profileView.active_profile_id}</dd></div>
              <div><dt>当前版本</dt><dd>{profileView.active_generation}</dd></div>
              <div><dt>数据范围标识</dt><dd>{profileView.active_lease.scope_id}</dd></div>
            </>
          ) : null}
          {!guestMode ? <div><dt>凭据存储</dt><dd>登录凭据保存在系统凭据库。服务密钥保留在本机。</dd></div> : null}
        </dl>
        {!guestMode && profileView?.profiles.length ? (
          <div className="cloud-session-list">
            <h4>本机工作区列表</h4>
            {profileView.profiles.map((record) => (
              <article key={record.profile_id}>
                <div>
                  <strong>{record.kind === "guest" ? "本机工作区" : record.service_origin || "账号工作区"}</strong>
                  <span>{record.profile_id}{record.account_id ? ` · ${record.account_id}` : ""}</span>
                </div>
                <div className="cloud-actions">
                  {record.profile_id === profileView.active_profile_id ? <Badge tone="info">当前工作区</Badge> : null}
                  {record.reauthentication_required ? <Badge tone="warning">需重新登录</Badge> : null}
                </div>
              </article>
            ))}
          </div>
        ) : null}
      </details>
    </div>
  );
}
