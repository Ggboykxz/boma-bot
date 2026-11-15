import { Suspense } from "react";
import { SignUpScreen } from "@/components/auth/sign-up-screen";

/** `/sign-up` (spec/08-ui-ux.md §8.2, REQ-040). */
export default function SignUpPage() {
  return (
    <Suspense fallback={null}>
      <SignUpScreen />
    </Suspense>
  );
}
