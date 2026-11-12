import { LandingScreen } from "@/components/shell/landing-screen";

/**
 * The root screen (spec/08-ui-ux.md §8.1). Sign-in redirects to `/app` when signed in,
 * otherwise the sign-in screen; sessions arrive in M1.
 */
export default function HomePage() {
  return <LandingScreen />;
}
