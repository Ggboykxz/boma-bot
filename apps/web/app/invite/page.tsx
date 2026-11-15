import { Suspense } from "react";
import { InviteScreen } from "@/components/invite/accept-screen";

/**
 * `/invite?token=<token>` (spec/08-ui-ux.md §8.2, REQ-042).
 *
 * Accepts an invitation as the signed-in invitee, or passes the visitor through sign-in inline.
 */
export default function InvitePage() {
  return (
    <Suspense fallback={null}>
      <InviteScreen />
    </Suspense>
  );
}
