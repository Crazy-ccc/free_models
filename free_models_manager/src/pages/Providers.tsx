import { useEffect, useMemo, useState } from 'react';
import { Alert, Modal } from 'antd';
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
  const [selectedProxyIds, setSelectedProxyIds] = useState<Set<string>>(new Set());
  const [importSearch, setImportSearch] = useState('');
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState('');

  const [credOpen, setCredOpen] = useState(false);
  const [credProvider, setCredProvider] = useState<Provider | null>(null);
  const [credentials, setCredentials] = useState<ProviderCredential[]>([]);
  const [credEditing, setCredEditing] = useState<ProviderCredential | null>(null);
  const [credFormOpen, setCredFormOpen] = useState(false);
  const [credForm, setCredForm] = useState({
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

  const filteredProviders = useMemo(() => {
    if (!searchQuery) return providers;
    const q = searchQuery.toLowerCase();
    return providers.filter((p) =>
      p.name.toLowerCase().includes(q) || p.base_url.toLowerCase().includes(q)
    );
  }, [providers, searchQuery]);

  const existingModelIds = useMemo(() => {
    return new Set(models.map((m: any) => m.model_id));
  }, [models]);

  const filteredProxyModels = useMemo(() => {
    if (!importSearch) return proxyModels;
    const q = importSearch.toLowerCase();
    return proxyModels.filter((m) => m.id.toLowerCase().includes(q));
  }, [proxyModels, importSearch]);

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

  const openCredentials = (provider: Provider) => {
    setCredProvider(provider);
    setCredOpen(true);
    setCredFormOpen(false);
    invoke<ProviderCredential[]>('fetch_provider_credentials', { serverUrl, providerId: provider.id })
      .then(setCredentials)
      .catch(() => setCredentials([]));
  };

  const closeCredentials = () => {
    setCredOpen(false);
    setCredProvider(null);
    setCredentials([]);
    setCredFormOpen(false);
  };

  const openCredCreate = () => {
    setCredEditing(null);
    setCredForm({ name: '', api_key: '', account: '', password: '', priority: 0, is_active: true });
    setCredFormOpen(true);
  };

  const openCredEdit = (cred: ProviderCredential) => {
    setCredEditing(cred);
    setCredForm({
      name: cred.name,
      api_key: cred.api_key,
      account: cred.account ?? '',
      password: cred.password ?? '',
      priority: cred.priority,
      is_active: cred.is_active,
    });
    setCredFormOpen(true);
  };

  const handleCredSave = async () => {
    if (!credProvider) return;
    const payload = {
      provider_id: credProvider.id,
      name: credForm.name || undefined,
      api_key: credForm.api_key,
      account: credForm.account || null,
      password: credForm.password || null,
      priority: credForm.priority,
      is_active: credForm.is_active,
    };
    try {
      if (credEditing) {
        await invoke('update_provider_credential', { serverUrl, id: credEditing.id, data: payload });
      } else {
        await invoke('create_provider_credential', { serverUrl, data: payload });
      }
      const list = await invoke<ProviderCredential[]>('fetch_provider_credentials', { serverUrl, providerId: credProvider.id });
      setCredentials(list);
      setCredFormOpen(false);
    } catch (e) {
      console.error(e);
    }
  };

  const handleCredDelete = (cred: ProviderCredential) => {
    Modal.confirm({
      title: '确定删除？',
      content: `将删除凭证「${cred.name || cred.id}」`,
      okText: '确定',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: async () => {
        await invoke('delete_provider_credential', { serverUrl, id: cred.id });
        if (credProvider) {
          const list = await invoke<ProviderCredential[]>('fetch_provider_credentials', { serverUrl, providerId: credProvider.id });
          setCredentials(list);
        }
      },
    });
  };

  const toggleCredActive = async (cred: ProviderCredential) => {
    const newActive = !cred.is_active;
    setCredentials(prev => prev.map(c => c.id === cred.id ? { ...c, is_active: newActive } : c));
    try {
      await invoke('update_provider_credential', {
        serverUrl,
        id: cred.id,
        data: { ...cred, is_active: newActive, account: cred.account || null, password: cred.password || null },
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
        credential_id: cred.id,
        model_id: modelId,
        prompt: undefined,
      });
      if (result.success) {
        Modal.info({
          title: '测试成功',
          content: `模型 ${result.model_id} 响应时间 ${result.response_time_ms}ms`,
        });
      } else {
        Modal.error({
          title: '测试失败',
          content: result.error || '未知错误',
        });
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
    if (!credProvider) return;
    const providerModels = models.filter((m: any) => m.provider_id === credProvider.id);
    if (providerModels.length === 0) {
      Modal.error({ title: '无可用模型', content: '该供应商下没有模型，无法测试' });
      return;
    }
    if (providerModels.length === 1) {
      runTest(cred, providerModels[0].model_id);
      return;
    }
    setTestCred(cred);
    setTestModelId(providerModels[0].model_id);
    setTestOpen(true);
  };

  const confirmTest = () => {
    if (!testCred) return;
    runTest(testCred, testModelId);
  };

  const openImport = (provider: Provider) => {
    setImportProvider(provider);
    setProxyModels([]);
    setSelectedProxyIds(new Set());
    setImportSearch('');
    invoke<any[]>('fetch_provider_models_by_url', { baseUrl: provider.base_url })
      .then((data) => {
        setProxyModels(data);
        setImportOpen(true);
      })
      .catch(() => {
        setImportError('该供应商不支持一键导入模型');
      });
  };

  const toggleSelect = (id: string) => {
    setSelectedProxyIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const confirmImport = async () => {
    if (!importProvider || selectedProxyIds.size === 0) return;
    setImporting(true);
    for (const modelId of selectedProxyIds) {
      try {
        await invoke('create_model', {
          serverUrl,
          data: {
            name: modelId,
            model_id: modelId,
            provider_id: importProvider.id,
            protocols: 'openai',
            priority: 0,
            timeout: 30,
            status: 'available',
          },
        });
      } catch (e) {
        console.error(e);
      }
    }
    setImporting(false);
    setImportOpen(false);
    load();
  };

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
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openCredentials(p); }}>凭证</button>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openImport(p); }}>一键添加模型</button>
                    <button className="ant-btn ant-btn-dangerous" onClick={(e) => { e.stopPropagation(); handleDeleteRow(p); }}>删除</button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {importError && (
        <div style={{ marginBottom: 16 }}>
          <Alert type="error" message={importError} closable showIcon
            onClose={() => setImportError('')} />
        </div>
      )}

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

      {importOpen && (
        <div className="modal-overlay" onClick={() => setImportOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span>导入模型 — {importProvider?.name}</span>
              <span className="modal-close" onClick={() => setImportOpen(false)}>&times;</span>
            </div>
            <div className="modal-search">
              <input
                className="ant-input"
                placeholder="搜索模型..."
                value={importSearch}
                onChange={(e) => setImportSearch(e.target.value)}
              />
            </div>
            <div className="modal-list">
              {filteredProxyModels.length === 0 ? (
                <div className="modal-empty">暂无匹配模型</div>
              ) : (
                filteredProxyModels.map((m) => {
                  const imported = existingModelIds.has(m.id);
                  return (
                    <label
                      key={m.id}
                      className={`modal-row${imported ? ' imported' : ''}`}
                    >
                      <input
                        type="checkbox"
                        checked={selectedProxyIds.has(m.id)}
                        disabled={imported}
                        onChange={() => !imported && toggleSelect(m.id)}
                      />
                      <span>{m.id}</span>
                      {imported && <span className="modal-tag">已导入</span>}
                    </label>
                  );
                })
              )}
            </div>
            <div className="modal-footer">
              <span className="modal-count">
                已选 {selectedProxyIds.size} 项
              </span>
              <button
                className="ant-btn ant-btn-primary"
                onClick={confirmImport}
                disabled={importing || selectedProxyIds.size === 0}
              >
                {importing ? '导入中...' : '确认导入'}
              </button>
            </div>
          </div>
        </div>
      )}

      {credOpen && (
        <div className="modal-overlay" onClick={closeCredentials}>
          <div className="modal-content cred-modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span>凭证管理 — {credProvider?.name}</span>
              <span className="modal-close" onClick={closeCredentials}>&times;</span>
            </div>
            {!credFormOpen ? (
              <>
                <div className="modal-search" style={{ justifyContent: 'space-between', display: 'flex' }}>
                  <span style={{ color: '#666', fontSize: 14 }}>共 {credentials.length} 条凭证</span>
                  <button className="ant-btn ant-btn-primary" onClick={openCredCreate}>+ 新增凭证</button>
                </div>
                <div className="modal-list">
                  {credentials.length === 0 ? (
                    <div className="modal-empty">暂无凭证</div>
                  ) : (
                    credentials.map((c) => (
                      <div key={c.id} className="cred-row">
                        <div className="cred-info">
                          <span className="cred-name">{c.name || `#${c.id}`}</span>
                          <span className="cred-key mono">{maskApiKey(c.api_key)}</span>
                          {c.account && <span className="cred-account">{c.account}</span>}
                          <span className="cred-priority">优先级: {c.priority}</span>
                        </div>
                        <div className="cred-actions">
                          <Toggle checked={c.is_active} onChange={() => toggleCredActive(c)} />
                          <button className="ant-btn ant-btn-sm" onClick={() => openTest(c)}>测试</button>
                          <button className="ant-btn ant-btn-sm" onClick={() => openCredEdit(c)}>编辑</button>
                          <button className="ant-btn ant-btn-sm ant-btn-dangerous" onClick={() => handleCredDelete(c)}>删除</button>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </>
            ) : (
              <div className="cred-form">
                <div className="form-field">
                  <label>名称（可选）</label>
                  <input className="ant-input" value={credForm.name} onChange={(e) => setCredForm({ ...credForm, name: e.target.value })} placeholder="给这组凭证起个名字" />
                </div>
                <div className="form-field">
                  <label>API Key</label>
                  <input className="ant-input" value={credForm.api_key} onChange={(e) => setCredForm({ ...credForm, api_key: e.target.value })} />
                </div>
                <div className="form-field">
                  <label>账号（可选）</label>
                  <input className="ant-input" value={credForm.account} onChange={(e) => setCredForm({ ...credForm, account: e.target.value })} />
                </div>
                <div className="form-field">
                  <label>密码（可选）</label>
                  <input className="ant-input" type="password" value={credForm.password} onChange={(e) => setCredForm({ ...credForm, password: e.target.value })} />
                </div>
                <div className="form-row">
                  <div className="form-field">
                    <label>优先级</label>
                    <input type="number" className="ant-input" value={credForm.priority} onChange={(e) => setCredForm({ ...credForm, priority: Number(e.target.value) })} />
                  </div>
                  <div className="form-field">
                    <label>状态</label>
                    <div className="toggle-wrap"><Toggle checked={credForm.is_active} onChange={(checked) => setCredForm({ ...credForm, is_active: checked })} /></div>
                  </div>
                </div>
                <div className="cred-form-footer">
                  <button className="ant-btn" onClick={() => setCredFormOpen(false)}>取消</button>
                  <button className="ant-btn ant-btn-primary" onClick={handleCredSave}>保存</button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {testOpen && (
        <div className="modal-overlay" onClick={() => !testLoading && setTestOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span>选择测试模型 — {testCred?.name || `#${testCred?.id}`}</span>
              <span className="modal-close" onClick={() => !testLoading && setTestOpen(false)}>&times;</span>
            </div>
            <div className="modal-list">
              {models
                .filter((m: any) => m.provider_id === credProvider?.id)
                .map((m: any) => (
                  <label key={m.id} className="modal-row">
                    <input
                      type="radio"
                      name="test-model"
                      value={m.model_id}
                      checked={testModelId === m.model_id}
                      onChange={() => setTestModelId(m.model_id)}
                      disabled={testLoading}
                    />
                    <span>{m.model_id}</span>
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

export default Providers;
