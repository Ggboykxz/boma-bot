/**
 * The real wire shape of `GET /api/auth/capabilities` (spec/openapi.yaml).
 *
 * Every spec file seeds this exact document — the client renders what the binary actually
 * reports, or the fixtures drift from the product again.
 */
export const CAPABILITIES = {
  sso: null,
  passwordAuth: true,
  passwordReset: true,
  resetUrl: "http://127.0.0.1:5173/reset-password",
  emailCode: false,
  billing: false,
  passkeyRpId: null,
  nativePasskeys: false,
  localFirst: false,
};
