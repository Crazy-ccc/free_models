import { useEffect, useState } from 'react';
import Toolbar from '../components/Toolbar';
import { invoke } from '@tauri-apps/api/core';
import type { UsageLogStatsResponse, Provider, Model, ProviderCredential, ApiKey } from '../types';
import './Stats.less';

type GroupBy = 'provider' | 'credential' | 'model' | 'api_key' | 'day';

const GROUP_OPTIONS: Array<{ value: GroupBy; label: string }> = [
  { value: 'provider', label: '供应商' },
  { value: 'credential', label: '凭证' },
  { value: 'model', label: '模型' },
  { value: 'api_key', label: 'API Key' },
  { value: 'day', label: '按天' },
];

const DIMENSION_LABELS: Record<GroupBy, string> = {
  provider: '供应商',
  credential: '凭证',
  model: '模型',
  api_key: 'API Key',
  day: '日期',
};

function formatNum(n: number): string {
  if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
  if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
  return String(n);
}

function Stats() {
  const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
  const [loading, setLoading] = useState(false);

  const [groupBy, setGroupBy] = useState<GroupBy>('provider');
  const [startTime, setStartTime] = useState('');
  const [endTime, setEndTime] = useState('');

  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<Model[]>([]);
  const [credentials, setCredentials] = useState<ProviderCredential[]>([]);
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);

  const [providerId, setProviderId] = useState<number | undefined>(undefined);
  const [credentialId, setCredentialId] = useState<number | undefined>(undefined);
  const [modelId, setModelId] = useState<number | undefined>(undefined);
  const [apiKeyId, setApiKeyId] = useState<number | undefined>(undefined);

  const [stats, setStats] = useState<UsageLogStatsResponse | null>(null);

  useEffect(() => {
    invoke<Provider[]>('fetch_providers', { serverUrl }).then(setProviders).catch(() => {});
    invoke<Model[]>('fetch_models', { serverUrl }).then(setModels).catch(() => {});
    invoke<ApiKey[]>('fetch_api_keys', { serverUrl }).then(setApiKeys).catch(() => {});
  }, []);

  useEffect(() => {
    if (providerId) {
      invoke<ProviderCredential[]>('fetch_provider_credentials', { serverUrl, providerId })
        .then(setCredentials)
        .catch(() => setCredentials([]));
    } else {
      setCredentials([]);
    }
  }, [providerId, serverUrl]);

  const loadData = async () => {
    setLoading(true);
    try {
      const data = await invoke<UsageLogStatsResponse>('fetch_usage_log_stats', {
        serverUrl,
        groupBy,
        startTime: startTime || undefined,
        endTime: endTime || undefined,
        providerId: providerId ?? undefined,
        credentialId: credentialId ?? undefined,
        modelId: modelId ?? undefined,
        apiKeyId: apiKeyId ?? undefined,
      });
      setStats(data);
    } catch {
      setStats(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, [groupBy]);

  const resetFilters = () => {
    setStartTime('');
    setEndTime('');
    setProviderId(undefined);
    setCredentialId(undefined);
    setModelId(undefined);
    setApiKeyId(undefined);
  };

  const showFilters = groupBy !== 'day';

  return (
    <div className="stats-page">
      <Toolbar title="使用统计">
        <button className="ant-btn" onClick={loadData} disabled={loading}>
          {loading ? '加载中...' : '查询'}
        </button>
        <button className="ant-btn" onClick={resetFilters}>重置筛选</button>
      </Toolbar>
      <div className="stats-content">
        <div className="stats-filter-bar">
          <div className="filter-group">
            <label>开始日期</label>
            <input
              className="ant-input"
              type="date"
              value={startTime}
              onChange={(e) => setStartTime(e.target.value)}
            />
          </div>
          <div className="filter-group">
            <label>结束日期</label>
            <input
              className="ant-input"
              type="date"
              value={endTime}
              onChange={(e) => setEndTime(e.target.value)}
            />
          </div>
          <div className="filter-group">
            <label>分组维度</label>
            <select
              className="ant-select"
              value={groupBy}
              onChange={(e) => setGroupBy(e.target.value as GroupBy)}
            >
              {GROUP_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>
          {showFilters && (
            <>
              <div className="filter-group">
                <label>供应商</label>
                <select
                  className="ant-select"
                  value={providerId ?? ''}
                  onChange={(e) => setProviderId(e.target.value ? Number(e.target.value) : undefined)}
                >
                  <option value="">全部</option>
                  {providers.map((p) => (
                    <option key={p.id} value={p.id}>{p.name}</option>
                  ))}
                </select>
              </div>
              <div className="filter-group">
                <label>凭证</label>
                <select
                  className="ant-select"
                  value={credentialId ?? ''}
                  onChange={(e) => setCredentialId(e.target.value ? Number(e.target.value) : undefined)}
                >
                  <option value="">全部</option>
                  {credentials.map((c) => (
                    <option key={c.id} value={c.id}>{c.name}</option>
                  ))}
                </select>
              </div>
              <div className="filter-group">
                <label>模型</label>
                <select
                  className="ant-select"
                  value={modelId ?? ''}
                  onChange={(e) => setModelId(e.target.value ? Number(e.target.value) : undefined)}
                >
                  <option value="">全部</option>
                  {models.map((m) => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))}
                </select>
              </div>
              <div className="filter-group">
                <label>API Key</label>
                <select
                  className="ant-select"
                  value={apiKeyId ?? ''}
                  onChange={(e) => setApiKeyId(e.target.value ? Number(e.target.value) : undefined)}
                >
                  <option value="">全部</option>
                  {apiKeys.map((k) => (
                    <option key={k.id} value={k.id}>{k.name}</option>
                  ))}
                </select>
              </div>
            </>
          )}
        </div>

        {stats && (
          <>
            <div className="stats-summary-row">
              <div className="stats-summary-card">
                <div className="stats-summary-label">总请求数</div>
                <div className="stats-summary-value">{formatNum(stats.total.requests)}</div>
              </div>
              <div className="stats-summary-card">
                <div className="stats-summary-label">总 Token 数</div>
                <div className="stats-summary-value">{formatNum(stats.total.total_tokens)}</div>
              </div>
              <div className="stats-summary-card">
                <div className="stats-summary-label">Prompt Tokens</div>
                <div className="stats-summary-value">{formatNum(stats.total.prompt_tokens)}</div>
              </div>
              <div className="stats-summary-card">
                <div className="stats-summary-label">Completion Tokens</div>
                <div className="stats-summary-value">{formatNum(stats.total.completion_tokens)}</div>
              </div>
              <div className="stats-summary-card">
                <div className="stats-summary-label">平均耗时</div>
                <div className="stats-summary-value">{stats.total.avg_duration_ms.toFixed(0)}ms</div>
              </div>
            </div>

            <div className="stats-table-wrap">
              <table className="stats-table">
                <thead>
                  <tr>
                    <th>{DIMENSION_LABELS[groupBy]}</th>
                    <th>请求数</th>
                    <th>Prompt Tokens</th>
                    <th>Completion Tokens</th>
                    <th>总 Tokens</th>
                    <th>Cache Hit</th>
                    <th>Cache Miss</th>
                    <th>缓存命中率</th>
                    <th>平均耗时</th>
                    <th>最大耗时</th>
                  </tr>
                </thead>
                <tbody>
                  {stats.items.length === 0 && (
                    <tr>
                      <td colSpan={10} className="stats-empty">暂无数据</td>
                    </tr>
                  )}
                  {stats.items.map((item, i) => {
                    const totalCache = item.cache_hit_tokens + item.cache_miss_tokens;
                    const hitRate = totalCache > 0 ? (item.cache_hit_tokens / totalCache * 100).toFixed(1) : '0.0';
                    return (
                      <tr key={i}>
                        <td>{item.dimension_name}</td>
                        <td>{formatNum(item.requests)}</td>
                        <td>{formatNum(item.prompt_tokens)}</td>
                        <td>{formatNum(item.completion_tokens)}</td>
                        <td>{formatNum(item.total_tokens)}</td>
                        <td>{formatNum(item.cache_hit_tokens)}</td>
                        <td>{formatNum(item.cache_miss_tokens)}</td>
                        <td>{hitRate}%</td>
                        <td>{item.avg_duration_ms.toFixed(0)}ms</td>
                        <td>{item.max_duration_ms.toFixed(0)}ms</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </>
        )}

        {!stats && !loading && (
          <div className="stats-empty-hint">点击"查询"加载统计数据</div>
        )}
      </div>
    </div>
  );
}

export default Stats;
