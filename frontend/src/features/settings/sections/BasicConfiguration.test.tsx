import { renderToStaticMarkup } from "react-dom/server";
import { describe,it,expect } from "vitest";
import BasicConfiguration from "./BasicConfiguration";
describe("basic service navigation",()=>{
 it("offers all seven service entries in reading order and leaves dialogs closed until chosen",()=>{
  const html=renderToStaticMarkup(<BasicConfiguration modelPanel={<div>models</div>} renderVoice={side=><div>{side}</div>} embeddingPanel={<div>embedding</div>}/>);
  const labels=["对话模型与模型池","语音识别","语音合成","联网搜索","图像识别","图像生成","向量模型"];
  let previous=-1;for(const label of labels){const index=html.indexOf(label);expect(index).toBeGreaterThan(previous);previous=index;}expect(html).not.toContain('role="dialog"');
 });
});
