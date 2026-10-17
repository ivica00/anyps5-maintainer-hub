#[allow(dead_code)]
#[path = "../src/github.rs"]
mod github;
fn main() -> Result<(), String> {
    let number = std::env::args()
        .nth(1)
        .unwrap_or("6176".into())
        .parse()
        .map_err(|_| "Expected PR number")?;
    let checks = github::checks(number)?;
    println!(
        "PR #{number} · SHA {}\n{}\n{}",
        checks.head, checks.summary, checks.mergeability
    );
    Ok(())
}
