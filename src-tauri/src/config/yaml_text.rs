use std::path::Path;

use super::fileio::{atomic_write_text, ApplyError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub key: String,
    pub lines: Vec<String>,
}

pub fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

pub fn trimmed(line: &str) -> String {
    line.trim().to_string()
}

pub fn is_blank(line: &str) -> bool {
    trimmed(line).is_empty()
}

pub fn is_comment(line: &str) -> bool {
    trimmed(line).starts_with('#')
}

pub fn section_header(key: &str, lines: &[String]) -> Option<usize> {
    let prefix = format!("{key}:");
    lines.iter().position(|line| indent_of(line) == 0 && trimmed(line).starts_with(&prefix))
}

pub fn section_end(header: usize, lines: &[String]) -> usize {
    let mut index = header + 1;
    while index < lines.len() {
        let line = &lines[index];
        if !is_blank(line) && !is_comment(line) && indent_of(line) == 0 {
            return index;
        }
        index += 1;
    }
    lines.len()
}

pub fn value_of_child(key: &str, header: usize, lines: &[String]) -> Option<String> {
    let end = section_end(header, lines);
    let start = header + 1;
    let mut child_indent: Option<usize> = None;
    for index in start..end.max(start) {
        let line = &lines[index];
        if is_blank(line) || is_comment(line) {
            continue;
        }
        let level = indent_of(line);
        if child_indent.is_none() {
            child_indent = Some(level);
        }
        if Some(level) != child_indent {
            continue;
        }
        let body = trimmed(line);
        let Some(colon) = body.find(':') else {
            continue;
        };
        if scalar_key(&body[..colon]) != key {
            continue;
        }
        return Some(unquote(body[colon + 1..].trim()));
    }
    None
}

pub fn child_blocks(range: std::ops::Range<usize>, lines: &[String]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut child_indent: Option<usize> = None;
    let mut current: Option<Block> = None;
    for index in range {
        let line = lines[index].clone();
        if is_blank(&line) || is_comment(&line) {
            if let Some(block) = current.as_mut() {
                block.lines.push(line);
            }
            continue;
        }
        let level = indent_of(&line);
        if child_indent.is_none() {
            child_indent = Some(level);
        }
        if Some(level) == child_indent {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let body = trimmed(&line);
            let key = body
                .find(':')
                .map(|colon| scalar_key(&body[..colon]))
                .unwrap_or(body);
            current = Some(Block {
                key,
                lines: vec![line],
            });
        } else if current.is_some() {
            if let Some(block) = current.as_mut() {
                block.lines.push(line);
            }
        } else {
            current = Some(Block {
                key: String::new(),
                lines: vec![line],
            });
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks.into_iter().filter(|block| !block.key.is_empty()).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    Scalar(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<YamlValue>),
    Map(Vec<(String, YamlValue)>),
}

pub fn emit_children(pairs: &[(String, YamlValue)], indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    for (key, value) in pairs {
        match value {
            YamlValue::Scalar(raw) => {
                lines.push(format!("{}{}: {}", pad, scalar_key(key), quote(raw)));
            }
            YamlValue::Int(number) => {
                lines.push(format!("{}{}: {}", pad, scalar_key(key), number));
            }
            YamlValue::Float(number) => {
                lines.push(format!("{}{}: {}", pad, scalar_key(key), number));
            }
            YamlValue::Bool(flag) => {
                lines.push(format!(
                    "{}{}: {}",
                    pad,
                    scalar_key(key),
                    if *flag { "true" } else { "false" }
                ));
            }
            YamlValue::List(items) => {
                if items.is_empty() {
                    lines.push(format!("{}{}: []", pad, scalar_key(key)));
                    continue;
                }
                lines.push(format!("{}{}:", pad, scalar_key(key)));
                emit_list(items, indent + 2, lines);
            }
            YamlValue::Map(nested) => {
                if nested.is_empty() {
                    lines.push(format!("{}{}: {{}}", pad, scalar_key(key)));
                    continue;
                }
                lines.push(format!("{}{}:", pad, scalar_key(key)));
                emit_children(nested, indent + 2, lines);
            }
        }
    }
}

fn emit_list(items: &[YamlValue], indent: usize, lines: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    for item in items {
        match item {
            YamlValue::Scalar(raw) => lines.push(format!("{}- {}", pad, quote(raw))),
            YamlValue::Int(number) => lines.push(format!("{}- {}", pad, number)),
            YamlValue::Float(number) => lines.push(format!("{}- {}", pad, number)),
            YamlValue::Bool(flag) => {
                lines.push(format!("{}- {}", pad, if *flag { "true" } else { "false" }))
            }
            YamlValue::Map(pairs) => {
                let Some(first) = pairs.first() else {
                    lines.push(format!("{}- {{}}", pad));
                    continue;
                };
                let mut head: Vec<String> = Vec::new();
                emit_children(std::slice::from_ref(first), 0, &mut head);
                let body = head.join("\n");
                let mut split: Vec<&str> = body.split('\n').collect();
                let first_line = split.remove(0);
                lines.push(format!("{}- {}", pad, first_line));
                for line in split {
                    lines.push(format!("{}  {}", pad, line));
                }
                if pairs.len() > 1 {
                    let mut rest: Vec<String> = Vec::new();
                    emit_children(&pairs[1..], indent + 2, &mut rest);
                    lines.extend(rest);
                }
            }
            YamlValue::List(_) => lines.push(format!("{}-", pad)),
        }
    }
}

pub fn scalar_key(raw: &str) -> String {
    let needs_quote = raw.is_empty() || raw.chars().any(|c| !is_key_plain(c));
    if needs_quote {
        quote(raw)
    } else {
        raw.to_string()
    }
}

fn is_key_plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')
}

pub fn quote(value: &str) -> String {
    let needs_quote = value.is_empty()
        || value.chars().any(|c| !is_value_plain(c))
        || ["true", "false", "null", "yes", "no", "on", "off", "~"].contains(&value.to_lowercase().as_str())
        || value.parse::<f64>().is_ok();
    if !needs_quote {
        return value.to_string();
    }
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(character),
        }
    }
    format!("\"{escaped}\"")
}

