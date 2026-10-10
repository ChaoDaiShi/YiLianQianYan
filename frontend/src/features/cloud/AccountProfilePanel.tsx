import React, { useEffect, useState } from "react";
import {
  getCloudProfile,
  type ActivationPlan,
  type LogoutReport,
  type ProfileRecord,
  type ProfileView,
} from "../../api/cloud";
import {
  ProfileSwitchController,
  revocationPresentation,
  type SwitchState,
} from "./profileSwitch";
import { Badge, Button, Spinner } from "../../components/ui";
import { AlertTriangle, CheckCircle2, RefreshCw, Shield, User } from "lucide-react";

interface AccountProfilePanelProps {
  controller?: ProfileSwitchController;
  lastLogoutReport?: LogoutReport | null;
  onRestartPrompt?: (plan: ActivationPlan) => void;
  onRequestLogin?: () => void;
  initialLoading?: boolean;
  initialProfileView?: ProfileView | null;
}

export const AccountProfilePanel: React.FC<AccountProfilePanelProps> = ({
  controller = new ProfileSwitchController(),
  lastLogoutReport = null,
  onRestartPrompt: _onRestartPrompt,
  onRequestLogin,
  initialLoading = false,
  initialProfileView = null,
}) => {
  const [profileView, setProfileView] = useState<ProfileView | null>(initialProfileView);
  const [loading, setLoading] = useState(initialLoading);
  const [error, setError] = useState<string>("");
  const [switchState] = useState<SwitchState>("idle");
  const [restartPlan] = useState<ActivationPlan | null>(null);

  const fetchProfile = async () => {
    setLoading(true);
    setError("");
    const result = await getCloudProfile();
    if (result.ok) {
      setProfileView(result.data);
      controller.setProfileInfo(
        result.data.active_profile_id,
        result.data.active_generation
      );
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

  return (
    <div className="account-profile-panel space-y-6 p-4 rounded-lg bg-surface border border-border">
      <div className="flex items-center justify-between border-b border-border pb-3">
        <div className="flex items-center space-x-2">
          <Shield className="w-5 h-5 text-primary" />
          <h2 className="text-lg font-semibold">物理账号与 Profile 隔离</h2>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void fetchProfile()}
          disabled={loading || switchState === "quiescing"}
        >
          <RefreshCw className="w-4 h-4 mr-1" />
          刷新状态
        </Button>
      </div>

      {loading ? (
        <div className="flex items-center justify-center p-8">
          <Spinner className="w-6 h-6" />
          <span className="ml-2 text-muted-foreground">正在加载 Profile 元数据...</span>
        </div>
      ) : (
        <>
          {error && (
            <div className="p-3 bg-destructive/10 text-destructive rounded-md flex items-center space-x-2">
              <AlertTriangle className="w-5 h-5 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {/* Active Profile Info */}
          {profileView && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 bg-muted/30 p-4 rounded-md">
              <div>
                <span className="text-xs text-muted-foreground block">当前激活 Profile</span>
                <span className="font-mono text-sm font-medium">{profileView.active_profile_id}</span>
              </div>
              <div>
                <span className="text-xs text-muted-foreground block">当前租约 Generation</span>
                <span className="font-mono text-sm font-medium">{profileView.active_generation}</span>
              </div>
              <div className="col-span-1 md:col-span-2">
                <span className="text-xs text-muted-foreground block">ScopedDb 作用域 (SHA-256)</span>
                <span className="font-mono text-xs text-muted-foreground break-all">
                  {profileView.active_lease.scope_id}
                </span>
              </div>
            </div>
          )}

          {/* Revocation Report Banner */}
          {lastLogoutReport && (
            <div className="p-3 border rounded-md flex items-start space-x-3 bg-muted/20">
              <div className="flex flex-col gap-1 w-full">
                <div className="flex items-center space-x-2">
                  <Badge tone={lastLogoutReport.local_cleanup === "cleared" ? "success" : "danger"}>
                    {lastLogoutReport.local_cleanup === "cleared" ? "本地已退出" : "本地退出未完成"}
                  </Badge>
                  {revocationMeta && <Badge tone={revocationMeta.tone}>{revocationMeta.label}</Badge>}
                </div>
                <div className="text-sm mt-1">
                  <p className="font-medium">
                    {lastLogoutReport.local_cleanup === "cleared" ? "本地凭据已清除。" : "本地仍有未清理凭据，请重试。"}
                    {revocationMeta ? ` ${revocationMeta.detail}` : ""}
                  </p>
                  {lastLogoutReport.remote_revocation === "pending" && (
                    <p className="text-xs text-muted-foreground mt-1">
                      系统将在后台持专属撤销凭据进行指数退避重试，绝不会重新激活已退出账号。
                    </p>
                  )}
                  {lastLogoutReport.error_codes?.length > 0 && (
                    <p className="text-xs text-destructive mt-1 font-mono">
                      错误代码: {lastLogoutReport.error_codes.join(", ")}
                    </p>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Restart Required Warning Banner */}
          {switchState === "awaiting_restart" && restartPlan && (
            <div className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-md space-y-2">
              <div className="flex items-center space-x-2 text-amber-600 font-medium">
                <AlertTriangle className="w-5 h-5" />
                <span>配置文件已准备就绪，需要重启应用生效</span>
              </div>
              <p className="text-sm text-muted-foreground">
                新 Profile <code className="font-mono">{restartPlan.profile_id}</code> 已完成物理目录创建与事务门闩隔离。请重启桌面客户端以载入专属隔离数据库。
              </p>
              <div className="pt-2 flex items-center space-x-2">
                <Badge tone="warning">Awaiting Restart</Badge>
                <span className="text-xs text-muted-foreground">
                  已阻断旧 Profile 的写入与新操作。
                </span>
              </div>
            </div>
          )}

          {/* Controlled Profile Switch Section */}
          <div className="border-t border-border pt-4 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-medium flex items-center space-x-2">
                  <User className="w-4 h-4 text-primary" />
                  <span>切换账号</span>
                </h3>
                <p className="text-xs text-muted-foreground mt-1">
                  切换账号统一通过云端登录认证，目标账号由云端确认，隔离各账号数据。
                </p>
              </div>
              {onRequestLogin && (
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => onRequestLogin()}
                >
                  登录目标账号
                </Button>
              )}
            </div>
          </div>

          {/* Existing Profiles List */}
          {profileView && profileView.profiles.length > 0 && (
            <div className="border-t border-border pt-4 space-y-2">
              <h4 className="text-xs font-medium text-muted-foreground">已注册的本地 Profiles</h4>
              <div className="space-y-1">
                {profileView.profiles.map((record: ProfileRecord) => (
                  <div
                    key={record.id}
                    className="flex items-center justify-between p-2 rounded bg-muted/20 text-xs"
                  >
                    <div className="flex items-center space-x-2">
                      {record.id === profileView.active_profile_id ? (
                        <CheckCircle2 className="w-4 h-4 text-emerald-500" />
                      ) : (
                        <span className="w-4 h-4 rounded-full border border-border inline-block" />
                      )}
                      <span className="font-mono font-medium">{record.id}</span>
                      <Badge tone={record.kind === "guest" ? "default" : "info"}>
                        {record.kind}
                      </Badge>
                      {record.reauthentication_required && (
                        <Badge tone="warning">需重新登录</Badge>
                      )}
                    </div>
                    <span className="text-muted-foreground">
                      {record.service_origin ? `${record.service_origin} (${record.account_id})` : "本地游客"}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
};
