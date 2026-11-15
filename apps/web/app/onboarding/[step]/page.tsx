import { OnboardingScreen } from "@/components/onboarding/onboarding-screen";

const STEPS = ["1", "2", "3"] as const;
type StepParam = (typeof STEPS)[number];

function parseStep(raw: string): 1 | 2 | 3 {
  switch (raw) {
    case "2":
      return 2;
    case "3":
      return 3;
    default:
      return 1;
  }
}

/**
 * `/onboarding/<step>` (spec/08-ui-ux.md §8.2, REQ-042, REQ-052).
 *
 * The static export carries one page per step; the client wizard owns the rest.
 */
export function generateStaticParams(): Array<{ step: StepParam }> {
  return STEPS.map((step) => ({ step }));
}

export default async function OnboardingPage({
  params,
}: {
  params: Promise<{ step: string }>;
}) {
  const { step } = await params;
  return <OnboardingScreen step={parseStep(step)} />;
}
