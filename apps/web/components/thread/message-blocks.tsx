"use client";

import { RiCheckboxLine, RiFileTextLine, RiImageLine, RiTerminalLine } from "@remixicon/react";

import type { Block } from "@/lib/api/rpc";
import { useTranslation } from "@/lib/use-translation";

/**
 * The blocks of one message (spec/08-ui-ux.md §8.3).
 *
 * Text is plain text: there is no markdown engine in M1, and the text a bot writes is data, not
 * markup. The other blocks are one line each, except the tool call, which is a disclosure: one
 * line when it is collapsed, the details when it is opened.
 */
export function MessageBlocks({ blocks }: { blocks: readonly Block[] }) {
  return (
    <div className="flex flex-col gap-2">
      {blocks.map((block, index) => (
        <BlockView key={`${block.type}-${index}`} block={block} />
      ))}
    </div>
  );
}

function BlockView({ block }: { block: Block }) {
  const t = useTranslation();

  switch (block.type) {
    case "text":
      return (
        <p className="text-text-primary text-body-regular whitespace-pre-wrap break-words">
          {block.text}
        </p>
      );

    case "image":
      return (
        <p className="text-text-secondary text-body-2-regular flex items-center gap-2">
          <RiImageLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          <span>{t("thread.block.image")}</span>
          <span className="text-text-tertiary break-all">{block.artifactId}</span>
        </p>
      );

    case "file":
      return (
        <p className="text-text-secondary text-body-2-regular flex items-center gap-2">
          <RiFileTextLine className="text-foreground-icon-secondary" size={18} aria-hidden />
          <span className="text-text-primary break-all">{block.name}</span>
          <span>{t("thread.block.file")}</span>
        </p>
      );

    case "tool":
      return (
        <details className="border-separator-border rounded-xl border px-3 py-2">
          <summary className="boma-focus text-text-secondary flex cursor-pointer items-center gap-2 text-body-2-medium">
            <RiTerminalLine className="text-foreground-icon-secondary" size={18} aria-hidden />
            <span>{t("thread.block.tool")}</span>
            <span className="text-text-primary">{block.name}</span>
            <span className="text-text-tertiary">
              {block.running ? t("thread.tool.running") : t("thread.tool.finished")}
            </span>
          </summary>
          <p className="text-text-secondary text-body-2-regular mt-2">
            {t("thread.block.tool")}: {block.name}
          </p>
        </details>
      );

    case "card":
      return (
        <section
          role="region"
          aria-label={block.kind}
          className="bg-background-secondary-default border-separator-border border rounded-3xl p-3 flex flex-col gap-2"
        >
          <p className="text-text-primary text-body-medium">
            {t("thread.block.card")}
          </p>
          <p className="text-text-tertiary text-body-2-regular">{block.kind}</p>
          <button
            type="button"
            className="boma-target boma-focus bg-background-primary-default border-border-button-default border text-text-primary text-body-medium rounded-md px-3 py-2 hover:bg-background-secondary-hover inline-flex items-center gap-2"
          >
            {t("thread.card.approve")}
          </button>
          <input
            type="text"
            placeholder={t("thread.card.answer")}
            className="border-border-button-default border rounded-md bg-background-primary-default text-text-primary px-2 py-1 text-body-regular focus-visible:ring-2 focus-visible:ring-border-focus-ring outline-none"
          />
        </section>
      );

    default:
      return null;
  }
}
