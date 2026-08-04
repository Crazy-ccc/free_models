import type { CSSProperties } from 'react';
import DoodleCheckbox from './DoodleCheckbox';
import './DoodleCheckboxGroup.less';

export interface DoodleCheckboxGroupOption {
  label: string;
  value: string;
}

export interface DoodleCheckboxGroupProps {
  value?: string[];
  onChange?: (checked: string[]) => void;
  options?: DoodleCheckboxGroupOption[];
  disabled?: boolean;
  className?: string;
  style?: CSSProperties;
}

function DoodleCheckboxGroup({
  value = [],
  onChange,
  options = [],
  disabled,
  className,
  style,
}: DoodleCheckboxGroupProps) {
  const handleToggle = (opt: DoodleCheckboxGroupOption, checked: boolean) => {
    const next = checked ? [...value, opt.value] : value.filter((v) => v !== opt.value);
    onChange?.(next);
  };

  return (
    <div className={className ? `doodle-checkbox-group ${className}` : 'doodle-checkbox-group'} style={style}>
      {options.map((opt) => (
        <DoodleCheckbox
          key={String(opt.value)}
          checked={value.includes(opt.value)}
          disabled={disabled}
          onChange={(checked) => handleToggle(opt, checked)}
        >
          {opt.label}
        </DoodleCheckbox>
      ))}
    </div>
  );
}

export default DoodleCheckboxGroup;