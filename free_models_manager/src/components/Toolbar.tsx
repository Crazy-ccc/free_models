import type { ReactNode } from 'react';
import './Toolbar.less';

interface ToolbarProps {
  title: string;
  children?: ReactNode;
  showSearch?: boolean;
  searchValue?: string;
  onSearchChange?: (value: string) => void;
}

function Toolbar({ title, children, showSearch, searchValue, onSearchChange }: ToolbarProps) {
  return (
    <div className="toolbar">
      <h2 className="toolbar-title doodle-squiggle">{title}</h2>
      <div className="toolbar-spacer" />
      <div className="toolbar-actions">
        {showSearch && (
          <input
            className="toolbar-search"
            placeholder="搜索..."
            value={searchValue ?? ''}
            onChange={(e) => onSearchChange?.(e.target.value)}
          />
        )}
        {children}
      </div>
    </div>
  );
}

export default Toolbar;
