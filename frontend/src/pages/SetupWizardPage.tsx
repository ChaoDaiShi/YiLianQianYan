import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Button } from "../components/ui";
import { managementRequest } from "../api/productManagement";
import { listWorkspaces, type Workspace } from "../api/workspaces";
import { getSystemInfo } from "../api/system";
import { listCliProviders } from "../api/providerConnection";
import { useTheme } from "../theme";
import QuickModelForm from "../features/setup/QuickModelForm";
import SetupPetPreference from "../features/setup/SetupPetPreference";
import { useQuickModelSetup } from "../features/setup/useQuickModelSetup";
import "../styles/quickSetup.css";

const titles = [
  "欢迎使用忆涟千言",
  "你的基本资料",
  "你希望怎样交流",
  "权限与记忆",
  "设备与模型",
  "外观与工作空间",
  "确认个人设置",
];
const identity = {
  display_name: "希望小涟怎样称呼你",
  language: "交流语言",
  time_zone: "时区",
  background: "职业或学习背景",
  scenario: "主要使用场景",
};
const habits = {
  reply_length: "回复长度",
  style: "表达风格",
  initiative: "主动建议程度",
  depth: "技术解释深度",
  show_steps: "执行过程展示偏好",
};
const capabilities = {
  memory: "在对话中使用长期记忆",
  screen: "允许请求屏幕截图",
  microphone: "允许语音会话申请麦克风",
  files: "允许工具申请读写本地文件",
  external_model: "允许向外部聊天模型发送对话内容",
};
type Profile = { fields: Record<string, string>; permissions: Record<string, boolean> };
type Status = {
  revision: number;
  current_step: number;
  draft: Profile | null;
  profile: Profile;
  setup_completed: boolean;
  draft_workspace?: string | null;
};
const empty: Profile = { fields: {}, permissions: {} };

function DeviceSummary() {
  const [info, setInfo] = useState("正在读取设备信息…");
  const [cli, setCli] = useState("");
  const [local, setLocal] = useState("");
  const [scanning, setScanning] = useState(false);
  async function detectLocal() {
    setScanning(true);
    try {
      const result = await managementRequest<{ candidates: { kind: string; models?: string[] }[] }>(
        "POST",
        "/api/capabilities/local-discovery",
        { directories: [], include_listeners: true },
      );
      const models = result.candidates.filter((c) => c.kind === "model").flatMap((c) => c.models ?? []);
      setLocal(models.length ? models.join("、") : "没有发现可列出模型的本地服务。");
    } catch {
      setLocal("本地模型检测未完成，可继续配置。");
    } finally {
      setScanning(false);
    }
  }
  useEffect(() => {
    let active = true;
    void listCliProviders()
      .then((value) => {
        if (active)
          setCli(
            value
              ?.filter((item) => item.ready)
              .map((item) => item.label)
              .join("、") || "当前没有可用 CLI",
          );
      })
      .catch(() => {
        if (active) setCli("CLI 检测暂不可用");
      });
    void getSystemInfo()
      .then((v) => {
        if (active)
          setInfo(
            v?.cpu
              ? [
                  v.os,
                  v.cpu.name,
                  v.memory.total_gb + " GiB 内存（可用 " + v.memory.available_gb + " GiB）",
                  ...(v.gpu ?? []).map((g: { name: string }) => g.name),
                ]
                  .filter(Boolean)
                  .join(" · ")
              : "设备信息暂不可用，可继续设置。",
          );
      })
      .catch(() => {
        if (active) setInfo("设备检测暂不可用，可继续设置。");
      });
    return () => {
      active = false;
    };
  }, []);
  return (
    <div>
      <p role="status">{info}</p>
      <p>{cli}</p>
      <Button variant="secondary" disabled={scanning} onClick={() => void detectLocal()}>
        {scanning ? "正在检测…" : "检测本地模型服务"}
      </Button>
      {local && <p role="status">{local}</p>}
    </div>
  );
}

