extern crate diff;
extern crate mdsh;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate structopt;

use mdsh::cli::{FileArg, Opt, Parent};
use regex::{Captures, Regex};
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, ErrorKind, Write};
use std::process::{Command, Output, Stdio};
use structopt::StructOpt;

fn run_command(command: &str, work_dir: &Parent) -> Output {
    let mut cli = Command::new("bash");
    cli.arg("-c")
        .arg(command)
        .stdin(Stdio::null()) // don't read from stdin
        .stderr(Stdio::inherit()) // send stderr to stderr
        .current_dir(work_dir.as_path_buf())
        .output()
        .expect(
            format!(
                "fatal: failed to execute command `{:?}` in {}",
                cli,
                work_dir.as_path_buf().display()
            )
            .as_str(),
        )
}

fn read_file(f: &FileArg) -> Result<String, std::io::Error> {
    let mut buffer = String::new();

    match f {
        FileArg::StdHandle => {
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            handle.read_to_string(&mut buffer)?;
        }
        FileArg::File(path_buf) => {
            let mut file = File::open(path_buf)?;
            file.read_to_string(&mut buffer)?;
        }
    }

    Ok(buffer)
}

fn write_file(f: &FileArg, contents: String) -> Result<(), std::io::Error> {
    match f {
        FileArg::StdHandle => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            write!(handle, "{}", contents)?;
        }
        FileArg::File(path_buf) => {
            let mut file = File::create(path_buf)?;
            write!(file, "{}", contents)?;
            file.sync_all()?;
        }
    }

    Ok(())
}

fn trail_nl<T: AsRef<str>>(s: T) -> String {
    let r = s.as_ref();
    if r.ends_with('\n') {
        r.to_string()
    } else {
        format!("{}\n", r)
    }
}

// make sure that the string starts and ends with new lines
fn wrap_nl(s: String) -> String {
    if s.starts_with('\n') {
        trail_nl(s)
    } else if s.ends_with('\n') {
        format!("\n{}", s)
    } else {
        format!("\n{}\n", s)
    }
}

