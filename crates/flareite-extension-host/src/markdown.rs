//! Turns Markdown into the nodes of a preview.
//!
//! pulldown-cmark parses the text as CommonMark with GitHub's tables,
//! strikethrough, and task lists. Its events nest like the preview's nodes,
//! so each start event opens a node, each end event closes it, and text goes
//! into the node that's open. The exceptions:
//!
//! - **Code blocks and pictures** are single nodes, so their text (a
//!   picture's description) is collected until they end.
//! - **HTML** can't be shown, so it's reduced to its text: `<br>` becomes a
//!   line break, comments disappear, and an HTML block becomes a paragraph
//!   of whatever text is between its tags.
//! - **Front matter** (YAML or TOML at the top) is left out.
//!
//! Extensions' READMEs are rendered with it, and extensions call it
//! through `workbench::render-markdown` (1.14), so a Markdown preview and a
//! README look the same and no extension needs a Markdown parser of its own.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::bindings::flareite::extension::types::{
  ColumnAlignment, PreviewCode, PreviewElement, PreviewImage, PreviewLink, PreviewNode,
};
use crate::preview::{MAX_PREVIEW_NODES as MAX_NODES, MAX_PREVIEW_TEXT_BYTES};

/// Most Markdown `render-markdown` takes: what renders to no more text than
/// a preview may hold.
pub const MAX_MARKDOWN_BYTES: usize = MAX_PREVIEW_TEXT_BYTES;

fn options() -> Options {
  Options::ENABLE_TABLES
    | Options::ENABLE_STRIKETHROUGH
    | Options::ENABLE_TASKLISTS
    | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
    | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
}

/// Text collected instead of added as nodes.
enum Collecting {
  Nothing,
  Code {
    language: Option<String>,
    text: String,
  },
  Html(String),
  /// A picture's description. `depth` counts pictures inside it, which
  /// only add to the description.
  Image {
    image: PreviewImage,
    depth: usize,
  },
  /// Front matter, which isn't shown.
  Metadata,
}

struct Builder {
  nodes: Vec<PreviewNode>,
  /// The open elements: each one's node, or `None` for those that have no
  /// node of their own (their children go into the node around them).
  open: Vec<Option<u32>>,
  collecting: Collecting,
}

impl Builder {
  fn parent(&self) -> Option<u32> {
    self.open.iter().rev().find_map(|&node| node)
  }

  fn push(&mut self, element: PreviewElement) -> u32 {
    let index = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
    self.nodes.push(PreviewNode {
      parent: self.parent(),
      element,
    });
    index
  }

  fn open(&mut self, element: PreviewElement) {
    let index = self.push(element);
    self.open.push(Some(index));
  }

  /// Adds text, joining it to the text just before it in the same node.
  fn text(&mut self, text: &str) {
    let parent = self.parent();
    if let Some(PreviewNode {
      parent: last_parent,
      element: PreviewElement::Text(last),
    }) = self.nodes.last_mut()
      && *last_parent == parent
    {
      last.push_str(text);
      return;
    }
    self.push(PreviewElement::Text(text.to_owned()));
  }