export default function SetupWizardPage() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const theme = useTheme();
  const setup = useQuickModelSetup();
  const [status, setStatus] = useState<Status | null>(null);
  const [step, setStep] = useState(0);
  const [profile, setProfile] = useState<Profile>(empty);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspace, setWorkspace] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);
  const revision = useRef(0);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const draftTimer = useRef<ReturnType<typeof setTimeout>>();
  const dirty = useRef(false);
  function persist(path: string, current_step: number, values: Profile, workspaceId: string) {
    const request = queue.current.then(async () => {
      const result = await managementRequest<Status>("POST", "/api/v1/onboarding/" + path, {
        expected_revision: revision.current,
        current_step,
        profile:
          path === "complete"
            ? {
                ...values,
                permissions: Object.fromEntries(
                  Object.keys(capabilities).map((key) => [key, values.permissions[key] ?? key !== "screen"]),
                ),
              }
            : values,
        workspace_id: workspaceId || null,
      });
      revision.current = result.revision;
      return result;
    });
    queue.current = request.catch(() => undefined);
    return request;
  }
  useEffect(() => {
    if (!dirty.current) return;
    draftTimer.current = setTimeout(() => {
      void persist("steps", step, profile, workspace).catch((cause) => setError(String(cause)));
    }, 350);
    return () => clearTimeout(draftTimer.current);
  }, [profile, workspace, step]);
  useEffect(() => {
    let active = true;
    void managementRequest<Status>("GET", "/api/v1/onboarding/status")
      .then((value) => {
        if (!active) return;
        dirty.current = false;
        revision.current = value.revision;
        setStatus(value);
        const initial = value.draft ?? value.profile;
        setProfile(
          !value.draft && !value.setup_completed && !value.revision
            ? { ...initial, permissions: Object.fromEntries(Object.keys(capabilities).map((k) => [k, false])) }
            : initial,
        );
        setWorkspace(value.draft_workspace ?? "");
        setStep(params.has("permissions") ? 3 : value.draft ? value.current_step : params.has("model") ? 4 : 0);
        setError("");
      })
      .catch((cause) => {
        if (active) setError(String(cause));
      });
    void listWorkspaces().then((result) => {
      if (active && result.ok) setWorkspaces(result.data.filter((w) => w.status === "active"));
    });
    return () => {
      active = false;
    };
  }, [params, reload]);
  const field = (key: string, value: string) => {
    dirty.current = true;
    setProfile((p) => ({ ...p, fields: { ...p.fields, [key]: value } }));
  };
  async function save(next: number, complete = false, leave = false) {
    if (!status || busy) return;
    setBusy(true);
    setError("");
    try {
      clearTimeout(draftTimer.current);
      dirty.current = false;
      const result = await persist(complete ? "complete" : "steps", next, profile, workspace);
      setStatus(result);
      setStep(next);
      if (complete || leave) {
        if (leave) window.sessionStorage.setItem("yilian.setup.deferred", "true");
        else {
          window.sessionStorage.removeItem("yilian.setup.deferred");
          const prefs = await managementRequest("GET", "/api/system/preferences");
          window.dispatchEvent(new CustomEvent("yilian-product-preferences", { detail: prefs }));
        }
        navigate("/chat", { replace: true });
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "个人设置保存失败");
    } finally {
      setBusy(false);
    }
  }
  const inputs = step === 1 ? identity : habits;
  return (
    <main className="quick-setup-page page-canvas">
      <div className="quick-setup-scroll">
        <div className="quick-setup-content">
          <header className="quick-setup-brand">
            <img src="/favicon.png" alt="" />
            <span>忆涟千言</span>
          </header>
          <section className="quick-setup-card" aria-busy={busy}>
            {status && (
              <>
                <p className="quick-setup-note">
                  个人设置 · 第 {step + 1} 步，共 {titles.length} 步
                </p>
                <h1>{titles[step]}</h1>
                <fieldset className="profile-step" disabled={busy || setup.stage !== null}>
                  {step === 0 && (
                    <p>
                      资料、对话和工作空间保存在当前账号的本地数据库。使用外部模型时，会将本轮对话和所选上下文发送给服务商。你可以跳过个人资料，也可以随时在设置中回来修改。
                    </p>
                  )}
                  {(step === 1 || step === 2) &&
                    Object.entries(inputs).map(([key, label]) => (
                      <label key={key}>
                        {label}
                        {key === "show_steps" ? <select value={profile.fields[key] ?? ""} onChange={e=>field(key,e.target.value)}>
                          <option value="">跟随当前处理状态</option><option value="展开">默认展开</option><option value="收起">默认收起</option>
                          {profile.fields[key] && !["展开","收起"].includes(profile.fields[key]) && <option value={profile.fields[key]}>{profile.fields[key]}</option>}
                        </select> : <input
                          maxLength={512}
                          value={profile.fields[key] ?? ""}
                          onChange={(e) => field(key, e.target.value)}
                        />}
                      </label>
                    ))}
                  {step === 3 && (
                    <>
                      <p>每项能力单独设置。文件和桌面工具仍按现有授权范围执行。</p>
                      {Object.entries(capabilities).map(([key, label]) => (
                        <label className="profile-toggle" key={key}>
                          <input
                            type="checkbox"
                            checked={profile.permissions[key] ?? key !== "screen"}
                            onChange={(e) => {
                              dirty.current = true;
                              setProfile((p) => ({
                                ...p,
                                permissions: { ...p.permissions, [key]: e.target.checked },
                              }));
                            }}
                          />
                          {label}
                        </label>
                      ))}
                    </>
                  )}
                  {step === 4 && (
                    <>
                      <DeviceSummary />
                      <h2>连接你的第一个模型</h2>
                      {profile.permissions.external_model !== false ? (
                        <QuickModelForm setup={setup} onConnected={() => void save(5)} />
                      ) : (
                        <p>外部聊天模型已关闭，可在上一步更改。</p>
                      )}
                      <p>可以稍后配置模型。语音能力可以稍后配置。</p>
                    </>
                  )}
                  {step === 5 && (
                    <>
                      <SetupPetPreference />
                      <label>
                        界面外观
                        <select
                          value={theme.mode}
                          onChange={(e) => theme.setMode(e.target.value as typeof theme.mode)}
                        >
                          <option value="system">跟随系统</option>
                          <option value="light">浅色</option>
                          <option value="dark">深色</option>
                        </select>
                      </label>
                      <label>
                        默认工作空间
                        <select
                          value={workspace}
                          onChange={(e) => {
                            dirty.current = true;
                            setWorkspace(e.target.value);
                          }}
                        >
                          <option value="">保留当前工作空间</option>
                          {workspaces.map((w) => (
                            <option key={w.id} value={w.id}>
                              {w.name}
                            </option>
                          ))}
                        </select>
                      </label>
                    </>
                  )}
                  {step === 6 && (
                    <>
                      <p>
                        {profile.fields.display_name ? profile.fields.display_name + "，" : ""}
                        保存后，这些资料和交流偏好会用于后续对话。
                      </p>
                      <dl>
                        {Object.entries({ ...identity, ...habits })
                          .filter(([k]) => profile.fields[k])
                          .map(([k, label]) => (
                            <div key={k}>
                              <dt>{label}</dt>
                              <dd>{profile.fields[k]}</dd>
                            </div>
                          ))}
                      </dl>
                      <p>
                        已开启：
                        {Object.entries(capabilities)
                          .filter(([k]) => profile.permissions[k] ?? k !== "screen")
                          .map(([, v]) => v)
                          .join("、") || "无"}
                      </p>
                    </>
                  )}
                </fieldset>
                <div className="quick-setup-footer">
                  <Button
                    variant="ghost"
                    disabled={busy || step === 0 || setup.stage !== null}
                    onClick={() => void save(step - 1)}
                  >
                    上一步
                  </Button>
                  {step === 3 && status.setup_completed && (
                    <Button variant="secondary" disabled={busy} onClick={() => void save(3, true)}>
                      立即保存权限
                    </Button>
                  )}
                  <Button
                    disabled={busy || setup.stage !== null}
                    onClick={() => void save(Math.min(6, step + 1), step === 6)}
                  >
                    {step === 6 ? "保存并开始使用" : "下一步"}
                  </Button>
                </div>
                <button
                  className="quick-setup-defer"
                  disabled={busy || setup.stage !== null}
                  onClick={() => void save(step, false, true)}
                >
                  保存进度，稍后配置
                </button>
              </>
            )}
            {!status && !error && <p role="status">正在读取本地资料…</p>}
            {error && (
              <div className="quick-setup-error" role="alert">
                {error} <button onClick={() => setReload((n) => n + 1)}>重新载入已保存资料</button>
              </div>
            )}
          </section>
        </div>
      </div>
    </main>
  );
}
