import type { ReactNode } from 'react';
import { FileSidebar } from '../../components/FileSidebar';

export function DiffSidebar({ children }: { children: ReactNode }) {
  return (
    <FileSidebar storageKey="ongleet.diff.sidebarWidth" label="Diff files" className="diff-sidebar">
      {children}
    </FileSidebar>
  );
}
