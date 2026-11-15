import { PluginsScreen } from "@/components/admin/plugins-screen";

/**
 * `/admin/plugins` (spec/08-ui-ux.md §8.7, REQ-058).
 *
 * The submissions queue for the space admin. Approving never executes anything: the review
 * only changes the submission's state, and the actual install happens through the ordinary
 * plugin-install flow afterwards.
 */
export default function PluginsPage() {
  return <PluginsScreen />;
}
