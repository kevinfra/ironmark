# Third-Party Notices

`ironmark` embeds or depends on the following third-party open-source software.

## Bundled Assets

### Mermaid (`assets/mermaid.min.js.gz`)

- **Project:** [Mermaid](https://mermaid.js.org/)
- **Version:** `11.17.2`
- **Source:** npm package `mermaid@11.17.2` (`dist/mermaid.min.js`, gzip-compressed as `assets/mermaid.min.js.gz`)
  - URL: `https://www.npmjs.com/package/mermaid/v/11.17.2`
- **SHA-256 (`assets/mermaid.min.js.gz`):**
  `f13232c47193bc5f8e5f8c38260b1fda5b82189b1eeaa068472f3fae69483bff`
- **License:** MIT License

```text
The MIT License (MIT)

Copyright (c) 2014 - 2024 Knut Sveidqvist

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

#### Updating the bundled Mermaid asset

```bash
# Example: download dist/mermaid.min.js from npm and re-compress
curl -fsSL "https://cdn.jsdelivr.net/npm/mermaid@<VERSION>/dist/mermaid.min.js" | gzip -9 > assets/mermaid.min.js.gz
shasum -a 256 assets/mermaid.min.js.gz
```

Update the version and SHA-256 checksum in this file whenever `assets/mermaid.min.js.gz` is refreshed.

## Rust Crates

Direct Rust dependencies are declared in `Cargo.toml` and pinned in `Cargo.lock` (including `tao`, `wry`, `pulldown-cmark`, `ammonia`, `flate2`, `notify`, `rfd`, `open`, `serde`, `serde_json`, and `percent-encoding`), each under its respective open-source license (MIT / Apache-2.0).
