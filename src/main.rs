use std::{env, process::Command};

fn get_default_branch() -> String {
  String::from("master")
}

fn p(message: &'static str) {
  println!("{}", message);
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
    exit(&String::from_utf8(output.stderr).expect("Couldn't convert output.stderr to string"));
  }

  String::from_utf8(output.stdout)
    .expect("Failed to convert git command '{arg}' stdout to string")
}

fn git_target_branch() -> String {
  match env::args().nth(1) {
    None => {
      let default_branch = get_default_branch();
      p("Target branch not specified. Defaulting to {default_branch}");
      default_branch
    },
    Some(value) => value
  }
}

fn git_current_branch() -> String {
  git(&["rev-parse", "--abbrev-ref", "HEAD"])
}

fn git_commits_between_branches(current_branch:&str, target_branch: &str) -> String {
  git(&["log", "--oneline", target_branch, &format!("{}{}", "^", current_branch.trim())])
}

fn main() {
    let current_branch = git_current_branch();
    let target_branch = git_target_branch();
    let commits = git_commits_between_branches(&current_branch, &target_branch);

    for commit in commits.split("\n").filter(|line| !line.is_empty()) {
      println!("commit: {}", commit);
    }
}
