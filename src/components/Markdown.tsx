import type { ReactNode } from "react";
import * as ipc from "../lib/ipc";

/**
 * A small Markdown renderer for the three legal documents.
 *
 * These documents are ours and ship inside the binary, so the job is headings,
 * paragraphs, lists, tables and emphasis — not arbitrary HTML. Everything is
 * rendered to React elements rather than `innerHTML`, so there is no path from a
 * document to script execution whatever a future edit adds to them.
 *
 * The important part is that it groups *blocks* before rendering. The sources
 * are hard-wrapped at about 78 columns, so a renderer that treats one source
 * line as one paragraph turns every real paragraph into three stacked fragments
 * with gaps between them. Markdown's rule is that consecutive non-blank lines
 * are one paragraph, and that is what makes these read like documents.
 */

type Block =
  | { kind: "heading"; level: 1 | 2 | 3; text: string }
  | { kind: "para"; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "quote"; text: string }
  | { kind: "table"; head: string[]; rows: string[][] }
  | { kind: "rule" };

const isTableRow = (line: string) => line.startsWith("|");
/** The `| --- | --- |` line under a table header carries no content. */
const isTableDivider = (line: string) => /^\|[\s:|-]+\|?$/.test(line) && line.includes("-");

const cells = (line: string) =>
  line
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());

function parse(source: string): Block[] {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i] ?? "";

    if (line.trim() === "") {
      i += 1;
      continue;
    }

    if (/^(-{3,}|_{3,}|\*{3,})$/.test(line.trim())) {
      blocks.push({ kind: "rule" });
      i += 1;
      continue;
    }

    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({
        kind: "heading",
        level: heading[1]!.length as 1 | 2 | 3,
        text: heading[2]!.trim(),
      });
      i += 1;
      continue;
    }

    if (isTableRow(line)) {
      const raw: string[] = [];
      while (i < lines.length && isTableRow(lines[i] ?? "")) {
        raw.push(lines[i]!);
        i += 1;
      }
      const body = raw.filter((r) => !isTableDivider(r));
      const head = body.length > 0 ? cells(body[0]!) : [];
      blocks.push({ kind: "table", head, rows: body.slice(1).map(cells) });
      continue;
    }

    if (line.startsWith(">")) {
      const parts: string[] = [];
      while (i < lines.length && (lines[i] ?? "").startsWith(">")) {
        parts.push((lines[i] ?? "").replace(/^>\s?/, ""));
        i += 1;
      }
      // Blank quoted lines separate paragraphs *within* the quote; collapse the
      // hard wrapping inside each one but keep the breaks between them.
      const text = parts
        .join("\n")
        .split(/\n{2,}/)
        .map((p) => p.split("\n").join(" ").trim())
        .filter(Boolean)
        .join("\n\n");
      blocks.push({ kind: "quote", text });
      continue;
    }

    if (/^[-*]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*]\s+/.test(lines[i] ?? "")) {
        let item = (lines[i] ?? "").replace(/^[-*]\s+/, "");
        i += 1;
        // Indented continuations belong to the item above.
        while (i < lines.length && /^\s{2,}\S/.test(lines[i] ?? "")) {
          item += ` ${(lines[i] ?? "").trim()}`;
          i += 1;
        }
        items.push(item);
      }
      blocks.push({ kind: "list", items });
      continue;
    }

    const para: string[] = [];
    while (i < lines.length) {
      const next = lines[i] ?? "";
      if (
        next.trim() === "" ||
        next.startsWith("#") ||
        next.startsWith(">") ||
        isTableRow(next) ||
        /^[-*]\s+/.test(next)
      ) {
        break;
      }
      para.push(next.trim());
      i += 1;
    }
    blocks.push({ kind: "para", text: para.join(" ") });
  }

  return blocks;
}

/* ── inline formatting ─────────────────────────────────────────
   Bold, italic, code, and links. Deliberately no `_emphasis_`: these documents
   are full of identifiers like `fast_image_resize` and `%APPDATA%\Cursed`, and
   treating underscores as markup mangles every one of them.
   ──────────────────────────────────────────────────────────── */

const INLINE =
  /(\*\*[^*]+\*\*|\*[^*\s][^*]*\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\)|<https?:\/\/[^>\s]+>)/g;

