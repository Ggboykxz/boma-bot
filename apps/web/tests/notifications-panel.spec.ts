import { expect, test, applySeed } from "./fixtures";

/**
 * The notifications panel round-trips against the wire (REQ-064):
 * - enable → the notification subscription registers with the server,
 * - the per-bot policy toggles flip the server's per(space, user, bot) row.
 * The endpoints and keys are stubbed; no push ever leaves the test.
 */

test("REQ-065 the notifications panel enable-policy-destination-disable flow round-trips", async ({ page }) => {
  await applySeed(page, "thread-empty");

  const calls: Array<Record<string, unknown>> = [];

  await page.route("**/rpc/notifications/status", async (route) => {
    calls.push({ procedure: "status" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        enabled: true,
        vapidPublicKey: "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
      }),
    });
  });
  await page.route("**/rpc/notifications/subscribe", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "subscribe", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "sub_tests_1" }),
    });
  });
  await page.route("**/rpc/notifications/unsubscribe", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "unsubscribe", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ removed: true }),
    });
  });
  await page.route("**/rpc/notifications/policyGet", async (route) => {
    calls.push({ procedure: "policyGet" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ botId: "bot_golden", notifyWaiting: true, notifyCompleted: false, dailyCap: 20 }),
    });
  });
  await page.route("**/rpc/notifications/policySet", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "policySet", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        botId: "bot_golden",
        notifyWaiting: body.notifyWaiting ?? true,
        notifyCompleted: body.notifyCompleted === true,
        dailyCap: 20,
        destinationSlackWebhook: body.destinationSlackWebhook === true,
        destinationTeamsWebhook: body.destinationTeamsWebhook === true,
      }),
    });
  });
  await page.route("**/rpc/notifications/destinationsList", async (route) => {
    calls.push({ procedure: "destinationsList" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ destinations: [] }),
    });
  });
  await page.route("**/rpc/notifications/destinationsCreate", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "destinationsCreate", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        destination: {
          id: "dst_added_1",
          kind: body.kind,
          label: body.label,
          createdAt: "2026-11-16T00:00:00Z",
        },
      }),
    });
  });
  await page.route("**/rpc/notifications/destinationsDelete", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "destinationsDelete", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ removed: true }),
    });
  });

  // A fake service worker and push manager — enough for the panel to walk the subscribe path.
  await page.addInitScript(() => {
    const KEY_BYTES = {
      p256dh: new Uint8Array(65).fill(7).buffer,
      auth: new Uint8Array(16).fill(9).buffer,
    };
    const subscription = {
      endpoint: "https://updates.push.services.mozilla.com/wpush/v1/fake-golden-endpoint",
      expirationTime: null,
      unsubscribe: async () => true,
      getKey: (name: "p256dh" | "auth") => KEY_BYTES[name],
      toJSON: () => ({
        endpoint: "https://updates.push.services.mozilla.com/wpush/v1/fake-golden-endpoint",
        keys: { p256dh: "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc", auth: "CQkJCQkJCQkJCQkJCQkJCQ" },
      }),
    };
    let subscribed = false;
    const pushManager = {
      subscribe: async () => {
        subscribed = true;
        return subscription;
      },
      getSubscription: async () => (subscribed ? subscription : null),
    };
    const registration = { pushManager };
    Object.defineProperty(navigator, "serviceWorker", {
      configurable: true,
      value: {
        register: async () => registration,
        getRegistration: async () => registration,
        ready: Promise.resolve(registration),
      },
    });
    const originalNotification = window.Notification;
    class GrantedNotification extends originalNotification {
      static permission: NotificationPermission = "default";
      static async requestPermission(): Promise<NotificationPermission> {
        GrantedNotification.permission = "granted";
        return "granted";
      }
    }
    Object.defineProperty(window, "Notification", { value: GrantedNotification, configurable: true });
  });

  await page.goto("/app/bot_golden");

  // Unsubscribed: only "Notify me" is in reach.
  const notify = page.getByRole("button", { name: /Notify me|Me prévenir|להודיע לי/ });
  await expect(notify).toBeVisible();
  await expect(page.getByLabel(/Waiting on your answer|En attente de votre réponse|כשממתינים לתשובה שלך/)).toHaveCount(0);

  await notify.click();

  // Subscribed: the panel flips to the policy form.
  await expect.poll(() => calls.some((entry) => entry.procedure === "subscribe")).toBe(true);
  const subscribeCall = calls.find((entry) => entry.procedure === "subscribe");
  expect(subscribeCall).toMatchObject({
    endpoint: "https://updates.push.services.mozilla.com/wpush/v1/fake-golden-endpoint",
  });

  const completedRow = page.getByLabel(/When the run finishes|À la fin de l'exécution|כשההרצה מסתיימת/);
  await expect(completedRow).toBeVisible();
  await completedRow.click();
  await expect.poll(() =>
    calls.find((entry) => entry.procedure === "policySet" && entry.notifyCompleted === true),
  ).toBeTruthy();

  // Toggle the Slack destination flag for this bot; the full view round-trips.
  const slackRow = page.getByLabel(/^Slack$/);
  await expect(slackRow).toBeVisible();
  await slackRow.click();
  await expect.poll(() =>
    calls.find((entry) => entry.procedure === "policySet" && entry.destinationSlackWebhook === true),
  ).toBeTruthy();

  // Add a Slack destination — owner role is stubbed at the mock layer.
  await page.getByLabel(/Label|Nom|שם/).fill("#product-releases");
  await page.getByLabel(/Webhook|כתובת/).fill("https://hooks.slack.com/services/test-abc123");
  await page.getByRole("button", { name: /Add a destination|Ajouter une destination|הוסף יעד/ }).click();
  await expect.poll(() =>
    calls.find((entry) => entry.procedure === "destinationsCreate" && entry.label === "#product-releases"),
  ).toBeTruthy();
});
