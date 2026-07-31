import type { CSSProperties, ReactNode } from "react";

interface IconProps {
  color?: string;
  size?: number;
  className?: string;
  style?: CSSProperties;
}

function Icon({ color = "currentColor", size = 16, className, style, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      style={{ display: "block", flexShrink: 0, ...style }}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function FolderIcon(props: IconProps) {
  return <Icon {...props}><path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" /></Icon>;
}

export function FilterIcon(props: IconProps) {
  return <Icon {...props}><path d="M4 5h16l-6.2 7.1v5.1l-3.6 1.8v-6.9z" /></Icon>;
}

export function SettingsIcon(props: IconProps) {
  return <Icon {...props}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.1h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H3v-4h.1a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V3h4v.1a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.1v4H21a1.7 1.7 0 0 0-1.6 1z" /></Icon>;
}

export function SidebarIcon({ collapsed, ...props }: IconProps & { collapsed: boolean }) {
  return <Icon {...props} style={{ transform: collapsed ? "rotate(180deg)" : undefined, transition: "transform 180ms ease", ...props.style }}><rect x="3.5" y="4" width="17" height="16" rx="3" /><path d="M9 4v16" /><path d="m14 9 3 3-3 3" /></Icon>;
}

export function MoonIcon(props: IconProps) {
  return <Icon {...props}><path d="M20.5 14.1A8.5 8.5 0 0 1 9.9 3.5 8.5 8.5 0 1 0 20.5 14z" /></Icon>;
}

export function SunIcon(props: IconProps) {
  return <Icon {...props}><circle cx="12" cy="12" r="3.8" /><path d="M12 2.5v2M12 19.5v2M4.5 4.5l1.4 1.4M18.1 18.1l1.4 1.4M2.5 12h2M19.5 12h2M4.5 19.5l1.4-1.4M18.1 5.9l1.4-1.4" /></Icon>;
}

export function ExternalLinkIcon(props: IconProps) {
  return <Icon {...props}><path d="M13 5H6.5a2 2 0 0 0-2 2v10.5a2 2 0 0 0 2 2H17a2 2 0 0 0 2-2V11" /><path d="M14 4.5h5.5V10" /><path d="m11 13 8.2-8.2" /></Icon>;
}

export function LogoutIcon(props: IconProps) {
  return <Icon {...props}><path d="M10 5H6.5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2H10" /><path d="M14 8.5 17.5 12 14 15.5M17 12H9" /></Icon>;
}

export function ImportIcon({ spinning = false, ...props }: IconProps & { spinning?: boolean }) {
  return <Icon {...props} style={{ animation: spinning ? "spin .85s linear infinite" : undefined, ...props.style }}><path d="M5 15.5v2a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-2" /><path d="M12 4.5v10M8.5 8 12 4.5 15.5 8" /></Icon>;
}

export function ExportIcon({ spinning = false, ...props }: IconProps & { spinning?: boolean }) {
  return <Icon {...props} style={{ animation: spinning ? "spin .85s linear infinite" : undefined, ...props.style }}><path d="M5 15.5v2a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-2" /><path d="M12 14.5v-10M8.5 11 12 14.5l3.5-3.5" /></Icon>;
}

export function TrashIcon(props: IconProps) {
  return <Icon {...props}><path d="M5 7h14M9 7V4.5h6V7M7 7l.7 12h8.6L17 7M10 10.5v5M14 10.5v5" /></Icon>;
}

export function CopyIcon(props: IconProps) {
  return <Icon {...props}><rect x="8.5" y="8.5" width="11" height="11" rx="2" /><path d="M15.5 8.5v-2a2 2 0 0 0-2-2h-7a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2" /></Icon>;
}

export function CheckIcon(props: IconProps) {
  return <Icon {...props}><path d="m5 12.5 4.3 4.2L19 7" /></Icon>;
}

export function SearchIcon(props: IconProps) {
  return <Icon {...props}><circle cx="10.8" cy="10.8" r="6.3" /><path d="m15.6 15.6 4 4" /></Icon>;
}

export function MessageIcon(props: IconProps) {
  return <Icon {...props}><path d="M5.5 5.5h13a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2H10l-5.5 3v-12a2 2 0 0 1 2-2z" /><path d="M8.5 10h7M8.5 13h4.5" /></Icon>;
}

export function ChevronRightIcon(props: IconProps) {
  return <Icon {...props}><path d="m9.5 5.5 6.5 6.5-6.5 6.5" /></Icon>;
}

export function DocIcon(props: IconProps) {
  return <Icon {...props}><path d="M7 3.5h7l4 4V20H7a2 2 0 0 1-2-2V5.5a2 2 0 0 1 2-2z" /><path d="M14 3.5v4h4M8.5 12h7M8.5 15.5h5" /></Icon>;
}

export function CloseIcon(props: IconProps) {
  return <Icon {...props}><path d="m6.5 6.5 11 11M17.5 6.5l-11 11" /></Icon>;
}

export function GlobeIcon(props: IconProps) {
  return <Icon {...props}><circle cx="12" cy="12" r="8.5" /><path d="M3.5 12h17M12 3.5c2.2 2.3 3.3 5.1 3.3 8.5S14.2 18.2 12 20.5C9.8 18.2 8.7 15.4 8.7 12S9.8 5.8 12 3.5z" /></Icon>;
}

export function SparkIcon(props: IconProps) {
  return <Icon {...props}><path d="m12 3 1.5 6.3L20 11l-6.5 1.7L12 19l-1.5-6.3L4 11l6.5-1.7z" /></Icon>;
}

export function SyncIcon({ spinning, small = false, ...props }: IconProps & { spinning: boolean; small?: boolean }) {
  return <Icon {...props} size={small ? 12 : (props.size ?? 16)} style={{ animation: spinning ? "spin .85s linear infinite" : undefined, ...props.style }}><path d="M19 8a7.5 7.5 0 0 0-12.6-2L4 8.5" /><path d="M4 4.5v4h4" /><path d="M5 16a7.5 7.5 0 0 0 12.6 2l2.4-2.5" /><path d="M20 19.5v-4h-4" /></Icon>;
}

export function SpinnerIcon(props: IconProps) {
  return <Icon {...props} size={props.size ?? 18} style={{ animation: "spin .85s linear infinite", ...props.style }}><path d="M20 12a8 8 0 1 1-2.3-5.7" /></Icon>;
}

export function LockIcon(props: IconProps) {
  return <Icon {...props}><rect x="5" y="10" width="14" height="10" rx="2.5" /><path d="M8.5 10V7.5a3.5 3.5 0 0 1 7 0V10M12 14v2" /></Icon>;
}

export function StorageIcon(props: IconProps) {
  return <Icon {...props}><ellipse cx="12" cy="6" rx="7.5" ry="3" /><path d="M4.5 6v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3V6M4.5 12v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6" /></Icon>;
}

export function AppWindowIcon(props: IconProps) {
  return <Icon {...props}><rect x="3.5" y="4" width="17" height="16" rx="3" /><path d="M3.5 8.5h17M7 6.3h.1M10 6.3h.1" /></Icon>;
}

export function AppearanceIcon(props: IconProps) {
  return <Icon {...props}><path d="M12 3.5a8.5 8.5 0 1 0 0 17h1.1a1.9 1.9 0 0 0 0-3.8h-.8a1.8 1.8 0 0 1 0-3.6H16a4.5 4.5 0 0 0 0-9z" /><circle cx="7.5" cy="10" r=".7" fill="currentColor" stroke="none" /><circle cx="9.5" cy="6.8" r=".7" fill="currentColor" stroke="none" /><circle cx="14" cy="6.5" r=".7" fill="currentColor" stroke="none" /></Icon>;
}

export function InfoIcon(props: IconProps) {
  return <Icon {...props}><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5M12 8h.01" /></Icon>;
}

export function ImageIcon(props: IconProps) {
  return <Icon {...props}><rect x="3.5" y="4.5" width="17" height="15" rx="2.5" /><circle cx="9" cy="9.5" r="1.5" /><path d="m5.5 17 4.2-4 3.1 2.7 2.7-2.4 3 3" /></Icon>;
}

export function VideoIcon(props: IconProps) {
  return <Icon {...props}><rect x="3.5" y="6" width="12.5" height="12" rx="2.5" /><path d="m16 10 4-2v8l-4-2z" /></Icon>;
}

export function PdfIcon(props: IconProps) {
  return <Icon {...props}><path d="M7 3.5h7l4 4V20H7a2 2 0 0 1-2-2V5.5a2 2 0 0 1 2-2z" /><path d="M14 3.5v4h4M8.5 12.5h7M8.5 16h4" /></Icon>;
}
