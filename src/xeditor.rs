use anyhow::Result;
use std::fs;
use std::fs::File;
use std::io::LineWriter;
use std::path::Path;
use std::str;
use xml::attribute::OwnedAttribute;
use xml::common::Position;
use xml::name::OwnedName;
use xml::reader::{EventReader, XmlEvent};

use std::collections::HashMap;
use std::io::prelude::*;

#[derive(Debug)]
struct ElementLocation {
    start_line: u64,
    end_line: u64,
}

fn find_location_to_strip(
    file_content: &str,
    matcher: &ElementMatcher,
) -> Result<Option<ElementLocation>> {
    let mut parser = EventReader::new(file_content.as_bytes());
    let mut in_skipped_element = false;
    let mut start_line: u64 = 0;
    let mut start_depth = -1;
    let mut depth = 0;
    loop {
        let e = parser.next();
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                depth += 1;
                let pos = parser.position();
                if matcher.matches(&name, &attributes) {
                    in_skipped_element = true;
                    start_depth = depth;
                    start_line = pos.row;
                }
            }
            Ok(XmlEvent::EndElement { .. }) => {
                let pos = parser.position();
                if in_skipped_element && depth == start_depth {
                    let end_line = pos.row;
                    return Ok(Some(ElementLocation {
                        start_line,
                        end_line,
                    }));
                }
                in_skipped_element = false;
                depth -= 1;
            }
            Ok(XmlEvent::EndDocument) => return Ok(None),
            Err(e) => return Err(anyhow::Error::new(e)),
            _ => {}
        }
    }
}

pub fn remove_element(path: &Path, matcher: &ElementMatcher) -> Result<bool> {
    // We're potentially going to have to read the file twice: once for the xml
    // parser, and again for the buffer to write out with an element trimmed out.
    // Start off by reading it all into memory.
    let file_content = fs::read_to_string(path)?;

    let location = find_location_to_strip(&file_content, matcher)?;
    match location {
        Some(location) => {
            let file = File::create(path)?;
            let mut file = LineWriter::new(file);
            let mut line_number = 0;
            for line in file_content.lines() {
                if line_number < location.start_line || line_number > location.end_line {
                    file.write_all(line.as_bytes())?;
                    file.write_all(b"\n")?;
                }
                line_number -= 1;
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

pub struct ElementMatcher {
    local_name: String,
    local_attribute_values: HashMap<String, String>,
}

impl ElementMatcher {
    pub fn for_local_name(local_name: &str) -> ElementMatcher {
        ElementMatcher {
            local_name: local_name.to_string(),
            local_attribute_values: HashMap::new(),
        }
    }

    pub fn attr<'a>(&'a mut self, local_name: &str, value: &str) -> &'a mut ElementMatcher {
        self.local_attribute_values
            .insert(local_name.to_string(), value.to_string());

        self
    }

    fn matches(&self, name: &OwnedName, attrs: &[OwnedAttribute]) -> bool {
        if !self.local_name.eq(&name.local_name) {
            return false;
        }

        for attr in attrs {
            if let Some(required_val) = self.local_attribute_values.get(&attr.name.local_name) {
                if !required_val.eq(&attr.value) {
                    return false;
                }
            }
        }

        true
    }
}
