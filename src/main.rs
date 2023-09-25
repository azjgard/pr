use dotenv;
use edit;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json;
use std::{env, process::Command};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;
use loading::Loading;
use dialoguer::{console::Term, theme::ColorfulTheme, MultiSelect};


fn exit(message: &str) -> ! {
    println!("{}", message);
    panic!();
}

fn verify_dependencies() {
    // TODO: Check if git and gh are installed
    // TODO: Check if gh is authenticated
    thread::sleep(Duration::from_millis(1000));
}

fn git(args: &[&str], err_on_std_err: bool) -> String {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("Failed to execute git command");

    if !output.stderr.is_empty() {
        let error_message = String::from_utf8(output.stderr).unwrap();
        if err_on_std_err {
            exit(&error_message);
        }

        eprintln!("{}", error_message);
    }

    String::from_utf8(output.stdout).expect("Failed to convert git command stdout to string")
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

fn get_overview_str(commits: &[Commit]) -> String {
    let overview_str = commits
        .iter()
        .map(|commit| format!("- {}", commit.message))
        .collect::<Vec<String>>()
        .join("\n");

    overview_str
}

fn get_context_str(linear_ticket: &Option<LinearIssue>) -> String {
    linear_ticket
        .as_ref()
        .map(|ticket| format!("{}\n\n{}", ticket.url, ticket.description))
        .unwrap_or_default()
}

// TODO: If no name has been generated, should open an editor for it / prompt for it
fn get_pr_title(
    linear_ticket: &Option<LinearIssue>,
    linear_ticket_id: &Option<String>,
) -> Option<String> {
    if linear_ticket.is_none() {
        Some("<!--- The title of your pull request. Save and close this file to continue. --->".to_string())
    } else {
        linear_ticket.as_ref().map(|ticket| {
            let ticket_id = linear_ticket_id.clone().unwrap();
            format!(
                r"<!--- The title of your pull request. Save and close this file to continue. --->
[{ticket_id}] {ticket_title}",
                ticket_id = ticket_id,
                ticket_title = ticket.title
            )
        })
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
- [ ] 

### Reviewer checklist
- [ ] Have you thought about additional items that should be tested? Add them below if necessary.
",
        overview = overview,
        context = context
    )
}

fn fetch_github_reviewers() -> Vec<String> {
    let gh_users = Command::new("gh")
        .arg("api")
        .arg("orgs/dittowords/members")
        .arg("--jq")
        .arg(".[].login")
        .output()
        .unwrap();

    let gh_users = String::from_utf8_lossy(&gh_users.stdout)
        .split_whitespace()
        .map(String::from)
        .collect::<Vec<String>>();

    gh_users
}


#[derive(Debug, StructOpt)]
#[structopt(name = "pr-cli", about = "Create a pull request")]
struct CliArgs {
    /// Show version
    #[structopt(short,long)]
    version: bool,
    /// Skip confirmation prompt
    #[structopt(long)]
    no_confirm: bool,
    /// Skip selecting reviewers
    #[structopt(long)]
    no_reviewers: bool,
}

fn main() {
    dotenv::dotenv().ok();

    let args = CliArgs::from_args();

    if args.version {
        let version = env!("CARGO_PKG_VERSION");
        println!("pr version {version}\n", version = version);
        return;
    }

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

    let mut reviewers: Vec<String> = vec![];

    if !args.no_reviewers {
        let gh_users = fetch_github_reviewers();
        let selection = MultiSelect::with_theme(&ColorfulTheme::default())
            .items(&gh_users)
            .defaults(&[])
            .interact_on_opt(&Term::stderr())
            .unwrap_or_else(|err| {
                eprintln!("Error: {}", err);
                None
            });

    
        match selection {
            Some(positions) => {
                reviewers = positions
                    .iter()
                    .map(|&index| gh_users[index].clone())
                    .collect();
                println!("Selected reviewers: {:?}", reviewers);
            }
            None => println!("User exited using Esc or q")
        }
    }


    if !args.no_confirm {
        println!("Confirm creating pull request (y): ");

        let term = Term::stdout();

        if let Ok(input_char) = term.read_char() {
            if !input_char.eq_ignore_ascii_case(&'y') {
                println!("Pull request creation aborted.");
                return;
            }
        } else {
            eprintln!("Failed to read input");
        }
    }

    let loading = Loading::default();

    loading.text("Pushing branch upstream..");
    git(&["push", "-u", "origin", &current_branch], false);

    // TODO: add reviewer
    loading.text("Opening pull request..");
    let mut gh_command = Command::new("gh");
    gh_command.arg("pr")
        .arg("create")
        .arg("--title")
        .arg(&pr_title)
        .arg("--body")
        .arg(&pr_body)
        .arg("--base")
        .arg(&target_branch);

    if !reviewers.is_empty() {
        let reviewer_string = reviewers.join(",");
        gh_command.arg("--reviewer").arg(reviewer_string);
    }

    let gh_output = gh_command.output()
        .unwrap();

    let gh_output_stderr = String::from_utf8_lossy(&gh_output.stderr);
    if !gh_output_stderr.is_empty() {
        loading.fail("Failed to create PR!");
        loading.end();

        eprint!("{}", gh_output_stderr);
        return;
    }

    let gh_output_stdout = String::from_utf8_lossy(&gh_output.stdout);
    let pr_url = gh_output_stdout.trim();

    loading.success(format!("PR opened successfully: {}", pr_url));
    loading.end();
}
