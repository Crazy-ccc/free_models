import type { CSSProperties, MouseEventHandler, ReactNode } from 'react';
import './DoodleButton.less';

export interface DoodleButtonProps {
  type?: 'primary' | 'teal' | 'danger' | 'ghost' | 'default';
  size?: 'small' | 'middle';
  disabled?: boolean;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  className?: string;
  children?: ReactNode;
  style?: CSSProperties;
  htmlType?: 'button' | 'submit' | 'reset';
}

function DoodleButton({
  type = 'default',
  size = 'middle',
  disabled,
  onClick,
  className,
  children,
  style,
  htmlType = 'button',
}: DoodleButtonProps) {
  const classes = ['doodle-btn', `doodle-btn-${type}`, size === 'small' ? 'doodle-btn-small' : '']
    .filter(Boolean)
    .join(' ');
  return (
    <button
      type={htmlType}
      className={className ? `${classes} ${className}` : classes}
      style={style}
      disabled={disabled}
      onClick={disabled ? undefined : onClick}
    >
      {children}
    </button>
  );
}

export default DoodleButton;
