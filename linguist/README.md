# GitHub Linguist materials for Wand

Goal: make GitHub recognise `.w` and `.wh` as **Wand** instead of CWeb / OpenEdge ABL
(`.w` is already claimed by both; `.wh` is currently unclaimed).

Nothing here changes the Wand language or the compiler. These files only help editors
and GitHub describe the existing syntax.

## Path A — immediate, per-repository override

Add to `.gitattributes` in the repository that contains Wand sources:

```gitattributes
*.w  linguist-language=Wand
*.wh linguist-language=Wand
```

Caveats:

- This is a **local override**: it tells Linguist which language to report for files in
  that repository. It does **not** create a Wand language definition on GitHub.
- Until Wand is registered upstream, expect no Wand language colour, no useful language
  statistics, and no Wand-specific syntax highlighting. Files will often render as
  plain text or with a fallback grammar.
- The override is not inherited by forks via GitHub's language stats in the same way as
  upstream registration; each repository that wants it needs its own `.gitattributes`.
- Zed does not read `.gitattributes`; install the dev extension in `zed-wand/` for
  editor highlighting. The two mechanisms are independent.

This is the safe shortcut for the wand2c, UTMS7 and benchmark repositories today.

## Path B — upstream registration in `github-linguist/linguist`

A real registration requires a PR against <https://github.com/github-linguist/linguist>.
Use `PR_TEMPLATE.md` in this directory; an unfilled template will not be reviewed.

Hard requirements (from Linguist's `CONTRIBUTING.md`):

- **At least 2000 `.w` files indexed by GitHub code search in the past year, excluding
  forks** (`.w` is expected to occur more than once per repo). Provide the search URL
  from the template.
- Results must show a reasonable distribution across distinct `user/repo` pairs. If
  one account dominates (for example the language author), maintainers will ask for a
  filtered query such as `-user:USERNAME`.
- Real-world samples only; "hello world" and tutorial examples are rejected. Include
  the source URL and the licence for each sample.
- Because `.w` is already assigned to CWeb and OpenEdge ABL, add a disambiguation
  heuristic. Linguist also needs at least two real `.w` samples for **each** language
  sharing the extension.
- Omit `language_id` from the new entry; upstream generates it with `script/update-ids`.

Files to prepare for the PR:

- `languages.yml.snippet` — paste into `lib/linguist/languages.yml`. It uses
  `tm_scope: source.c` because the Zed extension deliberately reuses Zed's built-in C
  grammar. GitHub then renders Wand with its C TextMate grammar (C-ish highlighting,
  not Wand-specific). If a TextMate `source.wand` grammar is added later, change only
  that field and add the grammar with `script/add-grammar`.
- `heuristics.yml.snippet` — current Linguist format; paste the `.w` block into
  `lib/linguist/heuristics.yml`.
- `heuristic.rb.snippet` — the legacy Ruby form of the same idea, kept because the
  task brief asked for a `.rb` snippet. Modern upstream does **not** take Ruby rules;
  use the YAML snippets for an actual PR.
- `samples/` — three short, real files (source paths are recorded in comments at the
  top of each file). Copy them into `samples/Wand/` in the Linguist repo when filing
  the PR:
  - `fibonacci.w` and `section_struct_array.w` come from the wand2c benchmark/probe
    corpus, covered by the repository's UOPL v1.6.4 `LICENSE`.
  - `utms7_ip.w` comes from `utms7/apps/net/ip.w`, covered by UTMS7's UOPL v1.6.4
    `LICENSE`. Confirm with the sample owners before submitting upstream.

Until the upstream PR is merged, Path A is the only way to influence GitHub's language
label. After a merge it can take a GitHub release/deploy before the change is visible;
see Linguist's troubleshooting docs.

## Current blocker

Public GitHub usage almost certainly does not meet the 2000-file threshold yet, and
there is no TextMate `source.wand` grammar. Registering Wand upstream is therefore not
ready to submit; the material here is prepared so it can be submitted once those facts
change. Do not weaken the heuristic to make an undersized PR look viable.
