import next from "eslint-config-next";

/**
 * Flat config. `eslint-config-next` exports an array of configs, which is why it is spread rather
 * than wrapped in `extends`. The generated outputs are ignored: linting them would report the
 * framework's own code.
 */
export default [
  {
    ignores: [
      ".next/**",
      "out/**",
      "node_modules/**",
      "playwright-report/**",
      "test-results/**",
      "next-env.d.ts",
    ],
  },
  ...next,
];
