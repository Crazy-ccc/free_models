import { useEffect, useMemo, useState } from 'react';
import { message, Modal, Checkbox, Input, Empty, Tag, Button, Select } from 'antd';
import Toolbar from '../components/Toolbar';
import Toggle from '../components/Toggle';
import Drawer from '../components/Drawer';
import { invoke } from '@tauri-apps/api/core';
import type { Provider, ProviderCredential, TestCredentialResult } from '../types';
import './Providers.less';

function maskApiKey(key: string): string {
  if (!key) return '';
  if (key.length <= 7) return key;
  return `${key.slice(0, 3)}****${key.slice(-4)}`;
}

/* ───── 凭证管理子页面 ───── */

function CredentialsPage({
  providerId,
  providerName,
  serverUrl,
  models,
  onBack,
}: {
  providerId: number;
  providerName: string;
  serverUrl: string;
  models: any[];
  onBack: () => void;
}) {
  const [credentials, setCredentials] = useState<ProviderCredential[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingCred, setEditingCred] = useState<ProviderCredential | null>(null);
  const [form, setForm] = useState({
    name: '',
    api_key: '',
    account: '',
    password: '',
    priority: 0,
    is_active: true,
  });

  const [testOpen, setTestOpen] = useState(false);
  const [testCred, setTestCred] = useState<ProviderCredential | null>(null);
  const [testModelId, setTestModelId] = useState('');
  const [testLoading, setTestLoading] = useState(false);

  const load = () => {
    invoke<ProviderCredential[]>('fetch_provider_credentials', { serverUrl, providerId })
      .then(setCredentials)
      .catch(() => setCredentials([]));
  };

  useEffect(() => {
    load();
  }, [providerId]);

  const openCreate = () => {
    setEditingCred(null);
    setForm({ name: '', api_key: '', account: '', password: '', priority: 0, is_active: true });
    setDrawerOpen(true);
  };

  const openEdit = (cred: ProviderCredential) => {
    setEditingCred(cred);
    setForm({
      name: cred.name,
      api_key: cred.api_key,
      account: cred.account ?? '',
      password: cred.password ?? '',
      priority: cred.priority,
      is_active: cred.is_active,
    });
    setDrawerOpen(true);
  };

  const closeDrawer = () => {
    setDrawerOpen(false);
    setEditingCred(null);
  };

  const handleSave = async () => {
    try {
      const payload = {
        provider_id: providerId,
        name: form.name || undefined,
        api_key: form.api_key,
        account: form.account || null,
        password: form.password || null,
        priority: form.priority,
        is_active: form.is_active,
      };
      if (editingCred) {
        await invoke('update_provider_credential', { serverUrl, id: editingCred.id, data: payload });
      } else {
        await invoke('create_provider_credential', { serverUrl, data: payload });
      }
      closeDrawer();
      load();
    } catch (e) {
      console.error(e);
    }
  };

  const handleDelete = (cred: ProviderCredential) => {
    Modal.confirm({
      title: '确定删除？',
      content: `将删除凭证「${cred.name || cred.id}」`,
      okText: '确定',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: async () => {
        await invoke('delete_provider_credential', { serverUrl, id: cred.id });
        load();
      },
    });
  };

  const toggleActive = async (cred: ProviderCredential) => {
    const newActive = !cred.is_active;
    setCredentials(prev => prev.map(c => c.id === cred.id ? { ...c, is_active: newActive } : c));
    try {
      await invoke('update_provider_credential', {
        serverUrl,
        id: cred.id,
        data: { provider_id: cred.provider_id, is_active: newActive, account: cred.account || null, password: cred.password || null },
      });
    } catch (e) {
      setCredentials(prev => prev.map(c => c.id === cred.id ? { ...c, is_active: !newActive } : c));
    }
  };

  const runTest = async (cred: ProviderCredential, modelId: string) => {
    setTestLoading(true);
    try {
      const result = await invoke<TestCredentialResult>('test_provider_credential', {
        serverUrl,
        credentialId: cred.id,
        modelId,
        prompt: '你好',
      });
      if (result.success) {
        Modal.info({ title: '测试成功', content: `模型 ${result.model_id} 响应时间 ${result.response_time_ms}ms` });
      } else {
        Modal.error({ title: '测试失败', content: result.error || '未知错误' });
      }
    } catch (e: any) {
      Modal.error({ title: '测试失败', content: String(e) });
    } finally {
      setTestLoading(false);
      setTestOpen(false);
      setTestCred(null);
    }
  };

  const openTest = (cred: ProviderCredential) => {
    if (testLoading) return;
    if (models.length === 0) {
      Modal.error({ title: '无可用模型', content: '没有模型，无法测试' });
      return;
    }
    if (models.length === 1) {
      runTest(cred, models[0].id);
      return;
    }
    setTestCred(cred);
    setTestModelId(models[0].id);
    setTestOpen(true);
  };

  const confirmTest = () => {
    if (testLoading || !testCred) return;
    runTest(testCred, testModelId);
  };

  return (
    <div className="providers-page">
      <div className="toolbar">
        <div className="toolbar-title">
          <button className="back-arrow" onClick={onBack}>←</button>
          凭证管理 - {providerName}
        </div>
        <div className="toolbar-actions">
          <button className="ant-btn ant-btn-primary" onClick={openCreate}>+ 新增凭证</button>
        </div>
      </div>
      <div className="providers-table-wrap">
        <table className="providers-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>Key</th>
              <th>账号</th>
              <th>优先级</th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {credentials.length === 0 ? (
              <tr>
                <td colSpan={6} className="providers-empty">暂无凭证</td>
              </tr>
            ) : (
              credentials.map((c) => (
                <tr key={c.id}>
                  <td>{c.name || `#${c.id}`}</td>
                  <td className="mono">{maskApiKey(c.api_key)}</td>
                  <td>{c.account || '-'}</td>
                  <td>{c.priority}</td>
                  <td>
                    <div className="toggle-wrap">
                      <Toggle checked={c.is_active} onChange={() => toggleActive(c)} />
                    </div>
                  </td>
                  <td>
                    <button className="ant-btn ant-btn-sm" style={{ marginRight: 8 }} disabled={testLoading} onClick={() => openTest(c)}>测试</button>
                    <button className="ant-btn ant-btn-sm" style={{ marginRight: 8 }} onClick={() => openEdit(c)}>编辑</button>
                    <button className="ant-btn ant-btn-sm ant-btn-dangerous" onClick={() => handleDelete(c)}>删除</button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <Drawer
        open={drawerOpen}
        title={editingCred ? '编辑凭证' : '新增凭证'}
        onClose={closeDrawer}
        onSave={handleSave}
      >
        <div className="form-field">
          <label>名称（可选）</label>
          <input className="ant-input" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="给这组凭证起个名字" />
        </div>
        <div className="form-field">
          <label>API Key</label>
          <input className="ant-input" value={form.api_key} onChange={(e) => setForm({ ...form, api_key: e.target.value })} />
        </div>
        <div className="form-field">
          <label>账号（可选）</label>
          <input className="ant-input" value={form.account} onChange={(e) => setForm({ ...form, account: e.target.value })} />
        </div>
        <div className="form-field">
          <label>密码（可选）</label>
          <input className="ant-input" type="password" value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })} />
        </div>
        <div className="form-row">
          <div className="form-field">
            <label>优先级</label>
            <input type="number" className="ant-input" value={form.priority} onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })} />
          </div>
          <div className="form-field">
            <label>状态</label>
            <div className="toggle-wrap"><Toggle checked={form.is_active} onChange={(checked) => setForm({ ...form, is_active: checked })} /></div>
          </div>
        </div>
      </Drawer>

      {testOpen && (
        <div className="modal-overlay" onClick={() => !testLoading && setTestOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span>选择测试模型 — {testCred?.name || `#${testCred?.id}`}</span>
              <span className="modal-close" onClick={() => !testLoading && setTestOpen(false)}>&times;</span>
            </div>
            <div className="modal-list">
              {models.map((m: any) => (
                <label
                  key={m.id}
                  className={`test-model-row${testModelId === m.id ? ' selected' : ''}`}
                >
                  <input
                    type="radio"
                    name="test-model"
                    value={m.id}
                    checked={testModelId === m.id}
                    onChange={() => setTestModelId(m.id)}
                    disabled={testLoading}
                  />
                  <span className="test-model-name">{m.name}</span>
                  <span className="test-model-id">{m.id}</span>
                </label>
              ))}
            </div>
            <div className="modal-footer">
              <button className="ant-btn" onClick={() => setTestOpen(false)} disabled={testLoading}>取消</button>
              <button className="ant-btn ant-btn-primary" onClick={confirmTest} disabled={testLoading}>
                {testLoading ? '测试中...' : '确认测试'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Providers() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<any[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<Provider | null>(null);
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
  const [searchQuery, setSearchQuery] = useState('');

  const [importOpen, setImportOpen] = useState(false);
  const [importProvider, setImportProvider] = useState<Provider | null>(null);
  const [proxyModels, setProxyModels] = useState<{ id: string }[]>([]);
  const [importedModelIds, setImportedModelIds] = useState<Set<string>>(new Set());
  const [importModelDetails, setImportModelDetails] = useState<Record<string, { provider_model_id: string; model_id: string; protocols: string; context_length?: number }>>({});
  const [importSearch, setImportSearch] = useState('');
  const [importing, setImporting] = useState(false);

  const [createModelOpen, setCreateModelOpen] = useState(false);
  const [createModelName, setCreateModelName] = useState('');
  const [createModelContextLength, setCreateModelContextLength] = useState(256000);
  const [createModelLoading, setCreateModelLoading] = useState(false);

  const [view, setView] = useState<'list' | 'credentials'>('list');
  const [credProviderId, setCredProviderId] = useState(0);
  const [credProviderName, setCredProviderName] = useState('');

  const filteredProviders = useMemo(() => {
    if (!searchQuery) return providers;
    const q = searchQuery.toLowerCase();
    return providers.filter((p) =>
      p.name.toLowerCase().includes(q) || p.base_url.toLowerCase().includes(q)
    );
  }, [providers, searchQuery]);

  const filteredProxyModels = useMemo(() => {
    if (!importSearch) return proxyModels;
    const q = importSearch.toLowerCase();
    return proxyModels.filter((m) => m.id.toLowerCase().includes(q));
  }, [proxyModels, importSearch]);

  const selectableModels = useMemo(
    () => filteredProxyModels.filter((m) => !importedModelIds.has(m.id)),
    [filteredProxyModels, importedModelIds]
  );
  const selectedCount = Object.keys(importModelDetails).length;
  const allChecked = selectableModels.length > 0 && selectableModels.every((m) => m.id in importModelDetails);
  const indeterminate = !allChecked && selectableModels.some((m) => m.id in importModelDetails);
  const hasEmptyModelId = Object.values(importModelDetails).some((d) => !d.model_id);

  const load = () => {
    invoke<Provider[]>('fetch_providers', { serverUrl }).then(setProviders);
    invoke<any[]>('fetch_models', { serverUrl }).then(setModels);
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setBaseUrl('');
    setDrawerOpen(true);
  };

  const openEdit = (provider: Provider) => {
    setEditing(provider);
    setName(provider.name);
    setBaseUrl(provider.base_url);
    setDrawerOpen(true);
  };

  const closeDrawer = () => setDrawerOpen(false);

  const handleSave = () => {
    const payload: Partial<Provider> = {
      name,
      base_url: baseUrl,
    };
    const done = editing
      ? invoke<Provider>('update_provider', { serverUrl, id: editing.id, data: payload })
      : invoke<Provider>('create_provider', { serverUrl, data: payload });
    done.then(() => {
      load();
      setDrawerOpen(false);
    });
  };

  const handleDeleteRow = (provider: Provider) => {
    Modal.confirm({
      title: '确定删除？',
      content: `将删除供应商「${provider.name}」`,
      okText: '确定',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: () => {
        invoke('delete_provider', { serverUrl, id: provider.id }).then(() => {
          load();
        });
      },
    });
  };

  const openCredentialsPage = (provider: Provider) => {
    setCredProviderId(provider.id);
    setCredProviderName(provider.name);
    setView('credentials');
  };

  const openImport = (provider: Provider) => {
    setImportProvider(provider);
    setProxyModels([]);
    setImportModelDetails({});
    setImportSearch('');
    setImportedModelIds(new Set());
    Promise.all([
      invoke<any[]>('fetch_provider_models', { serverUrl, providerId: provider.id }),
      invoke<any[]>('fetch_provider_model_maps', { serverUrl, modelId: null, providerId: provider.id }),
    ])
      .then(([data, maps]) => {
        setProxyModels(data);
        setImportedModelIds(new Set(maps.map((m: any) => m.provider_model_id)));
        setImportOpen(true);
      })
      .catch(() => {
        message.error('该供应商不支持一键导入模型');
      });
  };

  const toggleSelect = (id: string) => {
    setImportModelDetails((prev) => {
      const next = { ...prev };
      if (id in next) {
        delete next[id];
      } else {
        next[id] = { provider_model_id: id, model_id: '', protocols: 'openai' };
      }
      return next;
    });
  };

  const confirmImport = async () => {
    if (!importProvider || Object.keys(importModelDetails).length === 0) return;
    setImporting(true);
    try {
      const models = Object.entries(importModelDetails).map(([_modelId, details]) => ({
        model_id: details.model_id,
        provider_model_id: details.provider_model_id,
        protocols: details.protocols,
        context_length: details.context_length,
      }));
      await invoke('import_provider_models', { serverUrl, providerId: importProvider.id, data: models });
      message.success(`成功导入 ${models.length} 个模型`);
      setImportOpen(false);
      load();
    } catch (e) {
      message.error('导入失败');
    } finally {
      setImporting(false);
    }
  };

  const handleSelectAllChange = (e: { target: { checked: boolean } }) => {
    setImportModelDetails((prev) => {
      const next = { ...prev };
      if (e.target.checked) {
        selectableModels.forEach((m) => {
          if (!(m.id in next)) next[m.id] = { provider_model_id: m.id, model_id: '', protocols: 'openai' };
        });
      } else {
        selectableModels.forEach((m) => {
          delete next[m.id];
        });
      }
      return next;
    });
  };

  const openCreateModel = () => {
    setCreateModelOpen(true);
    setCreateModelName('');
  };

  const confirmCreateModel = async () => {
    if (!createModelName.trim()) {
      message.warning('请输入模型名称');
      return;
    }
    setCreateModelLoading(true);
    try {
      const created = await invoke<any>('create_model', { serverUrl, data: { name: createModelName, context_length: createModelContextLength } });
      setCreateModelOpen(false);
      // New global model appears in Select options (models state), not in proxyModels.
      setModels((prev) => (prev.some((m) => m.name === created.name) ? prev : [...prev, created]));
      message.success('模型创建成功，请在下方列表中选择它作为映射目标');
    } catch (e) {
      message.error('创建失败');
    } finally {
      setCreateModelLoading(false);
    }
  };

  if (view === 'credentials') {
    return (
      <CredentialsPage
        providerId={credProviderId}
        providerName={credProviderName}
        serverUrl={serverUrl}
        models={models}
        onBack={() => setView('list')}
      />
    );
  }

  return (
    <div className="providers-page">
      <Toolbar title="供应商" showSearch={true} searchValue={searchQuery} onSearchChange={setSearchQuery}>
        <button className="ant-btn ant-btn-primary" onClick={openCreate}>+ 新增</button>
      </Toolbar>
      <div className="providers-table-wrap">
        <table className="providers-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>Base URL</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {filteredProviders.length === 0 ? (
              <tr>
                <td colSpan={3} className="providers-empty">暂无数据</td>
              </tr>
            ) : (
              filteredProviders.map((p) => (
                <tr key={p.id}>
                  <td className="clickable-name" onClick={() => openEdit(p)}>{p.name}</td>
                  <td>{p.base_url}</td>
                  <td>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openEdit(p); }}>编辑</button>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openCredentialsPage(p); }}>凭证</button>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openImport(p); }}>一键添加模型</button>
                    <button className="ant-btn ant-btn-dangerous" onClick={(e) => { e.stopPropagation(); handleDeleteRow(p); }}>删除</button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <Drawer
        open={drawerOpen}
        title={editing ? '编辑供应商' : '新增供应商'}
        onClose={closeDrawer}
        onSave={handleSave}
      >
        <div className="form-field">
          <label>名称</label>
          <input className="ant-input" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="form-field">
          <label>Base URL</label>
          <input className="ant-input" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </div>
      </Drawer>

      <Modal
        open={importOpen}
        onCancel={() => setImportOpen(false)}
        title={`导入模型 — ${importProvider?.name}`}
        width={520}
        footer={null}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 12 }}>
          <Button size="small" onClick={openCreateModel}>新建模型</Button>
          <Checkbox
            checked={allChecked}
            indeterminate={indeterminate}
            onChange={handleSelectAllChange}
          >全选</Checkbox>
        </div>
        <Input
          allowClear
          placeholder="搜索模型..."
          value={importSearch}
          onChange={(e) => setImportSearch(e.target.value)}
          style={{ marginBottom: 12 }}
        />
        <div style={{ maxHeight: 360, overflowY: 'auto' }}>
          {filteredProxyModels.length === 0 ? (
            <Empty description="暂无匹配模型" />
          ) : (
            filteredProxyModels.map((m) => {
              const imported = importedModelIds.has(m.id);
              const selected = m.id in importModelDetails;
              const details = importModelDetails[m.id];
              return (
                <div key={m.id} style={{ padding: '8px 0', borderBottom: '1px solid rgba(0,0,0,0.06)' }}>
                  <Checkbox
                    checked={selected}
                    disabled={imported}
                    onChange={() => !imported && toggleSelect(m.id)}
                  >
                    <span>{m.id}</span>
                    {imported && <Tag style={{ marginLeft: 8 }}>已导入</Tag>}
                  </Checkbox>
                  {selected && !imported && (
                    <div style={{ margin: '8px 0 4px 24px', display: 'flex', flexDirection: 'column', gap: 6 }}>
                      <div><label>映射到全局模型（必选）</label></div>
                      <Select
                        size="small"
                        showSearch
                        placeholder="请选择全局模型"
                        value={details.model_id || undefined}
                        onChange={(value: string) => setImportModelDetails((prev) => ({ ...prev, [m.id]: { ...prev[m.id], model_id: value } }))}
                        options={models.map((mm) => ({ label: mm.name, value: mm.name }))}
                        style={{ width: '100%' }}
                      />
                      <div><label>Protocols</label></div>
                      <Input size="small" value={details.protocols} onChange={(e) => setImportModelDetails((prev) => ({ ...prev, [m.id]: { ...prev[m.id], protocols: e.target.value } }))} />
                      <div><label>Context Length（可选）</label></div>
                      <Input type="number" size="small" value={details.context_length ?? ''} onChange={(e) => setImportModelDetails((prev) => ({ ...prev, [m.id]: { ...prev[m.id], context_length: e.target.value ? parseInt(e.target.value) : undefined } }))} placeholder="默认 256000" />
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 12 }}>
          <span>已选 {selectedCount} 项</span>
          <Button type="primary" onClick={confirmImport} disabled={importing || selectedCount === 0 || hasEmptyModelId}>
            {importing ? '导入中...' : '确认导入'}
          </Button>
        </div>
      </Modal>

      {createModelOpen && (
        <Modal
          open={createModelOpen}
          title="新建模型"
          onCancel={() => setCreateModelOpen(false)}
          onOk={confirmCreateModel}
          confirmLoading={createModelLoading}
          okText="创建"
          cancelText="取消"
        >
          <div style={{ marginBottom: 12 }}>
            <div><label>模型名称</label></div>
            <Input value={createModelName} onChange={(e) => setCreateModelName(e.target.value)} placeholder="如 gpt-4o" />
          </div>
          <div>
            <div><label>上下文长度</label></div>
            <Input type="number" value={createModelContextLength} onChange={(e) => setCreateModelContextLength(e.target.value ? parseInt(e.target.value) : 0)} />
          </div>
        </Modal>
      )}
    </div>
  );
}

export default Providers;
