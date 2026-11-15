import Link from "next/link";
import type { ReactNode } from "react";

import { useTranslation } from "@/lib/use-translation";

export interface AuthShellProps {
  title: string;
  subtitle?: string;
  children: ReactNode;
}

/**
 * The one frame for every public auth screen: one column, the mark, the wordmark, and no
 * navigation chrome — the routes are entry points, not places to wander.
 */
export function AuthShell({ title, subtitle, children }: AuthShellProps) {
  const t = useTranslation();

  return (
    <div className="bg-background-full flex min-h-dvh flex-col items-center justify-center gap-8 px-4 py-10">
      <Link
        href="/"
        className="boma-target boma-focus text-text-primary hover:text-text-secondary text-headline-medium inline-flex items-center rounded-md px-2 py-1"
      >
        {t("app.name")}
      </Link>
      <main className="flex w-full max-w-sm flex-col gap-6">
        <header className="flex flex-col gap-1 text-center">
          <h1 className="text-title-3-semibold text-text-primary">{title}</h1>
          {subtitle ? <p className="text-text-secondary text-body-regular">{subtitle}</p> : null}
        </header>
        <div className="border-border-button-default bg-background-primary-default flex flex-col gap-4 rounded-3xl border p-6">
          {children}
        </div>
      </main>
    </div>
  );
}
