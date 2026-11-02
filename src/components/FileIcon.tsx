type FileIconProps = {
  status?: string;
  statusLabel?: string;
};

export function FileIcon({ status, statusLabel }: FileIconProps) {
  if (status) {
    return (
      <span
        className="file-icon diff-status"
        data-status={status}
        aria-label={statusLabel ?? status}
      >
        {status}
      </span>
    );
  }
  return (
    <svg
      className="file-icon"
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M14 3H5v18h14V8l-5-5Z" />
      <path d="M14 3v5h5M8 12h8M8 16h6" />
    </svg>
  );
}
