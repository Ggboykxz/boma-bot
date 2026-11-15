/**
 * Hand-written until the OpenAPI procedure codegen lands (ADR-003); keep the shapes in sync with crates/api/src/wire.rs.
 *
 * The client speaks the RPC surface of the Rust binary: `POST /rpc/<namespace>/<procedure>`, one
 * JSON body, one JSON answer, and a stable error envelope (`{ code, error }`).
 *
 * Three rules the client never breaks:
 *
 * 1. **The session is a cookie.** Every call carries `credentials: "include"`; the client never
 *    invents a token, never reads the cookie and never writes it to a log.
 * 2. **The space travels in a header.** `x-space-id` is sent whenever a space is known; the server
 *    falls back to the caller's default space when it is absent.
 * 3. **A failure is typed.** The server's envelope becomes an `RpcError` carrying the machine code
 *    and the message that is safe to show, so a screen can branch on the code instead of parsing
 *    a string.
 */

/** Who wrote a message. The four roles the domain spells. */
export type MessageRole = "user" | "bot" | "system" | "tool";

/** A piece of a message. The union is discriminated by `type`. */
export type Block =
  | { type: "text"; text: string }
  | { type: "image"; artifactId: string }
  | { type: "file"; artifactId: string; name: string }
  | { type: "tool"; name: string; running: boolean }
  | { type: "card"; cardId: string; kind: string };

/** One emoji reaction, as the client sees it (REQ-008). */
export interface ReactionView {
  emoji: string;
  count: number;
  reacted: boolean;
}

/** A message, as the transcript shows it. */
export interface MessageView {
  id: string;
  seq: number;
  role: MessageRole;
  blocks: Block[];
  runId?: string;
  createdAt: string;
  replyToMessageId?: string;
  replyQuote?: string;
  reactions?: ReactionView[];
}

/** One frame of a thread's stream. */
export interface EventView {
  threadId: string;
  seq: number;
  type: string;
  payload: unknown;
  createdAt: string;
}

/** A run, as the activity list shows it. */
export interface RunView {
  id: string;
  status: string;
  trigger: string;
  promptSnippet?: string;
  startedAt?: string;
  completedAt?: string;
}

/** The reply of `threads.send`. */
export interface SendReply {
  messageId: string;
  runId: string;
  status: string;
}

/** The head of a thread, as the unread divider is rendered (REQ-009). `lastMessageAt` is null for an empty thread. */
export interface ThreadHeadReply {
  threadId: string;
  lastSeq: number;
  lastMessageAt: string | null;
  readSeq: number;
  unreadCount: number;
}

/** One bot of the roster, as the rail renders it (REQ-009). */
export interface RosterBotView {
  id: string;
  name: string;
  threadId: string;
  archivedAt: string | null;
  snoozedUntil: string | null;
  unreadCount: number;
}

/** The reply of `bots.list` (REQ-009). */
export interface BotsListReply {
  bots: RosterBotView[];
}

/** The reply of `threads.snooze` and `threads.unsnooze` (REQ-009). */
export interface SnoozeReply {
  threadId: string;
  snoozedUntil: string | null;
}

/** The reply of `threads.react` (REQ-008). The truth for the message after the change. */
export interface ReactReply {
  messageId: string;
  reactions: ReactionView[];
}

/** The reply of `threads.messages`. The server answers newest first. */
export interface MessagesReply {
  messages: MessageView[];
  nextCursor?: string;
}

/** The reply of `bots.create` and `bots.get`. */
export interface BotReply {
  id: string;
  name: string;
  threadId: string;
  archivedAt?: string | null;
  createdAt: string;
}

/** The reply of `spaces.create`. */
export interface SpaceReply {
  id: string;
  name: string;
  isDefault: boolean;
  role: string;
}

/** The reply of `spaces.invite`. */
export interface InvitationReply {
  invitationId: string;
  expiresAt: string;
  status: string;
}

/** The reply of `runs.list`. */
export interface RunsReply {
  runs: RunView[];
  nextCursor?: string;
}

