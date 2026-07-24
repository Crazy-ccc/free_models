import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Modal } from 'antd';
import Toolbar from '../components/Toolbar';
import Drawer from '../components/Drawer';
import { invoke } from '@tauri-apps/api/core';
import type { Model, Provider } from '../types';
import './Models.less';

interface FormState {
  name: string;
  model_id: string;
  provider_id: number;
  protocols: string[];
  priority: number;
  timeout: number;
  context_length: number;
  status: string;
}

const EMPTY_FORM: FormState = {
  name: '',
  model_id: '',
  provider_id: 0,
  protocols: [],
  priority: 0,
  timeout: 30,
  context_length: 4096,
  status: 'available',
};

const PROTOCOL_OPTIONS = ['openai', 'anthropic'];

/* ───── 多选筛选组件（类 Ant Design table filter） ───── */

interface ColFilterProps {
  title: string;
  options: { value: string; label: string }[];
  selected: Set<string>;
  onConfirm: (selected: Set<string>) => void;
  onReset: () => void;
  searchable?: boolean;
}

function ColFilter({ title, options, selected, onConfirm, onReset, searchable }: ColFilterProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [pending, setPending] = useState<Set<string>>(new Set(selected));
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const ref = useRef<HTMLDivElement>(null);
  const iconRef = useRef<HTMLButtonElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node) &&
          dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  const filtered = search
    ? options.filter((o) => o.label.toLowerCase().includes(search.toLowerCase()))
    : options;

  const hasSelection = selected.size > 0;

  const togglePending = (value: string) => {
    setPending((prev) => {
      const next = new Set(prev);
      if (next.has(value)) {
        next.delete(value);
      } else {
        next.add(value);
      }
      return next;
    });
  };

  const handleOpen = () => {
    if (!iconRef.current) return;
    const rect = iconRef.current.getBoundingClientRect();
    setPos({ left: rect.left, top: rect.bottom + 4 });
    setPending(new Set(selected));
    setSearch('');
    setOpen(true);
  };

  const handleConfirm = () => {
    onConfirm(pending);
    setOpen(false);
  };

  const handleReset = () => {
    onReset();
    setPending(new Set());
    setOpen(false);
  };

  return (
    <div ref={ref} style={{ display: 'inline-flex', alignItems: 'center', gap: 2 }}>
      <span>{title}</span>
      <button
        ref={iconRef}
        className={`antd-filter-icon${hasSelection ? ' active' : ''}`}
        onClick={handleOpen}
      >
        <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M2 3h12l-4.5 5.5v4l-3 1.5v-5.5z" />
        </svg>
      </button>
      {open && createPortal(
        <div
          ref={dropdownRef}
          className="antd-filter-dropdown"
          style={{ position: 'fixed', left: pos.left, top: pos.top }}
        >
          {searchable && (
            <div className="antd-filter-search">
              <input
                className="ant-input"
                placeholder="搜索..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                autoFocus
              />
            </div>
          )}
          <div className="antd-filter-options">
            {filtered.length === 0 ? (
              <div className="antd-filter-empty">无匹配项</div>
            ) : (
              filtered.map((opt) => (
                <label
                  key={opt.value}
                  className={`antd-filter-option${pending.has(opt.value) ? ' selected' : ''}`}
                >
                  <input
                    type="checkbox"
                    checked={pending.has(opt.value)}
                    onChange={() => togglePending(opt.value)}
                  />
                  <span>{opt.label}</span>
                </label>
              ))
            )}
          </div>
          <div className="antd-filter-footer">
            <button className="antd-filter-btn antd-filter-btn-reset" onClick={handleReset}>重置</button>
            <button className="antd-filter-btn antd-filter-btn-ok" onClick={handleConfirm}>确定</button>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
}

function Models() {
  const [models, setModels] = useState<Model[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [searchQuery, setSearchQuery] = useState('');
  const [filterProviderIds, setFilterProviderIds] = useState<Set<string>>(new Set());
  const [filterProtocols, setFilterProtocols] = useState<Set<string>>(new Set());
  const [prioritySort, setPrioritySort] = useState<'asc' | 'desc' | null>(null);
  const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';

  const load = () => {
    invoke<Model[]>('fetch_models', { serverUrl }).then(setModels);
  };

  useEffect(() => {
    load();
    invoke<Provider[]>('fetch_providers', { serverUrl }).then(setProviders);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const providerMap = new Map<number, Provider>(
    providers.map((p) => [p.id, p] as [number, Provider])
  );

  const protocolsOf = (m: Model): string[] =>
    m.protocols
      ? m.protocols.split(',').map((s) => s.trim()).filter(Boolean)
      : [];

  const filteredModels = useMemo(() => {
    let result = models;
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      result = result.filter((m) =>
        m.name.toLowerCase().includes(q) || m.model_id.toLowerCase().includes(q)
      );
    }
    if (filterProviderIds.size > 0) {
      result = result.filter((m) => filterProviderIds.has(String(m.provider_id)));
    }
    if (filterProtocols.size > 0) {
      result = result.filter((m) => protocolsOf(m).some((p) => filterProtocols.has(p)));
    }
    if (prioritySort) {
      result = [...result].sort((a, b) =>
        prioritySort === 'asc' ? a.priority - b.priority : b.priority - a.priority
      );
    }
    return result;
  }, [models, searchQuery, filterProviderIds, filterProtocols, prioritySort]);

  const togglePrioritySort = () => {
    setPrioritySort((prev) => (prev === 'asc' ? 'desc' : prev === 'desc' ? null : 'asc'));
  };

  const openNew = () => {
    setEditingId(null);
    setForm({ ...EMPTY_FORM, provider_id: providers[0]?.id ?? 0 });
    setDrawerOpen(true);
  };

  const openEdit = (m: Model) => {
    setEditingId(m.id);
    setForm({
      name: m.name,
      model_id: m.model_id,
      provider_id: m.provider_id,
      protocols: m.protocols
        ? m.protocols.split(',').map((s) => s.trim()).filter(Boolean)
        : [],
      priority: m.priority,
      timeout: m.timeout,
      context_length: m.context_length,
      status: m.status,
    });
    setDrawerOpen(true);
  };

  const closeDrawer = () => {
    setDrawerOpen(false);
  };

  const handleSave = async () => {
    const payload: Partial<Model> = {
      name: form.name,
      model_id: form.model_id,
      provider_id: form.provider_id,
      protocols: form.protocols.join(','),
      priority: form.priority,
      timeout: form.timeout,
      context_length: form.context_length,
      status: form.status as Model['status'],
    };
    try {
      if (editingId !== null) {
        await invoke<Model>('update_model', { serverUrl, id: editingId, data: payload });
        load();
      } else {
        await invoke<Model>('create_model', { serverUrl, data: payload });
        load();
      }
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

  const handleStatusChange = async (m: Model, newStatus: string) => {
    const prevStatus = m.status;
    setModels((prev) =>
      prev.map((item) => (item.id === m.id ? { ...item, status: newStatus as Model['status'] } : item))
    );
    try {
      await invoke<Model>('update_model', {
        serverUrl,
        id: m.id,
        data: { ...m, status: newStatus },
      });
    } catch (e) {
      console.error(e);
      setModels((prev) =>
        prev.map((item) => (item.id === m.id ? { ...item, status: prevStatus } : item))
      );
    } finally {
      load();
    }
  };

  const selectProtocol = (p: string) => {
    setForm((f) => ({ ...f, protocols: [p] }));
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
              <th>
                <ColFilter
                  title="供应商"
                  options={providers.map((p) => ({ value: String(p.id), label: p.name }))}
                  selected={filterProviderIds}
                  onConfirm={(s) => setFilterProviderIds(s)}
                  onReset={() => setFilterProviderIds(new Set())}
                  searchable={true}
                />
              </th>
              <th>
                <ColFilter
                  title="协议"
                  options={PROTOCOL_OPTIONS.map((p) => ({ value: p, label: p }))}
                  selected={filterProtocols}
                  onConfirm={(s) => setFilterProtocols(s)}
                  onReset={() => setFilterProtocols(new Set())}
                />
              </th>
              <th className="sortable-th" onClick={togglePrioritySort}>
                优先级{sortIcon}
              </th>
              <th>超时</th>
              <th>上下文长度</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {filteredModels.length === 0 ? (
              <tr>
                <td colSpan={8} className="models-empty">暂无数据</td>
              </tr>
            ) : (
              filteredModels.map((m) => (
                <tr key={m.id}>
                  <td className="clickable-name" onClick={() => openEdit(m)}>{m.name}</td>
                  <td>{providerMap.get(m.provider_id)?.name ?? '-'}</td>
                  <td>
                    {PROTOCOL_OPTIONS.filter((p) => protocolsOf(m).includes(p)).map((p) => (
                      <span key={p} className={`protocol-tag ${p}`}>{p}</span>
                    ))}
                  </td>
                  <td>{m.priority}</td>
                  <td>{m.timeout}s</td>
                  <td>{m.context_length.toLocaleString()}</td>
                  <td>
                    <select
                      className="status-select"
                      value={m.status}
                      onChange={(e) => handleStatusChange(m, e.target.value)}
                    >
                      <option value="available">可用</option>
                      <option value="unavailable">不可用</option>
                      <option value="deprecated">废弃</option>
                    </select>
                  </td>
                  <td>
                    <button className="ant-btn" style={{ marginRight: 8 }} onClick={(e) => { e.stopPropagation(); openEdit(m); }}>编辑</button>
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
        <div className="form-field">
          <label className="form-label">Model ID</label>
          <input
            className="form-input ant-input"
            value={form.model_id}
            onChange={(e) => setForm({ ...form, model_id: e.target.value })}
          />
        </div>
        <div className="form-field">
          <label className="form-label">供应商</label>
          <select
            className="form-select ant-input"
            value={form.provider_id}
            onChange={(e) =>
              setForm({ ...form, provider_id: Number(e.target.value) })
            }
          >
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
        <div className="form-field">
          <label className="form-label">协议</label>
          <div className="protocol-select">
            {PROTOCOL_OPTIONS.map((p) => (
              <span
                key={p}
                className={`protocol-tag-select ${p}${
                  form.protocols[0] === p ? ' selected' : ''
                }`}
                onClick={() => selectProtocol(p)}
              >
                {p}
              </span>
            ))}
          </div>
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
          <label className="form-label">状态</label>
          <select
            className="form-select ant-input"
            value={form.status}
            onChange={(e) => setForm({ ...form, status: e.target.value })}
          >
            <option value="available">可用</option>
            <option value="unavailable">不可用</option>
            <option value="deprecated">废弃</option>
          </select>
        </div>
      </Drawer>
    </div>
  );
}

export default Models;
