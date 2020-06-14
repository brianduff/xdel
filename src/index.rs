use anyhow::{Result};
use rayon::prelude::*;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::vec::Vec;
use xml::attribute::OwnedAttribute;
use xml::common::Position;
use xml::reader::{EventReader, XmlEvent};
use glob::glob;
use multimap::MultiMap;
use bincode;
use std::io::Write;
use std::fs;

pub struct ResourceFile {
    path: String,
    string_keys: Vec<String>,
}

pub struct ResourceIndex {
    key_to_files: MultiMap<String, String>
}

impl ResourceIndex {
    pub fn get_files(&self, key: &str) -> Option<&Vec<String>> {
        self.key_to_files.get_vec(key)
    }

    pub fn write_index(&self, path: &Path) -> Result<()> {
        let encoded = bincode::serialize(&self.key_to_files)?;

        let mut buffer = File::create(path)?;
        buffer.write(&encoded)?;

        Ok(())
    }

    pub fn from_index(path: &Path) -> Result<ResourceIndex> {
        let bytes = fs::read(path)?;
        let key_to_files : MultiMap<String, String> = bincode::deserialize(&bytes)?;

        Ok(ResourceIndex{ key_to_files })
    }
}

// TODO: Put this in one place.
fn get_name_attr(attributes: &Vec<OwnedAttribute>) -> Option<String> {
    for attr in attributes {
        if attr.name.local_name.eq("name") {
            return Some(attr.value.to_owned());
        }
    }

    None
}

fn index_file(path: &Path) -> Result<ResourceFile> {
    let file = File::open(path)?;
    let file = BufReader::new(file);
    let mut parser = EventReader::new(file);

    let mut string_keys = Vec::new();
    loop {
        let e = parser.next();
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                let pos = parser.position();
                if name.local_name.eq("string") {
                    if let Some(name) = get_name_attr(&attributes) {
                        string_keys.push(name);
                    }
                }
            }
            Ok(XmlEvent::EndDocument) => break,
            Err(e) => return Err(anyhow::Error::new(e).context(format!("In {:?}", path))),
            _ => {}
        }
    }

    Ok(ResourceFile {
        path: path.to_str().unwrap().to_string(),
        string_keys,
    })
}

pub fn index(file_glob: &str) -> Result<ResourceIndex> {
    println!("Globbing...");
    let files: Vec<_> = glob(file_glob)?
        .filter_map(|x| x.ok())
        .collect();


    println!("Parsing...");
    let files = files.par_iter()
        .map(|path| index_file(&path))
        .filter_map(|x| x.ok())
        .collect::<Vec<ResourceFile>>();

    println!("Organizing by key...");
    let mut map = MultiMap::new();
    for file in files {
        for key in file.string_keys {
            map.insert(key, file.path.to_owned());
        }
    }
    println!("Done");

    Ok(ResourceIndex { key_to_files: map })
}
