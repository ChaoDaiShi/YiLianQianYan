import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import ToolCallCard from "./ToolCallCard";
import MessageProcess from "./MessageProcess";
import type { ToolCallRecord } from "../../types";

const failure:ToolCallRecord={toolCallId:"vision-missing-key",name:"vision_analysis",args:{prompt:"描述这张已提供的图片"},status:"error",result:"No API key configured",verificationStatus:"failed",verificationReason:"工具执行失败"};
const render=(element:React.ReactNode)=>renderToStaticMarkup(<MemoryRouter>{element}</MemoryRouter>);

describe("visible tool failures",()=>{
  it("shows the real cause before technical details are opened",()=>{
    const html=render(<ToolCallCard {...failure} compact/>);
    expect(html).toContain("图像理解服务所选厂商账户尚未配置 API 密钥");
    expect(html).toContain('href="/settings?section=vision"');
    expect(html).toContain("执行失败");
    expect(html).not.toContain("验证失败：工具执行失败");
    expect(html).toContain('aria-expanded="false"');
  });
  it("opens a failed process and gives it an explicit failure count",()=>{
    const html=render(<MessageProcess process={{tools:[{toolCallId:failure.toolCallId,name:failure.name,args:failure.args,execution:failure}]}}/>);
    expect(html).toMatch(/<details[^>]*open=""/);
    expect(html).toContain("1 项失败");
  });
  it("keeps verification failure distinct when execution itself succeeded",()=>{
    const html=render(<ToolCallCard {...failure} status="success" result="工具返回成功" verificationReason="窗口状态未改变"/>);
    expect(html).toContain("验证失败：窗口状态未改变");
    expect(html).not.toContain("检查图像理解配置");
  });
});
