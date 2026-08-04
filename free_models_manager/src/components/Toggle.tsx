import './Toggle.less';

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

function Toggle({ checked, onChange }: ToggleProps) {
  return (
    <div
      className={`toggle${checked ? ' on' : ''}`}
      onClick={(e) => { e.stopPropagation(); onChange(!checked); }}
    >
      <div className="toggle-handle" />
    </div>
  );
}

export default Toggle;
