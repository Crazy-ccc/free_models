import type { CSSProperties, ReactNode } from 'react';
import DoodleCheckboxGroup from './DoodleCheckboxGroup';
import './DoodleCheckbox.less';

export interface DoodleCheckboxProps {
  checked?: boolean;
  indeterminate?: boolean;
  disabled?: boolean;
  onChange?: (checked: boolean) => void;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

function DoodleCheckbox({
  checked,
  indeterminate,
  disabled,
  onChange,
  children,
  className,
  style,
}: DoodleCheckboxProps) {
  const isChecked = !!checked || !!indeterminate;
  const classes = [
    'doodle-checkbox',
    isChecked ? 'doodle-checkbox-checked' : '',
    indeterminate ? 'doodle-checkbox-indeterminate' : '',
    disabled ? 'doodle-checkbox-disabled' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <label
      className={className ? `${classes} ${className}` : classes}
      style={style}
      onClick={() => {
        if (disabled) return;
        onChange?.(!checked);
      }}
    >
      <span className="doodle-checkbox-box">
        {indeterminate ? (
          <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
            <path d="M4.5 9.5 H13.5" fill="none" stroke="#FFFFFF" strokeWidth="2.5" strokeLinecap="round" />
          </svg>
        ) : checked ? (
          <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
            <path d="M4 9.5 L7.5 13 L14 5.5" fill="none" stroke="#FFFFFF" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : null}
      </span>
      {children != null && <span className="doodle-checkbox-label">{children}</span>}
    </label>
  );
}

export default DoodleCheckbox;

namespace DoodleCheckbox {
  export const Group = DoodleCheckboxGroup;
}