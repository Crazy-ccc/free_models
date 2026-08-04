import type { PageKey } from '../types';
import './Sidebar.less';

interface SidebarProps {
  activePage: PageKey;
  onNavigate: (page: PageKey) => void;
  serviceConnected: boolean;
}

const NAV_ITEMS: Array<{ key: PageKey; label: string; icon: string }> = [
  { key: 'overview', label: '概览', icon: '🎏' },
  { key: 'providers', label: '供应商', icon: '🧭' },
  { key: 'models', label: '模型', icon: '🤖' },
  { key: 'apiKeys', label: 'API Keys', icon: '🗝️' },
  { key: 'stats', label: '统计', icon: '📈' },
  { key: 'settings', label: '设置', icon: '🖊️' },
];

function Sidebar({ activePage, onNavigate, serviceConnected }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <div className="sidebar-brand-logo">✏️</div>
        <div className="sidebar-brand-text">
          <span className="sidebar-brand-name">Free Models</span>
          <span className="sidebar-brand-sub">模型中转 · 使用手册</span>
        </div>
      </div>
      <nav className="sidebar-nav">
        {NAV_ITEMS.map((item) => (
          <div
            key={item.key}
            className={`sidebar-nav-item${item.key === activePage ? ' active' : ''}`}
            onClick={() => onNavigate(item.key)}
          >
            <span className="sidebar-nav-icon">{item.icon}</span>
            <span className="sidebar-nav-label">{item.label}</span>
          </div>
        ))}
        <div className="sidebar-nav-divider" />
      </nav>
      <div className="sidebar-footer">
        <div className="sidebar-status">
          <span className={`sidebar-status-dot${serviceConnected ? ' ok' : ''}`} />
          <span className="sidebar-status-text">
            {serviceConnected ? '服务已连接' : '服务未连接'}
          </span>
        </div>
      </div>
    </aside>
  );
}

export default Sidebar;
