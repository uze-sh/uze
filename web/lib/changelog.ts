import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { getTableOfContents } from 'fumadocs-core/content/toc';

// The changelog is written by git-cliff at the repository root on every
// release (see cliff.toml); the page renders that file instead of keeping a
// copy under content/docs that would drift from it.
const changelogPath = join(process.cwd(), '..', 'CHANGELOG.md');

// `## [1.0.0-beta.1](…/compare/v…...v…) - 2026-09-27`, or `## [Unreleased]`.
const versionHeading = /^## \[([^\]]+)\](?:\(([^)]+)\))?(?: - (\S+))?\s*$/gm;

export function loadChangelog() {
  const raw = readFileSync(changelogPath, 'utf8');
  // The file's preamble explains how it is generated, which is the
  // maintainer's concern; the page starts at the newest release.
  const releases = raw.slice(Math.max(raw.search(/^## /m), 0));

  // A version heading carries its own id so a release keeps the same anchor
  // however its date or compare link changes; the date and the diff move to
  // the line beneath it, where they read as metadata rather than as the title.
  const body = releases.replace(versionHeading, (_, version: string, url?: string, date?: string) => {
    const meta = [date, url && `[Compare changes](${url})`].filter(Boolean).join(' · ');
    return `## ${version} [#${version}]${meta ? `\n\n${meta}` : ''}`;
  });

  return {
    body,
    versions: getTableOfContents(body).filter((item) => item.depth === 2),
  };
}
