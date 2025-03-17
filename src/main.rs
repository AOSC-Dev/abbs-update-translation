use std::{
    collections::HashMap,
    env::args,
    fs::{self, read_dir, File},
    io::{BufReader, BufWriter, Seek, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{bail, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
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
    let (f, mut json) = read_tree_en_json(&tree)?;

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

            modifly(i.path(), &mut json)?;
            has_modify = true;
        }
    }

    if !has_modify {
        bail!("Packages: {pkgs:?} does not exist or unsupport sub-package");
    }

    write_to_file(f, json)?;

    Ok(no_err)
}

fn read_tree_en_json(tree: &Path) -> Result<(File, HashMap<String, String>)> {
    let mut f = create_or_read_file(tree)?;

    let json = read_en_json(&f).or_else(|e| {
        eprintln!("Err: {e}, will create new file");
        f.rewind()?;
        f.set_len(0)?;
        f.write_all(b"{}")?;
        f.flush()?;
        anyhow::Ok(HashMap::new())
    })?;

    Ok((f, json))
}

fn create_or_read_file(tree: &Path) -> Result<File, anyhow::Error> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(tree.join("l10n").join("en.json"))?;

    Ok(f)
}

fn scan_all_translation() -> Result<bool> {
    let tree = get_tree(Path::new("."))?;
    let (f, mut json) = read_tree_en_json(&tree)?;

    let mut pkgs = vec![];

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

        pkgs.push((file_name.to_string(), i.path().to_path_buf()));
    }

    let results = pkgs
        .par_iter()
        .map(|(x, p)| {
            eprintln!("Scanning package: {x}");
            if let Err(e) = run_acbs(x) {
                eprintln!("{x}: {e}");
                return None;
            }
            Some(p)
        })
        .collect::<Vec<_>>();

    for r in results {
        match r {
            Some(p) => modifly(p, &mut json)?,
            None => no_err = false,
        }
    }

    write_to_file(f, json)?;

    Ok(no_err)
}

fn write_to_file(mut f: File, json: HashMap<String, String>) -> Result<()> {
    f.rewind()?;
    serde_json::to_writer(BufWriter::new(f), &json)?;

    Ok(())
}

fn modifly(i: &Path, json: &mut HashMap<String, String>) -> Result<()> {
    for i in read_dir(i)? {
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
        eprintln!("STDERR:\n{}", String::from_utf8_lossy(&out.stderr));
        eprintln!("STDOUT:\n{}", String::from_utf8_lossy(&out.stdout));

        bail!(
            "Run acbs-build get non-zero code: {}",
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
