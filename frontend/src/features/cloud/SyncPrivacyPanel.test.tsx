import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DEFAULT_SYNC_CATEGORIES, SyncPrivacyPanel } from './SyncPrivacyPanel';

describe('SyncPrivacyPanel', () => {
  it('renders all default synchronization categories', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel />);
    expect(html).toContain('云同步隐私与数据保护');
    expect(html).toContain('数据最小化与自由文本边界声明');

    for (const cat of DEFAULT_SYNC_CATEGORIES) {
      expect(html).toContain(cat.name);
    }
  });

  it('renders fail-closed security guarantee and capability usage notices', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel />);
    expect(html).toContain('安全网关与授权解析 (Fail-Closed)');
    expect(html).toContain('插件使用闭环 (Capability Usage)');
    expect(html).toContain('授权记录损坏即阻断 (Fail-Closed)');
  });

  it('renders explicit prohibited notices for sensitive fields', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel />);
    expect(html).toContain('内部 system 消息、开发调试提示与工具原始执行参数永不同步');
    expect(html).toContain('API 密钥、Token、环境变量与展开后的系统提示词禁止同步');
    expect(html).toContain('禁止同步完整输入参数与执行结果，保护数据隐私');
  });

  it('displays correct initial toggle states for custom categories', () => {
    const custom = [
      {
        key: 'custom_data',
        name: '自定义数据',
        description: '仅用于测试的分类',
        enabled: false,
        prohibitedNotice: '测试排除项',
      },
    ];
    const html = renderToStaticMarkup(<SyncPrivacyPanel initialCategories={custom} />);
    expect(html).toContain('自定义数据');
    expect(html).toContain('仅保留本地');
    expect(html).toContain('aria-checked="false"');
  });
});
