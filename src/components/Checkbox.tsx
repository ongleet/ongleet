import type { ComponentProps } from 'react';
import './checkbox.css';

export function Checkbox({ className = '', ...props }: Omit<ComponentProps<'input'>, 'type'>) {
  return <input {...props} type="checkbox" className={`ui-checkbox ${className}`.trim()} />;
}
