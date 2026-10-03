use std::fs;

use anyhow::{Context, Result, ensure};

use crate::db::{Item, ItemSet, Skill, load_skills};
use crate::mobs::Mob;
use crate::web::{WEB_BUILD_ID, WebBundle, WebData, dispatch_json};

const DIST_DIR: &str = "dist";
const SOURCE_BUNDLE_PATH: &str = "web/data/app-data.v1.json";
const DIST_BUNDLE_PATH: &str = "dist/data/app-data.v1.json";

pub fn build_web_bundle() -> Result<()> {
    let items: Vec<Item> = load_json("data/items.json")?;
    let item_sets: Vec<ItemSet> = load_json("data/item-sets.json")?;
    let skills: Vec<Skill> = load_skills()?;
    let mobs: Vec<Mob> = load_json("data/mobs.json")?;

    let bundle = WebBundle {
        schema_version: 2,
        web_build_id: WEB_BUILD_ID.to_owned(),
        items,
        item_sets,
        skills,
        mobs,
    };
    verify_bundle(&bundle)?;

    fs::create_dir_all("web/data")?;
    fs::write(SOURCE_BUNDLE_PATH, serde_json::to_vec(&bundle)?)?;

    let bytes = fs::metadata(SOURCE_BUNDLE_PATH)?.len();
    println!(
        "Built compact browser data at {SOURCE_BUNDLE_PATH} ({} items, {} sets, {} skills, {} mobs; {:.2} MiB)",
        bundle.items.len(),
        bundle.item_sets.len(),
        bundle.skills.len(),
        bundle.mobs.len(),
        bytes as f64 / 1_048_576.0
    );
    println!(
        "Commit the compact bundle, then run scripts/build-pages.sh to assemble the static site."
    );
    Ok(())
}

pub fn assemble_pages() -> Result<()> {
    let bundle: WebBundle = load_json(SOURCE_BUNDLE_PATH)?;
    verify_bundle(&bundle)?;

    fs::create_dir_all(format!("{DIST_DIR}/data"))?;
    fs::write(DIST_BUNDLE_PATH, fs::read(SOURCE_BUNDLE_PATH)?)?;
    fs::write(
        format!("{DIST_DIR}/styles.css"),
        fs::read("web/styles.css")?,
    )?;
    fs::write(format!("{DIST_DIR}/app.js"), fs::read("web/app.js")?)?;
    fs::write(format!("{DIST_DIR}/.nojekyll"), [])?;
    fs::write(format!("{DIST_DIR}/favicon.ico"), [])?;

    let index = fs::read_to_string("web/index.html")?
        .replace("data-runtime=\"server\"", "data-runtime=\"static\"");
    ensure!(
        index.contains("data-runtime=\"static\""),
        "web/index.html is missing data-runtime=\"server\""
    );
    fs::write(format!("{DIST_DIR}/index.html"), index)?;
    println!(
        "Assembled static assets in {DIST_DIR}/; generating WebAssembly is the remaining build step."
    );
    Ok(())
}

fn load_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("Could not load {path}"))
}

fn verify_bundle(bundle: &WebBundle) -> Result<()> {
    ensure!(!bundle.items.is_empty(), "refusing an empty item bundle");
    ensure!(
        !bundle.item_sets.is_empty(),
        "refusing an empty item-set bundle"
    );
    ensure!(!bundle.skills.is_empty(), "refusing an empty skill bundle");
    ensure!(!bundle.mobs.is_empty(), "refusing an empty mob bundle");

    let serialized = serde_json::to_vec(bundle)?;
    let parsed: WebBundle = serde_json::from_slice(&serialized)?;
    let data = WebData::from_bundle(parsed).map_err(anyhow::Error::msg)?;
    let summary = dispatch_json(&data, "GET", "/api/summary", &[]).map_err(anyhow::Error::msg)?;
    let summary: serde_json::Value = serde_json::from_str(&summary)?;
    ensure!(
        summary["items"] == bundle.items.len(),
        "bundle item count mismatch"
    );
    ensure!(
        summary["mobs"] == bundle.mobs.len(),
        "bundle mob count mismatch"
    );
    Ok(())
}