/** The machine codes the error envelope carries (spec/03-api-contract.md §3.1). */
export type RpcErrorCode =
  | "UNAUTHORIZED"
  | "FORBIDDEN"
  | "NOT_FOUND"
  | "BAD_REQUEST"
  | "CONFLICT"
  | "TOO_MANY_REQUESTS"
  | "INTERNAL";

/**
 * The failure of a procedure call.
 *
 * `code` is what a screen branches on; `message` is the server's own wording, already safe to
 * show. The status is kept for the rare case where the envelope could not be read at all.
 */
export class RpcError extends Error {
  readonly code: RpcErrorCode | string;
  readonly status: number;

  constructor(code: RpcErrorCode | string, message: string, status: number) {
    super(message);
    this.name = "RpcError";
    this.code = code;
    this.status = status;
  }
}

/** The base every call is made against: same origin, unless the build names another one. */
function apiBase(): string {
  const override = process.env.NEXT_PUBLIC_BOMA_API_BASE;
  if (!override) {
    return "";
  }
  return override.endsWith("/") ? override.slice(0, -1) : override;
}

/** The space the client acts in, when it knows one. */
let knownSpaceId: string | null = null;

/** Names the space every following call acts in. */
export function setSpaceId(spaceId: string | null): void {
  knownSpaceId = spaceId;
}

/** The space the client acts in, or `null` when the server should pick the default one. */
export function getSpaceId(): string | null {
  if (knownSpaceId) {
    return knownSpaceId;
  }
  if (typeof window === "undefined") {
    return null;
  }
  try {
    return window.localStorage.getItem("boma-space-id");
  } catch {
    return null;
  }
}

function headers(extra?: Record<string, string>): Record<string, string> {
  const result: Record<string, string> = {
    "content-type": "application/json",
    ...extra,
  };
  const spaceId = getSpaceId();
  if (spaceId) {
    result["x-space-id"] = spaceId;
  }
  return result;
}

async function readEnvelope(response: Response): Promise<{ code: string; error: string } | null> {
  try {
    const body = (await response.json()) as unknown;
    if (body && typeof body === "object") {
      const candidate = body as { code?: unknown; error?: unknown };
      if (typeof candidate.code === "string" && typeof candidate.error === "string") {
        return { code: candidate.code, error: candidate.error };
      }
    }
  } catch {
    // The body was not JSON: the status alone describes the failure.
  }
  return null;
}

/**
 * Calls one procedure.
 *
 * The session cookie travels with the call; nothing about it is read, copied or logged here.
 */
export async function call<T>(
  namespace: string,
  procedure: string,
  body: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const response = await fetch(`${apiBase()}/rpc/${namespace}/${procedure}`, {
    method: "POST",
    credentials: "include",
    headers: headers(),
    body: JSON.stringify(body),
    ...(signal ? { signal } : {}),
  });

  if (!response.ok) {
    if (response.status === 401) {
      unauthorizedHandler?.();
    }
    const envelope = await readEnvelope(response);
    throw new RpcError(
      envelope?.code ?? "INTERNAL",
      envelope?.error ?? "The request could not be completed.",
      response.status,
    );
  }

  return (await response.json()) as T;
}

/** `threads.send` — writes a message and starts the run that answers it. */
export function threadsSend(
  request: {
    botId: string;
    text: string;
    clientNonce?: string;
    replyToMessageId?: string;
    replyQuote?: string;
  },
  signal?: AbortSignal,
): Promise<SendReply> {
  return call<SendReply>("threads", "send", request, signal);
}

/**
 * Normalises one block.
 *
 * The frozen contract spells the artifact field `artifactId`; the Rust enum currently emits
 * `artifact_id`. Both are accepted, and the canonical camel case is what leaves this function, so
 * the screens never have to know about the difference.
 */
