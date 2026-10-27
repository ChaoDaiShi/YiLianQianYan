import { useState } from "react";
import { Check, CheckCircle2, ChevronLeft, ChevronRight, Mic, Search } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "../components/ui";
import WorkspaceIcon from "../components/ui/WorkspaceIcon";
import { useProductPreferences } from "../components/system/useProductPreferences";
import QuickModelForm from "../features/setup/QuickModelForm";
import { useQuickModelSetup } from "../features/setup/useQuickModelSetup";
import "../styles/quickSetup.css";

export default function SetupWizardPage() {
  const navigate = useNavigate();
  const { preferences, loaded, save } = useProductPreferences();
  const setup = useQuickModelSetup();
  const [step, setStep] = useState(0);
  const [finishing, setFinishing] = useState(false);
  const [finishError, setFinishError] = useState("");
  const busy = finishing || setup.stage !== null;

  const finish = async (destination = "/chat") => {
    setFinishing(true);
    setFinishError("");
    try {
      await save({ ...preferences, setup_completed: true });
      window.sessionStorage.removeItem("yilian.setup.deferred");
      navigate(destination, { replace: true });
    } catch {
      setFinishError("首次配置完成状态未能保存，请重试。已连接的模型会保留。");
    } finally {
      setFinishing(false);
    }
  };
  const defer = () => {
    window.sessionStorage.setItem("yilian.setup.deferred", "true");
    navigate("/chat", { replace: true });
  };

  return <div className="quick-setup-page page-canvas">
    <div className="quick-setup-scroll">
      <div className="quick-setup-content">
        <header className="quick-setup-brand"><img src="/favicon.png" alt="" /><span>忆涟千言<span>让小涟准备好，和你一起开始</span></span></header>
        <nav className="quick-setup-steps" aria-label="首次配置步骤">
          <span aria-current={step === 0 ? "step" : undefined}><b>{step === 1 ? <Check size={15} /> : "1"}</b>连接模型</span><i aria-hidden="true" /><span aria-current={step === 1 ? "step" : undefined}><b>2</b>开始使用</span>
        </nav>
        <main className="quick-setup-card">
          {step === 0 ? <>
            <div className="quick-setup-emblem"><WorkspaceIcon name="models-services" size={28} variant="mono" /></div>
            <header><p className="quick-setup-eyebrow">开始前，只需连接一个模型</p><h1>连接你的第一个模型</h1><p>选择服务商，填入密钥。小涟会测试连接，确认可用后再开始。</p></header>
            <QuickModelForm setup={setup} onConnected={() => setStep(1)} />
          </> : <>
            <div className="quick-setup-emblem quick-setup-emblem--success"><CheckCircle2 size={28} /></div>
            <header><p className="quick-setup-eyebrow">模型已连接并启用</p><h1>小涟准备好了</h1><p>从一次对话开始，也可以随时在设置中调整模型。</p></header>
            <div className="quick-setup-model-summary"><WorkspaceIcon name="models-services" size={24} variant="mono" /><div><strong>{setup.profile?.model}</strong><span>{setup.profile?.label}</span></div><span className="quick-setup-verified"><Check size={14} />连接正常</span></div>
            <Button size="lg" loading={finishing} loadingLabel="正在保存配置…" disabled={!loaded || busy || !setup.ready} onClick={() => void finish()}>开始使用<ChevronRight size={17} /></Button>
            <div className="quick-setup-optional"><h2>这些能力，可以以后再添加</h2><p>无需一次配置全部内容，按需要开启就好。</p><div>
              <button type="button" disabled={busy || !loaded} onClick={() => void finish("/account")}><WorkspaceIcon name="account" size={24} variant="mono" /><span><strong>账号与同步</strong><small>连接账号，管理跨设备同步</small></span><ChevronRight /></button>
              <button type="button" disabled={busy || !loaded} onClick={() => void finish("/settings?section=search")}><Search /><span><strong>联网搜索</strong><small>需要时再设置搜索服务</small></span><ChevronRight /></button>
              <button type="button" disabled={busy || !loaded} onClick={() => void finish("/settings?section=voice")}><Mic /><span><strong>语音对话</strong><small>语音能力可以稍后配置</small></span><ChevronRight /></button>
            </div></div>
            {finishError && <p className="quick-setup-error" role="alert">{finishError}</p>}
          </>}
        </main>
        <footer className="quick-setup-footer">{step === 1 ? <Button variant="ghost" disabled={busy} onClick={() => setStep(0)}><ChevronLeft size={16} />调整模型</Button> : <span>账号、搜索和语音均可稍后配置</span>}{step === 0 && <button type="button" className="quick-setup-defer" disabled={busy || setup.loading} onClick={defer}>稍后配置<ChevronRight size={14} /></button>}</footer>
      </div>
    </div>
  </div>;
}
