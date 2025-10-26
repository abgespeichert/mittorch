use std::fs;
use std::path::Path;
use colored::*;
use git2::Repository;

pub fn prepare_repository(
    account: &str,
    repository: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = Path::new(".data");
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let repo_path = target_dir.join(repository);

    let repo_url = match token.map(|t| t.trim()).filter(|t| !t.is_empty()) {
        Some(tok) => format!("https://{}@github.com/{}/{}.git", tok, account, repository),
        None => format!("https://github.com/{}/{}.git", account, repository),
    };

    if repo_path.exists() {
        println!(
            "{} Removing existing directory {}",
            "WARNING:".yellow().bold(),
            repo_path.display()
        );
        fs::remove_dir_all(&repo_path)?;
    }

    println!("{} Cloning {} (branch: {})", "UPDATED:".bright_black().bold(), repo_url, branch);

    match Repository::clone(&repo_url, &repo_path) {
        Ok(repo) => {
            let _ = repo.set_head(&format!("refs/heads/{}", branch));
            println!("{} Repository ready.", "SUCCESS:".green().bold());
        }
        Err(e) => {
            if repo_url.contains('@') {
                eprintln!(
                    "{} Failed to clone private repository — check token permissions: {}",
                    "FAILURE:".red().bold(),
                    e
                );
            } else {
                eprintln!(
                    "{} Failed to clone public repository: {}",
                    "FAILURE:".red().bold(),
                    e
                );
            }
            return Err(Box::new(e));
        }
    }

    Ok(())
}

pub fn get_local_commit_hash(repo: &Repository) -> Result<String, git2::Error> {
    let head = repo.head()?.peel_to_commit()?;
    Ok(head.id().to_string())
}

pub fn get_latest_remote_sha(
    account: &str,
    repository: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/branches/{}",
        account, repository, branch
    );

    let client = reqwest::blocking::Client::new();
    let mut req = client.get(&url).header("User-Agent", "mittorch");

    if let Some(tok) = token.map(|t| t.trim()).filter(|t| !t.is_empty()) {
        req = req.header("Authorization", format!("token {}", tok));
    }

    let resp = req.send()?;

    match resp.status().as_u16() {
        401 => return Err("unauthorized: invalid or missing token".into()),
        404 => return Err("repository not found (check visibility and account)".into()),
        code if !(200..300).contains(&code) => {
            return Err(format!("GitHub API error: {}", resp.status()).into());
        }
        _ => {}
    }

    let json: serde_json::Value = resp.json()?;
    Ok(json["commit"]["sha"].as_str().unwrap_or_default().to_string())
}
