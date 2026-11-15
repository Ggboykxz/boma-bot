import { Suspense } from "react";
import { IntegrationsSetupScreen } from "@/components/integrations/integrations-setup";

/**
 * `/integrations/setup` (spec/08-ui-ux.md §8.7, REQ-048).
 *
 * The post-install landing for connectors and plugins: confirmations, installed state and the
 * partial-install report. Internal page; the OAuth callbacks land here too.
 */
export default function IntegrationsSetupPage() {
  return (
    <Suspense fallback={null}>
      <IntegrationsSetupScreen />
    </Suspense>
  );
}
