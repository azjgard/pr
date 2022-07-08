use std::{env, process::Command};

fn get_default_branch() -> String {
  String::from("master")
}

fn p(message: &'static str) {
  println!("{}", message);
}

fn exit(message: &'static str) {
  println!("{}", message);
  panic!();
}

fn main() {
    let target_branch = match env::args().nth(1) {
      None => {
        let default_branch = get_default_branch();
        p("Target branch not specified. Defaulting to {default_branch}");
        default_branch
      },
      Some(value) => value
    };

    println!("Targeting {target_branch}");

    let status = Command::new("git")
      .arg("status")
      .output()
      .expect("failed to execute process");

    let status = String::from_utf8(status.stdout)
      .expect("failed to convert status to string");

    print!("{}", status);
}
