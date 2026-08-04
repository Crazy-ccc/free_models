import { useEffect, useMemo, useState } from 'react';
import Toolbar from '../components/Toolbar';
import Drawer from '../components/Drawer';
import Toggle from '../components/Toggle';
import {
  DoodleButton,
  DoodleTag,
  DoodleCheckbox,
  DoodleModal,
  type DoodleTagColor,
} from '../components/doodle';
import { invoke } from '@tauri-apps/api/core';
import type { Model, Provider, ProviderModelMap } from '../types';
import './Models.less';

interface FormState {
  name: string;
  priority: number;
  timeout: number;
  context_length: number;
  is_active: boolean;
}

const EMPTY_FORM: FormState = {
  name: '',
  priority: 0,
  timeout: 30,
  context_length: 256000,
  is_active: true,
};

/* ───── 供应商映射子页面 ───── */

const PROTOCOL_OPTIONS = ['openai', 'anthropic', 'responses'] as const;
const STATUS_OPTIONS = [
  { value: 'available', label: '可用' },
  { value: 'unavailable', label: '不可用' },
  { value: 'deprecated', label: '废弃' },
];
const STATUS_COLOR: Record<string, DoodleTagColor> = {
  available: 'available',
  unavailable: 'unavailable',
  deprecated: 'deprecated',
};

interface MappingFormState {
  provider_id: number;
  provider_model_id: string;
  protocols: string;
  priority: number;
  status: string;
  context_length: number;
  timeout: number;
  is_active: boolean;
}

interface ModelMappingsPageProps {
  modelId: number;
  modelName: string;
  serverUrl: string;
  providers: Provider[];
  onBack: () => void;
}

