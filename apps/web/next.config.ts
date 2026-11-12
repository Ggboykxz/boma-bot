import type { NextConfig } from "next";

/**
 * The web app is a static artifact: no route handlers, no server actions, no ISR (ADR-013).
 * `out/` is served by the Rust binary, which is the only production runtime.
 */
const nextConfig: NextConfig = {
  output: "export",
  // Directory-style URLs give every route an `index.html`, which is what the Rust static
  // handler serves for a deep link without guessing an extension (ADR-013).
  trailingSlash: true,
  reactStrictMode: true,
  poweredByHeader: false,
  images: { unoptimized: true },
};

export default nextConfig;
