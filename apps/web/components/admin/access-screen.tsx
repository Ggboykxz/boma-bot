"use client";

import Link from "next/link";
import { RiErrorWarningLine, RiRefreshLine } from "@remixicon/react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/base/buttons/button";
import {
  adminGrantsCreate,
  adminGrantsList,
  adminGrantsRevoke,
  adminInvitationsList,
  adminInvitationsRevoke,
  adminMembersList,
  type AdminGrantView,
  type AdminInvitationView,
  type AdminMemberView,
} from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

type Phase = "loading" | "ready" | "error" | "forbidden";

/**
 * `/admin/access` (spec/08-ui-ux.md §8.7, REQ-058).
 *
 * One screen, three sections: who belongs to this space, which invitations are still pending,
 * and which deployment-level access grants exist. The role changes the screen performs are
 * deliberately absent for now: the grants are the new server-level decision the product is
 * missing, and a role flip needs the same evidence flow, recorded, not optimistic guessing.
 */
export function AccessScreen() {
  const t = useTranslation();

  const [members, setMembers] = useState<AdminMemberView[]>([]);
  const [invitations, setInvitations] = useState<AdminInvitationView[]>([]);
  const [grants, setGrants] = useState<AdminGrantView[]>([]);
  const [phase, setPhase] = useState<Phase>("loading");
  const [reloadToken, setReloadToken] = useState(0);
  const [email, setEmail] = useState("");
  const [reason, setReason] = useState("");
  const [granting, setGranting] = useState(false);
  const [grantError, setGrantError] = useState<string | null>(null);
  const [revokingEmail, setRevokingEmail] = useState<string | null>(null);
  const [revokingInvitation, setRevokingInvitation] = useState<string | null>(null);

  const reload = useCallback(() => {
    setPhase("loading");
    setReloadToken((token) => token + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    Promise.all([
      adminMembersList(controller.signal),
      adminInvitationsList(controller.signal),
      adminGrantsList(controller.signal),
    ])
      .then(([membersReply, invitationsReply, grantsReply]) => {
        if (controller.signal.aborted) {
          return;
        }
        setMembers(membersReply.members);
        setInvitations(invitationsReply.invitations);
        setGrants(grantsReply.grants);
        setPhase("ready");
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        const forbidden = typeof error === "object" && error !== null && "code" in error && (error as { code?: unknown }).code === "FORBIDDEN";
        setPhase(forbidden ? "forbidden" : "error");
      });

    return () => {
      controller.abort();
    };
  }, [reloadToken]);

  const createGrant = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      const trimmed = email.trim().toLowerCase();
      if (!trimmed || !reason.trim()) {
        return;
      }
      setGranting(true);
      setGrantError(null);
      try {
        const reply = await adminGrantsCreate({ email: trimmed, reason: reason.trim() });
        // Optimistic insert is honest: the server echoed the stored row back.
        setGrants((current) =>
          [...current, reply.grant].sort((left, right) => left.email.localeCompare(right.email)),
        );
        setEmail("");
        setReason("");
      } catch (error: unknown) {
        setGrantError(error instanceof Error ? error.message : t("admin.plugins.error"));
      } finally {
        setGranting(false);
      }
    },
    [email, reason, t],
  );

  const revokeGrant = useCallback(
    async (target: string) => {
      setRevokingEmail(target);
      try {
        await adminGrantsRevoke({ email: target });
        setGrants((current) => current.filter((grant) => grant.email !== target));
      } catch {
        setGrantError(t("admin.plugins.error"));
      } finally {
        setRevokingEmail(null);
      }
    },
    [t],
  );

  const revokeInvitation = useCallback(
    async (invitationId: string) => {
      setRevokingInvitation(invitationId);
      try {
        await adminInvitationsRevoke({ invitationId });
        setInvitations((current) => current.filter((invitation) => invitation.id !== invitationId));
      } catch {
        // The row stays visible; a refresh restores truth.
      } finally {
        setRevokingInvitation(null);
      }
    },
    [],
  );

  const sections = useMemo(() => phase === "ready", [phase]);

  return (
    <div className="bg-background-full min-h-dvh">
      <header className="border-separator-border border-b">
        <nav
          aria-label={t("shell.nav.primary")}
          className="mx-auto flex w-full max-w-5xl items-center justify-between gap-2 px-4 py-3"
        >
          <div className="flex items-center gap-4">
            <Link href="/" className="boma-target boma-focus text-text-primary hover:text-text-secondary text-headline-medium inline-flex items-center rounded-md px-2 py-1">
              {t("app.name")}
            </Link>
            <span className="text-text-secondary text-body-2-medium uppercase tracking-wide">{t("admin.access.title")}</span>
          </div>
          <div className="flex items-center gap-2">
            <Link href="/admin/plugins" className="boma-target boma-focus text-text-primary hover:bg-background-secondary-hover inline-flex items-center rounded-md px-3 py-1.5 text-body-medium">
              {t("admin.plugins.title")}
            </Link>
          </div>
        </nav>
      </header>

      <main className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 py-6">
        <h1 className="text-title-3-semibold text-text-primary">{t("admin.access.title")}</h1>

        {phase === "loading" ? (
          <div aria-hidden="true" className="flex flex-col gap-4">
            <div className="bg-background-secondary-default h-40 w-full rounded-3xl" />
            <div className="bg-background-secondary-default h-28 w-full rounded-3xl" />
            <div className="bg-background-secondary-default h-28 w-full rounded-3xl" />
          </div>
        ) : null}

        {phase === "forbidden" ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-4">
            <RiErrorWarningLine className="text-foreground-icon-primary h-6 w-6" aria-hidden="true" />
            <p className="text-text-primary text-body-medium">{t("admin.access.forbidden")}</p>
          </div>
        ) : null}

        {phase === "error" ? (
          <div className="border-border-error-default flex flex-col items-start gap-2 rounded-xl border p-4">
            <RiErrorWarningLine className="text-foreground-icon-primary h-6 w-6" aria-hidden="true" />
            <p className="text-text-primary text-body-medium">{t("admin.access.error")}</p>
            <Button variant="secondary" size="medium" leadingIcon={RiRefreshLine} onClick={reload}>
              {t("common.retry")}
            </Button>
          </div>
        ) : null}

        {sections ? (
          <>
            <section aria-labelledby="admin-members" className="border-border-button-default overflow-hidden rounded-3xl border">
              <h2 id="admin-members" className="bg-background-secondary-default text-text-primary text-headline-medium border-separator-border border-b px-4 py-3">
                {t("admin.access.members")}
              </h2>
              <div className="overflow-x-auto">
                <table className="text-text-primary w-full min-w-[560px] border-collapse text-body-regular">
                  <thead>
                    <tr className="text-text-secondary text-start text-body-2-medium">
                      <th scope="col" className="border-separator-border border-b px-4 py-2 text-start">Name</th>
                      <th scope="col" className="border-separator-border border-b px-4 py-2 text-start">{t("admin.access.grants.email")}</th>
                      <th scope="col" className="border-separator-border border-b px-4 py-2 text-start">Role</th>
                    </tr>
                  </thead>
                  <tbody>
                    {members.length === 0 ? (
                      <tr><td colSpan={3} className="text-text-tertiary px-4 py-6 text-center">{t("admin.access.members.empty")}</td></tr>
                    ) : (
                      members.map((member) => (
                        <tr key={member.userId}>
                          <td className="border-separator-border border-b px-4 py-2">{member.name || "—"}</td>
                          <td dir="auto" className="border-separator-border border-b px-4 py-2">{member.email}</td>
                          <td className="border-separator-border border-b px-4 py-2">{member.role}</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            </section>

            <section aria-labelledby="admin-invitations" className="border-border-button-default overflow-hidden rounded-3xl border">
              <h2 id="admin-invitations" className="bg-background-secondary-default text-text-primary text-headline-medium border-separator-border border-b px-4 py-3">
                {t("admin.access.invitations")}
              </h2>
              {invitations.length === 0 ? (
                <p className="text-text-tertiary px-4 py-4 text-body-regular">{t("admin.access.invitations.empty")}</p>
              ) : (
                <ul className="divide-separator-border divide-y">
                  {invitations.map((invitation) => (
                    <li key={invitation.id} className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
                      <div className="flex min-w-0 flex-col">
                        <span dir="auto" className="text-text-primary text-body-medium">{invitation.email}</span>
                        <span className="text-text-tertiary text-body-2-medium">{invitation.role}{invitation.expiresAt ? ` · ${invitation.expiresAt}` : ""}</span>
                      </div>
                      <Button
                        variant="secondary"
                        size="small"
                        onClick={() => revokeInvitation(invitation.id)}
                        disabled={revokingInvitation === invitation.id}
                      >
                        {t("admin.access.invitations.revoke")}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <section aria-labelledby="admin-grants" className="border-border-button-default overflow-hidden rounded-3xl border">
              <h2 id="admin-grants" className="bg-background-secondary-default text-text-primary text-headline-medium border-separator-border border-b px-4 py-3">
                {t("admin.access.grants")}
              </h2>
              <form
                onSubmit={createGrant}
                className="border-separator-border flex flex-wrap items-end gap-3 border-b px-4 py-3"
              >
                <label className="flex min-w-56 flex-1 flex-col gap-1">
                  <span className="text-text-secondary text-body-2-medium">{t("admin.access.grants.email")}</span>
                  <input
                    required
                    autoComplete="off"
                    inputMode="email"
                    aria-label={t("admin.access.grants.email")}
                    className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-9 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                    placeholder="person@example.com"
                    value={email}
                    onChange={(event) => setEmail(event.target.value)}
                  />
                </label>
                <label className="flex min-w-56 flex-1 flex-col gap-1">
                  <span className="text-text-secondary text-body-2-medium">{t("admin.access.grants.reason")}</span>
                  <input
                    required
                    autoComplete="off"
                    aria-label={t("admin.access.grants.reason")}
                    maxLength={200}
                    className="boma-focus text-text-primary bg-background-primary-default border-border-button-default h-9 rounded-md border px-3 text-body-regular placeholder:text-text-placeholder"
                    placeholder="Why this grant exists"
                    value={reason}
                    onChange={(event) => setReason(event.target.value)}
                  />
                </label>
                <Button variant="primary" size="medium" type="submit" disabled={granting}>
                  {t("admin.access.grants.create")}
                </Button>
              </form>
              {grantError ? (
                <p className="text-text-error-primary px-4 pt-2 text-body-2-medium" role="alert">{grantError}</p>
              ) : null}
              {grants.length === 0 ? (
                <p className="text-text-tertiary px-4 py-4 text-body-regular">{t("admin.access.grants.empty")}</p>
              ) : (
                <ul className="divide-separator-border divide-y">
                  {grants.map((grant) => (
                    <li key={grant.email} className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
                      <div className="flex min-w-0 flex-col">
                        <span dir="auto" className="text-text-primary text-body-medium">{grant.email}</span>
                        <span className="text-text-tertiary text-body-2-medium">{grant.reason}</span>
                      </div>
                      <div className="flex items-center gap-3">
                        <span className="text-text-tertiary text-caption-1-semibold">{t("admin.access.grants.created")} {grant.grantedBy}</span>
                        <Button
                          variant="secondary"
                          size="small"
                          onClick={() => revokeGrant(grant.email)}
                          disabled={revokingEmail === grant.email}
                        >
                          {t("admin.access.grants.revoke")}
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </>
        ) : null}
      </main>
    </div>
  );
}
