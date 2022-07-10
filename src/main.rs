use std::{env, process::Command, fs::File, io::{Write, BufReader, Read}};
use regex::Regex;
use reqwest;
use serde::{Serialize, Deserialize, Deserializer};
use serde_json;
use dotenv;

fn verify_dependencies() -> () {
  // TODO: git installation
  // TODO: gh installation + authenticated
}

fn get_default_branch() -> String {
  String::from("master")
}

fn exit(message: &str) {
  println!("{}", message);
  panic!();
}

fn git(args: &[&str]) -> String {
  let mut output = Command::new("git");

  for arg in args {
    output.arg(arg);
  }

  let output = output
    .output()
    .expect("Failed to execute git command '{arg}'");

  if !output.stderr.is_empty() {
    // TODO: git push -u origin writes to stderr if the branch is already pushed
    // exit(&String::from_utf8(output.stderr).unwrap());
  }

  String::from_utf8(output.stdout)
    .expect("Failed to convert git command '{arg}' stdout to string")
}

fn git_current_branch() -> String {
  git(&["rev-parse", "--abbrev-ref", "HEAD"])
}

fn git_target_branch() -> String {
  match env::args().nth(1) {
    None => {
      let default_branch = get_default_branch();
      println!("Target branch not specified. Defaulting to `{default_branch}`");
      default_branch
    },
    Some(value) => value
  }
}

#[derive(Debug)]
struct Commit {
  hash: String,
  message: String
}

fn git_commit_from_line(line: &str, pattern: &Regex) -> Commit {
  let captures = pattern.captures(line).unwrap();

  Commit {
    hash: String::from(&captures["commit_hash"]),
    message: String::from(&captures["commit_message"])
  }
}

fn git_commits_between_branches(current_branch:&str, target_branch: &str) -> Vec<Commit> {
  let commits_str = git(&["log", "--oneline", &format!("{}..{}", target_branch.trim(), current_branch.trim())]);

  let pattern = Regex::new(r"^(?P<commit_hash>\w+)\s(?P<commit_message>.+)$").unwrap();

  commits_str
    .split('\n')
    .rev()
    .filter(|line| !line.is_empty())
    .map(|line| git_commit_from_line(&line, &pattern))
    .collect::<Vec<Commit>>()
}

fn get_linear_ticket_id(branch_name: &str) -> Option<String> {
  let pattern = Regex::new(r"(?i)(?P<linear_ticket_id>dit-\d{3,5})").unwrap();

  match pattern.captures(branch_name) {
    None => None,
    Some(captures) => Some(String::from(&captures["linear_ticket_id"]).to_uppercase())
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
  issue: LinearIssue
}

#[derive(Debug, Serialize, Deserialize)]
struct LinearIssueResponse {
  data: LinearIssueResponseData 
}

fn get_linear_ticket(ticket_id: &Option<String>) -> Option<LinearIssue> {
  match ticket_id {
    None => None,
    Some(id) => {
      let client = reqwest::blocking::Client::new();

      let json_str = format!(r#"{{ "query": "{{ issue(id: \"{}\") {{ title description url }} }}" }}"#, id);
      let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

      let key = env::var("LINEAR_API_KEY").expect("Missing LINEAR_API_KEY");

      let response_body: LinearIssueResponse  = client
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
        format!(
          "{}\n\n{}", 
          &ticket.url,
          &ticket.description
        )
      }
    }
}

// TODO: If no name has been generated, should open an editor for it / prompt for it
fn get_pr_name(linear_ticket: &Option<LinearIssue>, linear_ticket_id: &Option<String>) -> Option<String> {
    match &linear_ticket {
      None => None,
      Some(ticket) => {
        let ticket_id = linear_ticket_id.clone().unwrap();
        let ticket_title = &ticket.title;
        Some(format!("[{ticket_id}] {ticket_title}"))
      }
    }
}

fn get_pr_template(overview: &str, context: &str) -> String {
format!(r"## Overview
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

    verify_dependencies();

    let current_branch = git_current_branch();
    let target_branch = git_target_branch();

    let linear_ticket_id = get_linear_ticket_id(&current_branch);
    let linear_ticket = get_linear_ticket(&linear_ticket_id);

    let commits = git_commits_between_branches(&current_branch, &target_branch);

    let overview_str = get_overview_str(&commits);
    let context_str = get_context_str(&linear_ticket);

    let pr_template = get_pr_template(&overview_str, &context_str);
    let pr_name = get_pr_name(&linear_ticket, &linear_ticket_id);

    let temp_dir = env::temp_dir();

    let mut template_file_path = temp_dir.clone();
    template_file_path.push("pr_description"); // TODO: generate random name?
    template_file_path.set_extension("md");

    // TODO: add instructions to the data being written, similar to git commit
    let mut template_file = File::create(&template_file_path).expect("Failed to create temporary file");
    template_file.write_all(pr_template.as_bytes()).expect("Failed to write pr template to temporary file");

    // TODO: use env $EDITOR with fallbacks
    Command::new("nvim")
      .arg(template_file_path.to_str().unwrap())
      .status()
      .unwrap();

    let mut pr_name_file_path = temp_dir.clone();
    pr_name_file_path.push("pr_name"); // TODO: generate random name?
    pr_name_file_path.set_extension("md");

    // TODO: add instructions to the data being written, similar to git commit
    let mut pr_name_file = File::create(&pr_name_file_path).expect("Failed to create temporary file");
    pr_name_file.write_all(pr_template.as_bytes()).expect("Failed to write pr name to temporary file");

    if matches!(pr_name, None) {
      // TODO: use env $EDITOR with fallbacks
      Command::new("nvim")
        .arg(pr_name_file_path.to_str().unwrap())
        .status()
        .unwrap();
    }

    git(&["push", "-u", "origin", &target_branch]);

    // TODO: neeed to reopen file here (this is where i left off)
    let mut reader = BufReader::new(pr_name_file);
    let mut pr_title = String::new();
    reader.read_to_string(&mut pr_title).unwrap();

    let mut reader = BufReader::new(template_file);
    let mut pr_description = String::new();
    reader.read_to_string(&mut pr_description).unwrap();

    // TODO: add reviewers
    let gh_output = Command::new("gh")
      .arg("pr")
      .arg("create")
      .arg("--title")
      .arg(&pr_title)
      .arg("--description")
      .arg(&pr_description)
      .output()
      .unwrap();

    // TODO: format this output uniformly
    println!("{}", String::from_utf8(gh_output.stdout).unwrap());
}