fn is_value_plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '@' | '+' | '-')
}

pub fn unquote(raw: &str) -> String {
    let value = raw;
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let mut inner = value[1..value.len() - 1].to_string();
        inner = inner.replace("\\n", "\n");
        inner = inner.replace("\\\"", "\"");
        inner = inner.replace("\\\\", "\\");
        return inner;
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

pub fn replace_section(key: &str, lines: &[String], header_line: &str, body: &[String]) -> Vec<String> {
    let Some(header) = section_header(key, lines) else {
        if body.is_empty() {
            return lines.to_vec();
        }
        let mut next = lines.to_vec();
        if let Some(last) = next.last() {
            if !is_blank(last) {
                next.push(String::new());
            }
        }
        next.push(if header_line.is_empty() {
            format!("{key}:")
        } else {
            header_line.to_string()
        });
        next.extend(body.iter().cloned());
        return next;
    };
    let end = section_end(header, lines);
    let mut next: Vec<String> = lines[..header].to_vec();
    if !body.is_empty() {
        next.push(if header_line.is_empty() {
            format!("{key}:")
        } else {
            header_line.to_string()
        });
        next.extend(body.iter().cloned());
    }
    next.extend(lines[end..].iter().cloned());
    trim_trailing_blank(next)
}

pub fn trim_trailing_blank(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(|last| is_blank(last)) {
        lines.pop();
    }
    lines
}

pub fn render(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

pub fn write(lines: &[String], path: &Path, permissions: u32) -> Result<(), ApplyError> {
    atomic_write_text(path, &render(lines), permissions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_plain_and_special() {
        assert_eq!(quote("abc"), "abc");
        assert_eq!(quote("true"), "\"true\"");
        assert_eq!(quote("1.5"), "\"1.5\"");
        assert_eq!(quote("a b"), "\"a b\"");
        assert_eq!(unquote("\"a\\nb\""), "a\nb");
    }

    #[test]
    fn replace_section_rewrites_or_appends() {
        let lines = vec!["providers:".to_string(), "  a: 1".to_string(), "other: x".to_string()];
        let next = replace_section("providers", &lines, "", &["  b: 2".to_string()]);
        assert_eq!(next, vec!["providers:".to_string(), "  b: 2".to_string(), "other: x".to_string()]);

        let appended = replace_section("mcp", &next, "", &["  dbx: {}".to_string()]);
        assert_eq!(appended.last().map(String::as_str), Some("  dbx: {}"));
        assert!(appended.iter().any(|line| line == "mcp:"));
    }
}
