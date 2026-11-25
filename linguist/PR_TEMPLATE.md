<!---
DRAFT for github-linguist/linguist. Copy the body below into a new PR.
Every `FILL IN:` marker MUST be replaced; Linguist will not review an
unfilled template. Do not submit until the 2000-file requirement is met.
-->

## Description

Add **Wand** (also called WandC) as a programming language for `.w` and `.wh`.
Wand is a low-level systems language with C-like blocks, an `sc.true`/`sc.false`
environment token on line one, `#import` module imports, `fn` functions, Wand
primitive types (`u64`, `i32`, `f64`, ...), `sect`/`EOS` global sections and
`*adr` address-taking.

`.w` is currently shared with CWeb and OpenEdge ABL, so the PR also adds a
content heuristic. `.wh` is Wand declaration files and is currently unclaimed.

Links: FILL IN: official Wand repository / documentation URL.

## Checklist

- [x] **I am adding a new language.**
  - [ ] The extension is used in at least 2000 files indexed in the last year,
        excluding forks (required for `.w`, which occurs more than once per repo).
    - Search results for each extension (FILL IN: paste the result count shown at
      the top of each page and confirm the count is >= 2000 for `.w`):
      - `.w`: https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.w+%22sc.true%22
      - `.w` (freestanding): https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.w+%22sc.false%22
      - `.wh`: https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.wh+%22sc.true%22
    - FILL IN: confirm results span many distinct `user/repo` pairs; if one account
      dominates, add `-user:USERNAME` to the query and paste that URL too.
  - [x] I have included real-world usage samples for both extensions.
    - Sample source(s):
      - FILL IN: public URL for the source of `samples/Wand/fibonacci.w`
      - FILL IN: public URL for the source of `samples/Wand/section_struct_array.w`
      - FILL IN: public URL for the source of `samples/Wand/utms7_ip.w`
    - Sample license(s):
      - FILL IN: UOPL v1.6.4 (see source `LICENSE`); confirm each sample owner
        agrees to upstream inclusion, or replace with samples carrying a license
        Linguist accepts.
  - [ ] I have included a syntax highlighting grammar.
    - FILL IN: choose one. Current snippet uses `tm_scope: source.c` because the
      Zed extension reuses Zed's C grammar; GitHub would highlight Wand as C.
      If maintainers require a Wand-specific TextMate grammar, add it with
      `script/add-grammar URL` and change `tm_scope` to `source.wand`.
      - Grammar URL: FILL IN
  - [x] I have added a color: `#FFB86C`.
    - Rationale: warm amber chosen to be visually distinct from CWeb (`#00007a`)
      and OpenEdge ABL (`#5ce600`); no official Wand branding guidelines yet.
  - [x] I have updated the heuristics to distinguish Wand from other languages
        using `.w` (see `heuristics.yml.snippet`).
  - [ ] `language_id` is omitted; `script/update-ids` has been run upstream.

## Before submitting — required facts to verify

- [ ] Code-search result counts meet the ≥2000 indexed-file rule **per extension**
      and exclude forks (`.wh` may be assessed under the 200-file rule if it occurs
      only once per repository; confirm with maintainers).
- [ ] At least two real `.w` samples exist for Wand **and** for CWeb/OpenEdge ABL.
- [ ] Sample licences and source URLs are filled in above.
- [ ] `script/check-regex-compatibility lib/linguist/heuristics.yml` passes.
- [ ] `bundle exec rake test` passes.
