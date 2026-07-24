import { useEffect, useState } from 'react';
import Toolbar from '../components/Toolbar';
import { invoke } from '@tauri-apps/api/core';
import type { ServiceStatus } from '../types';
import './Overview.less';

interface OverviewProps {
  onStatusChange?: (connected: boolean) => void;
}

function Overview({ onStatusChange }: OverviewProps) {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [loading, setLoading] = useState(true);

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
          <button className="ant-btn" onClick={handleRefresh}>刷新缓存</button>
        </Toolbar>
        <div className="overview-loading">加载中...</div>
      </div>
    );
  }

  return (
    <div className="overview-page">
      <Toolbar title="概览">
        <button className="ant-btn" onClick={handleRefresh}>刷新缓存</button>
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

        <div className="penalties">
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
