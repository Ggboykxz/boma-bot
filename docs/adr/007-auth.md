# ADR-007 — Authentication: opaque session cookies, Argon2id, passkeys and OIDC/SCIM as adapters

Status: proposed (awaiting owner go)

## Context

REQ-040 and REQ-041 require sign-up, password sign-in, password reset, passkeys, SSO, e-mail codes,
sessions with explicit lifetimes, a trusted-origin allow-list, and SCIM provisioning. `05-known-defects.md`
D-08 records that the reference delegates the entire auth surface to a library without asserting its
defaults. AGENTS.md mandates Argon2id if passwords exist and constant-time secret comparison.

## Alternatives

1. **Own the session model** (opaque random token, hashed at rest, `httpOnly`/`SameSite=Lax`/`Secure`
   cookie, server-side revocation) with Argon2id passwords, WebAuthn passkeys, and OIDC/SCIM as
   adapters into the same user table.
2. Delegate to an external IdP (Keycloak, Ory Kratos, Zitadel) and treat it as the only login.
3. Use a Rust auth library that bundles everything (e.g. an oauth2 + webauthn composition) without a
   session model of our own.
4. Keep the reference's library behaviour (better-auth) behind a compatibility shim.

## Scoring

| Criterion | 1 own sessions | 2 external IdP | 3 library bundle | 4 shim |
|---|---|---|---|---|
| 1 fits the spec | 1 | 3 (self-hosted single-binary story breaks; SCIM/OIDC still needed) | 2 | 4 (a second runtime) |
| 2 maturity | 3 (we own the code) | 1 | 2 | 2 |
| 3 security | 2 (auditable, standard primitives) | 1 | 2 | 4 (unasserted defaults) |
| 4 performance | 1 | 3 | 1 | 2 |
| 5 simplicity | 2 | 5 | 2 | 5 |
| 6 typed contract | 1 | 2 | 1 | 3 |

## Decision

Option 1, with the primitives taken from vetted crates rather than written by hand: `argon2` for
passwords, `webauthn-rs` for passkey ceremonies, `openidconnect` for SSO discovery, `rand` for tokens,
and constant-time comparison for every secret. Sessions are rows with an expiry, a rotation on
privilege change, and revocation on password reset, member removal and account deletion. The auth
surface is mounted at `/api/auth/*` with an explicit route list, not a catch-all, and a test asserts
the exact route set plus the cookie attributes — which is what D-08 says the reference never did.

## Consequences

- Password reset, invitation acceptance and e-mail verification share one token primitive
  (random, hashed at rest, single use, bounded TTL).
- SCIM is an adapter that writes through the same user/member service, so provisioning and manual
  invitations cannot diverge.
- Passkeys require a stable RP id per deployment; it is configuration validated at boot, and the
  `/api/auth/capabilities` reply tells the client which methods exist.
- An external IdP remains possible later as an OIDC adapter; nothing in the session model prevents it.
