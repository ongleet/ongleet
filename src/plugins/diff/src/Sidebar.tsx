import type { ReactNode } from 'react';
import { FileSidebar } from '../../../components/FileSidebar';

/** Use the shared file sidebar layout for comparison navigation. */
export function DiffSidebar({ children }: { children: ReactNode }) {
  return (
    <FileSidebar storageKey="ongleet.diff.sidebarWidth" label="Diff files" className="diff-sidebar">
      {children}
    </FileSidebar>
  );
}