function normalizeBlock(raw: unknown): Block | null {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const block = raw as Record<string, unknown>;
  switch (block.type) {
    case "text":
      return { type: "text", text: typeof block.text === "string" ? block.text : "" };
    case "image": {
      const artifactId = block.artifactId ?? block.artifact_id;
      return typeof artifactId === "string"
        ? { type: "image", artifactId }
        : { type: "image", artifactId: "" };
    }
    case "file": {
      const artifactId = block.artifactId ?? block.artifact_id;
      return {
        type: "file",
        artifactId: typeof artifactId === "string" ? artifactId : "",
        name: typeof block.name === "string" ? block.name : "",
      };
    }
    case "tool":
      return {
        type: "tool",
        name: typeof block.name === "string" ? block.name : "",
        running: block.running === true,
      };
    case "card":
      return {
        type: "card",
        cardId: typeof block.cardId === "string" ? block.cardId : "",
        kind: typeof block.kind === "string" ? block.kind : "",
      };
    default:
      return null;
  }
}

/** Normalises the reactions of a message; invalid entries are dropped (REQ-008). */
function normalizeReactions(raw: unknown): ReactionView[] {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw
    .map((entry): ReactionView | null => {
      if (!entry || typeof entry !== "object") {
        return null;
      }
      const reaction = entry as Record<string, unknown>;
      if (typeof reaction.emoji !== "string" || reaction.emoji.length === 0) {
        return null;
      }
      return {
        emoji: reaction.emoji,
        count: typeof reaction.count === "number" ? reaction.count : 0,
        reacted: reaction.reacted === true,
      };
    })
    .filter((reaction): reaction is ReactionView => reaction !== null);
}

/** Normalises one message, dropping the blocks that cannot be read. */
export function normalizeMessage(raw: unknown): MessageView | null {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const message = raw as Record<string, unknown>;
  if (typeof message.id !== "string") {
    return null;
  }
  const blocks = Array.isArray(message.blocks)
    ? message.blocks.map(normalizeBlock).filter((block): block is Block => block !== null)
    : [];
  const role = message.role;
  const reactions = normalizeReactions(message.reactions);
  return {
    id: message.id,
    seq: typeof message.seq === "number" ? message.seq : 0,
    role: isMessageRole(role) ? role : "system",
    blocks,
    ...(typeof message.runId === "string" ? { runId: message.runId } : {}),
    ...(typeof message.replyToMessageId === "string"
      ? { replyToMessageId: message.replyToMessageId }
      : {}),
    ...(typeof message.replyQuote === "string" ? { replyQuote: message.replyQuote } : {}),
    ...(reactions.length > 0 ? { reactions } : {}),
    createdAt: typeof message.createdAt === "string" ? message.createdAt : "",
  };
}

/** Whether a value is one of the four roles the domain spells. */
export function isMessageRole(value: unknown): value is MessageRole {
  return value === "user" || value === "bot" || value === "system" || value === "tool";
}

/** `threads.head` — the tail of the thread and the caller's own read position (REQ-009). */
export function threadsHead(
  request: { threadId: string },
  signal?: AbortSignal,
): Promise<ThreadHeadReply> {
  return call<ThreadHeadReply>("threads", "head", request, signal);
}

/** `threads.markRead` — move the caller's read position to the newest message (REQ-009). */
export function threadsMarkRead(
  request: { threadId: string },
  signal?: AbortSignal,
): Promise<ThreadHeadReply> {
  return call<ThreadHeadReply>("threads", "markRead", request, signal);
}

/** `threads.markUnread` — move the caller's read position back to zero (REQ-009). */
export function threadsMarkUnread(
  request: { threadId: string },
  signal?: AbortSignal,
): Promise<ThreadHeadReply> {
  return call<ThreadHeadReply>("threads", "markUnread", request, signal);
}

/** `bots.list` — the roster the rail renders from, with per-member unread and snooze (REQ-009). */
export function botsList(signal?: AbortSignal): Promise<BotsListReply> {
  return call<BotsListReply>("bots", "list", {}, signal);
}

/** `threads.snooze` — hide the chat from the regular list until `until` (REQ-009). */
export function threadsSnooze(
  request: { threadId: string; until?: string },
  signal?: AbortSignal,
): Promise<SnoozeReply> {
  return call<SnoozeReply>("threads", "snooze", request, signal);
}

