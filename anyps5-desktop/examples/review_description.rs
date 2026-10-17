#[allow(dead_code)]
#[path = "../src/github.rs"]
mod github;
fn main() -> Result<(), String> {
    let number = std::env::args()
        .nth(1)
        .unwrap_or("6176".into())
        .parse()
        .map_err(|_| "Expected PR number")?;
    let description = github::description(number, None)?;
    println!(
        "PR #{number}: description fetched, {} characters",
        description.chars().count()
    );
    Ok(())
}
