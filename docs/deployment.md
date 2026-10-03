# Static Deployment

## Table of contents

- [Architecture](#architecture)
- [Why this cannot create a hosting bill](#why-this-cannot-create-a-hosting-bill)
- [One-time local tooling](#one-time-local-tooling)
- [Refreshing deployment data](#refreshing-deployment-data)
- [Building and testing locally](#building-and-testing-locally)
- [Enabling GitHub Pages](#enabling-github-pages)
- [Deploying updates](#deploying-updates)
- [Published and excluded data](#published-and-excluded-data)
- [Limitations](#limitations)

## Architecture

The deployed calculator is a static GitHub Pages site. HTML, CSS, JavaScript, a compact data bundle, and the Rust calculation core compiled to WebAssembly are downloaded by each visitor. Build inspection, encounter estimation, grinding calculations, searches, and detail lookups execute in the visitor's browser. No hosted API, database, virtual machine, container, or metered compute service is involved.

The local `fo2-dps serve` command remains available and uses the same route dispatcher and calculation code. The static frontend sends its existing route-shaped requests to WebAssembly instead of HTTP, which keeps native and static behavior aligned.

## Why this cannot create a hosting bill

GitHub Pages does not require a cloud billing account for this deployment. The workflow uses GitHub's built-in Pages deployment token and does not contain payment credentials or third-party cloud secrets. If GitHub applies a usage limit, the consequence is throttling or unavailable builds rather than an automatically scaled compute bill.

Do not add a paid GitHub plan, metered external API, Git LFS storage, custom-domain purchase, or third-party deployment action with billing credentials if the strict no-cost property must remain intact.

## One-time local tooling

Install the Rust WebAssembly target and a `wasm-bindgen` CLI version matching `Cargo.toml`:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128 --locked
```

The standard native Rust toolchain is still required for scraping, validation, bundle generation, and tests.

## Refreshing deployment data

After completing a scrape, normalize and validate the authoritative snapshots before generating browser data:

```sh
cargo run -- normalize-data
cargo run -- validate-data
cargo run -- build-web-bundle
```

`build-web-bundle` writes `web/data/app-data.v1.json`. Commit this compact generated file with the corresponding source and frontend changes. GitHub Pages builds do not need the large raw mob HTML snapshot.

The bundle generator:

- Merges canonical and supplemental skills.
- Preserves item, set, skill, mob, drop, faction, location, spawn, tooltip, and calculation fields.
- Derives and preserves the current boss-candidate marker before removing source HTML.
- Removes raw page HTML, parsed raw tables, duplicate raw cells, and scrape-only fallback structures.
- Verifies schema and build identifiers.
- Loads the generated bundle through the shared route dispatcher and checks record counts.

## Building and testing locally

Build the complete static artifact with:

```sh
./scripts/build-pages.sh
```

The output is written to the ignored `dist/` directory. Serve it with any static file server, for example:

```sh
python -m http.server 4173 --directory dist
```

Then open `http://127.0.0.1:4173/`. The reusable `browser-check.spec.js` test verifies that the static site initializes WebAssembly, explores records, inspects a build, runs a grinding leaderboard, and makes no `/api` network requests.

The local Rust-backed application remains available separately:

```sh
cargo run -- serve
```

## Enabling GitHub Pages

1. Create a public GitHub repository and push the project to its `main` branch.
2. Open the repository's **Settings → Pages** screen.
3. Under **Build and deployment**, choose **GitHub Actions** as the source.
4. Open the **Actions** tab and run **Deploy static calculator to GitHub Pages**, or push to `main`.
5. After deployment, GitHub reports the generated `https://<account>.github.io/<repository>/` URL.

The workflow is `.github/workflows/pages.yml`. It installs the pinned Rust target and binding generator, runs native tests, builds the static artifact, uploads it to Pages, and deploys it using GitHub's built-in token.

## Deploying updates

For a normal code-only update:

```sh
cargo test --offline
./scripts/build-pages.sh
```

Push the validated source change to `main`; the Pages workflow deploys it.

For a data update:

```sh
cargo run -- normalize-data
cargo run -- validate-data
cargo run -- build-web-bundle
./scripts/build-pages.sh
```

Inspect and commit the changed `web/data/app-data.v1.json`, then push to `main`.

## Published and excluded data

The Pages artifact contains:

- `index.html`
- `styles.css`
- `app.js`
- WebAssembly bindings and the compiled calculation module
- `data/app-data.v1.json`
- An empty favicon and `.nojekyll`

It does not contain SQLite, scraper checkpoints, debug HTML, validation reports, raw mob page HTML, a server executable, or secrets.

## Limitations

- The repository and deployed game data are public.
- There is no private guild authentication in the free static design.
- First load downloads and parses the compact bundle and WebAssembly module; subsequent visits can use browser caching.
- Every visitor performs calculations locally, so performance depends on their device.
- GitHub may enforce Pages or Actions usage limits. A limit can stop deployment or availability but cannot trigger metered server scaling in this architecture.
- A custom domain is optional but the domain registration itself would not be free.
