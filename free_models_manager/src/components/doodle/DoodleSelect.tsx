import { useEffect, useRef, useState } from 'react';
import type { CSSProperties, MouseEvent } from 'react';
import './DoodleSelect.less';

export interface DoodleSelectOption {
  label: string;
  value: string;
}

export interface DoodleSelectProps {
  value?: string;
  onChange?: (v: string) => void;
  placeholder?: string;
  options?: DoodleSelectOption[];
  disabled?: boolean;
  showSearch?: boolean;
  size?: 'small' | 'middle';
  className?: string;
  style?: CSSProperties;
  allowClear?: boolean;
}

function DoodleSelect({
  value,
  onChange,
  placeholder,
  options = [],
  disabled,
  showSearch,
  size = 'middle',
  className,
  style,
  allowClear,
}: DoodleSelectProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: globalThis.MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('click', onDocClick);
    return () => document.removeEventListener('click', onDocClick);
  }, [open]);

  const selected = options.find((o) => o.value === value);
  const filtered = showSearch && search ? options.filter((o) => String(o.label).includes(search)) : options;

  const toggleOpen = (e: MouseEvent) => {
    e.stopPropagation();
    if (disabled) return;
    setOpen((v) => !v);
  };

  const pick = (e: MouseEvent, opt: DoodleSelectOption) => {
    e.stopPropagation();
    onChange?.(opt.value);
    setOpen(false);
    setSearch('');
  };

  const clear = (e: MouseEvent) => {
    e.stopPropagation();
    onChange?.('');
    setOpen(false);
    setSearch('');
  };

  const displayLabel = selected ? selected.label : value !== undefined ? value : '';
  const canClear = allowClear && value !== undefined && !disabled;

  const classes = ['doodle-select', size === 'small' ? 'doodle-select-small' : '', open ? 'doodle-select-open' : '']
    .filter(Boolean)
    .join(' ');

  return (
    <div ref={rootRef} className={className ? `${classes} ${className}` : classes} style={style}>
      <div className="doodle-select-trigger" onClick={toggleOpen}>
        <span className={`doodle-select-value${selected === undefined ? ' doodle-select-placeholder' : ''}`}>
          {selected === undefined && value === undefined ? (placeholder ?? '') : displayLabel}
        </span>
        {canClear && <span className="doodle-select-clear" onClick={clear}>✕</span>}
        <span className="doodle-select-arrow">▼</span>
      </div>
      {open && (
        <div className="doodle-select-panel">
          {showSearch && (
            <input
              className="doodle-select-search"
              autoFocus
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onClick={(e) => e.stopPropagation()}
              placeholder="搜索…"
            />
          )}
          <div className="doodle-select-list">
            {filtered.length === 0 && <div className="doodle-select-empty">无匹配项</div>}
            {filtered.map((opt) => {
              const isSelected = opt.value === value;
              return (
                <div
                  key={String(opt.value)}
                  className={`doodle-select-option${isSelected ? ' selected' : ''}`}
                  onClick={(e) => pick(e, opt)}
                >
                  <span className="doodle-select-option-label">{String(opt.label)}</span>
                  {isSelected && <span className="doodle-select-check">✓</span>}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

export default DoodleSelect;