/** `threads.unsnooze` — bring the snoozed chat back to the regular list (REQ-009). */
export function threadsUnsnooze(
  request: { threadId: string },
  signal?: AbortSignal,
): Promise<SnoozeReply> {
  return call<SnoozeReply>("threads", "unsnooze", request, signal);
}

/** `threads.react` — adds or removes an emoji reaction on a message (REQ-008). */
export function threadsReact(
  request: { messageId: string; emoji: string; action: "add" | "remove" },
  signal?: AbortSignal,
): Promise<ReactReply> {
  return call<ReactReply>("threads", "react", request, signal);
}

/** `threads.messages` — a page of the thread's history, newest first. */
export async function threadsMessages(
  request: { threadId: string; cursor?: string; limit?: number },
  signal?: AbortSignal,
): Promise<MessagesReply> {
  const reply = await call<MessagesReply>("threads", "messages", request, signal);
  const messages = Array.isArray(reply.messages)
    ? reply.messages
        .map((message) => normalizeMessage(message))
        .filter((message): message is MessageView => message !== null)
    : [];
  return {
    messages,
    ...(typeof reply.nextCursor === "string" ? { nextCursor: reply.nextCursor } : {}),
  };
}

/** `bots.create` — creates a bot and its thread in one transaction. */
export function botsCreate(
  request: { name: string; description?: string; color?: string },
  signal?: AbortSignal,
): Promise<BotReply> {
  return call<BotReply>("bots", "create", request, signal);
}

/** `bots.get` — reads one bot. */
export function botsGet(request: { botId: string }, signal?: AbortSignal): Promise<BotReply> {
  return call<BotReply>("bots", "get", request, signal);
}

/** `spaces.create` — creates a space and joins it as its owner. */
export function spacesCreate(request: { name: string }, signal?: AbortSignal): Promise<SpaceReply> {
  return call<SpaceReply>("spaces", "create", request, signal);
}

/** `spaces.invite` — invites somebody into the caller's space. */
export function spacesInvite(
  request: { email: string; role?: string },
  signal?: AbortSignal,
): Promise<InvitationReply> {
  return call<InvitationReply>("spaces", "invite", request, signal);
}

/** `runs.list` — the activity projection, newest first. */
export function runsList(
  request: { botId?: string; cursor?: string; limit?: number },
  signal?: AbortSignal,
): Promise<RunsReply> {
  return call<RunsReply>("runs", "list", request, signal);
}

/**
 * `runs.retry` — puts a failed run back in the queue.
 *
 * The procedure is not part of the M1 surface (the Rust dispatcher answers `not found` until the
 * leases and the fence tokens land in M2); the call is written here so the screen can offer the
 * action the spec asks for, and the failure is reported like any other.
 */
export function runsRetry(
  request: { runId: string },
  signal?: AbortSignal,
): Promise<{ ok: boolean }> {
  return call<{ ok: boolean }>("runs", "retry", request, signal);
}

/** One frame of a thread's stream, as the browser hands it over. */
export interface StreamFrame {
  /** The event name the server wrote, when it wrote one. */
  event?: string;
  /** The decoded body. */
  data: unknown;
}

/**
 * Splits a buffer of server-sent events into the frames it holds.
 *
 * The parser is deliberately small: the server writes `event:` and `data:` lines and separates
 * the frames with a blank line. A partial frame stays in the buffer for the next chunk.
 */
export function parseEventStream(buffer: string): { frames: StreamFrame[]; rest: string } {
  const frames: StreamFrame[] = [];
  const normalized = buffer.replace(/\r\n/g, "\n");
  const parts = normalized.split("\n\n");

  for (const part of parts.slice(0, -1)) {
    let event: string | undefined;
    const data: string[] = [];

    for (const line of part.split("\n")) {
      if (line.startsWith(":")) {
        continue;
      }
      const separator = line.indexOf(":");
      const field = separator === -1 ? line : line.slice(0, separator);
      const value = separator === -1 ? "" : line.slice(separator + 1).replace(/^ /, "");
      if (field === "event") {
        event = value;
      } else if (field === "data") {
        data.push(value);
      }
    }

    if (data.length === 0) {
      continue;
    }

    const body = data.join("\n");
    try {
      frames.push({ event, data: JSON.parse(body) as unknown });
    } catch {
      // A frame that is not JSON is not a frame: the stream keeps going.
    }
  }

  return { frames, rest: parts[parts.length - 1] ?? "" };
}

