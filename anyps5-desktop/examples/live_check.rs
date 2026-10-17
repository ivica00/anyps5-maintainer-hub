#[allow(dead_code)]
#[path = "../src/models.rs"]
mod models;
#[path = "../src/prompt.rs"]
mod prompt;
#[path = "../src/supabase.rs"]
mod supabase;
use std::collections::{HashMap, HashSet};
fn main() -> Result<(), String> {
    let start = std::time::Instant::now();
    let batches = supabase::fetch_batches()?;
    let prs = supabase::fetch_prs()?;
    let count = prs.len();
    let map: HashMap<_, _> = prs.into_iter().map(|pr| (pr.number, pr)).collect();
    println!(
        "Live: {} batches, {} PR rows, {} unique PRs, {:?}",
        batches.len(),
        count,
        map.len(),
        start.elapsed()
    );
    let mut failures = Vec::new();
    let mut referenced = HashSet::new();
    for batch in &batches {
        referenced.extend(batch.pr_numbers.iter().copied());
        if let Err(error) = prompt::validate_batch(batch, &map) {
            failures.push(format!("{}: {error}", batch.id));
        } else {
            prompt::generate_ai_prompt(batch, &map)?;
            prompt::generate_merge_script(batch, &map)?;
        }
    }
    println!(
        "Valid batches: {}; blocked batches: {}; referenced PRs: {}; missing PRs: {}",
        batches.len() - failures.len(),
        failures.len(),
        referenced.len(),
        referenced.iter().filter(|n| !map.contains_key(n)).count()
    );
    for failure in failures.iter().take(15) {
        println!("{failure}");
    }
    println!(
        "Local fallback: {:?}",
        supabase::load_local_prs().map(|prs| prs.len())
    );
    Ok(())
}
