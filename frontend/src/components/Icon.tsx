type IconName =
  | 'alert'
  | 'chart'
  | 'check'
  | 'chevron-up'
  | 'download'
  | 'file'
  | 'help'
  | 'key'
  | 'moon'
  | 'play'
  | 'settings'
  | 'sigma'
  | 'star'
  | 'sun'
  | 'table';

interface IconProps {
  name: IconName;
  className?: string;
}

const PATHS: Record<IconName, string> = {
  alert: 'M12 9v4m0 4h.01M10.3 4.3 2.7 17.5A2 2 0 0 0 4.4 20h15.2a2 2 0 0 0 1.7-2.5L13.7 4.3a2 2 0 0 0-3.4 0Z',
  chart: 'M4 19V5m0 14h16M8 16v-5m4 5V8m4 8v-7',
  check: 'm5 12 4 4L19 6',
  'chevron-up': 'm6 14 6-6 6 6',
  download: 'M12 3v11m0 0 4-4m-4 4-4-4M5 19h14',
  file: 'M7 3h7l4 4v14H7V3Zm7 0v5h5',
  help: 'M12 18h.01M9.5 9a2.6 2.6 0 1 1 4.2 2c-1.1.8-1.7 1.4-1.7 2.5M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z',
  key: 'M15 7a4 4 0 1 0-2.8 6.8L10 16H8v2H6v2H4v-2l6.2-6.2A4 4 0 0 0 15 7Zm0 0h.01',
  moon: 'M20 14.5A7.5 7.5 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z',
  play: 'M8 5v14l11-7L8 5Z',
  settings: 'M12 15.5A3.5 3.5 0 1 0 12 8a3.5 3.5 0 0 0 0 7.5ZM19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.4 1a7 7 0 0 0-1.7-1L14.5 3h-5l-.3 3a7 7 0 0 0-1.7 1L5 6 3 9.5 5.1 11a7 7 0 0 0 0 2L3 14.5 5 18l2.5-1a7 7 0 0 0 1.7 1l.3 3h5l.3-3a7 7 0 0 0 1.7-1l2.5 1 2-3.5-2.1-1.5c.1-.3.1-.7.1-1Z',
  sigma: 'M18 5H7l6 7-6 7h11',
  star: 'm12 3 2.7 5.5 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.8 1-6.1-4.4-4.3 6.1-.9L12 3Z',
  sun: 'M12 4V2m0 20v-2m8-8h2M2 12h2m14.4-6.4 1.4-1.4M4.2 19.8l1.4-1.4m0-12.8L4.2 4.2m15.6 15.6-1.4-1.4M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z',
  table: 'M4 5h16v14H4V5Zm0 5h16M9 5v14',
};

export default function Icon({ name, className }: IconProps) {
  return (
    <svg
      className={`icon${className ? ` ${className}` : ''}`}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
