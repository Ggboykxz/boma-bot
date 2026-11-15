import { redirect } from "next/navigation";

/** `/onboarding` → the first step (spec §8.2). */
export default function OnboardingIndex() {
  redirect("/onboarding/1/");
}
