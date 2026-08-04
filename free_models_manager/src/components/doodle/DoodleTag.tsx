import type { CSSProperties, ReactNode } from 'react';
import './DoodleTag.less';

export type DoodleTagColor =
  | 'available'
  | 'unavailable'
  | 'deprecated'
  | 'success'
  | 'error'
  | 'warning'
  | 'info'
  | 'purple'
  | 'cyan'
  | 'orange'
  | 'geekblue'
  | 'default';

export interface DoodleTagProps {
  color?: DoodleTagColor;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

function DoodleTag({ color = 'default', children, className, style }: DoodleTagProps) {
  const classes = ['doodle-tag', `doodle-tag-${color}`].filter(Boolean).join(' ');
  return <span className={className ? `${classes} ${className}` : classes} style={style}>{children}</span>;
}

export default DoodleTag;
