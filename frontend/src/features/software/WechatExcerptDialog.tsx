import {useRef, useState, type PointerEvent} from "react";
import {recognizeWechatCrop} from "../../api/software";
import {Button, Modal, Textarea} from "../../components/ui";
import {chatSelection, suggestChatSelection, type ChatPoint} from "./chatExcerpt";

interface Props {
  source: {png: string} | {text: string};
  onClose: () => void;
  onUse: (text: string) => void;
}

export default function WechatExcerptDialog({source, onClose, onUse}: Props) {
  const png = "png" in source ? source.png : undefined;
  const image = useRef<HTMLImageElement>(null);
  const anchor = useRef<ChatPoint | null>(null);
  const [area, setArea] = useState<ReturnType<typeof chatSelection> | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [cropPreview, setCropPreview] = useState("");
  const [text, setText] = useState<string | null>("text" in source ? source.text : null);

  function point(event: PointerEvent<HTMLDivElement>): ChatPoint {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)),
      y: Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height)),
    };
  }

  function suggestArea(img: HTMLImageElement) {
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(img, 0, 0);
    setArea(previous => previous ?? suggestChatSelection(ctx.getImageData(0, 0, canvas.width, canvas.height)));
  }

  async function recognize() {
    const img = image.current;
    if (!img?.naturalWidth || !area || busy) return;
    setBusy(true);
    setError("");
    try {
      const x = Math.floor(area.x * img.naturalWidth), y = Math.floor(area.y * img.naturalHeight);
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil((area.x + area.width) * img.naturalWidth) - x;
      canvas.height = Math.ceil((area.y + area.height) * img.naturalHeight) - y;
      if (canvas.width < 8 || canvas.height < 8) throw new Error("选区太小，请重新框选消息。");
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("无法读取选区图像。");
      ctx.drawImage(img, x, y, canvas.width, canvas.height, 0, 0, canvas.width, canvas.height);
      const preview = canvas.toDataURL("image/png");
      const recognized = await recognizeWechatCrop(preview.split(",")[1]);
      setCropPreview(preview);
      setText(recognized);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return <Modal open title="读取当前群聊" size="xl" onClose={onClose} footer={<>
    <Button variant="secondary" onClick={onClose}>取消</Button>
    {png && text !== null && <Button variant="secondary" onClick={() => {setText(null); setCropPreview(""); setError("");}}>重新框选</Button>}
    {text === null ? <Button loading={busy} disabled={busy || !area?.width || !area.height} onClick={() => void recognize()}>识别选区</Button> :
      <Button disabled={!text.trim()} onClick={() => onUse(text)}>带入 AI 对话</Button>}
  </>}>
    {text === null ? <>
      <p>核对快照中的群名和消息。可以拖动重新框选，再识别选区。</p>
      <div className="wechat-frame-container"><div className="wechat-frame-selection"
        onPointerDown={e => {
          if (busy || e.button !== 0 || !e.isPrimary) return;
          anchor.current = point(e); setArea(null); setError("");
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={e => {if (anchor.current && e.isPrimary) setArea(chatSelection(anchor.current, point(e)));}}
        onPointerUp={e => {
          if (!anchor.current || !e.isPrimary) return;
          setArea(chatSelection(anchor.current, point(e))); anchor.current = null;
        }}
        onPointerCancel={e => {if (e.isPrimary) {anchor.current = null; setArea(null);}}}>
        <img ref={image} src={`data:image/png;base64,${png}`} alt="本次微信窗口快照" draggable={false} onLoad={e => suggestArea(e.currentTarget)} />
        {area && <div aria-hidden="true" className="wechat-frame-outline" style={{left: `${area.x * 100}%`, top: `${area.y * 100}%`, width: `${area.width * 100}%`, height: `${area.height * 100}%`}}/>}
      </div></div>
    </> : <>
      {cropPreview && <details className="wechat-crop-preview"><summary>对照选区原图</summary><img src={cropPreview} alt="本次识别的微信聊天选区" /></details>}
      <Textarea label="核对群标题和消息" hint="可修改识别错字。下一步会打开对话草稿。" rows={9} value={text} onChange={e => setText(e.target.value)}/>
    </>}
    {error && <p role="alert" className="software-error">{error}</p>}
  </Modal>;
}
