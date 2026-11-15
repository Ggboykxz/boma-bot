import { Suspense } from "react";
import { SignInScreen } from "@/components/auth/sign-in-screen";

/** `/sign-in` (spec/08-ui-ux.md §8.2, REQ-040). */
export default function SignInPage() {
  return (
    <Suspense fallback={null}>
      <SignInScreen />
    </Suspense>
  );
}