function ModelMappingsPage({ modelId, modelName, serverUrl, providers, onBack }: ModelMappingsPageProps) {
  const [maps, setMaps] = useState<ProviderModelMap[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerTitle, setDrawerTitle] = useState('编辑供应商映射');
  const [editingItem, setEditingItem] = useState<ProviderModelMap | null>(null);
  const [form, setForm] = useState<MappingFormState>({
    provider_id: 0,
    provider_model_id: '',
    protocols: '',
    priority: 0,
    status: 'available',
    context_length: 256000,
    timeout: 30,
    is_active: true,
  });

  const loadMaps = () => {
    invoke<ProviderModelMap[]>('fetch_provider_model_maps', { serverUrl, modelId }).then(setMaps);
  };

  useEffect(() => {
    loadMaps();
  }, [modelId]);

  const openEdit = (item: ProviderModelMap) => {
    setEditingItem(item);
    setForm({
      provider_id: item.provider_id,
      provider_model_id: item.provider_model_id,
      protocols: item.protocols,
      priority: item.priority,
      status: item.status,
      context_length: item.context_length ?? 256000,
      timeout: item.timeout ?? 30,
      is_active: item.is_active,
    });
    setDrawerTitle('编辑供应商映射');
    setDrawerOpen(true);
  };

  const openCreate = () => {
    setEditingItem(null);
    setForm({
      provider_id: providers[0]?.id ?? 0,
      provider_model_id: '',
      protocols: 'openai',
      priority: 0,
      status: 'available',
      context_length: 256000,
      timeout: 30,
      is_active: true,
    });
    setDrawerTitle('新建供应商映射');
    setDrawerOpen(true);
  };

  const closeDrawer = () => {
    setDrawerOpen(false);
    setEditingItem(null);
  };

  const handleSave = async () => {
    try {
      if (editingItem) {
        await invoke<ProviderModelMap>('update_provider_model_map', {
          serverUrl,
          id: editingItem.id,
          data: {
            provider_id: form.provider_id,
            provider_model_id: form.provider_model_id,
            protocols: form.protocols,
            priority: form.priority,
            status: form.status,
            context_length: form.context_length,
            timeout: form.timeout,
            is_active: form.is_active,
          },
        });
      } else {
        await invoke<ProviderModelMap>('create_provider_model_map', {
          serverUrl,
          data: {
            model_id: modelId,
            provider_id: form.provider_id,
            provider_model_id: form.provider_model_id,
            protocols: form.protocols,
            priority: form.priority,
            status: form.status,
            context_length: form.context_length,
            timeout: form.timeout,
            is_active: form.is_active,
          },
        });
      }
      closeDrawer();
      loadMaps();
    } catch (e) {
      console.error(e);
    }
  };

  const handleDelete = (id: number) => {
    DoodleModal.confirm({
      title: '确定删除？',
      content: '将删除该供应商映射',
      okText: '确定',
      cancelText: '取消',
      danger: true,
      onOk: async () => {
        await invoke('delete_provider_model_map', { serverUrl, id });
        loadMaps();
      },
    });
  };

  const handleToggleMappingActive = async (item: ProviderModelMap, checked: boolean) => {
    const prevState = item.is_active;
    setMaps((prev) =>
      prev.map((m) => (m.id === item.id ? { ...m, is_active: checked } : m))
    );
    try {
      await invoke<ProviderModelMap>('update_provider_model_map', {
        serverUrl,
        id: item.id,
        data: { is_active: checked },
      });
    } catch (e) {
      console.error(e);
      setMaps((prev) =>
        prev.map((m) => (m.id === item.id ? { ...m, is_active: prevState } : m))
      );
      loadMaps();
    }
  };

  return (
    <div className="models-page">
      <div className="toolbar">
        <div className="toolbar-title">
          <DoodleButton size="small" type="ghost" className="back-arrow" onClick={onBack}>←</DoodleButton>
          供应商映射 - {modelName}
        </div>
        <div className="toolbar-spacer" />
        <div className="toolbar-actions">
          <DoodleButton type="primary" onClick={openCreate}>+ 新建映射</DoodleButton>
        </div>
      </div>
      <div className="models-table-wrap">
        <table className="models-table">
          <thead>
            <tr>
              <th>供应商</th>
              <th style={{ minWidth: 140 }}>Provider Model ID</th>
              <th>协议</th>
              <th>优先级</th>
              <th>上下文长度</th>
              <th>超时</th>
              <th>状态</th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {maps.length === 0 ? (
              <tr>
                <td colSpan={9} className="models-empty">暂无映射</td>
              </tr>
            ) : (
              maps.map((item) => (
                <tr key={item.id}>
                  <td>{providers.find((p) => p.id === item.provider_id)?.name ?? item.provider_id}</td>
                  <td>{item.provider_model_id}</td>
                  <td>{item.protocols || '-'}</td>
                  <td>{item.priority}</td>
                  <td>{item.context_length?.toLocaleString() ?? '-'}</td>
                  <td>{item.timeout != null ? `${item.timeout}s` : '-'}</td>
                  <td>
                    <DoodleTag color={STATUS_COLOR[item.status] ?? 'default'}>
                      {STATUS_OPTIONS.find((o) => o.value === item.status)?.label ?? item.status}
                    </DoodleTag>
                  </td>
                  <td>
                    <div className="toggle-wrap">
                      <Toggle checked={item.is_active} onChange={(checked) => handleToggleMappingActive(item, checked)} />
                    </div>
                  </td>
                  <td>
                    <div className="act">
                      <DoodleButton size="small" onClick={() => openEdit(item)}>编辑</DoodleButton>
                      <DoodleButton size="small" type="danger" onClick={() => handleDelete(item.id)}>删除</DoodleButton>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <Drawer
        open={drawerOpen}
        title={drawerTitle}
        onClose={closeDrawer}
        onSave={handleSave}
      >
        <div className="form-field">
          <label className="form-label">供应商</label>
          <select
            className="form-select ant-input"
            value={form.provider_id}
            onChange={(e) => setForm({ ...form, provider_id: Number(e.target.value) })}
          >
            {providers.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
        <div className="form-field">
          <label className="form-label">Provider Model ID</label>
          <input
            className="form-input ant-input"
            value={form.provider_model_id}
            onChange={(e) => setForm({ ...form, provider_model_id: e.target.value })}
          />
        </div>
        <div className="form-field">
          <label className="form-label">协议</label>
          <DoodleCheckbox.Group
            value={form.protocols ? form.protocols.split(',').filter(Boolean) : []}
            onChange={(checked) => setForm({ ...form, protocols: checked.join(',') })}
            options={PROTOCOL_OPTIONS.map((p) => ({ label: p, value: p }))}
          />
        </div>
        <div className="form-row">
          <div className="form-field">
            <label className="form-label">优先级</label>
            <input
              type="number"
              className="form-number ant-input"
              value={form.priority}
              onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })}
            />
          </div>
          <div className="form-field">
            <label className="form-label">状态</label>
            <select
              className="form-select ant-input"
              value={form.status}
              onChange={(e) => setForm({ ...form, status: e.target.value })}
            >
              {STATUS_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
          </div>
        </div>
        <div className="form-row">
          <div className="form-field">
            <label className="form-label">上下文长度</label>
            <input
              type="number"
              className="form-number ant-input"
              value={form.context_length}
              onChange={(e) => setForm({ ...form, context_length: Number(e.target.value) })}
            />
          </div>
          <div className="form-field">
            <label className="form-label">启用</label>
            <div className="toggle-wrap">
              <Toggle
                checked={form.is_active}
                onChange={(checked) => setForm({ ...form, is_active: checked })}
              />
            </div>
          </div>
        </div>
        <div className="form-row">
          <div className="form-field">
            <label className="form-label">超时秒数</label>
            <input
              type="number"
              className="form-number ant-input"
              value={form.timeout}
              onChange={(e) => setForm({ ...form, timeout: Number(e.target.value) })}
            />
          </div>
        </div>
      </Drawer>
    </div>
  );
}

/* ───── 模型管理页 ───── */

function Models() {
  const [models, setModels] = useState<Model[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [searchQuery, setSearchQuery] = useState('');
  const [prioritySort, setPrioritySort] = useState<'asc' | 'desc' | null>(null);
  const [view, setView] = useState<'list' | 'mappings'>('list');
  const [mappingModelId, setMappingModelId] = useState(0);
  const [mappingModelName, setMappingModelName] = useState('');
  const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';

  const load = () => {
    invoke<Model[]>('fetch_models', { serverUrl }).then(setModels);
  };

  useEffect(() => {
    load();
    invoke<Provider[]>('fetch_providers', { serverUrl }).then(setProviders);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filteredModels = useMemo(() => {
    let result = models;
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      result = result.filter((m) => m.name.toLowerCase().includes(q));
    }
    if (prioritySort) {
      result = [...result].sort((a, b) =>
        prioritySort === 'asc' ? a.priority - b.priority : b.priority - a.priority
      );
    }
    return result;
  }, [models, searchQuery, prioritySort]);

  const togglePrioritySort = () => {
    setPrioritySort((prev) => (prev === 'asc' ? 'desc' : prev === 'desc' ? null : 'asc'));
  };

  const openNew = () => {
    setEditingId(null);
    setForm({ ...EMPTY_FORM });
    setDrawerOpen(true);
  };

  const openEdit = (m: Model) => {
    setEditingId(m.id);
    setForm({
      name: m.name,
      priority: m.priority,
      timeout: m.timeout,
      context_length: m.context_length,
      is_active: m.is_active,
    });
    setDrawerOpen(true);
  };

  const closeDrawer = () => {
    setDrawerOpen(false);
  };

  const handleSave = async () => {
    const payload: Partial<Model> = {
      name: form.name,
      priority: form.priority,
      timeout: form.timeout,
      context_length: form.context_length,
      is_active: form.is_active,
    };
    try {
      if (editingId !== null) {
        await invoke<Model>('update_model', { serverUrl, id: editingId, data: payload });
      } else {
        await invoke<Model>('create_model', { serverUrl, data: payload });
      }
      load();
      setDrawerOpen(false);
    } catch (e) {
      console.error(e);
    }
  };

  const handleDeleteRow = (model: Model) => {
    DoodleModal.confirm({
      title: '确定删除？',
      content: `将删除模型「${model.name}」`,
      okText: '确定',
      cancelText: '取消',
      danger: true,
      onOk: () => {
        invoke('delete_model', { serverUrl, id: model.id }).then(() => {
          load();
        });
      },
    });
  };

  const handleToggleActive = async (model: Model, checked: boolean) => {
    const prevState = model.is_active;
    setModels((prev) =>
      prev.map((m) => (m.id === model.id ? { ...m, is_active: checked } : m))
    );
    try {
      await invoke<Model>('update_model', {
        serverUrl,
        id: model.id,
        data: { is_active: checked },
      });
    } catch (e) {
      console.error(e);
      setModels((prev) =>
        prev.map((m) => (m.id === model.id ? { ...m, is_active: prevState } : m))
      );
      load();
    }
  };

  const openMappingPage = (m: Model) => {
    setMappingModelId(m.id);
    setMappingModelName(m.name);
    setView('mappings');
  };

  const sortIcon = prioritySort === 'asc' ? ' ▲' : prioritySort === 'desc' ? ' ▼' : '';

  if (view === 'mappings') {
    return (
      <ModelMappingsPage
        modelId={mappingModelId}
        modelName={mappingModelName}
        serverUrl={serverUrl}
        providers={providers}
        onBack={() => setView('list')}
      />
    );
  }

  return (
    <div className="models-page">
      <Toolbar title="模型" showSearch={true} searchValue={searchQuery} onSearchChange={setSearchQuery}>
        <DoodleButton type="primary" onClick={openNew}>+ 新增</DoodleButton>
      </Toolbar>
      <div className="models-table-wrap">
        <table className="models-table">
          <thead>
            <tr>
              <th>名称</th>
              <th className="sortable-th" onClick={togglePrioritySort}>
                优先级{sortIcon}
              </th>
              <th>超时</th>
              <th>上下文长度</th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {filteredModels.length === 0 ? (
              <tr>
                <td colSpan={6} className="models-empty">暂无数据</td>
              </tr>
            ) : (
              filteredModels.map((m) => (
                <tr key={m.id}>
                  <td className="clickable-name" onClick={() => openEdit(m)}>{m.name}</td>
                  <td>{m.priority}</td>
                  <td>{m.timeout}s</td>
                  <td>{m.context_length.toLocaleString()}</td>
                  <td>
                    <div className="toggle-wrap">
                      <Toggle checked={m.is_active} onChange={(checked) => handleToggleActive(m, checked)} />
                    </div>
                  </td>
                  <td>
                    <div className="act">
                      <DoodleButton size="small" onClick={(e) => { e.stopPropagation(); openEdit(m); }}>编辑</DoodleButton>
                      <DoodleButton size="small" onClick={(e) => { e.stopPropagation(); openMappingPage(m); }}>供应商映射</DoodleButton>
                      <DoodleButton size="small" type="danger" onClick={(e) => { e.stopPropagation(); handleDeleteRow(m); }}>删除</DoodleButton>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      <Drawer
        open={drawerOpen}
        title={editingId !== null ? '编辑模型' : '新增模型'}
        onClose={closeDrawer}
        onSave={handleSave}
      >
        <div className="form-field">
          <label className="form-label">名称</label>
          <input
            className="form-input ant-input"
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
        </div>
        <div className="form-row">
          <div className="form-field">
            <label className="form-label">优先级</label>
            <input
              type="number"
              className="form-number ant-input"
              value={form.priority}
              onChange={(e) =>
                setForm({ ...form, priority: Number(e.target.value) })
              }
            />
          </div>
          <div className="form-field">
            <label className="form-label">超时秒数</label>
            <input
              type="number"
              className="form-number ant-input"
              value={form.timeout}
              onChange={(e) =>
                setForm({ ...form, timeout: Number(e.target.value) })
              }
            />
          </div>
        </div>
        <div className="form-field">
          <label className="form-label">上下文长度</label>
          <input
            type="number"
            className="form-number ant-input"
            value={form.context_length}
            onChange={(e) =>
              setForm({ ...form, context_length: Number(e.target.value) })
            }
          />
        </div>
        <div className="form-field">
          <label className="form-label">启用</label>
          <div className="toggle-wrap">
            <Toggle checked={form.is_active} onChange={(checked) => setForm({ ...form, is_active: checked })} />
          </div>
        </div>
      </Drawer>
    </div>
  );
}

export default Models;