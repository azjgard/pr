use dotenv;
use edit;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json;
use std::{env, process::Command};

use loading::Loading;
use std::thread;
use std::time::Duration;

fn exit(message: &str) {
    println!("{}", message);
    panic!();
}

fn verify_dependencies() -> () {
    // TODO: git is installed
    // TODO: gh is installed
    // TODO: gh is authenticated
    thread::sleep(Duration::from_millis(1000));
}

fn git(args: &[&str], err_on_std_err: bool) -> String {
    let mut output = Command::new("git");

    for arg in args {
        output.arg(arg);
    }

    let output = output
        .output()
        .expect("Failed to execute git command '{arg}'");

    if !output.stderr.is_empty() {
        let error_message = String::from_utf8(output.stderr).unwrap();
        if err_on_std_err {
            exit(&error_message)
        }

        eprintln!("{}", error_message);
    }

    String::from_utf8(output.stdout)
        .expect("Failed to convert git command '{arg}' stdout to string")
}

// TODO: add support for reading default target from config file
fn get_default_target_branch() -> String {
    let branches = git(&["branch"], true);
    let branches_by_line = branches.split('\n').map(|line| line.trim());

    let mut default_branch = String::new();
    for branch in branches_by_line {
        if branch.eq("main") {
            default_branch.push_str("main");
            break;
        }
        if branch.eq("master") {
            default_branch.push_str("master");
            break;
        }
    }

    if default_branch.is_empty() {
        exit("Failed to determine default branch");
    }

    default_branch
}

fn git_current_branch() -> String {
    git(&["rev-parse", "--abbrev-ref", "HEAD"], true)
        .trim()
        .to_string()
}

fn git_target_branch() -> String {
    match env::args().nth(1) {
        None => get_default_target_branch(),
        Some(value) => value,
    }
}

#[derive(Debug)]
struct Commit {
    _hash: String,
    message: String,
}

fn git_commit_from_line(line: &str, pattern: &Regex) -> Commit {
    let captures = pattern.captures(line).unwrap();

    Commit {
        _hash: String::from(&captures["commit_hash"]),
        message: String::from(&captures["commit_message"]),
    }
}

fn git_commits_between_branches(current_branch: &str, target_branch: &str) -> Vec<Commit> {
    let commits_str = git(
        &[
            "log",
            "--oneline",
            &format!("{}..{}", target_branch.trim(), current_branch.trim()),
        ],
        true,
    );

    let pattern = Regex::new(r"^(?P<commit_hash>\w+)\s(?P<commit_message>.+)$").unwrap();

    commits_str
        .lines()
        .rev()
        .filter(|line| !line.is_empty())
        .map(|line| git_commit_from_line(&line, &pattern))
        .collect::<Vec<Commit>>()
}

fn get_linear_ticket_id(branch_name: &str) -> Option<String> {
    let pattern = Regex::new(r"(?i)(?P<linear_ticket_id>dit-\d{3,5})").unwrap();

    match pattern.captures(branch_name) {
        None => None,
        Some(captures) => Some(String::from(&captures["linear_ticket_id"]).to_uppercase()),
    }
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Debug, Serialize, Deserialize)]
struct LinearIssue {
    url: String,
    title: String,

