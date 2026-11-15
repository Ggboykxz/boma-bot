import { AppShell } from "@/components/shell/app-shell";
import { ArtifactsGrid } from "@/components/artifacts/artifacts-grid";

/**
 * `/app/artifacts` (spec/08-ui-ux.md §8.5, REQ-036).
 *
 * The list of everything the space produced, newest families first in the client. The grid
 * talks to the RPC only; the shell around it is the application itself.
 */
export default function ArtifactsPage() {
  return (
    <AppShell>
      <ArtifactsGrid />
    </AppShell>
  );
}