lazy_static! {
    static ref RE_FENCE_LINK_STR: String = String::from(r"^\[\$ (?P<link>[^\]]+)\]\([^\)]+\)\s*$");
    static ref RE_MD_LINK_STR: String = String::from(r"^\[> (?P<link>[^\]]+)\]\([^\)]+\)\s*$");
    static ref RE_FENCE_COMMAND_STR: String = String::from(r"^`\$ (?P<command>[^`]+)`\s*$");
    static ref RE_MD_COMMAND_STR: String = String::from(r"^`> (?P<command>[^`]+)`\s*$");
    static ref RE_MD_BLOCK_STR: String = String::from(r"^<!-- BEGIN mdsh -->.+?^<!-- END mdsh -->");
    static ref RE_FENCE_BLOCK_STR: String = String::from(r"^```.+?^```");
    static ref RE_MATCH_FENCE_BLOCK_STR: String = format!(
        r"(?sm)({}|{})[\s\n]+({}|{})",
        RE_FENCE_COMMAND_STR.to_string(),
        RE_FENCE_LINK_STR.to_string(),
        RE_FENCE_BLOCK_STR.to_string(),
        RE_MD_BLOCK_STR.to_string(),
    );
    static ref RE_MATCH_FENCE_BLOCK: Regex = Regex::new(&RE_MATCH_FENCE_BLOCK_STR).unwrap();
    static ref RE_MATCH_MD_BLOCK_STR: String = format!(
        r"(?sm)({}|{})[\s\n]+({}|{})",
        RE_MD_COMMAND_STR.to_string(),
        RE_MD_LINK_STR.to_string(),
        RE_MD_BLOCK_STR.to_string(),
        RE_FENCE_BLOCK_STR.to_string(),
    );
    static ref RE_MATCH_MD_BLOCK: Regex = Regex::new(&RE_MATCH_MD_BLOCK_STR).unwrap();
    static ref RE_MATCH_FENCE_COMMAND_STR: String =
        format!(r"(?sm){}", RE_FENCE_COMMAND_STR.to_string());
    static ref RE_MATCH_FENCE_COMMAND: Regex = Regex::new(&RE_MATCH_FENCE_COMMAND_STR).unwrap();
    static ref RE_MATCH_MD_COMMAND_STR: String = format!(r"(?sm){}", RE_MD_COMMAND_STR.to_string());
    static ref RE_MATCH_MD_COMMAND: Regex = Regex::new(&RE_MATCH_MD_COMMAND_STR).unwrap();
    static ref RE_MATCH_FENCE_LINK_STR: String = format!(r"(?sm){}", RE_FENCE_LINK_STR.to_string());
    static ref RE_MATCH_FENCE_LINK: Regex = Regex::new(&RE_MATCH_FENCE_LINK_STR).unwrap();
    static ref RE_MATCH_MD_LINK_STR: String = format!(r"(?sm){}", RE_MD_LINK_STR.to_string());
    static ref RE_MATCH_MD_LINK: Regex = Regex::new(&RE_MATCH_MD_LINK_STR).unwrap();
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    let clean = opt.clean;
    let frozen = opt.frozen;
    let input = opt.input;
    let output = opt.output.unwrap_or_else(|| input.clone());
    let work_dir: Parent = opt.work_dir.map_or_else(
        || {
            input
                .clone()
                .parent()
                .expect("fatal: your input file has no parent directory.")
        },
        |buf| Parent::from_parent_path_buf(buf),
    );
    let original_contents = read_file(&input)?;
    let contents = original_contents.clone();

    eprintln!(
        "Using clean={:?} input={:?} output={:?}",
        clean, &input, output,
    );

    let contents = RE_MATCH_FENCE_BLOCK.replace_all(&contents, |caps: &Captures| {
        // println!("caps1: {:?}", caps);
        caps[1].to_string()
    });

    let contents = RE_MATCH_MD_BLOCK.replace_all(&contents, |caps: &Captures| {
        // println!("caps2: {:?}", caps);
        caps[1].to_string()
    });

    // Write the contents and return if --clean is passed
    if clean {
        write_file(&output, contents.to_string())?;
        return Ok(());
    }

    let contents = RE_MATCH_FENCE_COMMAND.replace_all(&contents, |caps: &Captures| {
        let command = &caps["command"];

        eprintln!("$ {}", command);

        let result = run_command(command, &work_dir);

        // TODO: if there is an error, write to stdout
        let stdout = String::from_utf8(result.stdout).unwrap();

        format!("{}```{}```", trail_nl(&caps[0]), wrap_nl(stdout))
    });

    let contents = RE_MATCH_MD_COMMAND.replace_all(&contents, |caps: &Captures| {
        let command = &caps["command"];

        eprintln!("> {}", command);

        let result = run_command(command, &work_dir);

        // TODO: if there is an error, write to stdout
        let stdout = String::from_utf8(result.stdout).unwrap();

        format!(
            "{}<!-- BEGIN mdsh -->{}<!-- END mdsh -->",
            trail_nl(&caps[0]),
            wrap_nl(stdout)
        )
    });

    let contents = RE_MATCH_FENCE_LINK.replace_all(&contents, |caps: &Captures| {
        let link = &caps["link"];

        eprintln!("[$ {}]", link);

        let result = read_file(&FileArg::from_str_unsafe(link))
            .unwrap_or_else(|_| String::from("[mdsh error]: failed to read file"));

        format!("{}```{}```", trail_nl(&caps[0]), wrap_nl(result))
    });

    let contents = RE_MATCH_MD_LINK.replace_all(&contents, |caps: &Captures| {
        let link = &caps["link"];

        eprintln!("[> {}]", link);

        let result = read_file(&FileArg::from_str_unsafe(link))
            .unwrap_or_else(|_| String::from("failed to read file"));

        format!(
            "{}<!-- BEGIN mdsh -->{}<!-- END mdsh -->",
            trail_nl(&caps[0]),
            wrap_nl(result)
        )
    });

    // Special path if the file is frozen
    if frozen {
        if original_contents == contents {
            return Ok(());
        }

        eprintln!("Found differences in output:");
        let mut line = 0;
        for diff in diff::lines(&original_contents, &contents) {
            match diff {
                diff::Result::Left(l) => {
                    line += 1;
                    eprintln!("{}- {}", line, l)
                }
                diff::Result::Both(_, _) => {
                    // nothing changed, just increase the line number
                    line += 1
                }
                diff::Result::Right(l) => eprintln!("{}+ {}", line, l),
            };
        }

        return Err(std::io::Error::new(
            ErrorKind::Other,
            "--frozen: input is not the same",
        ));
    }

    write_file(&output, contents.to_string())?;

    Ok(())
}
