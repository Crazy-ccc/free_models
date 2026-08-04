import { useRef } from 'react';
import type { ChangeEvent, CSSProperties } from 'react';
import './DoodleInput.less';

export interface DoodleInputProps {
  value?: string | number;
  onChange?: (e: ChangeEvent<HTMLInputElement>) => void;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  allowClear?: boolean;
  size?: 'small' | 'middle';
  className?: string;
  style?: CSSProperties;
}

function DoodleInput({
  value,
  onChange,
  placeholder,
  type = 'text',
  disabled,
  allowClear,
  size = 'middle',
  className,
  style,
}: DoodleInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  const handleClear = () => {
    if (!onChange) return;
    const syntheticEvent = {
      target: { value: '' },
      currentTarget: { value: '' },
      preventDefault: () => {},
      stopPropagation: () => {},
    } as React.ChangeEvent<HTMLInputElement>;
    onChange(syntheticEvent);
  };

  const hasValue = value !== undefined && value !== null && String(value) !== '';

  const classes = [
    'doodle-input',
    size === 'small' ? 'doodle-input-small' : '',
    allowClear && hasValue ? 'doodle-input-clearable' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <span className={className ? `${classes} ${className}` : classes} style={style}>
      <input
        ref={inputRef}
        type={type}
        value={value}
        onChange={onChange}
        placeholder={placeholder}
        disabled={disabled}
      />
      {allowClear && hasValue && !disabled && (
        <span className="doodle-input-clear" onClick={handleClear} aria-label="clear">
          ✕
        </span>
      )}
    </span>
  );
}

export default DoodleInput;
