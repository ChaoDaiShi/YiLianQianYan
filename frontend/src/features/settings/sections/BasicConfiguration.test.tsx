import { renderToStaticMarkup } from "react-dom/server";
import { describe,it,expect } from "vitest";
import BasicConfiguration from "./BasicConfiguration";
import BasicServicePanel from "./BasicServicePanel";
describe("basic service navigation",()=>{
 it("opens image service controls first and keeps model pool management collapsed",()=>{
  for(const kind of ["vision","image"] as const){
   const html=renderToStaticMarkup(<BasicServicePanel kind={kind}/>);
   expect(html).toContain(kind==="vision"?"图像识别服务":"图像生成服务");
   expect(html).toContain(kind==="vision"?"新建识图服务":"新建生图服务");
   expect(html).toContain('aria-label="服务档案"');
   expect(html).toContain("高级：模型池调度");
   expect(html).not.toContain("新建模型池");
   expect(html).not.toContain("厂商账户与模型池");
  }
 });
 it("offers all seven service entries in reading order and leaves dialogs closed until chosen",()=>{
  const html=renderToStaticMarkup(<BasicConfiguration modelPanel={<div>models</div>} renderVoice={side=><div>{side}</div>} embeddingPanel={<div>embedding</div>}/>);
  const labels=["对话模型与模型池","语音识别","语音合成","联网搜索","图像识别","图像生成","向量模型"];
  let previous=-1;for(const label of labels){const index=html.indexOf(label);expect(index).toBeGreaterThan(previous);previous=index;}expect(html).not.toContain('role="dialog"');
 });
});
