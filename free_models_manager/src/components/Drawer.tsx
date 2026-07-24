import type { ReactNode } from 'react';
import './Drawer.less';

interface DrawerProps {
  open: boolean;
  title: string;
  onClose: () => void;
  onSave?: () => void;
  children: ReactNode;
}

function Drawer({ open, title, onClose, onSave, children }: DrawerProps) {
  return (
    <div className={`drawer-root${open ? ' open' : ''}`}>
      <div className="drawer-overlay" onClick={onClose} />
      <div className="drawer-panel">
        <div className="drawer-header">
          <span className="drawer-title">{title}</span>
          <span className="drawer-close" onClick={onClose}>✕</span>
        </div>
        <div className="drawer-body">{children}</div>
        <div className="drawer-footer">
          <button className="ant-btn" onClick={onClose}>取消</button>
          {onSave && <button className="ant-btn ant-btn-primary" onClick={onSave}>保存</button>}
        </div>
      </div>
    </div>
  );
}

export default Drawer;
