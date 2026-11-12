import { App as AntdApp } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PoliciesSection } from '../memory-engine/app/sections/PoliciesSection';
import { api } from './api';
import { ConfigEditor } from './pages';
import type { ConfigDefinition } from './types';

vi.mock('./api', () => ({ api: {
  catalog: vi.fn(), effective: vi.fn(), draft: vi.fn(), saveDraft: vi.fn(),
  validateDraft: vi.fn(), publishDraft: vi.fn(), createCustomDefinition: vi.fn(),
} }));

const published = { 'test-common': {
  context_window_tokens: 9137, output_reserve_tokens: 1137,
  protocol_overhead_estimator_id: 'responses-json-v1',
  source_urls: ['https://specification.example/common'],
} };
const definition: ConfigDefinition = {
  id: 'common', key: 'shared.ai.model_capabilities', display_name: '已发布模型请求预算',
  description: '正式发布的共同预算', category: 'AI / Model Capabilities', scope: 'shared',
  value_type: 'json', default_value: {}, nullable: false, enum_options: [],
  sensitivity: 'public', reload_mode: 'restart_required', criticality: 'normal',
  env_aliases: [], owner_team: 'platform', ui_order: 1, deprecated: false,
};

function mount(path = '/config/definitions?area=platform-shared&search=shared.ai.') {
  return render(
    <AntdApp><QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <MemoryRouter initialEntries={[path]}><Routes>
        <Route path="/config/definitions" element={<ConfigEditor environment="production" />} />
        <Route path="/memory/policies" element={<PoliciesSection
          policies={[]} loading={false} selectedKey="summary" onSelect={vi.fn()} onReload={vi.fn()}
        />} />
      </Routes></MemoryRouter>
    </QueryClientProvider></AntdApp>,
  );
}

describe('configuration JSON editing and policy navigation', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    vi.mocked(api.catalog).mockResolvedValue([
      definition,
      { ...definition, id: 'other', key: 'user_service.other', display_name: '其他服务参数',
        category: 'User Service / Runtime', service_name: 'user-service', value_type: 'string', default_value: '' },
    ]);
    vi.mocked(api.effective).mockResolvedValue({ environment: 'production', revision: 14,
      values: { [definition.key]: published } });
    vi.mocked(api.draft).mockResolvedValue({ environment: 'production', active_revision: 14 });
  });

  it('opens the published shared JSON from the policy page rather than showing an empty string', async () => {
    localStorage.setItem('chatos.configuration-center.config-area', 'user-service');
    mount('/memory/policies');
    fireEvent.click(screen.getByRole('link', { name: '查看并修改配置中心的统一容量' }));
    const input = await screen.findByRole('textbox', { name: definition.display_name });
    expect(JSON.parse((input as HTMLTextAreaElement).value)).toEqual(published);
    expect(screen.queryByText('其他服务参数')).toBeNull();
    expect((screen.getByRole('button', { name: /保\s*存\s*草\s*稿/ }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('blocks saving and publishing invalid JSON, then saves corrected JSON as an object', async () => {
    vi.mocked(api.saveDraft).mockResolvedValue({} as never);
    mount();
    const input = await screen.findByRole('textbox', { name: definition.display_name });
    const save = screen.getByRole('button', { name: /保\s*存\s*草\s*稿/ }) as HTMLButtonElement;
    const publish = screen.getByRole('button', { name: /发\s*布/ }) as HTMLButtonElement;
    const changed = { ...published, 'test-common': { ...published['test-common'], context_window_tokens: 17137 } };
    fireEvent.change(input, { target: { value: JSON.stringify(changed) } });
    expect(publish.disabled).toBe(false);
    fireEvent.change(input, { target: { value: '{"test-common":' } });
    expect(save.disabled).toBe(true);
    expect(publish.disabled).toBe(true);
    expect(input.getAttribute('aria-invalid')).toBe('true');
    fireEvent.click(save);
    expect(api.saveDraft).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: JSON.stringify(changed) } });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => expect(api.saveDraft).toHaveBeenCalledWith('production', { [definition.key]: changed }));
  });

  it('retains an unfinished JSON edit and its validation when switching categories', async () => {
    mount();
    const input = await screen.findByRole('textbox', { name: definition.display_name });
    fireEvent.change(input, { target: { value: '{"unfinished":' } });
    fireEvent.click(screen.getByRole('tab', { name: /用户服务/ }));
    await screen.findByText('其他服务参数');
    fireEvent.click(screen.getByRole('tab', { name: /平台与共享/ }));
    const restored = await screen.findByRole('textbox', { name: definition.display_name });
    expect((restored as HTMLTextAreaElement).value).toBe('{"unfinished":');
    expect((screen.getByRole('button', { name: /保\s*存\s*草\s*稿/ }) as HTMLButtonElement).disabled).toBe(true);
  });
});
