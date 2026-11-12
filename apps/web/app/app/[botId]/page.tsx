import { AppShell } from "@/components/shell/app-shell";
import { ThreadScreen } from "@/components/thread/thread-screen";

/**
 * The thread of one bot (spec/08-ui-ux.md §8.1, §8.3).
 *
 * The page is a server component: it renders the shell and hands the bot's id to the screen, and
 * the screen — a client component — reads the transcript and keeps it live.
 */

/**
 * The export carries one shell for the whole route.
 *
 * The screen reads the bot's id from the URL, so the same file serves every bot: the static
 * handler maps `/app/<botId>/` to the shell of this route (ADR-013).
 */
export function generateStaticParams(): Array<{ botId: string }> {
  // `bot_golden` is the id the UI goldens pin (`spec/golden/ui/REQ-001-send-message.json`): the
  // development server refuses an unknown param outright, and the static export needs the file to
  // exist. Every other bot is served by the static handler's `/app/*` fallback (ADR-013).
  return [{ botId: "bot" }, { botId: "bot_golden" }];
}

export default async function ThreadPage({
  params,
}: {
  params: Promise<{ botId: string }>;
}) {
  const { botId } = await params;

  return (
    <AppShell>
      <ThreadScreen botId={botId} />
    </AppShell>
  );
}
