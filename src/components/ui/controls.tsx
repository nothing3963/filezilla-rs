import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "danger" | "subtle";
  size?: "default" | "sm";
  icon?: ReactNode;
}

export function Button({
  variant = "default",
  size = "default",
  icon,
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      className={cn("btn", variant !== "default" && variant, size === "sm" && "sm", className)}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  active?: boolean;
  size?: number;
}

export function IconButton({ label, active, className, children, size, ...rest }: IconButtonProps) {
  return (
    <button
      className={cn("icon-btn", active && "active", className)}
      title={label}
      aria-label={label}
      style={size ? { width: size, height: size } : undefined}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
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
      className={cn("switch", checked && "on")}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className="check-row"
      onClick={() => onChange(!checked)}
    >
      <span className={cn("check-box", checked && "checked")}>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3.4">
          <path d="M20 6 9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
      <span>{label}</span>
    </button>
  );
}