  fn start(&mut self, tag: Tag<'_>) {
    let element = match tag {
      Tag::Paragraph => PreviewElement::Paragraph,
      Tag::Heading { level, .. } => PreviewElement::Heading(level as u8),
      Tag::BlockQuote(_) => PreviewElement::Quote,
      Tag::CodeBlock(kind) => {
        let language = match kind {
          CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_owned),
          CodeBlockKind::Indented => None,
        };
        self.collecting = Collecting::Code {
          language,
          text: String::new(),
        };
        return;
      }
      Tag::HtmlBlock => {
        self.collecting = Collecting::Html(String::new());
        return;
      }
      Tag::List(Some(start)) => {
        PreviewElement::NumberedList(u32::try_from(start).unwrap_or(u32::MAX))
      }
      Tag::List(None) => PreviewElement::BulletedList,
      Tag::Item => PreviewElement::ListItem(None),
      Tag::Table(alignments) => PreviewElement::Table(
        alignments
          .into_iter()
          .map(|alignment| match alignment {
            Alignment::None => ColumnAlignment::None,
            Alignment::Left => ColumnAlignment::Left,
            Alignment::Center => ColumnAlignment::Center,
            Alignment::Right => ColumnAlignment::Right,
          })
          .collect(),
      ),
      Tag::TableHead => PreviewElement::TableHead,
      Tag::TableRow => PreviewElement::TableRow,
      Tag::TableCell => PreviewElement::TableCell,
      Tag::Emphasis => PreviewElement::Emphasis,
      Tag::Strong => PreviewElement::Strong,
      Tag::Strikethrough => PreviewElement::Strikethrough,
      Tag::Link {
        link_type,
        dest_url,
        title,
        ..
      } => PreviewElement::Link(PreviewLink {
        destination: if link_type == LinkType::Email {
          format!("mailto:{dest_url}")
        } else {
          dest_url.into_string()
        },
        title: non_empty(&title),
      }),
      Tag::Image {
        dest_url, title, ..
      } => {
        self.collecting = Collecting::Image {
          image: PreviewImage {
            source: dest_url.into_string(),
            description: String::new(),
            title: non_empty(&title),
          },
          depth: 1,
        };
        return;
      }
      Tag::MetadataBlock(_) => {
        self.collecting = Collecting::Metadata;
        return;
      }
      // Not enabled, so not expected: show their contents in place.
      _ => {
        self.open.push(None);
        return;
      }
    };
    self.open(element);
  }

  fn end(&mut self) {
    self.open.pop();
  }

  fn event(&mut self, event: Event<'_>) {
    match &mut self.collecting {
      Collecting::Nothing => {}
      Collecting::Code { language, text } => {
        match event {
          Event::Text(more) => text.push_str(&more),
          Event::End(TagEnd::CodeBlock) => {
            let mut text = std::mem::take(text);
            if text.ends_with('\n') {
              text.pop();
            }
            let language = language.take();
            self.collecting = Collecting::Nothing;
            self.push(PreviewElement::CodeBlock(PreviewCode { language, text }));
          }
          _ => {}
        }
        return;
      }
      Collecting::Html(html) => {
        match event {
          Event::Html(more) | Event::Text(more) => html.push_str(&more),
          Event::End(TagEnd::HtmlBlock) => {
            let text = html_text(html);
            self.collecting = Collecting::Nothing;
            if !text.is_empty() {
              self.open(PreviewElement::Paragraph);
              self.text(&text);
              self.end();
            }
          }
          _ => {}
        }
        return;
      }
      Collecting::Image { image, depth } => {
        match event {
          Event::Text(text) | Event::Code(text) => image.description.push_str(&text),
          Event::SoftBreak | Event::HardBreak => image.description.push(' '),
          Event::Start(Tag::Image { .. }) => *depth += 1,
          Event::End(TagEnd::Image) => {
            *depth -= 1;
            if *depth == 0 {
              let Collecting::Image { image, .. } =
                std::mem::replace(&mut self.collecting, Collecting::Nothing)
              else {
                unreachable!();
              };
              self.push(PreviewElement::Image(image));
            }
          }
          _ => {}
        }
        return;
      }
      Collecting::Metadata => {
        if let Event::End(TagEnd::MetadataBlock(_)) = event {
          self.collecting = Collecting::Nothing;
        }
        return;
      }
    }
    match event {
      Event::Start(tag) => self.start(tag),
      Event::End(_) => self.end(),
      Event::Text(text) => self.text(&text),
      Event::Code(code) => {
        self.push(PreviewElement::Code(code.into_string()));
      }
      Event::SoftBreak => self.text("\n"),
      Event::HardBreak => {
        self.push(PreviewElement::LineBreak);
      }
      Event::Rule => {
        self.push(PreviewElement::Rule);
      }
      Event::TaskListMarker(done) => self.check_item(done),
      Event::InlineHtml(html) => {
        if is_line_break(&html) {
          self.push(PreviewElement::LineBreak);
        }
      }
      // Not enabled, so not expected: shown as written.
      Event::Html(text)
      | Event::InlineMath(text)
      | Event::DisplayMath(text)
      | Event::FootnoteReference(text) => self.text(&text),
    }
  }

  /// Marks the innermost open list item as a task, done or not.
  fn check_item(&mut self, done: bool) {
    for &index in self.open.iter().rev().flatten() {
      if let Some(PreviewNode {
        element: PreviewElement::ListItem(checked),
        ..
      }) = self.nodes.get_mut(index as usize)
      {
        *checked = Some(done);
        return;
      }
    }
  }

  /// Keeps the content within the host's limit: cuts it before a top-level
  /// node, and says so.
  fn finish(mut self) -> Vec<PreviewNode> {
    if self.nodes.len() > MAX_NODES {
      let cut = self.nodes[..MAX_NODES - 2]
        .iter()
        .rposition(|node| node.parent.is_none())
        .unwrap_or(0);
      self.nodes.truncate(cut);
      self.open.clear();
      self.open(PreviewElement::Paragraph);
      self.open(PreviewElement::Emphasis);
      self.text("The rest of this document is too long to preview.");
    }
    self.nodes
  }
}

