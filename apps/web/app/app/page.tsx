import { AppEntry } from "@/components/shell/app-entry";
import { AppShell } from "@/components/shell/app-shell";

/**
 * The threads home (spec/08-ui-ux.md §8.1).
 *
 * The screen lists the bots of the space; the shell's rail carries them. M1 opens on the
 * placeholder: the bot list procedure is not part of the wire contract yet.
 */
export default function ThreadsHomePage() {
  return (
    <AppShell>
      <AppEntry />
    </AppShell>
  );
}
