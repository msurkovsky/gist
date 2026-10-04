# Review page assets

Compiled into `gk` and served by `gk md-review serve` under `/assets/`.
Design: `docs/md-review-page.md`.

- `page.html`, `page.css`, `page.js`: the page and its DOM glue.
- `anchor.js`, `margin.js`: pure functions, tested by `*.test.js` with
  `node --test` (`just test-page`). `package.json` only marks the
  directory as ES modules for Node; there are no npm dependencies.
- `mermaid.min.js`: mermaid 11.17.2, `dist/mermaid.min.js` from
  <https://registry.npmjs.org/mermaid/-/mermaid-11.17.2.tgz>, whose npm
  integrity was checked on download. MIT, see `mermaid.LICENSE`. Its
  SHA-256 is in `mermaid.min.js.sha256`, and a test in `serve.rs` fails
  when the embedded file differs from it
  ([ADR 0015](../../../../../../docs/adr/0015-md-review-http-stack.md)).

To update mermaid, replace `mermaid.min.js` and `mermaid.LICENSE` from
the new tarball, run `sha256sum mermaid.min.js > mermaid.min.js.sha256`
here, and change the version above.
