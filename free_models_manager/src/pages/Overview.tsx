import { useEffect, useState } from 'react';
import { DoodleButton } from '../components/doodle';
import Toolbar from '../components/Toolbar';
import { invoke } from '@tauri-apps/api/core';
import type { ServiceStatus, UsageLogStatItem } from '../types';
import './Overview.less';

interface OverviewProps {
  onStatusChange?: (connected: boolean) => void;
}

function formatTokens(n: number): string {
  if (n < 1000) return n.toString();
  if (n < 1_000_000) return (n / 1000).toFixed(1) + 'K';
  return (n / 1_000_000).toFixed(1) + 'M';
}

function Overview({ onStatusChange }: OverviewProps) {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [combinedStats, setCombinedStats] = useState<UsageLogStatItem[]>([]);

  const load = async () => {
    try {
      const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
      const data = await invoke<any>('get_service_status', { serverUrl });
      setStatus(data);
      onStatusChange?.(true);
    } catch {
      setStatus(null);
      onStatusChange?.(false);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    const yesterday = new Date(Date.now() - 86400000).toISOString().slice(0, 10);
    const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
    invoke<any>('fetch_usage_log_stats', { serverUrl, groupBy: 'provider_model', startTime: yesterday, endTime: yesterday })
      .then(res => setCombinedStats(res?.items ?? []))
      .catch(() => setCombinedStats([]));
  }, []);

  const handleRefresh = async () => {
    setLoading(true);
    try {
      const serverUrl = localStorage.getItem('server_url') || 'http://localhost:8080';
      await invoke('refresh_cache', { serverUrl });
    } catch {
    }
    await load();
  };

  if (loading && !status) {
    return (
      <div className="overview-page">
        <Toolbar title="概览">
          <DoodleButton onClick={handleRefresh}>刷新缓存</DoodleButton>
        </Toolbar>
        <div className="overview-loading">加载中...</div>
      </div>
    );
  }

  return (
    <div className="overview-page">
      <Toolbar title="概览">
        <DoodleButton onClick={handleRefresh}>刷新缓存</DoodleButton>
      </Toolbar>
      <div className="overview-content">
        <div className="stats-row">
          <div className="stat-block">
            <div className="stat-label">服务状态</div>
            <div className="stat-value status-text">
              <span className={`status-dot ${status?.healthy ? 'ok' : 'down'}`} />
              {status?.healthy ? 'Healthy' : 'Down'}
            </div>
          </div>
          <div className="stat-block">
            <div className="stat-label">模型数</div>
            <div className="stat-group">
              <span className="stat-sub">总数:</span>
              <span className="stat-figure stat-total">{status?.models.total ?? 0}</span>
              <span className="stat-sub">可用:</span>
              <span className="stat-figure stat-active">{status?.models.active ?? 0}</span>
              <span className="stat-sub">不可用:</span>
              <span className="stat-figure stat-inactive">{status?.models.inactive ?? 0}</span>
            </div>
          </div>
          <div className="stat-block">
            <div className="stat-label">供应商数</div>
            <div className="stat-group">
              <span className="stat-sub">总数:</span>
              <span className="stat-figure stat-total">{status?.providers.total ?? 0}</span>
              <span className="stat-sub">可用:</span>
              <span className="stat-figure stat-active">{status?.providers.active ?? 0}</span>
              <span className="stat-sub">不可用:</span>
              <span className="stat-figure stat-inactive">{status?.providers.inactive ?? 0}</span>
            </div>
          </div>
          <div className="stat-block">
            <div className="stat-label">API Keys 数</div>
            <div className="stat-group">
              <span className="stat-sub">总数:</span>
              <span className="stat-figure stat-total">{status?.apiKeys.total ?? 0}</span>
              <span className="stat-sub">可用:</span>
              <span className="stat-figure stat-active">{status?.apiKeys.active ?? 0}</span>
              <span className="stat-sub">不可用:</span>
              <span className="stat-figure stat-inactive">{status?.apiKeys.inactive ?? 0}</span>
            </div>
          </div>
        </div>

        {!loading && (
          <div className="yesterday-stats">
            <div className="yesterday-section doodle-paper-card">
              <div className="yesterday-title">昨日 Token 消耗</div>
              <table className="yesterday-table">
                <thead>
                  <tr>
                    <th>供应商</th><th>模型</th><th>请求数</th><th>Prompt Tokens</th><th>Completion Tokens</th><th>Total Tokens</th><th>缓存命中</th><th>缓存命中率</th>
                  </tr>
                </thead>
                <tbody>
                  {combinedStats.map((item, i) => {
                    const totalCache = item.cache_hit_tokens + item.cache_miss_tokens;
                    const hitRate = totalCache > 0 ? (item.cache_hit_tokens / totalCache * 100).toFixed(1) : '0';
                    const parts = item.dimension_name.split(' / ');
                    return (
                      <tr key={i}>
                        <td>{parts[0]}</td>
                        <td>{parts[1]}</td>
                        <td>{item.requests}</td>
                        <td>{formatTokens(item.prompt_tokens)}</td>
                        <td>{formatTokens(item.completion_tokens)}</td>
                        <td>{formatTokens(item.total_tokens)}</td>
                        <td>{formatTokens(item.cache_hit_tokens)}</td>
                        <td>{hitRate}%</td>
                      </tr>
                    );
                  })}
                  {combinedStats.length === 0 && <tr><td colSpan={8} className="yesterday-empty">昨日无数据</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        )}

        <div className="penalties doodle-paper-card">
          <div className="penalties-title">惩罚中的模型</div>
          {status?.penalties && status.penalties.length > 0 ? (
            <div className="penalties-list">
              {status.penalties.map((p, i) => (
                <div className="penalty-row" key={i}>
                  <div className="penalty-info">
                    <span className="penalty-model">{p.modelName}</span>
                    <span className="penalty-sep">|</span>
                    <span className="penalty-provider">{p.providerName}</span>
                  </div>
                  <span className="penalty-remaining">
                    剩余 {Math.ceil(p.remainingSecs / 60)}min
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <div className="penalties-empty">暂无惩罚中的模型</div>
          )}
        </div>
      </div>
    </div>
  );
}

export default Overview;
