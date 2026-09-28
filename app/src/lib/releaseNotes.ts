// Release notes for "What's new": docs/releases/v*.md are built into the app, so the window can
// show what changed since the version the user saw last, however they updated.
//
// A notes file is Markdown in English, optionally followed by `---` and translations that start
// with a `<!-- lang: ru -->` line. A "## Install" section is left out: it is for the download page.

const files = import.meta.glob("../../../docs/releases/v*.md", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;

export interface Release {
  version: string;
  notes: string;
}

/** `[1, 2, 0]` for "1.2.0" or "v1.2.0"; pre-release suffixes are ignored. */
function parts(version: string): number[] {
  return version
    .replace(/^v/, "")
    .split("-")[0]
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
}

export function compareVersions(a: string, b: string): number {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** The part of `markdown` for `language` (English if there is none), without "## Install". */
export function notesFor(markdown: string, language: string): string {
  const blocks = markdown.replace(/\r\n/g, "\n").split(/^---$/m);
  const tagged = blocks.map((b) => {
    const m = b.match(/^\s*<!--\s*lang:\s*([\w-]+)\s*-->\s*\n/);
    // A translation's own heading ("## По-русски") is for the GitHub page.
    return m ? { lang: m[1], text: b.slice(m[0].length).replace(/^\s*#+ .*\n/, "") } : { lang: "en", text: b };
  });
  const chosen = tagged.find((b) => b.lang === language) ?? tagged.find((b) => b.lang === "en") ?? tagged[0];
  return chosen.text.replace(/^## Install\b[\s\S]*?(?=^## |(?![\s\S]))/m, "").trim();
}

/** Releases newer than `since` and up to `current`, newest first. */
export function releasesSince(since: string, current: string, language: string): Release[] {
  return Object.entries(files)
    .map(([path, text]) => ({ version: path.match(/v([\d.]+[^/]*)\.md$/)![1], text }))
    .filter((r) => compareVersions(r.version, since) > 0 && compareVersions(r.version, current) <= 0)
    .sort((a, b) => compareVersions(b.version, a.version))
    .map((r) => ({ version: r.version, notes: notesFor(r.text, language) }));
}
