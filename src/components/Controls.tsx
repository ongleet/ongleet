import type { ComponentProps } from 'react';

/** Native controls preserve browser semantics, form submission and React 19 refs.
 * Keep shared appearance in style.css; contextual classes may adjust layout.
 */
export function Button(props: ComponentProps<'button'>) {
  return <button {...props} />;
}

/** Use Checkbox for checkbox inputs. */
export function Input(props: ComponentProps<'input'>) {
  return <input {...props} />;
}

export function Select(props: ComponentProps<'select'>) {
  return <select {...props} />;
}

export function Textarea(props: ComponentProps<'textarea'>) {
  return <textarea {...props} />;
}

export function Dialog(props: ComponentProps<'dialog'>) {
  return <dialog {...props} />;
}

export function Disclosure(props: ComponentProps<'details'>) {
  return <details {...props} />;
}

export function DisclosureTrigger(props: ComponentProps<'summary'>) {
  return <summary {...props} />;
}

export function TabList(props: ComponentProps<'div'>) {
  return <div {...props} role="tablist" />;
}

export function Tab(props: ComponentProps<'button'>) {
  return (
    <Button
      type="button"
      tabIndex={props['aria-selected'] === true ? 0 : -1}
      {...props}
      role="tab"
    />
  );
}

export function TabPanel(props: ComponentProps<'section'>) {
  return <section {...props} role="tabpanel" />;
}
