import { useEffect, useMemo, useState } from 'react';
import { Modal } from 'antd';
import Toolbar from '../components/Toolbar';
import Drawer from '../components/Drawer';
import Toggle from '../components/Toggle';
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

/* ───── 供应商映射弹窗 ───── */

const PROTOCOL_OPTIONS = ['openai', 'anthropic'];
const STATUS_OPTIONS = [
  { value: 'available', label: '可用' },
  { value: 'unavailable', label: '不可用' },
  { value: 'deprecated', label: '废弃' },
];

interface MappingModalProps {
  visible: boolean;
  modelId: number;
  modelName: string;
  onClose: () => void;
  serverUrl: string;
  providers: Provider[];
}

function ProviderMappingModal({ visible, modelId, modelName, onClose, serverUrl, providers }: MappingModalProps) {
  const [maps, setMaps] = useState<ProviderModelMap[]>([]);

  const loadMaps = () => {
    invoke<ProviderModelMap[]>('fetch_provider_model_maps', { serverUrl, modelId }).then(setMaps);
  };

  useEffect(() => {
    if (visible) {
      loadMaps();
    }
  }, [visible, modelId]);

  const handleFieldChange = (index: number, field: string, value: any) => {
    setMaps((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], [field]: value };
      return next;
    });
  };

  const handleSave = async (item: ProviderModelMap) => {
    try {
      if (item.id > 0) {
        await invoke<ProviderModelMap>('update_provider_model_map', {
          serverUrl,
          id: item.id,
          data: {
            provider_id: item.provider_id,
            provider_model_id: item.provider_model_id,
            protocols: item.protocols,
            priority: item.priority,
            status: item.status,
            is_active: item.is_active,
          },
        });
      } else {
        await invoke<ProviderModelMap>('create_provider_model_map', {
          serverUrl,
          data: {
            model_id: modelId,
            provider_id: item.provider_id,
            provider_model_id: item.provider_model_id,
            protocols: item.protocols,
            priority: item.priority,
            status: item.status,
            is_active: item.is_active,
          },
        });
      }
      loadMaps();
    } catch (e) {
      console.error(e);
    }
  };

  const handleDelete = async (id: number) => {
    Modal.confirm({
      title: '确定删除？',
      content: '将删除该供应商映射',
      okText: '确定',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: async () => {
        await invoke('delete_provider_model_map', { serverUrl, id });
        loadMaps();
      },
    });
  };

  const handleAdd = () => {
    setMaps((prev) => [
      ...prev,
      {
        id: 0,
        model_id: modelId,
        provider_id: providers[0]?.id ?? 0,
        provider_model_id: '',
        protocols: '',
        priority: 0,
        status: 'available',
        is_active: true,
      },
    ]);
  };

  return (
    <Modal
      title={`供应商映射 - ${modelName}`}
      open={visible}
      onCancel={onClose}
      footer={null}
      width={900}
    >
      <div className="mapping-table-wrap">
        <table className="models-table mapping-table">
          <thead>
            <tr>
              <th>供应商</th>
              <th style={{ minWidth: 140 }}>供应商 Model ID *</th>
              <th>协议</th>
              <th>优先级</th>
              <th>状态</th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {maps.length === 0 ? (
              <tr>
                <td colSpan={7} className="models-empty">暂无映射</td>
              </tr>
            ) : (
              maps.map((item, idx) => (
                <tr key={item.id || `new-${idx}`}>
                  <td>
                    <select
                      className="form-select ant-input"
                      value={item.provider_id}
                      onChange={(e) => handleFieldChange(idx, 'provider_id', Number(e.target.value))}
                      style={{ width: 120 }}
                    >
                      {providers.map((p) => (
                        <option key={p.id} value={p.id}>{p.name}</option>
                      ))}
                    </select>
                  </td>
                  <td>
                    <input
                      className="ant-input"
                      value={item.provider_model_id}
                      onChange={(e) => handleFieldChange(idx, 'provider_model_id', e.target.value)}
                      placeholder="必填"
                      style={{ width: '100%' }}
                    />
                  </td>
                  <td>
                    <select
                      className="form-select ant-input"
                      value={item.protocols}
                      onChange={(e) => handleFieldChange(idx, 'protocols', e.target.value)}
                      style={{ width: 120 }}
                    >
                      <option value="">不限</option>
                      {PROTOCOL_OPTIONS.map((p) => (
                        <option key={p} value={p}>{p}</option>
                      ))}
                    </select>
                  </td>
                  <td>
                    <input
                      type="number"
                      className="ant-input"
                      value={item.priority}
                      onChange={(e) => handleFieldChange(idx, 'priority', Number(e.target.value))}
                      style={{ width: 70 }}
                    />
                  </td>
                  <td>
                    <select
                      className="form-select ant-input"
                      value={item.status}
                      onChange={(e) => handleFieldChange(idx, 'status', e.target.value)}
                      style={{ width: 100 }}
                    >
                      {STATUS_OPTIONS.map((o) => (
                        <option key={o.value} value={o.value}>{o.label}</option>
                      ))}
                    </select>
                  </td>
                  <td>
                    <Toggle
                      checked={item.is_active}
                      onChange={(checked) => handleFieldChange(idx, 'is_active', checked)}
                    />
                  </td>
                  <td>
                    <button
                      className="ant-btn"
                      style={{ marginRight: 6 }}
                      onClick={() => handleSave(item)}
                      disabled={!item.provider_model_id.trim()}
                    >
                      保存
                    </button>
                    <button
                      className="ant-btn ant-btn-dangerous"
                      onClick={() => handleDelete(item.id)}
                    >
                      删除
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      <div style={{ marginTop: 12, textAlign: 'right' }}>
        <button className="ant-btn ant-btn-primary" onClick={handleAdd}>+ 添加映射</button>
      </div>
    </Modal>
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
  const [mappingModal, setMappingModal] = useState<{ modelId: number; modelName: string } | null>(null);
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
    Modal.confirm({
      title: '确定删除？',
      content: `将删除模型「${model.name}」`,
      okText: '确定',
      cancelText: '取消',
      okButtonProps: { danger: true },
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
    } finally {
      load();
    }
  };

  const openMappingModal = (m: Model) => {
    setMappingModal({ modelId: m.id, modelName: m.name });
  };

  const sortIcon = prioritySort === 'asc' ? ' ▲' : prioritySort === 'desc' ? ' ▼' : '';

  return (
    <div className="models-page">
      <Toolbar title="模型" showSearch={true} searchValue={searchQuery} onSearchChange={setSearchQuery}>
        <button className="ant-btn ant-btn-primary" onClick={openNew}>+ 新增</button>
      </Toolbar>
      <div className="models-table-wrap">
        <table className="models-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>供应商</th>
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
                <td colSpan={7} className="models-empty">暂无数据</td>
              </tr>
            ) : (
              filteredModels.map((m) => (
                <tr key={m.id}>
                  <td className="clickable-name" onClick={() => openEdit(m)}>{m.name}</td>
                  <td>-</td>
                  <td>{m.priority}</td>
                  <td>{m.timeout}s</td>
                  <td>{m.context_length.toLocaleString()}</td>
                  <td>
                    <Toggle checked={m.is_active} onChange={(checked) => handleToggleActive(m, checked)} />
                  </td>
                  <td>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openEdit(m); }}>编辑</button>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openMappingModal(m); }}>供应商映射</button>
                    <button className="ant-btn ant-btn-dangerous" onClick={(e) => { e.stopPropagation(); handleDeleteRow(m); }}>删除</button>
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
      {mappingModal && (
        <ProviderMappingModal
          visible={true}
          modelId={mappingModal.modelId}
          modelName={mappingModal.modelName}
          onClose={() => setMappingModal(null)}
          serverUrl={serverUrl}
          providers={providers}
        />
      )}
    </div>
  );
}

export default Models;
