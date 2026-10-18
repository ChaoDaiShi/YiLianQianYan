import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import FormField from "./FormField";
import Select from "./Select";
import SearchField from "./SearchField";
import Tabs from "./Tabs";
import FilterBar from "./FilterBar";
import ListRow from "./ListRow";
import Inspector from "./Inspector";
import ProgressBar from "./ProgressBar";
import StatTile from "./StatTile";
import KeyValueRow from "./KeyValueRow";
import ConfirmDialog from "./ConfirmDialog";
import CodeBlock from "./CodeBlock";
import Section from "./Section";
import { RiskBadge, StatusBadge } from "./Badge";
import Button from "./Button";
import EmptyState from "./EmptyState";
import ErrorState from "./ErrorState";
import Skeleton from "./Skeleton";

describe("W2 Component Library Accessibility and Structure Contract", () => {
  describe("FormField", () => {
    it("renders label, associates htmlFor with input id, and links error with aria-describedby", () => {
      const html = renderToStaticMarkup(
        <FormField label="用户名" error="用户名不能为空">
          {({ id, "aria-describedby": describedBy, "aria-invalid": invalid }) => (
            <input id={id} aria-describedby={describedBy} aria-invalid={invalid} />
          )}
        </FormField>
      );
      expect(html).toContain('for="');
      expect(html).toContain('aria-invalid="true"');
      expect(html).toContain('role="alert"');
      expect(html).toContain("用户名不能为空");
    });
  });

  describe("Select", () => {
    it("renders styled select with options and aria descriptors", () => {
      const options = [
        { value: "a", label: "选项 A" },
        { value: "b", label: "选项 B" },
      ];
      const html = renderToStaticMarkup(
        <Select label="选择模型" options={options} defaultValue="a" />
      );
      expect(html).toContain("<select");
      expect(html).toContain("选项 A");
      expect(html).toContain("选项 B");
      expect(html).toContain('class="ui-select');
    });
  });

  describe("SearchField", () => {
    it("renders search input with aria-label and clear button when value is present", () => {
      const withValue = renderToStaticMarkup(
        <SearchField value="test query" onChange={() => {}} />
      );
      expect(withValue).toContain('type="search"');
      expect(withValue).toContain('aria-label="清空搜索"');

      const empty = renderToStaticMarkup(
        <SearchField value="" onChange={() => {}} autoFocusOnSlash shortcutHint="/" />
      );
      expect(empty).toContain("/");
      expect(empty).not.toContain('aria-label="清空搜索"');
    });
  });

  describe("Tabs", () => {
    it("renders tablist and tabs with correct aria-selected attributes", () => {
      const tabs = [
        { id: "tab1", label: "标签一" },
        { id: "tab2", label: "标签二" },
      ];
      const html = renderToStaticMarkup(
        <Tabs tabs={tabs} activeId="tab1" onChange={() => {}} variant="underline" />
      );
      expect(html).toContain('role="tablist"');
      expect(html).toContain('role="tab"');
      expect(html).toContain('aria-selected="true"');
      expect(html).toContain('aria-selected="false"');
    });

    it("renders segment variant with pill styling", () => {
      const tabs = [
        { id: "grid", label: "网格" },
        { id: "list", label: "列表" },
      ];
      const html = renderToStaticMarkup(
        <Tabs tabs={tabs} activeId="grid" onChange={() => {}} variant="segment" />
      );
      expect(html).toContain('role="tablist"');
      expect(html).toContain("网格");
    });

    it("omits aria-controls when the panel is not mounted (panelId false)", () => {
      const html = renderToStaticMarkup(
        <Tabs
          tabs={[
            { id: "login", label: "登录", panelId: false },
            { id: "register", label: "注册", panelId: false },
          ]}
          activeId="login"
          onChange={() => {}}
          variant="segment"
        />
      );
      expect(html).not.toContain("aria-controls");
    });

    it("keeps aria-controls when the caller renders a matching panel", () => {
      const html = renderToStaticMarkup(
        <Tabs
          tabs={[{ id: "runtime", label: "运行", panelId: "panel-runtime" }]}
          activeId="runtime"
          onChange={() => {}}
          variant="underline"
        />
      );
      expect(html).toContain('aria-controls="panel-runtime"');
    });
  });

  describe("FilterBar", () => {
    it("renders search input, filter slot, item count, and refresh button", () => {
      const html = renderToStaticMarkup(
        <FilterBar
          search={{ value: "", onChange: () => {}, placeholder: "筛选..." }}
          count={{ total: 42, filtered: 10, label: "任务" }}
          onRefresh={() => {}}
        />
      );
      expect(html).toContain("显示 10 / 42 任务");
      expect(html).toContain('aria-label="刷新"');
    });
  });

  describe("ListRow", () => {
    it("renders list item with option role and selection state", () => {
      const html = renderToStaticMarkup(
        <ListRow
          title="任务 1"
          meta="工作空间 A · 刚刚"
          statusBar="running"
          selected={true}
        />
      );
      expect(html).toContain('role="option"');
      expect(html).toContain('aria-selected="true"');
      expect(html).toContain("任务 1");
      expect(html).toContain("工作空间 A · 刚刚");
    });
  });

  describe("Inspector", () => {
    it("renders header, title, badge, and body", () => {
      const html = renderToStaticMarkup(
        <Inspector
          title="任务详情"
          badge={<span className="badge">运行中</span>}
          description="任务 ID: 12345"
          onClose={() => {}}
        >
          <div className="content">详情内容</div>
        </Inspector>
      );
      expect(html).toContain("任务详情");
      expect(html).toContain("运行中");
      expect(html).toContain("任务 ID: 12345");
      expect(html).toContain('aria-label="关闭详情面板"');
      expect(html).toContain("详情内容");
    });

    it("renders emptyState when title is absent", () => {
      const html = renderToStaticMarkup(
        <Inspector emptyState={<p>请选择一个项目查看详情</p>} />
      );
      expect(html).toContain("请选择一个项目查看详情");
    });
  });

  describe("ProgressBar", () => {
    it("renders bar variant with progressbar role and aria values", () => {
      const html = renderToStaticMarkup(
        <ProgressBar value={65} variant="bar" showValue />
      );
      expect(html).toContain('role="progressbar"');
      expect(html).toContain('aria-valuenow="65"');
      expect(html).toContain("65%");
    });

    it("renders ring variant with SVG circle", () => {
      const html = renderToStaticMarkup(
        <ProgressBar value={80} variant="ring" showValue />
      );
      expect(html).toContain('role="progressbar"');
      expect(html).toContain("<svg");
      expect(html).toContain("80%");
    });
  });

  describe("StatTile", () => {
    it("renders label, formatted metric, unit, and progress", () => {
      const html = renderToStaticMarkup(
        <StatTile
          label="内存使用率"
          value={72}
          unit="%"
          progress={72}
          description="已使用 11.5 GB / 16 GB"
        />
      );
      expect(html).toContain("内存使用率");
      expect(html).toContain("72");
      expect(html).toContain("%");
      expect(html).toContain("已使用 11.5 GB / 16 GB");
    });
  });

  describe("KeyValueRow", () => {
    it("renders label and value with copy button", () => {
      const html = renderToStaticMarkup(
        <KeyValueRow label="会话 ID" value="sess_9823482" mono copyable />
      );
      expect(html).toContain("会话 ID");
      expect(html).toContain("sess_9823482");
      expect(html).toContain('aria-label="复制"');
    });
  });

  describe("ConfirmDialog", () => {
    it("renders modal with danger confirmation and warning icon", () => {
      const html = renderToStaticMarkup(
        <ConfirmDialog
          open={true}
          title="删除工作空间"
          description="此操作不可撤销，空间内的所有配置将被永久删除。"
          confirmLabel="确认删除"
          tone="danger"
          onConfirm={() => {}}
          onCancel={() => {}}
        />
      );
      expect(html).toContain("删除工作空间");
      expect(html).toContain("此操作不可撤销");
      expect(html).toContain("确认删除");
    });
  });

  describe("CodeBlock", () => {
    it("renders language header, copy button, and code content", () => {
      const html = renderToStaticMarkup(
        <CodeBlock code="console.log('hello world');" language="javascript" />
      );
      expect(html).toContain("JAVASCRIPT");
      expect(html).toContain('aria-label="复制代码"');
      expect(html).toContain("console.log(&#x27;hello world&#x27;);");
    });
  });

  describe("Section", () => {
    it("renders section with title, description, and expand toggle if collapsible", () => {
      const html = renderToStaticMarkup(
        <Section title="系统配置" description="模型与连接偏好" collapsible>
          <p>表单内容</p>
        </Section>
      );
      expect(html).toContain("<section");
      expect(html).toContain("系统配置");
      expect(html).toContain("模型与连接偏好");
      expect(html).toContain('aria-expanded="true"');
      expect(html).toContain("表单内容");
    });
  });

  describe("RiskBadge and StatusBadge", () => {
    it("renders four risk levels with correct semantic labels", () => {
      expect(renderToStaticMarkup(<RiskBadge risk="low" />)).toContain("低风险");
      expect(renderToStaticMarkup(<RiskBadge risk="medium" />)).toContain("中风险");
      expect(renderToStaticMarkup(<RiskBadge risk="high" />)).toContain("高风险");
      expect(renderToStaticMarkup(<RiskBadge risk="critical" />)).toContain("极高风险");
    });

    it("renders status badge with label and tone", () => {
      const html = renderToStaticMarkup(
        <StatusBadge label="运行中" tone="running" dot="pulse" />
      );
      expect(html).toContain("运行中");
      expect(html).toContain("animate-pulse");
    });
  });

  describe("Button", () => {
    it("defaults to type='button' and does not lose accessibility during loading", () => {
      const html = renderToStaticMarkup(
        <Button loading loadingLabel="保存中...">
          保存
        </Button>
      );
      expect(html).toContain('type="button"');
      expect(html).toContain('aria-busy="true"');
      expect(html).toContain('aria-disabled="true"');
      expect(html).toContain("保存中...");
    });
  });

  describe("EmptyState & ErrorState & Skeleton", () => {
    it("renders EmptyState without hardcoded hexes", () => {
      const html = renderToStaticMarkup(
        <EmptyState title="没有任务" description="点击下方按钮创建第一个任务" />
      );
      expect(html).toContain("没有任务");
      expect(html).not.toContain("#ea91b9");
      expect(html).not.toContain("animate-ping");
    });

    it("renders ErrorState with role alert", () => {
      const html = renderToStaticMarkup(
        <ErrorState title="加载失败" description="网络连接超时" details="ETIMEDOUT 127.0.0.1:8000" />
      );
      expect(html).toContain('role="alert"');
      expect(html).toContain("加载失败");
      expect(html).toContain("展开详细信息");
    });

    it("renders Skeleton with aria-busy", () => {
      const html = renderToStaticMarkup(<Skeleton shape="card" count={3} />);
      expect(html).toContain('aria-busy="true"');
    });
  });
});
