import { AppShell } from "@/components/shell/app-shell";
import { GroupThreadScreen } from "@/components/group/group-thread-screen";

/**
 * `/app/g/<groupId>` (spec/08-ui-ux.md §8.3, REQ-010).
 *
 * The group room. One shared thread per group; several bots joined to it. The static export
 * carries one shell per group id; the id is read from the page at render time.
 */
export function generateStaticParams(): Array<{ groupId: string }> {
  return [{ groupId: "group_golden" }];
}

export default async function GroupPage({
  params,
}: {
  params: Promise<{ groupId: string }>;
}) {
  const { groupId } = await params;
  return (
    <AppShell>
      <GroupThreadScreen groupId={groupId} />
    </AppShell>
  );
}
