import type { ReactNode } from 'react';
import './DoodleEmpty.less';

export interface DoodleEmptyProps {
  description?: ReactNode;
}

function DoodleEmpty({ description }: DoodleEmptyProps) {
  return (
    <div className="doodle-empty">
      <div className="doodle-empty-icon" aria-hidden="true">
        📭
      </div>
      {description != null && <div className="doodle-empty-text">{description}</div>}
    </div>
  );
}

export default DoodleEmpty;