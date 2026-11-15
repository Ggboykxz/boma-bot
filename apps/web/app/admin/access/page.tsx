import { AccessScreen } from "@/components/admin/access-screen";

/**
 * `/admin/access` (spec/08-ui-ux.md §8.7, REQ-058).
 *
 * The spec does not route admin surfaces through the conversation shell: they are their own
 * screens, reachable by space admins only. The server refuses every call for anyone else, and
 * the screen says so plainly.
 */
export default function AccessPage() {
  return <AccessScreen />;
}
