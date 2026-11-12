import Link from "next/link";

const NAV = [
  { href: "/app/", label: "Threads", current: true },
  { href: "/app/", label: "Activity", current: false },
  { href: "/app/", label: "Settings", current: false },
] as const;

/**
 * The application shell (spec/08-ui-ux.md §8.1). Navigation and `main` are the landmarks the UI
 * goldens assert on; the seeded screens replace the placeholder inside `main` from M1.
 */
export default function AppShellPage() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-separator-border bg-background-subtle border-b">
        <nav aria-label="Primary" className="mx-auto flex w-full max-w-5xl gap-2 px-4 py-2">
          {NAV.map((item) => (
            <Link
              key={item.label}
              href={item.href}
              aria-current={item.current ? "page" : undefined}
              className="text-text-secondary hover:text-text-primary rounded-md px-3"
            >
              {item.label}
            </Link>
          ))}
        </nav>
      </header>
      <main className="mx-auto flex w-full max-w-5xl flex-1 flex-col gap-3 px-4 py-10">
        <h1 className="text-headline-medium text-text-primary">Threads</h1>
        <p className="text-text-secondary max-w-prose">
          The thread screen arrives with milestone M1. Until then this shell proves the layout, the
          landmarks and the token layer that the golden fixtures will assert against.
        </p>
      </main>
    </div>
  );
}
