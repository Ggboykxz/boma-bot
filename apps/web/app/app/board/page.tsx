import { AppShell } from "@/components/shell/app-shell";
import { BoardScreen } from "@/components/board/board-screen";

/**
 * `/app/board` (spec/08-ui-ux.md §8.6, REQ-038).
 *
 * Flag-gated at the deployment (`BOMA_BOARD`): when the flag is off, the screen renders the
 * not-found screen rather than a locked one — the spec says the route is absent, not closed.
 */
export default function BoardPage() {
  return (
    <AppShell>
      <BoardScreen />
    </AppShell>
  );
}
