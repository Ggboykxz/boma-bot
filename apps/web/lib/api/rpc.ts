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
    createdAt: typeof message.createdAt === "string" ? message.createdAt : "",
  };
}

/** Whether a value is one of the four roles the domain spells. */
export function isMessageRole(value: unknown): value is MessageRole {
  return value === "user" || value === "bot" || value === "system" || value === "tool";
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
