import { Fragment, type ReactNode } from "react";

interface MarkdownDocumentProps {
  content: string;
}

interface MarkdownBlock {
  type: "heading" | "paragraph" | "list" | "rule";
  level?: number;
  lines: string[];
}

export function MarkdownDocument({ content }: MarkdownDocumentProps) {
  return (
    <div className="markdown-document">
      {parseMarkdown(content).map((block, index) => {
        const key = `${block.type}-${index}`;

        if (block.type === "heading") {
          if (block.level === 1) return <h1 key={key}>{renderInline(block.lines[0] ?? "")}</h1>;
          if (block.level === 2) return <h2 key={key}>{renderInline(block.lines[0] ?? "")}</h2>;
          return <h3 key={key}>{renderInline(block.lines[0] ?? "")}</h3>;
        }

        if (block.type === "list") {
          return (
            <ul key={key}>
              {block.lines.map((line, lineIndex) => (
                <li key={`${line}-${lineIndex}`}>{renderInline(line)}</li>
              ))}
            </ul>
          );
        }

        if (block.type === "rule") return <hr key={key} />;

        return <p key={key}>{renderInline(block.lines.join(" "))}</p>;
      })}
    </div>
  );
}

function parseMarkdown(content: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  let paragraph: string[] = [];
  let list: string[] = [];

  function flushParagraph() {
    if (paragraph.length > 0) blocks.push({ type: "paragraph", lines: paragraph });
    paragraph = [];
  }

  function flushList() {
    if (list.length > 0) blocks.push({ type: "list", lines: list });
    list = [];
  }

  for (const rawLine of content.replaceAll("\r\n", "\n").split("\n")) {
    const line = rawLine.trim();
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    const listItem = /^[-*]\s+(.+)$/.exec(line);

    if (!line) {
      flushParagraph();
      flushList();
    } else if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ type: "heading", level: heading[1]?.length, lines: [heading[2] ?? ""] });
    } else if (listItem) {
      flushParagraph();
      list.push(listItem[1] ?? "");
    } else if (/^---+$/.test(line)) {
      flushParagraph();
      flushList();
      blocks.push({ type: "rule", lines: [] });
    } else {
      flushList();
      paragraph.push(line);
    }
  }

  flushParagraph();
  flushList();
  return blocks;
}

function renderInline(content: string): ReactNode[] {
  return content
    .split(/(\*\*[^*]+\*\*|`[^`]+`)/g)
    .filter(Boolean)
    .map((part, index) => {
      if (part.startsWith("**") && part.endsWith("**")) {
        return <strong key={`${part}-${index}`}>{part.slice(2, -2)}</strong>;
      }
      if (part.startsWith("`") && part.endsWith("`")) {
        return <code key={`${part}-${index}`}>{part.slice(1, -1)}</code>;
      }
      return <Fragment key={`${part}-${index}`}>{part}</Fragment>;
    });
}
