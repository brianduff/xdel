use anyhow::{anyhow, Result};
use std::env;
use std::fs;
use std::fs::File;
use std::io::LineWriter;
use std::str;
use xml::attribute::OwnedAttribute;
use xml::common::Position;
use xml::reader::{EventReader, XmlEvent};
use std::path::Path;

use std::io::prelude::*;

mod index;

use crate::index::index;

#[derive(Debug)]
struct ElementLocation {
    start_line: u64,
    end_line: u64,
}

fn get_name_attr(attributes: &Vec<OwnedAttribute>) -> Option<String> {
    for attr in attributes {
        if attr.name.local_name.eq("name") {
            return Some(attr.value.to_owned());
        }
    }

    None
}

fn strip(filename: &str, string_name: &str) -> Result<()> {
    // We're potentially going to have to read the file twice: once for the xml
    // parser, and again for the buffer to write out with an element trimmed out.
    // Start off by reading it all into memory.
    let file_content = fs::read_to_string(filename)?;

    let location = find_location_to_strip(&file_content, string_name)?;
    match location {
        Some(location) => {

            let file = File::create(filename)?;
            let mut file = LineWriter::new(file);
            let mut line_number = 0;
            for line in file_content.lines() {
                if line_number < location.start_line || line_number > location.end_line {
                    file.write_all(line.as_bytes())?;
                    file.write_all("\n".as_bytes())?;
                }
                line_number = line_number + 1;
            }
            println!("👍 Stripped {} from {}", string_name, filename);
        }
        None => {
            println!("🤷 Didn't find {} in {}", string_name, filename);
        }
    }
    Ok(())
}

fn find_location_to_strip(
    file_content: &String,
    string_name: &str,
) -> Result<Option<ElementLocation>> {
    let mut parser = EventReader::new(file_content.as_bytes());
    let mut in_skipped_element = false;
    let mut start_line: u64 = 0;
    loop {
        let e = parser.next();
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                let pos = parser.position();
                if name.local_name.eq("string") {
                    if let Some(name) = get_name_attr(&attributes) {
                        if name.eq(string_name) {
                            in_skipped_element = true;
                            start_line = pos.row;
                        }
                    }
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let pos = parser.position();
                if name.local_name.eq("string") && in_skipped_element {
                    let end_line = pos.row;
                    return Ok(Some(ElementLocation {
                        start_line,
                        end_line,
                    }));
                }
                in_skipped_element = false;
            }
            Ok(XmlEvent::EndDocument) => return Ok(None),
            Err(e) => return Err(anyhow::Error::new(e)),
            _ => {}
        }
    }
}

/// A simple program that reads an strings.xml file and strips
/// elements matching the given name out without disrupting the rest
/// of the file.
fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let index = index(&args[1])?;
    index.write_index(Path::new("/tmp/idex"))?;
    let files = index.get_files("quiet_mode_entered_by_swipe");
    match files {
        Some(files) => {
            println!("Locations defining 'quiet_mode_entered_by_swipe':");
            for file in files {
                println!("  {}", file);
            }        
        },
        None => {
            println!("Key not found: 'quiet_mode_entered_by_swipe'");
        }
    }


    match args.len() {
        3 => {
            let filename = &args[2];
            let string_name = &args[3];

            strip(&filename, &string_name)
        }
        _ => Err(anyhow!("Insufficient args given")),
    }
}