/** The callbacks a subscriber hands over. */
export interface SubscribeHandlers {
  /** One frame, decoded. */
  onFrame: (frame: EventView) => void;
  /** The stream failed or ended. */
  onClose: () => void;
}

/**
 * `threads.subscribe` — the live stream of a thread.
 *
 * The procedure answers a `text/event-stream`, so the reader is a `fetch` and not an
 * `EventSource`: the stream is opened with a POST body, exactly like every other procedure.
 */
export async function threadsSubscribe(
  request: { threadId: string; afterSeq?: number },
  handlers: SubscribeHandlers,
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${apiBase()}/rpc/threads/subscribe`, {
    method: "POST",
    credentials: "include",
    headers: headers({ accept: "text/event-stream" }),
    body: JSON.stringify(request),
    ...(signal ? { signal } : {}),
  });

  if (!response.ok || !response.body) {
    handlers.onClose();
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      buffer += decoder.decode(value, { stream: true });
      const { frames, rest } = parseEventStream(buffer);
      buffer = rest;
      for (const frame of frames) {
        const body = frame.data as Partial<EventView> | null;
        if (!body || typeof body !== "object" || typeof body.threadId !== "string") {
          continue;
        }
        handlers.onFrame({
          threadId: body.threadId,
          seq: typeof body.seq === "number" ? body.seq : 0,
          type: typeof body.type === "string" ? body.type : (frame.event ?? ""),
          payload: body.payload,
          createdAt: typeof body.createdAt === "string" ? body.createdAt : "",
        });
      }
    }
  } catch {
    // The stream broke: the caller decides what to do next.
  } finally {
    handlers.onClose();
  }
}

// ---------------------------------------------------------------------------
// M9 — admin surfaces (REQ-058)
// ---------------------------------------------------------------------------

/** One member of a space, as the admin surface sees them. */
export interface AdminMemberView {
  userId: string;
  email: string;
  name: string;
  role: string;
  lastSeenAt: string | null;
}

/** `admin.members.list`. */
export interface AdminMembersReply {
  members: AdminMemberView[];
}

/** One pending invitation, as the admin surface sees them. */
export interface AdminInvitationView {
  id: string;
  email: string;
  role: string;
  expiresAt: string;
}

/** `admin.invitations.list`. */
export interface AdminInvitationsReply {
  invitations: AdminInvitationView[];
}

/** One server-level grant of hosted access, as the admin surface sees them. */
export interface AdminGrantView {
  email: string;
  reason: string;
  grantedBy: string;
  createdAt: string;
}

/** `admin.grants.list`. */
export interface AdminGrantsReply {
  grants: AdminGrantView[];
}

/** One plugin submission, as the admin surface sees them. */
export interface AdminPluginSubmissionView {
  id: string;
  source: string;
  name: string | null;
  status: string;
  createdAt: string;
}

/** `admin.plugins.list`. */
export interface AdminPluginsReply {
  submissions: AdminPluginSubmissionView[];
}

/** `admin.members.list` — the caller's space only. */
export function adminMembersList(signal?: AbortSignal): Promise<AdminMembersReply> {
  return call<AdminMembersReply>("admin", "members.list", {}, signal);
}

/** `admin.invitations.list`. */
export function adminInvitationsList(signal?: AbortSignal): Promise<AdminInvitationsReply> {
  return call<AdminInvitationsReply>("admin", "invitations.list", {}, signal);
}

/** `admin.invitations.revoke`. */
export function adminInvitationsRevoke(
  request: { invitationId: string },
  signal?: AbortSignal,
): Promise<{ revoked: boolean }> {
  return call("admin", "invitations.revoke", request, signal);
}

/** `admin.grants.list` — the whole deployment. */
export function adminGrantsList(signal?: AbortSignal): Promise<AdminGrantsReply> {
  return call<AdminGrantsReply>("admin", "grants.list", {}, signal);
}

/** `admin.grants.create`. */
export function adminGrantsCreate(
  request: { email: string; reason: string },
  signal?: AbortSignal,
): Promise<{ grant: AdminGrantView }> {
  return call("admin", "grants.create", request, signal);
}

/** `admin.grants.revoke`. */
export function adminGrantsRevoke(
  request: { email: string },
  signal?: AbortSignal,
): Promise<{ revoked: boolean }> {
  return call("admin", "grants.revoke", request, signal);
}

/** `admin.plugins.list`. */
export function adminPluginsList(
  request: { status?: string },
  signal?: AbortSignal,
): Promise<AdminPluginsReply> {
  return call<AdminPluginsReply>("admin", "plugins.list", request, signal);
}

/** `admin.plugins.decide` — the reviewer's approve or reject (REQ-058). */
export function adminPluginsDecide(
  request: { submissionId: string; decision: "approved" | "rejected" },
  signal?: AbortSignal,
): Promise<{ status: string }> {
  return call("admin", "plugins.decide", request, signal);
}


// ---------------------------------------------------------------------------
// M10 — auth flows (spec §8.2)
// ---------------------------------------------------------------------------

/** The reply of `GET /api/auth/capabilities`. */
export interface AuthCapabilities {
  version: string;
  runtime: string;
  signup: "open" | "closed";
  password: boolean;
  emailCode: boolean;
  passkey: boolean;
  sso: string[];
}

/** `GET /api/auth/capabilities`. */
export async function authCapabilities(signal?: AbortSignal): Promise<AuthCapabilities> {
  const response = await fetch(`${apiBase()}/api/auth/capabilities`, {
    credentials: "include",
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    throw new RpcError("INTERNAL", "The sign-in methods could not be loaded.", response.status);
  }
  return (await response.json()) as AuthCapabilities;
}

/** `POST /api/auth/sign-in`. */
export async function authSignIn(
  request: { email: string; password: string },
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${apiBase()}/api/auth/sign-in`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    await throwErr(response);
  }
}

