import { AppShell } from "@/components/shell/app-shell";
import { ArtifactViewer } from "@/components/artifacts/artifact-viewer";

/**
 * `/app/artifacts/<artifactId>` (spec/08-ui-ux.md §8.5, REQ-036).
 *
 * The static export carries one shell for the `/app/artifacts/<artifactId>` subtree (ADR-013):
 * the id is read from the location, so any artifact id works with a single file. The Rust
 * static handler maps an unknown path under the subtree to this page's shell.
 */
export function generateStaticParams(): Array<{ artifactId: string }> {
  // `artifact_golden` is the placeholder the UI goldens and the fallback resolver point at;
  // the viewer always reads the id from the URL instead.
  return [{ artifactId: "artifact_golden" }];
}

export default function ArtifactViewerPage() {
  return (
    <AppShell>
      <ArtifactViewer />
    </AppShell>
  );
}
