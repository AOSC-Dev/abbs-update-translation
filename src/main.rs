use std::{
    collections::HashMap,
    env::args,
    fs::{self, read_dir, File},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;


#[derive(Debug, Deserialize, Serialize)]
struct SrcInfo {
    #[serde(rename = "PKGNAME")]
    pkgname: String,
    #[serde(rename = "PKGDES")]
    pkgdes: String,
}

fn main() -> ExitCode {
    let args = args().skip(1).collect::<Vec<_>>();

    let res = if args.is_empty() {
        scan_all_translation()
    } else {
        scan_by_args(args)
    };

    match res {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn scan_by_args(pkgs: Vec<String>) -> Result<bool> {
    let tree = get_tree(Path::new("."))?;
    let f = read_file(&tree)?;
    let mut json = read_en_json(&f)?;

    let mut has_modify = false;
    let mut no_err = true;

    for i in WalkDir::new(tree).min_depth(2).max_depth(2) {
        let i = i?;

        if i.path().to_string_lossy().contains(".git")
            || i.path().to_string_lossy().contains("assets")
            || i.path().to_string_lossy().contains("groups")
        {
            continue;
        }

        if i.path().is_file() {
            continue;
        }

        let file_name = i.file_name().to_string_lossy().to_string();

        if pkgs.contains(&file_name) {
            if let Err(e) = run_acbs(&file_name) {
                eprintln!("{}: {}", file_name, e);
                no_err = false;
                continue;
            }

            modifly(i, &mut json)?;
            has_modify = true;
        }
    }

    if !has_modify {
        bail!("Packages: {pkgs:?} does not exist or unsupport sub-package");
    }

    serde_json::to_writer(BufWriter::new(f), &json)?;

    Ok(no_err)
}

fn read_file(tree: &Path) -> Result<File, anyhow::Error> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tree.join("l10n").join("en.json"))?;

    Ok(f)
}

fn scan_all_translation() -> Result<bool> {
    let tree = get_tree(Path::new("."))?;
    let f = read_file(&tree)?;

    let mut json = read_en_json(&f)?;

    let mut no_err = true;

    for i in WalkDir::new(tree).min_depth(2).max_depth(2) {
        let i = i?;

        if i.path().to_string_lossy().contains(".git")
            || i.path().to_string_lossy().contains("assets")
            || i.path().to_string_lossy().contains("groups")
        {
            continue;
        }

        if i.path().is_file() {
            continue;
        }

        let file_name = i.file_name().to_string_lossy();

        println!("Scanning package {}", file_name);

        if let Err(e) = run_acbs(&file_name) {
            eprintln!("{}: {}", file_name, e);
            no_err = false;
            continue;
        }

        modifly(i, &mut json)?;
    }

    serde_json::to_writer(BufWriter::new(f), &json)?;

    Ok(no_err)
}

fn modifly(i: walkdir::DirEntry, json: &mut HashMap<String, String>) -> Result<()> {
    for i in read_dir(i.path())? {
        let i = i?;
        if i.path()
            .extension()
            .is_some_and(|x| x.to_string_lossy() == "json")
        {
            let pkg_json = BufReader::new(fs::File::open(i.path())?);
            let pkg_json: SrcInfo = serde_json::from_reader(pkg_json)?;

            match json.get_mut(&pkg_json.pkgname) {
                Some(x) if *x == pkg_json.pkgdes => continue,
                Some(x) => {
                    *x = pkg_json.pkgdes;
                }
                None => {
                    json.insert(pkg_json.pkgname, pkg_json.pkgdes);
                }
            }

            std::fs::remove_file(i.path())?;
        }
    }

    Ok(())
}

fn read_en_json(f: &File) -> Result<HashMap<String, String>, anyhow::Error> {
    let reader = BufReader::new(f);
    let json: HashMap<String, String> = serde_json::from_reader(reader)?;

    Ok(json)
}

fn run_acbs(pkg_name: &str) -> Result<()> {
    let out = Command::new("acbs-build")
        .arg("--generate-package-metadata")
        .arg(pkg_name)
        .output()?;

    if !out.status.success() {
        bail!(
            "acbs-build return non-zero code: {}",
            out.status.code().unwrap_or(1)
        )
    }

    Ok(())
}

fn get_tree(directory: &Path) -> Result<PathBuf> {
    let mut tree = directory.canonicalize()?;
    let mut has_groups;

    loop {
        has_groups = tree.join("groups").is_dir();

        if !has_groups && tree.to_str() == Some("/") {
            bail!("Failed to get ABBS tree");
        }

        if has_groups {
            return Ok(tree);
        }

        tree.pop();
    }
}
