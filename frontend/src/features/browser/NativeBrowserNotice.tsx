import { Button } from "../../components/ui";
/** A trusted notice above the native surface: a blocked subrequest need not mean the page failed. */
export default function NativeBrowserNotice({message,onDismiss}:{message:string;onDismiss:()=>void}) {
 if (!message) return null;
 return <div className="browser-native-notice" role="alert"><div><strong>网页状态提示</strong><p>{message.slice(0,1000)}</p><p>当前页面保留；此提示不代表整个网页加载失败。请检查实际页面后再决定是否继续。</p></div><Button variant="secondary" size="sm" onClick={onDismiss}>收起提示</Button></div>;
}
