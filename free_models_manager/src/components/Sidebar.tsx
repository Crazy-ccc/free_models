import type { PageKey } from '../types';
import './Sidebar.less';

interface SidebarProps {
  activePage: PageKey;
  onNavigate: (page: PageKey) => void;
  serviceConnected: boolean;
}

const NAV_ITEMS: Array<{ key: PageKey; label: string }> = [
  { key: 'overview', label: '概览' },
  { key: 'providers', label: '供应商' },
  { key: 'models', label: '模型' },
  { key: 'apiKeys', label: 'API Keys' },
  { key: 'stats', label: '统计' },
  { key: 'settings', label: '设置' },
];

function Sidebar({ activePage, onNavigate, serviceConnected }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">Free Models</div>
      <nav className="sidebar-nav">
        {NAV_ITEMS.map((item) => (
          <div
            key={item.key}
            className={`sidebar-nav-item${item.key === activePage ? ' active' : ''}`}
            onClick={() => onNavigate(item.key)}
          >
            {item.label}
          </div>
        ))}
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