fn non_empty(text: &str) -> Option<String> {
  (!text.is_empty()).then(|| text.to_owned())
}

/// `<br>`, `<br/>`, or `<br />`, in any case.
fn is_line_break(html: &str) -> bool {
  let tag = html.trim().to_ascii_lowercase();
  let inner = tag
    .strip_prefix('<')
    .and_then(|tag| tag.strip_suffix('>'))
    .map(|tag| tag.trim_end_matches('/').trim());
  inner == Some("br")
}

/// The text of some HTML: tags and comments removed, a few common entities
/// decoded, and white space collapsed.
fn html_text(html: &str) -> String {
  let mut text = String::new();
  let mut rest = html;
  while !rest.is_empty() {
    if let Some(after) = rest.strip_prefix("<!--") {
      rest = after.find("-->").map_or("", |end| &after[end + 3..]);
    } else if rest.starts_with('<') {
      rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
      text.push(' ');
    } else {
      let end = rest.find('<').unwrap_or(rest.len());
      text.push_str(&rest[..end]);
      rest = &rest[end..];
    }
  }
  let decoded = text
    .replace("&nbsp;", " ")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&");
  decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The preview of a Markdown document.
pub fn render(markdown: &str) -> Vec<PreviewNode> {
  let mut builder = Builder {
    nodes: Vec::new(),
    open: Vec::new(),
    collecting: Collecting::Nothing,
  };
  for event in Parser::new_ext(markdown, options()) {
    builder.event(event);
  }
  builder.finish()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The nodes as `parent: element`, one per line, for comparing.
  fn outline(markdown: &str) -> String {
    render(markdown)
      .iter()
      .enumerate()
      .map(|(index, node)| {
        let parent = node.parent.map_or("-".into(), |parent| parent.to_string());
        let element = format!("{:?}", node.element)
          .replace("PreviewElement::", "")
          .replace("ColumnAlignment::", "");
        format!("{index} in {parent}: {element}")
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn renders_blocks_and_inline_text() {
    let markdown =
      "# Title\n\nSome *very* **bold** text,\nwrapped.  \nBroken.\n\n> Quoted `code`.\n\n---\n";
    let expected = [
      "0 in -: Heading(1)",
      "1 in 0: Text(\"Title\")",
      "2 in -: Paragraph",
      "3 in 2: Text(\"Some \")",
      "4 in 2: Emphasis",
      "5 in 4: Text(\"very\")",
      "6 in 2: Text(\" \")",
      "7 in 2: Strong",
      "8 in 7: Text(\"bold\")",
      "9 in 2: Text(\" text,\\nwrapped.\")",
      "10 in 2: LineBreak",
      "11 in 2: Text(\"Broken.\")",
      "12 in -: Quote",
      "13 in 12: Paragraph",
      "14 in 13: Text(\"Quoted \")",
      "15 in 13: Code(\"code\")",
      "16 in 13: Text(\".\")",
      "17 in -: Rule",
    ];
    assert_eq!(outline(markdown), expected.join("\n"));
  }

  #[test]
  fn renders_lists_and_tasks() {
    let markdown = "3. three\n4. four\n\n- [x] done\n- [ ] to do\n  - nested\n";
    let expected = [
      "0 in -: NumberedList(3)",
      "1 in 0: ListItem(None)",
      "2 in 1: Text(\"three\")",
      "3 in 0: ListItem(None)",
      "4 in 3: Text(\"four\")",
      "5 in -: BulletedList",
      "6 in 5: ListItem(Some(true))",
      "7 in 6: Text(\"done\")",
      "8 in 5: ListItem(Some(false))",
      "9 in 8: Text(\"to do\")",
      "10 in 8: BulletedList",
      "11 in 10: ListItem(None)",
      "12 in 11: Text(\"nested\")",
    ];
    assert_eq!(outline(markdown), expected.join("\n"));
  }

  #[test]
  fn renders_tables() {
    let markdown = "| a | b |\n|:--|--:|\n| 1 | ~~2~~ |\n";
    let expected = [
      "0 in -: Table([Left, Right])",
      "1 in 0: TableHead",
      "2 in 1: TableCell",
      "3 in 2: Text(\"a\")",
      "4 in 1: TableCell",
      "5 in 4: Text(\"b\")",
      "6 in 0: TableRow",
      "7 in 6: TableCell",
      "8 in 7: Text(\"1\")",
      "9 in 6: TableCell",
      "10 in 9: Strikethrough",
      "11 in 10: Text(\"2\")",
    ];
    assert_eq!(outline(markdown), expected.join("\n"));
  }

  #[test]
  fn renders_code_links_and_pictures() {
    let markdown = "```rust title=x\nfn main() {}\n```\n\n    indented\n\n[a *b*](b.md \"Bee\") <me@example.com> ![A ![nested](n.png) `pic`](p.png)\n";
    let nodes = render(markdown);
    assert_eq!(
      nodes[0].element,
      PreviewElement::CodeBlock(PreviewCode {
        language: Some("rust".into()),
        text: "fn main() {}".into()
      })
    );
    assert_eq!(
      nodes[1].element,
      PreviewElement::CodeBlock(PreviewCode {
        language: None,
        text: "indented".into()
      })
    );
    assert_eq!(
      nodes[3].element,
      PreviewElement::Link(PreviewLink {
        destination: "b.md".into(),
        title: Some("Bee".into())
      })
    );
    assert!(nodes.iter().any(|node| node.element
      == PreviewElement::Link(PreviewLink {
        destination: "mailto:me@example.com".into(),
        title: None
      })));
    let image = nodes.last().expect("a picture");
    assert_eq!(
      image.element,
      PreviewElement::Image(PreviewImage {
        source: "p.png".into(),
        description: "A nested pic".into(),
        title: None
      })
    );
    assert_eq!(image.parent, Some(2), "in the paragraph");
  }

  #[test]
  fn reduces_html_to_text_and_leaves_out_front_matter() {
    let markdown = "---\ntitle: x\n---\n\n<!-- hidden -->\n\n<p align=\"center\">\n  <b>Hello</b> &amp; welcome\n</p>\n\none<br>two <span>three</span>\n";
    let expected = [
      "0 in -: Paragraph",
      "1 in 0: Text(\"Hello & welcome\")",
      "2 in -: Paragraph",
      "3 in 2: Text(\"one\")",
      "4 in 2: LineBreak",
      "5 in 2: Text(\"two three\")",
    ];
    assert_eq!(outline(markdown), expected.join("\n"));
  }

  #[test]
  fn long_documents_are_cut_with_a_note() {
    let markdown = "word\n\n".repeat(MAX_NODES);
    let nodes = render(&markdown);
    assert!(nodes.len() <= MAX_NODES);
    assert!(matches!(
      &nodes.last().expect("a note").element,
      PreviewElement::Text(text) if text.contains("too long")
    ));
    assert!(
      nodes
        .iter()
        .enumerate()
        .all(|(index, node)| node.parent.is_none_or(|parent| (parent as usize) < index))
    );
  }
}
