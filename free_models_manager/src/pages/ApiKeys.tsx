import { useEffect, useMemo, useState, type MouseEvent } from 'react';
import Toolbar from '../components/Toolbar';
import Toggle from '../components/Toggle';
import Drawer from '../components/Drawer';
import {
  DoodleButton,
  DoodleInput,
  DoodleMessage,
  DoodleModal,
  DoodleTag,
} from '../components/doodle';
import { invoke } from '@tauri-apps/api/core';
import type { ApiKey } from '../types';
import './ApiKeys.less';

function maskKey(key: string): string {
  if (!key) return '';
  if (key.length <= 7) return '****';
  return `${key.slice(0, 3)}****...${key.slice(-4)}`;
}

function formatTime(time?: string): string {
  if (!time) return '-';
  const d = new Date(time);
  if (isNaN(d.getTime())) return time;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function ApiKeys() {
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingKey, setEditingKey] = useState<ApiKey | null>(null);
  const [formName, setFormName] = useState('');
  const [formKeyValue, setFormKeyValue] = useState('');
  const [formIsActive, setFormIsActive] = useState(true);
  const [showKeyValue, setShowKeyValue] = useState(false);

  const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
  const [searchQuery, setSearchQuery] = useState('');

  const filteredApiKeys = useMemo(() => {
    if (!searchQuery) return apiKeys;
    const q = searchQuery.toLowerCase();
    return apiKeys.filter((k) =>
      k.name.toLowerCase().includes(q)
    );
  }, [apiKeys, searchQuery]);

  const loadApiKeys = () => {
    invoke<ApiKey[]>('fetch_api_keys', { serverUrl }).then(setApiKeys);
  };

  useEffect(() => {
    loadApiKeys();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openAdd = () => {
    setEditingKey(null);
    setFormName('');
    setFormKeyValue('');
    setFormIsActive(true);
    setShowKeyValue(false);
    setDrawerOpen(true);
  };

  const openEdit = (key: ApiKey) => {
    setEditingKey(key);
    setFormName(key.name);
    setFormKeyValue(key.key_value);
    setFormIsActive(key.is_active);
    setShowKeyValue(false);
    setDrawerOpen(true);
  };

  const handleClose = () => {
    setDrawerOpen(false);
  };

  const handleSave = async () => {
    try {
      if (editingKey) {
        const payload = {
          name: formName,
          key_value: formKeyValue,
          is_active: formIsActive,
        };
        await invoke<ApiKey>('update_api_key', { serverUrl, id: editingKey.id, data: payload });
        setDrawerOpen(false);
        loadApiKeys();
      } else {
        const payload = {
          name: formName,
          is_active: formIsActive,
        };
        const result = await invoke<ApiKey>('create_api_key', { serverUrl, data: payload });
        setDrawerOpen(false);
        DoodleModal.info({
          title: 'API Key 创建成功',
          content: (
            <div>
              <p>请立即复制保存此 Key，关闭后将不再显示：</p>
              <div className="api-key-result">{result.key_value}</div>
            </div>
          ),
          okText: '已复制保存',
        });
        loadApiKeys();
      }
    } catch (e) {
      console.error(e);
      DoodleMessage.error('保存失败');
    }
  };

  const handleDeleteRow = async (key: ApiKey) => {
    DoodleModal.confirm({
      title: '确定删除？',
      content: `将删除 API Key「${key.name}」`,
      okText: '确定',
      cancelText: '取消',
      danger: true,
      onOk: async () => {
      try {
        await invoke('delete_api_key', { serverUrl, id: key.id });
        loadApiKeys();
      } catch (e) {
        console.error(e);
        DoodleMessage.error('删除失败');
      }
    },
    });
  };

  const handleToggleActive = async (key: ApiKey, checked: boolean) => {
    const prevState = key.is_active;
    setApiKeys((prev) =>
      prev.map((k) => (k.id === key.id ? { ...k, is_active: checked } : k))
    );
    try {
      await invoke<ApiKey>('update_api_key', {
        serverUrl,
        id: key.id,
        data: { is_active: checked },
      });
    } catch (e) {
      console.error(e);
      setApiKeys((prev) =>
        prev.map((k) => (k.id === key.id ? { ...k, is_active: prevState } : k))
      );
      loadApiKeys();
    }
  };

  const handleCopy = (e: MouseEvent, key: string) => {
    e.stopPropagation();
    if (key) {
      navigator.clipboard.writeText(key);
      DoodleMessage.success('已复制到剪贴板');
    }
  };

  return (
    <div className="api-keys-page">
      <Toolbar title="API Keys" showSearch={true} searchValue={searchQuery} onSearchChange={setSearchQuery}>
        <DoodleButton type="primary" onClick={openAdd}>+ 新增</DoodleButton>
      </Toolbar>
      <div className="api-keys-table-wrap">
        <table className="api-keys-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>Key</th>
              <th>启用</th>
              <th>创建时间</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {filteredApiKeys.length === 0 ? (
              <tr>
                <td colSpan={5} className="api-keys-empty">暂无数据</td>
              </tr>
            ) : (
              filteredApiKeys.map((k) => (
                <tr key={k.id}>
                  <td className="clickable-name" onClick={() => openEdit(k)}>{k.name}</td>
                  <td>
                    <DoodleTag className="api-keys-mask">{maskKey(k.key_value)}</DoodleTag>
                    <svg className="api-keys-copy" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" onClick={(e) => handleCopy(e as any, k.key_value)} style={{ cursor: 'pointer', verticalAlign: 'middle' }}>
                      <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                    </svg>
                  </td>
                  <td>
                    <div className="toggle-wrap">
                      <Toggle checked={k.is_active} onChange={(checked) => handleToggleActive(k, checked)} />
                    </div>
                  </td>
                  <td className="api-keys-time">{formatTime(k.created_time)}</td>
                  <td>
                    <div className="act">
                      <DoodleButton size="small" onClick={(e) => { e.stopPropagation(); openEdit(k); }}>编辑</DoodleButton>
                      <DoodleButton size="small" type="danger" onClick={(e) => { e.stopPropagation(); handleDeleteRow(k); }}>删除</DoodleButton>
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
        title={editingKey ? '编辑 API Key' : '新增 API Key'}
        onClose={handleClose}
        onSave={handleSave}
      >
        <div className="api-keys-form">
          <div className="form-field">
            <label>名称</label>
            <DoodleInput
              type="text"
              value={formName}
              onChange={(e) => setFormName(e.target.value)}
              placeholder="输入名称"
            />
          </div>
          {editingKey && (
          <div className="form-field">
            <label>Key Value</label>
            <div className="key-input-wrap">
              <DoodleInput
                type={showKeyValue ? 'text' : 'password'}
                value={formKeyValue}
                disabled
                placeholder="输入 Key Value"
              />
              <DoodleButton
                type="ghost"
                size="small"
                className="key-toggle-btn"
                onClick={() => setShowKeyValue(!showKeyValue)}
              >
                {showKeyValue ? '隐藏' : '显示'}
              </DoodleButton>
            </div>
          </div>
          )}
          <div className="form-field">
            <label>状态</label>
            <div className="toggle-wrap">
              <Toggle checked={formIsActive} onChange={setFormIsActive} />
            </div>
          </div>
        </div>
      </Drawer>
    </div>
  );
}

export default ApiKeys;