use colored::*;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Child};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{thread, time::Duration};

mod config;
mod github;

use config::Config;
use github::{prepare_repository, get_latest_remote_sha, get_local_commit_hash};

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::load("mittorch.json")?;
    let repo_dir = format!(".data/{}", config.repository);
    let repo_path = Path::new(&repo_dir);

    println!("{} Starting mittorch orchestrator", "UPDATED:".bright_black().bold());

    if let Err(err) = prepare_repository(
        &config.account,
        &config.repository,
        &config.branch,
        config.token.as_deref(),
    ) {
        eprintln!("{} Initial clone failed: {}", "FAILURE:".red().bold(), err);
    } else {
        println!("{} Repository prepared.", "SUCCESS:".green().bold());
    }

    let mut child = if let Some(cmd) = &config.start_command {
        start_process(cmd, repo_path)?
    } else {
        return Err("No start-command configured.".into());
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\n{} Signal received, stopping...", "WARNING:".yellow().bold());
        r.store(false, Ordering::SeqCst);
    })?;

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(config.interval));

        // --- Crash or exit handling ---
        if let Some(status) = child.try_wait()? {
            eprintln!(
                "{} Supervised process exited with code {:?}",
                "WARNING:".yellow().bold(),
                status.code()
            );

            // Before restarting, check for possible repo update
            println!("{} Checking for possible updates before restart...", "UPDATED:".bright_black().bold());
            match git2::Repository::open(&repo_path) {
                Ok(repo) => {
                    let local_sha = get_local_commit_hash(&repo).unwrap_or_default();
                    match get_latest_remote_sha(
                        &config.account,
                        &config.repository,
                        &config.branch,
                        config.token.as_deref(),
                    ) {
                        Ok(remote_sha) => {
                            if !local_sha.is_empty() && !remote_sha.is_empty() && local_sha != remote_sha {
                                println!("{} Update available: {} → {}", 
                                    "UPDATED:".bright_black().bold(),
                                    short_sha(&local_sha), 
                                    short_sha(&remote_sha)
                                );

                                println!("{} Updating repository before restart...", "WARNING:".yellow().bold());
                                if let Err(err) = fs::remove_dir_all(&repo_path) {
                                    eprintln!("{} Cleanup failed: {}", "FAILURE:".red().bold(), err);
                                } else if let Err(err) = prepare_repository(
                                    &config.account,
                                    &config.repository,
                                    &config.branch,
                                    config.token.as_deref(),
                                ) {
                                    eprintln!("{} Re-clone failed: {}", "FAILURE:".red().bold(), err);
                                } else {
                                    println!("{} Repository updated successfully.", "SUCCESS:".green().bold());
                                }
                            } else {
                                println!("{} No new commits detected.", "UPDATED:".bright_black().bold());
                            }
                        }
                        Err(err) => eprintln!("{} Failed to query remote SHA: {}", "FAILURE:".red().bold(), err),
                    }
                }
                Err(_) => {
                    eprintln!("{} Could not open local repository during crash recovery.", "FAILURE:".red().bold());
                }
            }

            // Restart process regardless of update
            if let Some(start) = &config.start_command {
                child = start_process(start, repo_path)?;
                println!("{} Process restarted after crash.", "SUCCESS:".green().bold());
            } else {
                eprintln!("{} No start command configured — cannot restart.", "FAILURE:".red().bold());
            }

            continue;
        }

        // --- Regular update check loop ---
        let repo = match git2::Repository::open(&repo_path) {
            Ok(r) => r,
            Err(_) => {
                eprintln!("{} Local repo missing — retrying clone.", "WARNING:".yellow().bold());
                if let Err(err) = prepare_repository(
                    &config.account,
                    &config.repository,
                    &config.branch,
                    config.token.as_deref(),
                ) {
                    eprintln!("{} Retry failed: {}", "FAILURE:".red().bold(), err);
                } else {
                    println!("{} Repository re-cloned successfully.", "SUCCESS:".green().bold());
                }
                continue;
            }
        };

        let local_sha = get_local_commit_hash(&repo).unwrap_or_default();
        let remote_sha = match get_latest_remote_sha(
            &config.account,
            &config.repository,
            &config.branch,
            config.token.as_deref(),
        ) {
            Ok(sha) => sha,
            Err(err) => {
                eprintln!("{} Failed to query remote SHA: {}", "FAILURE:".red().bold(), err);
                continue;
            }
        };

        if local_sha.is_empty() || remote_sha.is_empty() {
            println!("{} Skipping (invalid SHAs)", "WARNING:".yellow().bold());
            continue;
        }

        if local_sha != remote_sha {
            println!(
                "{} Change detected: {} → {}",
                "UPDATED:".bright_black().bold(),
                short_sha(&local_sha),
                short_sha(&remote_sha)
            );

            if let Some(stop) = &config.stop_command {
                run_command("stop", stop, repo_path)?;
                thread::sleep(Duration::from_secs(1));
            } else {
                println!("{} Killing supervised process...", "WARNING:".yellow().bold());
                let _ = child.kill();
                let _ = child.wait();
            }

            println!("{} Removing old repository...", "WARNING:".yellow().bold());
            if let Err(err) = fs::remove_dir_all(&repo_path) {
                eprintln!("{} Cleanup failed: {}", "FAILURE:".red().bold(), err);
                continue;
            }

            if let Err(err) = prepare_repository(
                &config.account,
                &config.repository,
                &config.branch,
                config.token.as_deref(),
            ) {
                eprintln!("{} Re-clone failed: {}", "FAILURE:".red().bold(), err);
                continue;
            }

            if let Some(start) = &config.start_command {
                child = start_process(start, repo_path)?;
            }

            println!("{} Reloaded cleanly.", "SUCCESS:".green().bold());
        } else {
            println!("{} No changes detected.", "UPDATED:".bright_black().bold());
        }
    }

    println!("{} Stopping supervised process...", "WARNING:".yellow().bold());
    let _ = child.kill();
    let _ = child.wait();

    println!("{} Mittorch exited cleanly.", "SUCCESS:".green().bold());
    Ok(())
}

fn short_sha(sha: &str) -> String {
    if sha.len() >= 8 {
        sha[..8].to_string()
    } else {
        sha.to_string()
    }
}

fn run_command(name: &str, cmd: &str, repo_path: &Path) -> Result<(), Box<dyn Error>> {
    println!("{} Executing {}...", "UPDATED:".bright_black().bold(), name);
    let status = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(repo_path)
        .status()?;

    if !status.success() {
        eprintln!("{} {} command failed.", "FAILURE:".red().bold(), name);
    } else {
        println!("{} {} completed.", "SUCCESS:".green().bold(), name);
    }
    Ok(())
}

fn start_process(cmd: &str, repo_path: &Path) -> Result<Child, Box<dyn Error>> {
    println!("{} Starting supervised process...", "UPDATED:".bright_black().bold());
    let child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(repo_path)
        .spawn()?;
    println!("{} Process started (PID {}).", "SUCCESS:".green().bold(), child.id());
    Ok(child)
}
