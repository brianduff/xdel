use anyhow::{anyhow, Error, Result};
use std::str;

use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;

mod index;
mod xeditor;

#[derive(Debug, StructOpt)]
/// Finds and manipluates string resources
#[structopt(name = "aster", bin_name = "aster", no_version)]
struct Opt {
    #[structopt(short)]
    java_root: PathBuf,

    #[structopt(short)]
    res_root: PathBuf,

    #[structopt(short)]
    manifest_root: Option<PathBuf>,

    #[structopt(long)]
    cache_dir: Option<PathBuf>,

    #[structopt(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, StructOpt)]
enum Subcommand {
    Counts {},
    Index {},
    LsUnused {
        #[structopt(short)]
        show_location: bool,
    },
    RmUnused {
        #[structopt(short)]
        prefix: Option<String>,
    },
}

impl Opt {
    pub fn parse() -> Result<Opt> {
        let m = Opt::clap().get_matches();
        Ok(Opt::from_clap(&m))
    }
}

#[derive(Debug)]
enum Kind {
    Defined,
    Used,
    Unused,
}

impl FromStr for Kind {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "defined" => Ok(Kind::Defined),
            "used" => Ok(Kind::Used),
            "unused" => Ok(Kind::Unused),
            _ => Err(anyhow!("Unrecognized value")),
        }
    }
}

fn filtered_unused_strings(index: &index::ResourceIndex) -> Vec<&String> {
    let mut unused_strings: Vec<&String> = index
        .unused_strings()
        .iter()
        .cloned()
        .filter(|s| !s.contains("emoji") && !s.contains("f1gender") && !s.contains("m2gender"))
        .collect();

    unused_strings.sort();

    unused_strings
}

/// A simple program that reads an strings.xml file and strips
/// elements matching the given name out without disrupting the rest
/// of the file.
fn main() -> Result<()> {
    let opt = Opt::parse()?;

    let indexer = index::Indexer::new(
        opt.java_root,
        opt.res_root,
        opt.manifest_root,
        opt.cache_dir,
    )?;

    match opt.subcommand {
        Subcommand::Index {} => {
            let index = indexer.index()?;
            indexer.serialize(&index)?;
        }
        Subcommand::Counts { .. } => {
            let index = indexer.deserialize()?;
            println!("{} defined strings", index.defined_strings().len());
            println!("{} used strings", index.used_strings().len());
            println!("{} unused strings", filtered_unused_strings(&index).len());
        }
        Subcommand::LsUnused { show_location } => {
            let index = indexer.deserialize()?;

            let files_for_definition = index.files_for_definition();

            for unused in filtered_unused_strings(&index) {
                println!("{}", unused);
                if show_location {
                    for loc in files_for_definition.get_vec(unused).unwrap() {
                        println!("  {}", loc);
                    }
                }
            }
        }
        Subcommand::RmUnused { prefix } => {
            let index = indexer.deserialize()?;
            let files_for_definition = index.files_for_definition();

            let prefix = match prefix {
                Some(prefix) => prefix,
                None => "".to_string(),
            };

            for unused in filtered_unused_strings(&index) {
                if unused.starts_with(&prefix) {
                    let mut matcher = xeditor::ElementMatcher::for_local_name("string");
                    matcher.attr("name", unused);

                    for loc in files_for_definition.get_vec(unused).unwrap() {
                        xeditor::remove_element(Path::new(loc), &matcher)?;
                    }
                }
            }
        }
    }

    return Ok(());
}
