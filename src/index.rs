use anyhow::Result;
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::Searcher;
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use multimap::MultiMap;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use std::vec::Vec;
use xml::common::Position;
use xml::reader::{EventReader, XmlEvent};
use std::path::PathBuf;
use cachedir::CacheDirConfig;
use serde::{Serialize, Deserialize};


pub struct Indexer {
    java_root: PathBuf,
    res_root: PathBuf,
    cache_dir: PathBuf
}

#[derive(Serialize,Deserialize)]
pub struct ResourceFile {
    path: String,
    string_definitions: Vec<String>,
    string_usages: Vec<String>,
}

#[derive(Serialize,Deserialize)]
pub struct ResourceIndex {
    files: Vec<ResourceFile>,
}

impl ResourceIndex {
    pub fn new(files: Vec<ResourceFile>) -> ResourceIndex {
        ResourceIndex { files }
    }

    pub fn files_for_definition(&self) -> MultiMap<&String, String> {
        let mut definitions_to_files = MultiMap::new();
        for file in &self.files {
            for key in &file.string_definitions {
                definitions_to_files.insert(key, file.path.to_owned());
            }
        }
        definitions_to_files
    }

    pub fn files_for_usage(&self) -> MultiMap<&String, String> {
        let mut usages_to_files = MultiMap::new();
        for file in &self.files {
            for key in &file.string_usages {
                usages_to_files.insert(key, file.path.to_owned());
            }
        }
        usages_to_files
    }

    pub fn defined_strings(&self) -> HashSet<&String> {
        let mut defined_strings = HashSet::with_capacity(self.files.len());
        for file in &self.files {
            for key in &file.string_definitions {
                defined_strings.insert(key);
            }
        }

        defined_strings
    }

    pub fn used_strings(&self) -> HashSet<&String> {
        let mut used_strings = HashSet::with_capacity(self.files.len());
        for file in &self.files {
            for key in &file.string_usages {
                used_strings.insert(key);
            }
        }

        used_strings
    }

    pub fn unused_strings(&self) -> HashSet<&String> {
        let defined_strings = self.defined_strings();
        let used_strings = self.used_strings();

        defined_strings.difference(&used_strings).copied().collect()
    }
}

impl Indexer {
    pub fn new(java_root: PathBuf, res_root: PathBuf, cache_dir: Option<PathBuf>) -> Result<Indexer> {
        let cache_dir = match cache_dir {
            Some(cache_dir) => cache_dir,
            None => {
                CacheDirConfig::new("aster").get_cache_dir()?.into()
            }
        };


        Ok(Indexer { java_root, res_root, cache_dir })
    }

