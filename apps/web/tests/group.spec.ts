import { expect, test, applySeed } from "./fixtures";

test("REQ-010 the group room renders the shared thread with bot attribution", async ({
  page,
}) => {
  await applySeed(page, "group-golden-room");
  await page.goto("/app/g/group_golden");

  // The room header, the member names and the shared log.
  await expect(page.getByRole("heading", { name: "Triage" })).toBeVisible();
  await expect(page.getByRole("log")).toContainText(
    "@analyst please summarise the thread",
  );
  await expect(page.getByRole("log")).toContainText("three PRs merged");

  // The bot message is attributed to the analyst, and the member message to the caller.
  const log = page.getByRole("log");
  await expect(log.getByText("analyst", { exact: true })).toHaveCount(1);

  // The composer posts into the shared thread; the server routes the mention.
  const sent: Array<Record<string, unknown>> = [];
  await page.route("**/rpc/threads/send", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    sent.push(body);
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ messageId: "msg_sent", runId: "run_sent", status: "queued" }),
    });
  });

  const composer = page.getByLabel(/Message|Écris un message|כתוב הודעה/);
  await composer.fill("@writer polish this note.");
  await page.getByRole("button", { name: /Send|Envoyer|שליחה/ }).click();

  await expect.poll(() => sent.length).toBeGreaterThan(0);
  expect(sent[sent.length - 1].botId).toBe("bot_analyst");
  expect(sent[sent.length - 1].text).toContain("@writer");
});

test("REQ-010 the rail creates a group with the chosen bots and navigates to it", async ({
  page,
}) => {
  await applySeed(page, "group-golden-room");
  await page.goto("/app/g/group_golden");

  const sent: Array<Record<string, unknown>> = [];
  await page.route("**/rpc/groups/create", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    sent.push(body);
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        id: "group_new",
        name: body.name,
        threadId: "thr_group_new",
        memberBotIds: body.botIds,
      }),
    });
  });

  await page
    .getByRole("button", { name: /New group|Nouveau groupe|קבוצה חדשה/ })
    .click();
  await page.getByLabel(/Name|Nom|שם/).fill("Triage 2");
  await page.getByRole("checkbox", { name: "analyst" }).check();
  await page.getByRole("button", { name: /Create|Créer|צור/ }).click();

  await expect.poll(() => sent.length).toBeGreaterThan(0);
  const last = sent[sent.length - 1];
  expect(last.name).toBe("Triage 2");
  expect(last.botIds).toEqual(["bot_analyst"]);
});
