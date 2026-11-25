import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import {
  categoriesFromPolicy,
  DEFAULT_SYNC_CATEGORIES,
  policyFromCategories,
  SyncPrivacyPanel,
} from './SyncPrivacyPanel';

describe('SyncPrivacyPanel', () => {
  it('renders all default synchronization categories', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData={false} initialCategories={DEFAULT_SYNC_CATEGORIES} />);
    expect(html).toContain('同步范围');

    for (const cat of DEFAULT_SYNC_CATEGORIES) {
      expect(html).toContain(`aria-label="允许同步${cat.name}"`);
    }
  });

  it('explains category permissions and the existing data boundary', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData={false} />);
    expect(html).toContain('启用跨设备同步后才会按此范围同步');
    expect(html).toContain('关闭分类会暂停上传和恢复，已有本地和云端数据会保留');
    expect(html).toContain('请勿将密钥等秘密粘贴到对话正文');
    expect(html).not.toContain('同步开启');
  });

  it('renders explicit prohibited notices for sensitive fields', () => {
    const html = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData={false} initialCategories={DEFAULT_SYNC_CATEGORIES} />);
    expect(html).toContain('系统消息、调试提示与工具原始参数');
    expect(html).toContain('密钥、令牌、环境变量与展开后的系统提示词');
    expect(html).toContain('完整输入参数与执行结果');
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
    const html = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData={false} initialCategories={custom} />);
    expect(html).toContain('自定义数据');
    expect(html).toContain('不允许同步');
    expect(html).toContain('aria-checked="false"');
  });

  it('shows a single storage summary for accounts and no remote refresh action for guests', () => {
    const signedIn = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData />);
    expect(signedIn.match(/云端存储/g)).toHaveLength(1);
    expect(signedIn).toContain('刷新概览');
    const guest = renderToStaticMarkup(<SyncPrivacyPanel canReadCloudData={false} />);
    expect(guest).not.toContain('刷新概览');
  });

  it('maps persisted profile policy to the category controls and back', () => {
    const policy = {
      sync_conversations: false,
      sync_messages: false,
      sync_memories: true,
      sync_agent_profiles: true,
      sync_workflow_metadata: false,
      sync_artifact_metadata: true,
      sync_task_graphs: false,
      sync_canvas_views: false,
      sync_settings: true,
      sync_capability_usage: false,
    };
    const categories = categoriesFromPolicy(policy);
    expect(categories.find((item) => item.key === 'conversations')?.enabled).toBe(false);
    expect(categories.find((item) => item.key === 'memories')?.enabled).toBe(true);
    expect(policyFromCategories(categories)).toEqual(policy);
  });
});
