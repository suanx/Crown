import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

export function PanelTitle({
  title,
  description,
}: {
  title: string;
  description?: string;
}) {
  return (
    <div className="mb-6">
      <h1 className="text-xl font-semibold text-text-primary">{title}</h1>
      {description && (
        <p className="mt-1 text-sm text-text-secondary">{description}</p>
      )}
    </div>
  );
}

export function Section({
  title,
  children,
  className,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("mb-8", className)}>
      {title && (
        <h2 className="text-sm font-semibold text-text-primary mb-3">
          {title}
        </h2>
      )}
      <div className="rounded-lg border border-border-subtle bg-elevated divide-y divide-border-subtle">
        {children}
      </div>
    </section>
  );
}

export function Row({
  label,
  description,
  control,
}: {
  label: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div className="px-4 py-3 flex items-start gap-4">
      <div className="flex-1 min-w-0">
        <div className="text-sm text-text-primary font-medium">{label}</div>
        {description && (
          <div className="text-xs text-text-tertiary mt-0.5 leading-snug">
            {description}
          </div>
        )}
      </div>
      <div className="shrink-0 mt-0.5">{control}</div>
    </div>
  );
}