/** `POST /api/auth/sign-out`. */
export async function authSignOut(signal?: AbortSignal): Promise<void> {
  const response = await fetch(`${apiBase()}/api/auth/sign-out`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    ...(signal ? { signal } : {}),
    body: "{}",
  });
  if (!response.ok) {
    await throwErr(response);
  }
}

/** `POST /api/auth/sign-up`. */
export async function authSignUp(
  request: { email: string; password: string; name?: string; locale?: string },
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${apiBase()}/api/auth/sign-up`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    await throwErr(response);
  }
}

/** `POST /api/auth/forgot-password`. */
export async function authForgotPassword(
  request: { email: string },
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${apiBase()}/api/auth/forgot-password`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    await throwErr(response);
  }
}

/** `POST /api/auth/reset-password`. */
export async function authResetPassword(
  request: { token: string; password: string },
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(`${apiBase()}/api/auth/reset-password`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    await throwErr(response);
  }
}

/**
 * The handler a host wires once to react to a `401` from any RPC.
 *
 * The shell redirects to `/sign-in?next=<current path>`; the app entry does the wiring.
 */
type UnauthorizedHandler = () => void;

let unauthorizedHandler: UnauthorizedHandler | null = null;

/** Registers (or unregisters) the unauthorized handler; the last one wins. */
export function setUnauthorizedHandler(handler: UnauthorizedHandler | null): void {
  unauthorizedHandler = handler;
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

/**
 * Throws `RpcError` from a non-ok fetch response.
 *
 * The envelope is solved exactly like `call`'s path, so a screen branching on `code` behaves the
 * same across all endpoints.
 */
async function throwErr(response: Response): Promise<never> {
  let envelope: { code?: string; error?: string } | null = null;
  try {
    envelope = (await response.json()) as { code?: string; error?: string } | null;
  } catch {
    try {
      const text = await response.text();
      if (text.includes("email") && text.length < 300) {
        envelope = { error: text };
      }
    } catch {
      envelope = null;
    }
  }
  throw new RpcError(
    envelope?.code ?? "INTERNAL",
    envelope?.error ?? "The request could not be completed.",
    response.status,
  );
}


/** The body of `invitations.accept` (REQ-042). */
export interface InvitationAcceptReply {
  spaceId: string;
}

/** `invitations.accept` — joins the space the token names. */
export function invitationsAccept(
  request: { token: string },
  signal?: AbortSignal,
): Promise<InvitationAcceptReply> {
  return call<InvitationAcceptReply>("invitations", "accept", request, signal);
}

/** The public preview of `GET /api/v1/invites/{token}`. */
export interface InvitationPublicPreview {
  spaceName: string;
  inviterName: string | null;
  role: string;
  expiresAt: string;
}

/** `GET /api/v1/invites/{token}` — the anonymous invite page's view (REQ-042). */
export async function invitationPreview(
  token: string,
  signal?: AbortSignal,
): Promise<InvitationPublicPreview> {
  const response = await fetch(`${apiBase()}/api/v1/invites/${encodeURIComponent(token)}`, {
    credentials: "omit",
    ...(signal ? { signal } : {}),
  });
  if (!response.ok) {
    throw new RpcError("NOT_FOUND", "Not found.", response.status);
  }
  return (await response.json()) as InvitationPublicPreview;
}


// ---------------------------------------------------------------------------
// M10 — artifacts (REQ-036, spec §8.5)
// ---------------------------------------------------------------------------

/** The artifact as the API exposes it (REQ-036). */
export interface ArtifactView {
  id: string;
  botId?: string;
  groupId?: string;
  runId?: string;
  name: string;
  description: string;
  declaredMime: string;
  mimeType: string;
  size: number;
  hash: string;
  rootArtifactId: string;
  version: number;
  createdAt: string;
}

/** The reply of `artifacts.listSpace`. */
export interface ArtifactListSpaceReply {
  artifacts: ArtifactView[];
}

/** The reply of `artifacts.get` / `artifacts.getById`. */
export interface ArtifactGetReply {
  artifact: ArtifactView;
  contentBase64: string;
}

/** One version of a family (REQ-036). */
export interface ArtifactVersionView {
  id: string;
  version: number;
  rootArtifactId: string;
}

/** The reply of `artifacts.listVersions`. */
export interface ArtifactVersionsReply {
  versions: ArtifactVersionView[];
}

/** The reply of `artifacts.remove`. */
export interface ArtifactRemoveReply {
  removed: number;
}

/** `artifacts.listSpace` — the latest version of every family in the space. */
export function artifactsListSpace(
  signal?: AbortSignal,
): Promise<ArtifactListSpaceReply> {
  return call<ArtifactListSpaceReply>("artifacts", "listSpace", {}, signal);
}

/** `artifacts.get` — the bytes; `version` pins a specific one, undefined is the latest. */
export function artifactsGet(
  request: { artifactId: string; version?: number },
  signal?: AbortSignal,
): Promise<ArtifactGetReply> {
  return call<ArtifactGetReply>("artifacts", "get", request, signal);
}

/** `artifacts.listVersions` — the family, newest first. */
export function artifactsListVersions(
  request: { artifactId: string },
  signal?: AbortSignal,
): Promise<ArtifactVersionsReply> {
  return call<ArtifactVersionsReply>("artifacts", "listVersions", request, signal);
}

/** `artifacts.remove` — the family, every version, gone for the whole space. */
export function artifactsRemove(
  request: { artifactId: string },
  signal?: AbortSignal,
): Promise<ArtifactRemoveReply> {
  return call<ArtifactRemoveReply>("artifacts", "remove", request, signal);
}
