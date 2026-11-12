import { ActivityScreen } from "@/components/activity/activity-screen";
import { AppShell } from "@/components/shell/app-shell";

/**
 * The activity view (spec/08-ui-ux.md §8.4, REQ-011).
 *
 * The shell owns the landmarks; the screen owns the list of runs.
 */
export default function ActivityPage() {
  return (
    <AppShell>
      <ActivityScreen />
    </AppShell>
  );
}
