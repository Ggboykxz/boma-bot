import { SharedPageScreen } from "@/components/shared-page/shared-page-screen";

/**
 * `/p/<token>` (spec/08-ui-ux.md §8.1, §8.8, REQ-037).
 *
 * The public, session-less screen: one column, no navigation chrome, no buttons, no form. An
 * unknown or revoked token renders exactly the same document as a valid page that has no
 * actions — the backend answers them identically, and this screen keeps that promise.
 */
export function generateStaticParams(): Array<{ slugToken: string }> {
  // The goldens pin `token_golden` ({@link spec/golden/ui/REQ-037-shared-page.json}) and
  // `token_unknown` ({@link spec/golden/ui/REQ-037-shared-page-invalid.json}); the development
  // server refuses an unknown param outright, so those two need a file to exist. Every other
  // token is served by the static handler's `/p/*` mapping (ADR-013).
  return [{ slugToken: "token_golden" }, { slugToken: "token_unknown" }];
}

export default function SharedPage() {
  // The screen reads the token from the location (ADR-013): one shell serves every share link.
  return <SharedPageScreen />;
}
