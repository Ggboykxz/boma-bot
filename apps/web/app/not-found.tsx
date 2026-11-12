import Link from "next/link";

/** The `*` route of spec/08-ui-ux.md §8.1. One heading, one way back, no hint about what was not found. */
export default function NotFound() {
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-4 px-6 py-16">
      <h1 className="text-title-2-medium">Page not found</h1>
      <p className="text-text-secondary">
        This address does not match a screen. It may have moved, or it may never have existed.
      </p>
      <p>
        <Link href="/" className="text-accent underline underline-offset-4">
          Back to the start
        </Link>
      </p>
    </main>
  );
}
