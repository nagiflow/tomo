import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktop } from "../services/api";
export function Mascot({
  size = 32,
  mood = "waiting",
}: {
  size?: number;
  mood?: "waiting" | "happy" | "sleeping" | "working";
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      shapeRendering="crispEdges"
      role="img"
      aria-label={`Tomo ${mood}`}
    >
      <path
        fill="currentColor"
        d="M7 1h1v2H7zM5 3h6v1H5zM3 4h2v1H3zM11 4h2v1h-2zM2 5h1v2H2zM13 5h1v2h-1zM1 7h1v4H1zM14 7h1v4h-1zM2 11h1v1H2zM13 11h1v1h-1zM3 12h2v1H3zM11 12h2v1h-2zM4 13h8v1H4zM4 14h2v1H4zM10 14h2v1h-2z"
      />
      <path
        fill="var(--mascot-fill, white)"
        d="M5 4h6v1H5zM3 5h10v2H3zM2 7h12v4H2zM3 11h10v1H3zM5 12h6v1H5z"
      />
      <path
        fill="currentColor"
        d={
          mood === "sleeping"
            ? "M5 7h2v1H5zM9 7h2v1H9zM7 10h2v1H7z"
            : mood === "happy"
              ? "M5 6h2v2H5zM9 6h2v2H9zM5 9h1v1H5zM10 9h1v1h-1zM6 10h4v1H6z"
              : mood === "working"
                ? // ・v・ : awake and content
                  "M3 7h2v2H3zM11 7h2v2h-2zM6 9h1v1H6zM9 9h1v1H9zM7 10h2v1H7z"
                : "M5 6h2v2H5zM9 6h2v2H9zM7 10h2v1H7z"
        }
      />
    </svg>
  );
}
export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`switch ${checked ? "on" : ""}`}
    >
      <span />
    </button>
  );
}
export function Modal({
  open,
  onClose,
  title,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal">
          <div className="modal-heading">
            <Dialog.Title>{title}</Dialog.Title>
            <Dialog.Close className="icon-button" aria-label="Close dialog">
              <X size={18} />
            </Dialog.Close>
          </div>
          <Dialog.Description className="sr-only">
            Review the details below.
          </Dialog.Description>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export function TrafficLights() {
  return (
    <div className="traffic" data-tauri-drag-region>
      <button
        aria-label="Hide window"
        title="Hide window"
        onClick={() => desktop && getCurrentWindow().hide()}
        className="red"
      />
      <button
        aria-label="Minimize window"
        title="Minimize window"
        onClick={() => desktop && getCurrentWindow().minimize()}
        className="yellow"
      />
      <button
        aria-label="Maximize window"
        title="Maximize window"
        onClick={() => desktop && getCurrentWindow().toggleMaximize()}
        className="green"
      />
    </div>
  );
}
export function Empty({
  title,
  children,
  paused = false,
}: {
  title: string;
  children: ReactNode;
  paused?: boolean;
}) {
  return (
    <div className="empty">
      <Mascot size={64} mood={paused ? "sleeping" : "working"} />
      <h2>{title}</h2>
      <p>{children}</p>
    </div>
  );
}
export const time = (timestamp: string) =>
  new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestamp));