    // Edge case: `description` field from Linear API response is null
    // if the ticket is created and nothing is typed. If a ticket is
    // given a description value and then is cleared after the fact, the
    // field will be returned as an empty string instead of null.
    #[serde(deserialize_with = "deserialize_null_default")]
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LinearIssueResponseData {
    issue: LinearIssue,
}

#[derive(Debug, Serialize, Deserialize)]
struct LinearIssueResponse {
    data: LinearIssueResponseData,
}

fn get_linear_ticket(ticket_id: &Option<String>) -> Option<LinearIssue> {
    match ticket_id {
        None => None,
        Some(id) => {
            let client = reqwest::blocking::Client::new();

            let json_str = format!(
                r#"{{ "query": "{{ issue(id: \"{}\") {{ title description url }} }}" }}"#,
                id
            );
            let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

            let key = env::var("LINEAR_API_KEY").expect("Missing LINEAR_API_KEY");

            let response_body: LinearIssueResponse = client
                .post("https://api.linear.app/graphql")
                .header("Authorization", &key)
                .json(&json)
                .send()
                .expect("Failed to POST to Linear API")
                .json()
                .expect("Failed to parse JSON response from Linear API");

            Some(response_body.data.issue)
        }
    }
}

fn get_overview_str(commits: &Vec<Commit>) -> String {
    let mut overview_str = String::new();

    for (i, commit) in commits.iter().enumerate() {
        let mut str = format!("- {}", commit.message);
        if i < commits.len() - 1 {
            str.push('\n');
        }

        overview_str.push_str(&str);
    }

    overview_str
}

fn get_context_str(linear_ticket: &Option<LinearIssue>) -> String {
    match &linear_ticket {
        None => String::new(),
        Some(ticket) => {
            format!("{}\n\n{}", &ticket.url, &ticket.description)
        }
    }
}

// TODO: If no name has been generated, should open an editor for it / prompt for it
fn get_pr_title(
    linear_ticket: &Option<LinearIssue>,
    linear_ticket_id: &Option<String>,
) -> Option<String> {
    match &linear_ticket {
        None => None,
        Some(ticket) => {
            let ticket_id = linear_ticket_id.clone().unwrap();
            let ticket_title = &ticket.title;
            Some(format!(
                r"<!--- The title of your pull request. Save and close this file to continue. --->
[{ticket_id}] {ticket_title}"
            ))
        }
    }
}

fn get_pr_body(overview: &str, context: &str) -> String {
    format!(
        r"<!--- The body of your pull request. Save and close this file to continue. --->
## Overview
{overview}

## Context
{context}

## Screenshots

## Test Plan

"
    )
}

fn main() {
    dotenv::dotenv().ok();

    let loading = Loading::default();

    loading.text("Verifying dependencies..");
    verify_dependencies();

    loading.text("Collecting branch information..");
    let current_branch = git_current_branch();
    let target_branch = git_target_branch();

    loading.text("Checking for associated Linear ticket..");
    let linear_ticket_id = get_linear_ticket_id(&current_branch);
    let linear_ticket = get_linear_ticket(&linear_ticket_id);

    match &linear_ticket {
        None => loading.warn("No associated Linear ticket found."),
        Some(linear_ticket) => {
            loading.success(format!("Linear ticket: {}", &linear_ticket.url));
        }
    }

    loading.text("Checking commits..");
    let commits = git_commits_between_branches(&current_branch, &target_branch);
    let commits_len = commits.len();
    if commits_len == 0 {
        loading.fail(format!(
            "No difference in commits found between current branch and {}.",
            &target_branch
        ));
        loading.end();
        return;
    }

    loading.text("Generating PR information..");
    let overview_str = get_overview_str(&commits);
    let context_str = get_context_str(&linear_ticket);

    loading.success("Looks like we have everything we need!");
    loading.end();

    let pr_title = get_pr_title(&linear_ticket, &linear_ticket_id);
    let pr_title = edit::edit(pr_title.unwrap_or_default().as_bytes())
        .unwrap()
        .lines()
        .skip(1)
        .collect::<Vec<&str>>()
        .join("\n");

    let pr_body = get_pr_body(&overview_str, &context_str);
    let pr_body = edit::edit(pr_body)
        .unwrap()
        .lines()
        .skip(1)
        .collect::<Vec<&str>>()
        .join("\n");

    let loading = Loading::default();

    loading.text("Pushing branch upstream..");
    git(&["push", "-u", "origin", &current_branch], false);

    // TODO: add reviewer
    loading.text("Opening pull request..");
    let gh_output = Command::new("gh")
        .arg("pr")
        .arg("create")
        .arg("--title")
        .arg(&pr_title)
        .arg("--body")
        .arg(&pr_body)
        .arg("--base")
        .arg(&target_branch)
        .output()
        .unwrap();

    let gh_output_stderr = String::from_utf8(gh_output.clone().stderr).unwrap();
    if gh_output_stderr.len() > 0 {
        loading.fail("Failed to create PR!");
        loading.end();

        eprint!("{}", gh_output_stderr);
        return;
    }

    let gh_output_stdout = String::from_utf8(gh_output.stdout).unwrap();
    let pr_url = gh_output_stdout.trim();

    loading.success(format!("PR opened successfully: {}", pr_url));
    loading.end();
}