    fn index_xml_file(path: &Path) -> Result<ResourceFile> {
        let file = File::open(path)?;
        let file = BufReader::new(file);
        let mut parser = EventReader::new(file);
    
        let mut string_definitions = Vec::new();
        let mut string_usages = Vec::new();
    
        let string_id_usage_pattern = Regex::new(r"(?m)@string/(\w+)")?;
    
        loop {
            let e = parser.next();
            match e {
                Ok(XmlEvent::StartElement {
                    name, attributes, ..
                }) => {
                    let pos = parser.position();
                    for attr in attributes {
                        if attr.value.contains("@string") {
                            if let Some(captures) = string_id_usage_pattern.captures(&attr.value) {
                                if let Some(id) = captures.get(1) {
                                    string_usages.push(String::from(id.as_str()));
                                }
                            }
                        }
                        if name.local_name.eq("string") && attr.name.local_name.eq("name") {
                            string_definitions.push(attr.value);
                        }
                    }
                }
                Ok(XmlEvent::CData(data)) => {
                    if data.contains("@string") {
                        if let Some(captures) = string_id_usage_pattern.captures(&data) {
                            if let Some(id) = captures.get(1) {
                                string_usages.push(String::from(id.as_str()));
                            }
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
            string_definitions,
            string_usages,
        })
    }
    
    fn index_source_file(path: &Path) -> Result<ResourceFile> {
        let mut string_usages = Vec::new();
        let matcher = RegexMatcher::new(r"R.string.(\w+)")?;
        Searcher::new().search_path(
            &matcher,
            path,
            UTF8(|_, line| {
                let found = matcher.find(line.as_bytes())?.unwrap();
                let found = String::from(&line[found][9..]);
                string_usages.push(found);
    
                Ok(true)
            }),
        )?;
    
        Ok(ResourceFile {
            path: path.to_str().unwrap().to_string(),
            string_definitions: Vec::new(),
            string_usages,
        })
    }
    
    fn index_xml_files(&self) -> Result<Vec<ResourceFile>> {
        let mut builder = WalkBuilder::new(&self.res_root);
        let mut overrides =
            OverrideBuilder::new(&self.res_root);
        overrides.add("*.xml")?;
        builder.threads(36);
        builder.overrides(overrides.build()?);
    
        let (tx, rx) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
    
        thread::spawn(move || {
            let mut results = Vec::new();
            for received in rx {
                results.push(received);
            }
    
            tx2.send(results).unwrap();
        });
    
        let walker = builder.build_parallel();
        walker.run(move || {
            let tx = tx.clone();
            return Box::new(move |result| {
                let index = Indexer::index_xml_file(&result.unwrap().path());
                if let Ok(index) = index {
                    tx.send(index).unwrap();
                }
                WalkState::Continue
            });
        });
    
        let results = rx2.recv().unwrap();
        Ok(results)
    }
    
    fn index_source_files(&self) -> Result<Vec<ResourceFile>> {
        let mut builder = WalkBuilder::new(&self.java_root);
        let mut overrides = OverrideBuilder::new(&self.java_root);
        overrides.add("*.java")?;
        overrides.add("*.kt")?;
        builder.threads(36);
        builder.overrides(overrides.build()?);
    
        let (tx, rx) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
    
        thread::spawn(move || {
            let mut results = Vec::new();
            for received in rx {
                results.push(received);
            }
    
            tx2.send(results).unwrap();
        });
    
        let walker = builder.build_parallel();
        walker.run(move || {
            let tx = tx.clone();
            return Box::new(move |result| {
                let index = Indexer::index_source_file(&result.unwrap().path());
                if let Ok(index) = index {
                    tx.send(index).unwrap();
                }
                WalkState::Continue
            });
        });
    
        let results = rx2.recv().unwrap();
        Ok(results)
    }

    pub fn serialize(&self, index: &ResourceIndex) -> Result<()> {
        // TODO: use a hash of the java and res root.
        // TODO: compress with gz
        let now = Instant::now();

        let cache_file = self.cache_dir.join("res_cache.bin");
        let file = File::create(cache_file)?;
        bincode::serialize_into(file, &index)?;

        println!("Saved index in {}s", now.elapsed().as_secs());

        Ok(())
    }

    pub fn deserialize(&self) -> Result<ResourceIndex> {
        let now = Instant::now();
        let cache_file = self.cache_dir.join("res_cache.bin");
        let file = File::open(cache_file)?;
        let file = BufReader::new(&file);

        Ok(bincode::deserialize_from(file)?)
    }
    
    pub fn index(&self) -> Result<ResourceIndex> {
        println!("Indexing resources...");
    
        let now = Instant::now();
        let mut xml_files = self.index_xml_files()?;
        println!(
            "Indexed {} xml files in {}s",
            xml_files.len(),
            now.elapsed().as_secs()
        );
    
        let now = Instant::now();
        let mut source_files = self.index_source_files()?;
        println!(
            "Indexed {} source files in {}s",
            source_files.len(),
            now.elapsed().as_secs()
        );
    
        source_files.append(&mut xml_files);
    
        let now = Instant::now();
        let index = ResourceIndex::new(source_files);
        println!("Inverted index in {}s", now.elapsed().as_secs());
    
        Ok(index)
    }    
}
