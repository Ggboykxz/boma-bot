import Link from "next/link";

/**
 * Root screen (spec/08-ui-ux.md §8.1). Sign-in redirects to `/app` once sessions exist (M1);
 * at M0 this page is a static landmark that the shell and axe suites can rely on.
 */
export default function HomePage() {
  return (
    <main className="mx-auto flex min-h-dvh max-w-2xl flex-col justify-center gap-6 px-6 py-16">
      <p className="text-text-secondary text-body-strong">Boma Bot</p>
      <h1 className="text-title-2-medium text-text-primary">
        Persistent AI teammates, on hardware you own.
      </h1>
      <p className="text-text-secondary text-body-medium">
        This build carries the shell only. The thread, the run timeline and the computer panel
        arrive milestone by milestone; the API they talk to is already specified.
      </p>
      <p>
        <Link href="/app/" className="text-accent text-body-strong underline underline-offset-4">
          Open the app shell
        </Link>
      </p>
    </main>
  );
}