function Inline({ text }: { text: string }): ReactNode {
  return text.split(INLINE).map((piece, index) => {
    const key = `${index}`;
    if (!piece) return null;

    if (piece.startsWith("**") && piece.endsWith("**")) {
      return (
        <strong key={key} className="font-medium text-text">
          {piece.slice(2, -2)}
        </strong>
      );
    }
    if (piece.startsWith("*") && piece.endsWith("*") && piece.length > 2) {
      return (
        <em key={key} className="text-text">
          {piece.slice(1, -1)}
        </em>
      );
    }
    if (piece.startsWith("`") && piece.endsWith("`")) {
      return (
        <code
          key={key}
          className="mono rounded-xs border border-border bg-bg px-1 py-px text-[10.5px] break-all text-accent-hi"
        >
          {piece.slice(1, -1)}
        </code>
      );
    }

    const md = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(piece);
    if (md) return <Link key={key} href={md[2]!} label={md[1]!} />;

    if (piece.startsWith("<") && piece.endsWith(">")) {
      const url = piece.slice(1, -1);
      return <Link key={key} href={url} label={url.replace(/^https?:\/\//, "")} />;
    }

    return <span key={key}>{piece}</span>;
  });
}

/**
 * Opens in the user's browser, never in the webview.
 *
 * A document that could navigate the app's own window would replace the entire
 * interface with a web page and leave no way back — the window has no address
 * bar and no reload.
 */
function Link({ href, label }: { href: string; label: string }) {
  const safe = /^https?:\/\//i.test(href);
  if (!safe) return <span className="text-text">{label}</span>;
  return (
    <button
      type="button"
      title={href}
      onClick={() => void ipc.openExternal(href).catch(() => undefined)}
      className="text-accent-hi underline decoration-accent/40 underline-offset-2 hover:decoration-accent-hi"
    >
      {label}
    </button>
  );
}

/* ── blocks ────────────────────────────────────────────────── */

/** Stable anchor for the section index. */
export const slug = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

/** Every `##` heading, for the jump index above a document. */
export function sections(source: string): { id: string; text: string }[] {
  return parse(source)
    .filter((b): b is Extract<Block, { kind: "heading" }> => b.kind === "heading" && b.level === 2)
    .map((b) => ({ id: slug(b.text), text: b.text }));
}

export function Markdown({ source }: { source: string }) {
  if (!source) {
    return (
      <div className="space-y-2" aria-busy>
        <div className="h-2 w-1/3 rounded-full shimmer" />
        <div className="h-2 w-full rounded-full shimmer" />
        <div className="h-2 w-4/5 rounded-full shimmer" />
      </div>
    );
  }

  return (
    <div className="text-[12px] leading-[1.6] text-text-muted">
      {parse(source).map((block, index) => {
        const key = `${index}`;

        switch (block.kind) {
          case "heading": {
            if (block.level === 1) {
              // The document title is already in the header bar above.
              return null;
            }
            const number = /^(\d+)\.\s*(.*)$/.exec(block.text);
            return (
              <h2
                key={key}
                id={slug(block.text)}
                className="display mt-6 mb-2 flex scroll-mt-3 items-center gap-2 text-[11px] text-text first:mt-0"
              >
                {number && (
                  <span className="mono grid h-5 w-5 shrink-0 place-items-center rounded-xs border border-accent/40 bg-accent-dim/50 text-[10px] text-accent-hi">
                    {number[1]}
                  </span>
                )}
                <span className="min-w-0 flex-1">{number ? number[2] : block.text}</span>
              </h2>
            );
          }

          case "para":
            return (
              <p key={key} className="mb-2.5">
                <Inline text={block.text} />
              </p>
            );

          case "list":
            return (
              <ul key={key} className="mb-2.5 space-y-1.5">
                {block.items.map((item, n) => (
                  <li key={`${n}`} className="flex gap-2">
                    <span className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-accent" />
                    <span className="min-w-0 flex-1">
                      <Inline text={item} />
                    </span>
                  </li>
                ))}
              </ul>
            );

          case "quote":
            return (
              <blockquote
                key={key}
                className="mb-2.5 rounded-xs border-l-2 border-accent/50 bg-elevated/60 py-2 pr-3 pl-3 text-[11px] text-text-dim"
              >
                {block.text.split("\n\n").map((p, n) => (
                  <p key={`${n}`} className="mb-1.5 last:mb-0">
                    <Inline text={p} />
                  </p>
                ))}
              </blockquote>
            );

          case "table":
            // Narrow window, so the table scrolls sideways inside its own box
            // rather than forcing the whole document to.
            return (
              <div
                key={key}
                className="mb-2.5 overflow-x-auto rounded-xs border border-border"
              >
                <table className="w-full border-collapse text-left text-[10.5px]">
                  <thead>
                    <tr className="bg-elevated">
                      {block.head.map((cell, n) => (
                        <th
                          key={`${n}`}
                          className="display px-2 py-1.5 text-[9px] whitespace-nowrap text-text-dim"
                        >
                          {cell}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, n) => (
                      <tr key={`${n}`} className="border-t border-border">
                        {row.map((cell, m) => (
                          <td
                            key={`${m}`}
                            className={
                              m === 0
                                ? "px-2 py-1.5 whitespace-nowrap text-text"
                                : "px-2 py-1.5 whitespace-nowrap"
                            }
                          >
                            <Inline text={cell} />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );

          case "rule":
            return <hr key={key} className="my-4 border-0 border-t border-border" />;
        }
      })}
    </div>
  );
}
