<script lang="ts" module>
  // Just enough Markdown for release notes: headings, bullet and numbered lists, paragraphs,
  // **bold**, *italic*, `code` and [links](https://…). Text is never parsed as HTML.

  type Inline = { kind: "text" | "strong" | "em" | "code"; text: string } | { kind: "link"; text: string; href: string };
  type Block = { kind: "h"; inline: Inline[] } | { kind: "p"; inline: Inline[] } | { kind: "ul" | "ol"; items: Inline[][] };

  const INLINE = /\*\*(.+?)\*\*|\*(.+?)\*|_(.+?)_|`(.+?)`|\[(.+?)\]\((https:\/\/[^\s)]+)\)/g;

  function inline(text: string): Inline[] {
    const out: Inline[] = [];
    let last = 0;
    for (const m of text.matchAll(INLINE)) {
      if (m.index > last) out.push({ kind: "text", text: text.slice(last, m.index) });
      if (m[1]) out.push({ kind: "strong", text: m[1] });
      else if (m[2] || m[3]) out.push({ kind: "em", text: m[2] ?? m[3] });
      else if (m[4]) out.push({ kind: "code", text: m[4] });
      else out.push({ kind: "link", text: m[5], href: m[6] });
      last = m.index + m[0].length;
    }
    if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
    return out;
  }

  export function parse(markdown: string): Block[] {
    const blocks: Block[] = [];
    let para: string[] = [];
    let list: { kind: "ul" | "ol"; items: string[] } | null = null;
    const flush = () => {
      if (para.length) blocks.push({ kind: "p", inline: inline(para.join(" ")) });
      if (list) blocks.push({ kind: list.kind, items: list.items.map(inline) });
      para = [];
      list = null;
    };
    for (const raw of markdown.replace(/\r\n/g, "\n").split("\n")) {
      const line = raw.trim();
      const heading = line.match(/^#{1,6}\s+(.*)$/);
      const item = line.match(/^([-*]|\d+\.)\s+(.*)$/);
      if (!line) flush();
      else if (heading) {
        flush();
        blocks.push({ kind: "h", inline: inline(heading[1]) });
      } else if (item) {
        const kind = /\d/.test(item[1]) ? "ol" : "ul";
        if (para.length || (list && list.kind !== kind)) flush();
        list ??= { kind, items: [] };
        list.items.push(item[2]);
      } else if (list && raw.startsWith(" ")) {
        list.items[list.items.length - 1] += ` ${line}`;
      } else {
        if (list) flush();
        para.push(line);
      }
    }
    flush();
    return blocks;
  }
</script>

<script lang="ts">
  import * as api from "./api";

  let { source }: { source: string } = $props();
  const blocks = $derived(parse(source));
</script>

{#snippet spans(parts: Inline[])}
  {#each parts as part}
    {#if part.kind === "strong"}<strong>{part.text}</strong>
    {:else if part.kind === "em"}<em>{part.text}</em>
    {:else if part.kind === "code"}<code>{part.text}</code>
    {:else if part.kind === "link"}<a
        href={part.href}
        onclick={(e) => {
          e.preventDefault();
          api.openUrl(part.href);
        }}>{part.text}</a>
    {:else}{part.text}{/if}
  {/each}
{/snippet}

<div class="md">
  {#each blocks as block}
    {#if block.kind === "h"}
      <h3>{@render spans(block.inline)}</h3>
    {:else if block.kind === "p"}
      <p>{@render spans(block.inline)}</p>
    {:else if block.kind === "ul"}
      <ul>
        {#each block.items as item}<li>{@render spans(item)}</li>{/each}
      </ul>
    {:else}
      <ol>
        {#each block.items as item}<li>{@render spans(item)}</li>{/each}
      </ol>
    {/if}
  {/each}
</div>

<style>
  .md {
    font-size: 13px;
    line-height: 1.5;
    text-wrap: pretty;
  }
  .md > :first-child {
    margin-top: 0;
  }
  .md > :last-child {
    margin-bottom: 0;
  }
  h3 {
    margin: 14px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  p {
    margin: 6px 0;
  }
  ul,
  ol {
    margin: 6px 0;
    padding-inline-start: 20px;
  }
  li + li {
    margin-top: 3px;
  }
  code {
    font-family: "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--control-hover);
  }
  a {
    color: var(--accent-text);
  }
</style>
