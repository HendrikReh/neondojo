use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;

pub(crate) type Node = BTreeMap<String, Vec<String>>;

pub(crate) const MAX_SGF_BYTES: usize = 32 * 1024 * 1024;

const MAX_NODES: usize = 100_000;

pub(crate) fn escape_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace(']', "\\]")
}

pub(crate) fn write_property(output: &mut String, key: &str, values: &[String]) {
    output.push_str(key);
    for value in values {
        output.push('[');
        output.push_str(&escape_value(value));
        output.push(']');
    }
}

pub(crate) fn serialize_trees(trees: &[Vec<Node>]) -> String {
    let mut output = String::new();
    for nodes in trees {
        output.push('(');
        for node in nodes {
            output.push(';');
            for (key, values) in node {
                write_property(&mut output, key, values);
            }
        }
        output.push_str(")\n");
    }
    output
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.bytes.get(self.pos).copied()
    }
    fn expect(&mut self, c: u8) -> Result<()> {
        ensure!(
            self.peek() == Some(c),
            "SGF: expected '{}' at byte {}",
            c as char,
            self.pos
        );
        self.pos += 1;
        Ok(())
    }
    fn value(&mut self) -> Result<String> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        loop {
            let c = *self
                .bytes
                .get(self.pos)
                .context("SGF: unterminated property value")?;
            self.pos += 1;
            match c {
                b']' => return String::from_utf8(out).context("SGF: invalid UTF-8 property"),
                b'\\' => {
                    let next = *self.bytes.get(self.pos).context("SGF: incomplete escape")?;
                    self.pos += 1;
                    if next == b'\r' || next == b'\n' {
                        if self
                            .bytes
                            .get(self.pos)
                            .is_some_and(|b| (*b == b'\r' || *b == b'\n') && *b != next)
                        {
                            self.pos += 1;
                        }
                    } else {
                        out.push(next);
                    }
                }
                0 => bail!("SGF: NUL in property"),
                _ => out.push(c),
            }
            ensure!(
                out.len() <= 1024 * 1024,
                "SGF: property exceeds 1 MiB limit"
            );
        }
    }
    fn tree(&mut self, depth: usize) -> Result<Vec<Node>> {
        ensure!(depth < 64, "SGF: nesting limit exceeded");
        self.expect(b'(')?;
        let mut sequence = Vec::new();
        while self.peek() == Some(b';') {
            self.pos += 1;
            self.nodes += 1;
            ensure!(self.nodes <= MAX_NODES, "SGF: node limit exceeded");
            let mut node = Node::new();
            while self.peek().is_some_and(|c| c.is_ascii_uppercase()) {
                let start = self.pos;
                while self.bytes.get(self.pos).is_some_and(u8::is_ascii_uppercase) {
                    self.pos += 1;
                }
                let key = std::str::from_utf8(&self.bytes[start..self.pos])?.to_owned();
                let mut values = Vec::new();
                while self.peek() == Some(b'[') {
                    values.push(self.value()?);
                }
                ensure!(!values.is_empty(), "SGF: property {key} has no value");
                ensure!(
                    node.insert(key.clone(), values).is_none(),
                    "SGF: duplicate property {key}"
                );
            }
            sequence.push(node);
        }
        ensure!(!sequence.is_empty(), "SGF: empty game tree");
        if self.peek() == Some(b'(') {
            sequence.extend(self.tree(depth + 1)?);
            ensure!(
                self.peek() != Some(b'('),
                "SGF variations are ambiguous; export the played main line before importing"
            );
        }
        self.expect(b')')?;
        Ok(sequence)
    }
}

pub(crate) fn parse_trees(input: &str) -> Result<Vec<Vec<Node>>> {
    ensure!(input.len() <= MAX_SGF_BYTES, "SGF exceeds 32 MiB limit");
    let input = input.trim_start_matches('\u{feff}');
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
        nodes: 0,
    };
    let mut trees = Vec::new();
    while parser.peek().is_some() {
        trees.push(parser.tree(0)?);
    }
    ensure!(!trees.is_empty(), "SGF contains no games");
    Ok(trees)
}

pub(crate) fn one<'a>(node: &'a Node, name: &str) -> Result<Option<&'a str>> {
    let Some(values) = node.get(name) else {
        return Ok(None);
    };
    ensure!(values.len() == 1, "SGF: {name} requires exactly one value");
    Ok(Some(&values[0]))
}

pub(super) fn number(value: &str, name: &str) -> Result<i32> {
    let n: i32 = value
        .trim()
        .parse()
        .with_context(|| format!("SGF: invalid {name}: {value}"))?;
    ensure!(
        (0..=32767).contains(&n),
        "SGF: {name} exceeds GNUbg ID range"
    );
    Ok(n)
}
