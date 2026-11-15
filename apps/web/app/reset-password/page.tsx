import { Suspense } from "react";
import { ResetPasswordScreen } from "@/components/auth/reset-password-screen";

/** `/reset-password` (spec/08-ui-ux.md §8.2, REQ-040). */
export default function ResetPasswordPage() {
  return (
    <Suspense fallback={null}>
      <ResetPasswordScreen />
    </Suspense>
  );
}
